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
use std::process::{Command, Stdio};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaTargetEvidence {
    pub app_name: Option<String>,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub accessibility_class: Option<String>,
    pub explicit_profile: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaTargetRoute {
    JavaSpecific,
    NativeOsAccessibility,
    AskUser,
}

pub fn classify_java_target(evidence: &JavaTargetEvidence) -> JavaTargetRoute {
    if evidence.explicit_profile {
        return JavaTargetRoute::JavaSpecific;
    }
    let joined = [
        evidence.app_name.as_deref(),
        evidence.process_name.as_deref(),
        evidence.executable_path.as_deref(),
        evidence.accessibility_class.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_ascii_lowercase();

    if joined.contains("word")
        || joined.contains("excel")
        || joined.contains("powerpoint")
        || joined.contains("winword")
        || joined.contains("microsoft office")
    {
        JavaTargetRoute::NativeOsAccessibility
    } else if joined.contains("java")
        || joined.contains("javax.swing")
        || joined.contains("javafx")
        || joined.contains(".jar")
    {
        JavaTargetRoute::JavaSpecific
    } else {
        JavaTargetRoute::AskUser
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

    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        if let Ok(command) = std::env::var("GREENTIC_JAVA_EVENT_SOURCE_COMMAND") {
            // Local operator supplied recorder path is invoked directly without a shell.
            // foxguard: ignore[rs/no-command-injection]
            let spawn = Command::new(command)
                .arg(sink.session_id())
                .arg(&request.out)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if spawn.is_ok() {
                let _ = sink.update_heartbeat();
                return RecordingHandle {
                    backend_id: JAVA_RECORDER_BACKEND_ID.to_owned(),
                    capture_state: RecordingCaptureState::Recording,
                };
            }
        }
        let _ = sink.append_backend_warning(
            "Java recording requires a real Java Access Bridge event source command; synthetic events are disabled.",
        );

        RecordingHandle {
            backend_id: JAVA_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Blocked,
        }
    }
}

fn java_event_source_configured() -> bool {
    std::env::var("GREENTIC_JAVA_EVENT_SOURCE_COMMAND")
        .map(|value| !value.trim().is_empty())
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

    fn require_access_bridge(&self) -> AdapterResult<()> {
        if self
            .state
            .lock()
            .expect("java adapter mutex poisoned")
            .access_bridge_enabled
        {
            Ok(())
        } else {
            Err(AdapterError::ExecutionFailed(
                "Java Access Bridge is disabled; Java automation cannot execute.".to_owned(),
            ))
        }
    }
}

impl DesktopAdapter for JavaDesktopAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        if self
            .state
            .lock()
            .expect("java adapter mutex poisoned")
            .access_bridge_enabled
        {
            java_capabilities()
        } else {
            AdapterCapabilities::new(JAVA_ADAPTER_ID, env!("CARGO_PKG_VERSION"), [] as [&str; 0])
        }
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        self.require_access_bridge()?;
        let visible_text = java_sidecar("observe", None)?;
        Ok(Observation {
            adapter_id: JAVA_ADAPTER_ID.to_owned(),
            summary: format!("java session {} access_bridge={}", ctx.session_id, true),
            visible_text: visible_text.lines().map(str::to_owned).collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        self.require_access_bridge()?;
        let message = java_sidecar("execute", Some(&java_step_json(&step)))?;

        self.state
            .lock()
            .expect("java adapter mutex poisoned")
            .recorded
            .push(RecordedEvent {
                action: step.action.clone(),
                target: step.target,
                value: step.value,
            });

        Ok(StepResult {
            step_id: step.id,
            success: true,
            message,
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        self.require_access_bridge()?;
        let observed = java_sidecar("observe", None)?;
        let passed = match assertion.required_capability.as_str() {
            "java.assert_visible" | "java.assert_text" | "java.find_window" => {
                observed.contains(&assertion.expected)
            }
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

fn java_sidecar(action: &str, payload: Option<&str>) -> AdapterResult<String> {
    let command = std::env::var("GREENTIC_JAVA_ACCESS_BRIDGE_COMMAND").map_err(|_| {
        AdapterError::ExecutionFailed(
            "Java Access Bridge sidecar command is not configured in GREENTIC_JAVA_ACCESS_BRIDGE_COMMAND.".to_owned(),
        )
    })?;
    // Local operator supplied Java Access Bridge sidecar is invoked directly without a shell.
    // foxguard: ignore[rs/no-command-injection]
    let mut command = Command::new(command);
    command.arg(action);
    if let Some(payload) = payload {
        command.arg(payload);
    }
    let output = command.stdin(Stdio::null()).output().map_err(|err| {
        AdapterError::ExecutionFailed(format!("failed to run Java Access Bridge sidecar: {err}"))
    })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "Java Access Bridge sidecar failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn java_step_json(step: &RunnerStep) -> String {
    format!(
        r#"{{"id":"{}","action":"{}","required_capability":"{}","target":"{}","value":{}}}"#,
        escape_json(&step.id),
        escape_json(&step.action),
        escape_json(&step.required_capability),
        escape_json(&target_key(&step.target)),
        json_option(step.value.as_deref())
    )
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
    fn routing_keeps_native_office_apps_out_of_java() {
        let route = classify_java_target(&JavaTargetEvidence {
            app_name: Some("Microsoft Word".to_owned()),
            process_name: Some("WINWORD.EXE".to_owned()),
            executable_path: None,
            accessibility_class: None,
            explicit_profile: false,
        });

        assert_eq!(route, JavaTargetRoute::NativeOsAccessibility);
    }

    #[test]
    fn routing_uses_java_only_for_explicit_or_java_metadata() {
        let explicit = classify_java_target(&JavaTargetEvidence {
            app_name: Some("Billing".to_owned()),
            process_name: None,
            executable_path: None,
            accessibility_class: None,
            explicit_profile: true,
        });
        let detected = classify_java_target(&JavaTargetEvidence {
            app_name: Some("Billing".to_owned()),
            process_name: Some("java".to_owned()),
            executable_path: Some("/apps/billing.jar".to_owned()),
            accessibility_class: Some("javax.swing.JPanel".to_owned()),
            explicit_profile: false,
        });
        let ambiguous = classify_java_target(&JavaTargetEvidence {
            app_name: Some("Billing".to_owned()),
            process_name: None,
            executable_path: None,
            accessibility_class: None,
            explicit_profile: false,
        });

        assert_eq!(explicit, JavaTargetRoute::JavaSpecific);
        assert_eq!(detected, JavaTargetRoute::JavaSpecific);
        assert_eq!(ambiguous, JavaTargetRoute::AskUser);
    }

    #[test]
    fn access_bridge_replay_requires_real_sidecar_command() {
        let adapter = JavaDesktopAdapter::new(true);
        let target = stable_java_target(&metadata());
        let step = RunnerStep {
            id: "type".to_owned(),
            action: "type_text".to_owned(),
            target: target.clone(),
            value: Some("Acme".to_owned()),
            required_capability: "java.type_text".to_owned(),
        };
        let payload = java_step_json(&step);

        assert!(payload.contains("java.type_text"), "{payload}");
        assert!(payload.contains("customername"), "{payload}");

        let error = adapter
            .execute(step)
            .expect_err("missing sidecar should fail closed");
        assert!(
            error
                .to_string()
                .contains("GREENTIC_JAVA_ACCESS_BRIDGE_COMMAND"),
            "{error}"
        );
    }

    #[test]
    fn generic_app_workflow_fails_without_real_java_fixture() {
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
        .expect_err("missing real Java sidecar/fixture should fail");

        assert!(
            outcome
                .to_string()
                .contains("GREENTIC_JAVA_ACCESS_BRIDGE_COMMAND"),
            "{outcome}"
        );
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
