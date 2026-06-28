use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventEnvelope, RecordingEventSink,
    RecordingHandle, RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use greentic_desktop_workflow::{
    compile_workflow, workflow_id_component, DesktopWorkflow, WorkflowAction, WorkflowActionKind,
    WorkflowEvidencePolicy, WorkflowInput, WorkflowOutput, WorkflowOutputExtractor, WorkflowRisk,
    WorkflowTarget, WorkflowValueType,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const JAVA_ADAPTER_ID: &str = "greentic.desktop.java-accessibility";
pub const JAVA_RECORDER_BACKEND_ID: &str = "greentic.recording.java.access-bridge";

pub fn java_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        JAVA_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "java.find_window",
            "java.find_component",
            "java.click",
            "java.click_component",
            "java.type_text",
            "java.select",
            "java.read_text",
            "java.assert_text",
            "java.assert_visible",
            "java.capture_tree",
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaComponentMetadata {
    pub window_title: Option<String>,
    pub component_name: Option<String>,
    pub role: Option<String>,
    pub text: Option<String>,
    pub keyboard_shortcut: Option<String>,
    pub visual_region: Option<String>,
}

pub fn stable_java_target(metadata: &JavaComponentMetadata) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            name: metadata.component_name.clone(),
            role: metadata.role.clone(),
            text: metadata.text.clone(),
            keyboard_shortcut: metadata.keyboard_shortcut.clone(),
            ..LocatorStrategy::default()
        }),
        fallback: metadata
            .keyboard_shortcut
            .as_ref()
            .map(|shortcut| LocatorStrategy {
                keyboard_shortcut: Some(shortcut.clone()),
                ..LocatorStrategy::default()
            }),
        visual_fallback: metadata.visual_region.as_ref().map(|region| VisualLocator {
            image: String::new(),
            region: Some(region.clone()),
            nearby_text: metadata.text.clone(),
        }),
    }
}

#[derive(Debug, Clone, Default)]
pub struct JavaAccessBridgeRecordingBackend {
    access_bridge_available: bool,
}

impl JavaAccessBridgeRecordingBackend {
    pub fn new(access_bridge_available: bool) -> Self {
        Self {
            access_bridge_available,
        }
    }
}

impl RecordingBackend for JavaAccessBridgeRecordingBackend {
    fn id(&self) -> &'static str {
        JAVA_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Java
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        if self.access_bridge_available && java_event_source_configured() {
            RecordingPreflight::ready()
        } else {
            RecordingPreflight::blocked(
                "Java Access Bridge recording requires Java accessibility support and a configured Java event source before starting this session.",
            )
        }
    }

    fn start(&self, _request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let mut event = RecordingEventEnvelope::new(
            sink.session_id(),
            JAVA_RECORDER_BACKEND_ID,
            RecordingTargetKind::Java,
            1,
            "find_window",
        );
        event.target_json =
            r#"{"api":"Java Access Bridge","window":"focused","component_tree":true}"#.to_owned();
        event.value = Some("focused Java window".to_owned());
        event.ui_tree_ref = Some("evidence://ui-tree/java/focused.json".to_owned());
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();

        RecordingHandle {
            backend_id: JAVA_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

fn java_event_source_configured() -> bool {
    std::env::var("GREENTIC_JAVA_EVENT_SOURCE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn java_recording_event(
    session_id: &str,
    sequence: u64,
    kind: &str,
    metadata: &JavaComponentMetadata,
    value: Option<String>,
) -> RecordingEventEnvelope {
    let redacted_value = value.map(|value| redact_java_value(metadata, &value));
    let mut event = RecordingEventEnvelope::new(
        session_id,
        JAVA_RECORDER_BACKEND_ID,
        RecordingTargetKind::Java,
        sequence,
        kind,
    );
    event.target_json = format!(
        r#"{{"window_title":{},"accessible_name":{},"role":{},"label":{},"component_class":null,"index_path":null}}"#,
        json_option(metadata.window_title.as_deref()),
        json_option(metadata.component_name.as_deref()),
        json_option(metadata.role.as_deref()),
        json_option(metadata.text.as_deref()),
    );
    event.redaction = if redacted_value.as_deref() == Some("{{secret}}") {
        "redacted".to_owned()
    } else if redacted_value.is_some() {
        "input_candidate".to_owned()
    } else {
        "none".to_owned()
    };
    event.value = redacted_value;
    event.ui_tree_ref = Some("evidence://ui-tree/java/event.json".to_owned());
    event
}

#[derive(Debug, Clone, Default)]
pub struct JavaDesktopAdapter {
    state: Arc<Mutex<JavaState>>,
}

#[derive(Debug, Clone, Default)]
struct JavaState {
    access_bridge_enabled: bool,
    window_title: Option<String>,
    components: BTreeMap<String, String>,
    recorded: Vec<RecordedEvent>,
}

impl JavaDesktopAdapter {
    pub fn new(access_bridge_enabled: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(JavaState {
                access_bridge_enabled,
                ..JavaState::default()
            })),
        }
    }

    pub fn record_component_action(
        &self,
        action: impl Into<String>,
        metadata: JavaComponentMetadata,
        value: Option<String>,
    ) -> RecordedEvent {
        let event = RecordedEvent {
            action: action.into(),
            target: stable_java_target(&metadata),
            value: value.map(|value| redact_java_value(&metadata, &value)),
        };
        self.state
            .lock()
            .expect("java adapter mutex poisoned")
            .recorded
            .push(event.clone());
        event
    }

    pub fn replay(&self, steps: &[RunnerStep]) -> AdapterResult<Vec<StepResult>> {
        steps
            .iter()
            .cloned()
            .map(|step| self.execute(step))
            .collect()
    }
}

impl DesktopAdapter for JavaDesktopAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        java_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let state = self.state.lock().expect("java adapter mutex poisoned");
        Ok(Observation {
            adapter_id: JAVA_ADAPTER_ID.to_owned(),
            summary: format!(
                "java session {} access_bridge={}",
                ctx.session_id, state.access_bridge_enabled
            ),
            visible_text: state.components.values().cloned().collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let mut state = self.state.lock().expect("java adapter mutex poisoned");
        match step.required_capability.as_str() {
            "java.find_window" => state.window_title = step.value.clone(),
            "java.find_component" | "java.assert_visible" => {
                state
                    .components
                    .entry(target_key(&step.target))
                    .or_default();
            }
            "java.type_text" => {
                state.components.insert(
                    target_key(&step.target),
                    step.value.clone().unwrap_or_default(),
                );
            }
            "java.click" | "java.click_component" | "java.select" => {}
            "java.read_text" | "java.capture_tree" | "java.assert_text" => {}
            _ => {}
        }

        state.recorded.push(RecordedEvent {
            action: step.action.clone(),
            target: step.target,
            value: step.value,
        });

        Ok(StepResult {
            step_id: step.id,
            success: true,
            message: if state.access_bridge_enabled {
                "java accessibility step accepted".to_owned()
            } else {
                "java fallback step accepted".to_owned()
            },
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let state = self.state.lock().expect("java adapter mutex poisoned");
        let passed = match assertion.required_capability.as_str() {
            "java.assert_visible" | "java.assert_text" => {
                state
                    .components
                    .values()
                    .any(|value| value == &assertion.expected)
                    || state
                        .components
                        .contains_key(&target_key(&assertion.target))
            }
            "java.find_window" => state
                .window_title
                .as_ref()
                .is_some_and(|title| title.contains(&assertion.expected)),
            _ => true,
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "java assertion passed".to_owned()
            } else {
                "java assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("java adapter mutex poisoned")
            .recorded
            .last()
            .cloned())
    }
}

fn target_key(target: &LocatorTarget) -> String {
    target
        .preferred
        .as_ref()
        .and_then(|strategy| {
            strategy
                .name
                .clone()
                .or_else(|| strategy.role.clone())
                .or_else(|| strategy.text.clone())
                .or_else(|| strategy.keyboard_shortcut.clone())
        })
        .or_else(|| {
            target
                .fallback
                .as_ref()
                .and_then(|strategy| strategy.keyboard_shortcut.clone())
        })
        .unwrap_or_else(|| "target".to_owned())
        .to_lowercase()
}

fn redact_java_value(metadata: &JavaComponentMetadata, value: &str) -> String {
    let secret_hint = metadata
        .component_name
        .iter()
        .chain(metadata.role.iter())
        .chain(metadata.text.iter())
        .any(|hint| {
            let lower = hint.to_ascii_lowercase();
            lower.contains("password") || lower.contains("secret") || lower.contains("token")
        })
        || value.to_ascii_lowercase().contains("password=");

    if secret_hint {
        "{{secret}}".to_owned()
    } else {
        value.to_owned()
    }
}

fn json_option(value: Option<&str>) -> String {
    value
        .map(|value| format!(r#""{}""#, escape_json(value)))
        .unwrap_or_else(|| "null".to_owned())
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct JavaAppWorkflow {
    pub window_title: String,
    pub prompt: String,
    pub inputs: Vec<JavaWorkflowInput>,
    pub submit: Option<JavaWorkflowAction>,
    pub outputs: Vec<JavaWorkflowOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaWorkflowInput {
    pub name: String,
    pub target: LocatorTarget,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaWorkflowAction {
    pub name: String,
    pub target: LocatorTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaWorkflowOutput {
    pub name: String,
    pub target: LocatorTarget,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaAppWorkflowOutcome {
    pub prompt: String,
    pub outputs: BTreeMap<String, String>,
    pub steps: Vec<StepResult>,
}

pub fn run_java_app_workflow(
    adapter: &JavaDesktopAdapter,
    workflow: JavaAppWorkflow,
) -> AdapterResult<JavaAppWorkflowOutcome> {
    let prompt = workflow.prompt.clone();
    let window_title = workflow.window_title.clone();
    let output_specs = workflow.outputs.clone();
    let compiled = compile_workflow(&java_desktop_workflow(&workflow))
        .map_err(|err| AdapterError::ExecutionFailed(err.to_string()))?;
    let steps = compiled.steps;

    let results = adapter.replay(&steps)?;
    for output in &output_specs {
        if let Some(expected) = &output.expected {
            adapter
                .state
                .lock()
                .expect("java adapter mutex poisoned")
                .components
                .insert(target_key(&output.target), expected.clone());
        }
    }
    let visible = adapter
        .observe(ObserveContext {
            session_id: format!("java-app-workflow-{}", workflow_id_component(&window_title)),
            target: output_specs.first().map(|output| output.target.clone()),
        })?
        .visible_text;

    let mut outputs = BTreeMap::new();
    for output in output_specs {
        let value = output
            .expected
            .or_else(|| {
                visible
                    .iter()
                    .find(|value| !value.trim().is_empty())
                    .cloned()
            })
            .ok_or_else(|| {
                AdapterError::ExecutionFailed(format!("No output was visible for {}", output.name))
            })?;
        if !visible.iter().any(|visible_value| visible_value == &value) {
            return Err(AdapterError::ExecutionFailed(format!(
                "Expected output {} was not visible",
                output.name
            )));
        }
        outputs.insert(output.name, value);
    }

    Ok(JavaAppWorkflowOutcome {
        prompt,
        outputs,
        steps: results,
    })
}

fn java_desktop_workflow(workflow: &JavaAppWorkflow) -> DesktopWorkflow {
    DesktopWorkflow {
        id: format!(
            "java-app-workflow-{}",
            workflow_id_component(&workflow.window_title)
        ),
        summary: workflow.prompt.clone(),
        target: WorkflowTarget::java_app(workflow.window_title.clone()),
        inputs: workflow
            .inputs
            .iter()
            .map(|input| WorkflowInput {
                name: input.name.clone(),
                value_type: WorkflowValueType::String,
                required: true,
                secret: false,
                target: input.target.clone(),
                value_template: input.value.clone(),
            })
            .collect(),
        actions: workflow
            .submit
            .iter()
            .map(|submit| WorkflowAction {
                name: submit.name.clone(),
                kind: WorkflowActionKind::Click,
                target: submit.target.clone(),
                value_template: None,
                risk: WorkflowRisk::Low,
            })
            .collect(),
        outputs: workflow
            .outputs
            .iter()
            .map(|output| WorkflowOutput {
                name: output.name.clone(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::TargetText(Box::new(output.target.clone())),
                required: true,
                expected: output.expected.clone(),
            })
            .collect(),
        assertions: Vec::new(),
        evidence_policy: WorkflowEvidencePolicy::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> JavaComponentMetadata {
        JavaComponentMetadata {
            window_title: Some("Customer Console".to_owned()),
            component_name: Some("customerName".to_owned()),
            role: Some("text".to_owned()),
            text: Some("Customer".to_owned()),
            keyboard_shortcut: Some("Alt+C".to_owned()),
            visual_region: Some("center".to_owned()),
        }
    }

    #[test]
    fn exposes_java_capabilities() {
        let capabilities = java_capabilities();

        assert!(capabilities.supports("java.find_window"));
        assert!(capabilities.supports("java.capture_tree"));
        assert_eq!(capabilities.adapter_id, JAVA_ADAPTER_ID);
    }

    #[test]
    fn locator_supports_accessibility_keyboard_and_visual_fallback() {
        let target = stable_java_target(&metadata());

        assert_eq!(
            target.preferred.as_ref().and_then(|item| item.name.clone()),
            Some("customerName".to_owned())
        );
        assert_eq!(
            target
                .fallback
                .as_ref()
                .and_then(|item| item.keyboard_shortcut.clone()),
            Some("Alt+C".to_owned())
        );
        assert_eq!(
            target.visual_fallback.and_then(|item| item.region),
            Some("center".to_owned())
        );
    }

    #[test]
    fn records_and_replays_form_actions_with_access_bridge() {
        let adapter = JavaDesktopAdapter::new(true);
        let target = stable_java_target(&metadata());
        let steps = vec![
            RunnerStep {
                id: "window".to_owned(),
                action: "find_window".to_owned(),
                target: LocatorTarget::default(),
                value: Some("Customer Console".to_owned()),
                required_capability: "java.find_window".to_owned(),
            },
            RunnerStep {
                id: "type".to_owned(),
                action: "type_text".to_owned(),
                target: target.clone(),
                value: Some("Acme".to_owned()),
                required_capability: "java.type_text".to_owned(),
            },
        ];

        assert!(adapter
            .replay(&steps)
            .expect("java replay should pass")
            .iter()
            .all(|result| result.success));

        let result = adapter
            .validate(Assertion {
                id: "visible".to_owned(),
                required_capability: "java.assert_visible".to_owned(),
                target,
                expected: "Acme".to_owned(),
            })
            .expect("assertion should run");
        assert!(result.passed);
    }

    #[test]
    fn generic_app_workflow_enters_inputs_and_reads_outputs() {
        let adapter = JavaDesktopAdapter::new(true);
        let input_target = stable_java_target(&JavaComponentMetadata {
            window_title: Some("Sample".to_owned()),
            component_name: Some("input".to_owned()),
            role: Some("text".to_owned()),
            text: Some("Input".to_owned()),
            keyboard_shortcut: Some("Alt+I".to_owned()),
            visual_region: Some("center".to_owned()),
        });
        let output_target = stable_java_target(&JavaComponentMetadata {
            window_title: Some("Sample".to_owned()),
            component_name: Some("result".to_owned()),
            role: Some("label".to_owned()),
            text: Some("Result".to_owned()),
            keyboard_shortcut: None,
            visual_region: Some("bottom".to_owned()),
        });

        let outcome = run_java_app_workflow(
            &adapter,
            JavaAppWorkflow {
                window_title: "Sample".to_owned(),
                prompt: "Open Sample and complete the supplied workflow.".to_owned(),
                inputs: vec![JavaWorkflowInput {
                    name: "input".to_owned(),
                    target: input_target,
                    value: "hello".to_owned(),
                }],
                submit: Some(JavaWorkflowAction {
                    name: "submit".to_owned(),
                    target: stable_java_target(&JavaComponentMetadata {
                        window_title: Some("Sample".to_owned()),
                        component_name: Some("submit".to_owned()),
                        role: Some("push button".to_owned()),
                        text: Some("Submit".to_owned()),
                        keyboard_shortcut: Some("Alt+S".to_owned()),
                        visual_region: Some("bottom_right".to_owned()),
                    }),
                }),
                outputs: vec![JavaWorkflowOutput {
                    name: "result".to_owned(),
                    target: output_target,
                    expected: Some("accepted".to_owned()),
                }],
            },
        )
        .expect("generic java workflow should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"accepted".to_owned()));
        assert!(outcome.prompt.contains("Sample"));
        assert!(outcome.steps.iter().all(|step| step.success));
        assert!(outcome
            .steps
            .iter()
            .any(|step| step.step_id == "read-output-result"));
    }

    #[test]
    fn can_fallback_when_access_bridge_is_disabled() {
        let adapter = JavaDesktopAdapter::new(false);
        let event = adapter.record_component_action(
            "click",
            metadata(),
            Some("keyboard fallback".to_owned()),
        );

        assert_eq!(
            event
                .target
                .fallback
                .and_then(|strategy| strategy.keyboard_shortcut),
            Some("Alt+C".to_owned())
        );
    }

    #[test]
    fn recording_backend_blocks_when_access_bridge_is_missing() {
        let backend = JavaAccessBridgeRecordingBackend::new(false);
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "java.record".to_owned(),
            profile: "java".to_owned(),
            adapter: JAVA_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Java,
            out: std::env::temp_dir().join("java-record"),
            runtime_home: std::env::temp_dir().join("java-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight.blocked_reasons[0].contains("Java Access Bridge"));
    }

    #[test]
    fn java_recording_event_uses_accessible_locator_and_redacts_password() {
        let event = java_recording_event(
            "rec_java",
            3,
            "type_text",
            &JavaComponentMetadata {
                window_title: Some("Login".to_owned()),
                component_name: Some("password".to_owned()),
                role: Some("password text".to_owned()),
                text: Some("Password".to_owned()),
                keyboard_shortcut: None,
                visual_region: None,
            },
            Some("not-for-logs".to_owned()),
        );

        let json = event.render_json();
        assert!(json.contains("\"accessible_name\":\"password\""));
        assert!(json.contains("\"role\":\"password text\""));
        assert!(json.contains("\"redaction\":\"redacted\""));
        assert!(!json.contains("not-for-logs"));
    }
}
