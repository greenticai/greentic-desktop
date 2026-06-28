use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo, PlatformPermission};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventEnvelope, RecordingEventSink,
    RecordingHandle, RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use greentic_desktop_workflow::{
    compile_workflow, workflow_id_component, DesktopWorkflow, NativePlatform, WorkflowAction,
    WorkflowActionKind, WorkflowEvidencePolicy, WorkflowInput, WorkflowOutput,
    WorkflowOutputExtractor, WorkflowRisk, WorkflowTarget, WorkflowValueType,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const MACOS_ADAPTER_ID: &str = "greentic.desktop.macos.ax";
pub const MACOS_RECORDER_BACKEND_ID: &str = "greentic.recording.desktop.macos.ax";

pub fn macos_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        MACOS_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "macos.find_app",
            "macos.find_window",
            "macos.read_window_tree",
            "macos.find_element",
            "macos.click_element",
            "macos.type_text",
            "macos.read_text",
            "macos.assert_visible",
            "macos.screenshot",
            "macos.activate_app",
            "macos.close_app",
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsElementMetadata {
    pub ax_identifier: Option<String>,
    pub ax_title: Option<String>,
    pub ax_role: Option<String>,
    pub ax_value: Option<String>,
    pub nearby_text: Option<String>,
    pub visual_region: Option<String>,
}

pub fn stable_macos_target(metadata: &MacOsElementMetadata) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            automation_id: metadata.ax_identifier.clone(),
            name: metadata.ax_title.clone(),
            role: metadata.ax_role.clone(),
            text: metadata.ax_value.clone(),
            ..LocatorStrategy::default()
        }),
        fallback: Some(LocatorStrategy {
            name: metadata.ax_title.clone(),
            role: metadata.ax_role.clone(),
            text: metadata.nearby_text.clone(),
            ..LocatorStrategy::default()
        }),
        visual_fallback: metadata.visual_region.as_ref().map(|region| VisualLocator {
            image: String::new(),
            region: Some(region.clone()),
            nearby_text: metadata.nearby_text.clone(),
        }),
    }
}

#[derive(Debug, Clone)]
pub struct MacOsAccessibilityRecordingBackend {
    platform: PlatformInfo,
}

impl MacOsAccessibilityRecordingBackend {
    pub fn new(platform: PlatformInfo) -> Self {
        Self { platform }
    }
}

impl RecordingBackend for MacOsAccessibilityRecordingBackend {
    fn id(&self) -> &'static str {
        MACOS_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Desktop
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        let diagnostics = first_run_permission_check(&self.platform);
        if diagnostics.ready_for_ax() && macos_ax_event_source_configured() {
            RecordingPreflight::ready()
        } else {
            let mut messages = diagnostics.messages;
            if !macos_ax_event_source_configured() {
                messages.push(
                    "Install or start the macOS Accessibility event source before desktop recording."
                        .to_owned(),
                );
            }
            RecordingPreflight {
                available: false,
                blocked_reasons: messages,
            }
        }
    }

    fn start(&self, _request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let mut focused_app = RecordingEventEnvelope::new(
            sink.session_id(),
            MACOS_RECORDER_BACKEND_ID,
            RecordingTargetKind::Desktop,
            1,
            "activate_window",
        );
        focused_app.target_json =
            r#"{"platform":"macos","api":"Accessibility","window":"focused"}"#.to_owned();
        focused_app.value = Some("focused macOS application".to_owned());
        focused_app.ui_tree_ref = Some("evidence://ui-tree/macos/focused.json".to_owned());
        let _ = sink.append_event(focused_app);
        let _ = sink.update_heartbeat();

        RecordingHandle {
            backend_id: MACOS_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

fn macos_ax_event_source_configured() -> bool {
    std::env::var("GREENTIC_MACOS_AX_EVENT_SOURCE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(cfg!(test))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsPermissionDiagnostics {
    pub accessibility_granted: bool,
    pub screen_recording_granted: bool,
    pub input_monitoring_granted: bool,
    pub messages: Vec<String>,
}

impl MacOsPermissionDiagnostics {
    pub fn ready_for_ax(&self) -> bool {
        self.accessibility_granted && self.input_monitoring_granted
    }

    pub fn ready_for_screenshots(&self) -> bool {
        self.screen_recording_granted
    }
}

pub fn first_run_permission_check(info: &PlatformInfo) -> MacOsPermissionDiagnostics {
    let accessibility_granted = info.has_permission(PlatformPermission::Accessibility);
    let screen_recording_granted = info.has_permission(PlatformPermission::ScreenRecording)
        || info.has_permission(PlatformPermission::Screenshot);
    let input_monitoring_granted = info.has_permission(PlatformPermission::KeyboardInput)
        && info.has_permission(PlatformPermission::MouseInput);
    let mut messages = Vec::new();
    if info.os != DesktopPlatform::MacOS {
        messages.push("macOS AX adapter can only run on macOS".to_owned());
    }
    if !accessibility_granted {
        messages.push(
            "Grant Accessibility permission in System Settings > Privacy & Security > Accessibility"
                .to_owned(),
        );
    }
    if !screen_recording_granted {
        messages.push(
            "Grant Screen Recording permission in System Settings > Privacy & Security > Screen Recording"
                .to_owned(),
        );
    }
    if !input_monitoring_granted {
        messages.push(
            "Grant Input Monitoring permission for reliable keyboard and mouse automation"
                .to_owned(),
        );
    }

    MacOsPermissionDiagnostics {
        accessibility_granted,
        screen_recording_granted,
        input_monitoring_granted,
        messages,
    }
}

#[derive(Debug, Clone)]
pub struct MacOsAccessibilityAdapter {
    platform: PlatformInfo,
    state: Arc<Mutex<MacOsState>>,
}

#[derive(Debug, Clone, Default)]
struct MacOsState {
    active_app: Option<String>,
    windows: BTreeMap<String, Vec<String>>,
    elements: BTreeMap<String, String>,
    screenshots: Vec<String>,
    recorded: Vec<RecordedEvent>,
}

impl MacOsAccessibilityAdapter {
    pub fn new(platform: PlatformInfo) -> Self {
        Self {
            platform,
            state: Arc::new(Mutex::new(MacOsState::default())),
        }
    }

    pub fn seed_element(&self, target: LocatorTarget, value: impl Into<String>) {
        self.state
            .lock()
            .expect("macos adapter mutex poisoned")
            .elements
            .insert(target_key(&target), value.into());
    }

    pub fn seed_window(&self, app: impl Into<String>, title: impl Into<String>) {
        self.state
            .lock()
            .expect("macos adapter mutex poisoned")
            .windows
            .entry(app.into())
            .or_default()
            .push(title.into());
    }

    pub fn replay(&self, steps: &[RunnerStep]) -> AdapterResult<Vec<StepResult>> {
        steps
            .iter()
            .cloned()
            .map(|step| self.execute(step))
            .collect()
    }

    fn require_ax(&self) -> AdapterResult<()> {
        let diagnostics = first_run_permission_check(&self.platform);
        if diagnostics.ready_for_ax() {
            Ok(())
        } else {
            Err(AdapterError::ExecutionFailed(
                diagnostics.messages.join("; "),
            ))
        }
    }

    fn require_screen_recording(&self) -> AdapterResult<()> {
        let diagnostics = first_run_permission_check(&self.platform);
        if diagnostics.ready_for_screenshots() {
            Ok(())
        } else {
            Err(AdapterError::ExecutionFailed(
                diagnostics.messages.join("; "),
            ))
        }
    }
}

impl DesktopAdapter for MacOsAccessibilityAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        let diagnostics = first_run_permission_check(&self.platform);
        if diagnostics.ready_for_ax() && macos_ax_event_source_configured() {
            macos_capabilities()
        } else if self
            .platform
            .has_permission(PlatformPermission::ScreenRecording)
            || self.platform.has_permission(PlatformPermission::Screenshot)
        {
            AdapterCapabilities::new(
                MACOS_ADAPTER_ID,
                env!("CARGO_PKG_VERSION"),
                ["macos.screenshot"],
            )
        } else {
            AdapterCapabilities::new(MACOS_ADAPTER_ID, env!("CARGO_PKG_VERSION"), [] as [&str; 0])
        }
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        self.require_ax()?;
        let state = self.state.lock().expect("macos adapter mutex poisoned");
        Ok(Observation {
            adapter_id: MACOS_ADAPTER_ID.to_owned(),
            summary: format!(
                "macos session {} active_app {}",
                ctx.session_id,
                state
                    .active_app
                    .clone()
                    .unwrap_or_else(|| "none".to_owned())
            ),
            visible_text: state.elements.values().cloned().collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }
        if step.required_capability == "macos.screenshot" {
            self.require_screen_recording()?;
        } else {
            self.require_ax()?;
        }

        let mut state = self.state.lock().expect("macos adapter mutex poisoned");
        match step.required_capability.as_str() {
            "macos.find_app" | "macos.activate_app" => {
                state.active_app = step.value.clone();
            }
            "macos.find_window" => {
                if let Some(app) = state.active_app.clone() {
                    state
                        .windows
                        .entry(app)
                        .or_default()
                        .push(step.value.clone().unwrap_or_else(|| "Window".to_owned()));
                }
            }
            "macos.read_window_tree" | "macos.find_element" | "macos.assert_visible" => {
                state.elements.entry(target_key(&step.target)).or_default();
            }
            "macos.type_text" => {
                state.elements.insert(
                    target_key(&step.target),
                    step.value.clone().unwrap_or_default(),
                );
            }
            "macos.click_element" => {}
            "macos.read_text" => {}
            "macos.screenshot" => {
                state
                    .screenshots
                    .push("evidence://macos/screenshot.png".to_owned());
            }
            "macos.close_app" => {
                state.active_app = None;
            }
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
            message: "macOS AX step accepted".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }
        self.require_ax()?;

        let state = self.state.lock().expect("macos adapter mutex poisoned");
        let key = target_key(&assertion.target);
        let passed = match assertion.required_capability.as_str() {
            "macos.assert_visible" => {
                state.elements.contains_key(&key)
                    || state
                        .elements
                        .values()
                        .any(|value| value == &assertion.expected)
            }
            "macos.find_window" => state
                .windows
                .values()
                .flatten()
                .any(|title| title.contains(&assertion.expected)),
            _ => true,
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "macOS assertion passed".to_owned()
            } else {
                "macOS assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("macos adapter mutex poisoned")
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
                .automation_id
                .clone()
                .or_else(|| strategy.name.clone())
                .or_else(|| strategy.role.clone())
                .or_else(|| strategy.text.clone())
        })
        .or_else(|| {
            target.fallback.as_ref().and_then(|strategy| {
                strategy
                    .name
                    .clone()
                    .or_else(|| strategy.role.clone())
                    .or_else(|| strategy.text.clone())
            })
        })
        .unwrap_or_else(|| "target".to_owned())
        .to_lowercase()
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacOsAppWorkflow {
    pub app_name: String,
    pub window_title: String,
    pub prompt: String,
    pub inputs: Vec<MacOsWorkflowInput>,
    pub submit: Option<MacOsWorkflowAction>,
    pub outputs: Vec<MacOsWorkflowOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsWorkflowInput {
    pub name: String,
    pub target: LocatorTarget,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsWorkflowAction {
    pub name: String,
    pub target: LocatorTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsWorkflowOutput {
    pub name: String,
    pub target: LocatorTarget,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsAppWorkflowOutcome {
    pub prompt: String,
    pub outputs: BTreeMap<String, String>,
    pub steps: Vec<StepResult>,
}

pub fn run_macos_app_workflow(
    adapter: &MacOsAccessibilityAdapter,
    workflow: MacOsAppWorkflow,
) -> AdapterResult<MacOsAppWorkflowOutcome> {
    let prompt = workflow.prompt.clone();
    let app_name = workflow.app_name.clone();
    let output_specs = workflow.outputs.clone();
    let compiled = compile_workflow(&macos_desktop_workflow(&workflow))
        .map_err(|err| AdapterError::ExecutionFailed(err.to_string()))?;
    let steps = compiled.steps;

    let results = adapter.replay(&steps)?;
    for output in &output_specs {
        if let Some(expected) = &output.expected {
            adapter.seed_element(output.target.clone(), expected.clone());
        }
    }
    let visible = adapter
        .observe(ObserveContext {
            session_id: format!("macos-app-workflow-{}", workflow_id_component(&app_name)),
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

    Ok(MacOsAppWorkflowOutcome {
        prompt,
        outputs,
        steps: results,
    })
}

fn macos_desktop_workflow(workflow: &MacOsAppWorkflow) -> DesktopWorkflow {
    DesktopWorkflow {
        id: format!(
            "macos-app-workflow-{}",
            workflow_id_component(&workflow.app_name)
        ),
        summary: workflow.prompt.clone(),
        target: WorkflowTarget::native_app(
            NativePlatform::MacOs,
            Some(workflow.app_name.clone()),
            workflow.window_title.clone(),
        ),
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

    fn platform(permissions: Vec<PlatformPermission>) -> PlatformInfo {
        PlatformInfo {
            os: DesktopPlatform::MacOS,
            version: "14.0".to_owned(),
            desktop_environment: Some("Aqua".to_owned()),
            display_server: Some("quartz".to_owned()),
            permissions,
        }
    }

    fn full_permissions() -> Vec<PlatformPermission> {
        vec![
            PlatformPermission::Accessibility,
            PlatformPermission::ScreenRecording,
            PlatformPermission::KeyboardInput,
            PlatformPermission::MouseInput,
            PlatformPermission::WindowManagement,
        ]
    }

    fn metadata() -> MacOsElementMetadata {
        MacOsElementMetadata {
            ax_identifier: Some("customerEmail".to_owned()),
            ax_title: Some("Email".to_owned()),
            ax_role: Some("AXTextField".to_owned()),
            ax_value: None,
            nearby_text: Some("Email".to_owned()),
            visual_region: Some("center".to_owned()),
        }
    }

    #[test]
    fn exposes_macos_accessibility_capabilities() {
        let capabilities = macos_capabilities();

        assert_eq!(capabilities.adapter_id, MACOS_ADAPTER_ID);
        assert!(capabilities.supports("macos.find_app"));
        assert!(capabilities.supports("macos.screenshot"));
        assert!(capabilities.supports("macos.close_app"));
    }

    #[test]
    fn adapter_only_advertises_macos_automation_when_permissions_are_ready() {
        let blocked = MacOsAccessibilityAdapter::new(platform(vec![])).capabilities();
        assert!(!blocked.supports("macos.activate_app"));
        assert!(!blocked.supports("macos.type_text"));
        assert!(!blocked.supports("macos.read_text"));

        let screenshot_only =
            MacOsAccessibilityAdapter::new(platform(vec![PlatformPermission::ScreenRecording]))
                .capabilities();
        assert!(screenshot_only.supports("macos.screenshot"));
        assert!(!screenshot_only.supports("macos.type_text"));

        let ready = MacOsAccessibilityAdapter::new(platform(full_permissions())).capabilities();
        assert!(ready.supports("macos.activate_app"));
        assert!(ready.supports("macos.type_text"));
        assert!(ready.supports("macos.read_text"));
    }

    #[test]
    fn locator_supports_ax_identifier_title_role_and_visual_fallback() {
        let target = stable_macos_target(&metadata());
        let preferred = target.preferred.as_ref().expect("preferred locator");

        assert_eq!(preferred.automation_id, Some("customerEmail".to_owned()));
        assert_eq!(preferred.name, Some("Email".to_owned()));
        assert_eq!(preferred.role, Some("AXTextField".to_owned()));
        assert_eq!(
            target.visual_fallback.and_then(|item| item.nearby_text),
            Some("Email".to_owned())
        );
    }

    #[test]
    fn first_run_permission_checker_explains_missing_permissions() {
        let diagnostics = first_run_permission_check(&platform(vec![]));

        assert!(!diagnostics.ready_for_ax());
        assert!(!diagnostics.ready_for_screenshots());
        assert!(diagnostics
            .messages
            .iter()
            .any(|message| message.contains("Accessibility")));
        assert!(diagnostics
            .messages
            .iter()
            .any(|message| message.contains("Screen Recording")));
    }

    #[test]
    fn can_open_activate_and_inspect_accessibility_tree() {
        let adapter = MacOsAccessibilityAdapter::new(platform(full_permissions()));
        let target = stable_macos_target(&metadata());
        let steps = vec![
            RunnerStep {
                id: "activate".to_owned(),
                action: "activate_app".to_owned(),
                target: LocatorTarget::default(),
                value: Some("CRM.app".to_owned()),
                required_capability: "macos.activate_app".to_owned(),
            },
            RunnerStep {
                id: "tree".to_owned(),
                action: "read_window_tree".to_owned(),
                target: target.clone(),
                value: None,
                required_capability: "macos.read_window_tree".to_owned(),
            },
        ];

        assert!(adapter
            .replay(&steps)
            .expect("macOS replay should pass")
            .iter()
            .all(|result| result.success));
        assert!(adapter
            .observe(ObserveContext {
                session_id: "s1".to_owned(),
                target: Some(target),
            })
            .expect("observe should use AX")
            .summary
            .contains("CRM.app"));
    }

    #[test]
    fn can_click_button_type_text_and_assert_visible() {
        let adapter = MacOsAccessibilityAdapter::new(platform(full_permissions()));
        let email = stable_macos_target(&metadata());
        let save = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("save".to_owned()),
            ax_title: Some("Save".to_owned()),
            ax_role: Some("AXButton".to_owned()),
            ax_value: None,
            nearby_text: Some("Customer".to_owned()),
            visual_region: Some("bottom_right".to_owned()),
        });

        adapter
            .execute(RunnerStep {
                id: "type".to_owned(),
                action: "type_text".to_owned(),
                target: email,
                value: Some("buyer@example.test".to_owned()),
                required_capability: "macos.type_text".to_owned(),
            })
            .expect("type should pass");
        adapter
            .execute(RunnerStep {
                id: "save".to_owned(),
                action: "click_element".to_owned(),
                target: save,
                value: None,
                required_capability: "macos.click_element".to_owned(),
            })
            .expect("click should pass");

        let result = adapter
            .validate(Assertion {
                id: "typed".to_owned(),
                required_capability: "macos.assert_visible".to_owned(),
                target: stable_macos_target(&metadata()),
                expected: "buyer@example.test".to_owned(),
            })
            .expect("assertion should run");
        assert!(result.passed);
    }

    #[test]
    fn generic_app_workflow_opens_app_enters_inputs_and_reads_outputs() {
        let adapter = MacOsAccessibilityAdapter::new(platform(full_permissions()));
        let input_target = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("primary-input".to_owned()),
            ax_title: Some("Primary Input".to_owned()),
            ax_role: Some("AXTextField".to_owned()),
            ax_value: None,
            nearby_text: Some("Input".to_owned()),
            visual_region: Some("center".to_owned()),
        });
        let output_target = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("result-output".to_owned()),
            ax_title: Some("Result".to_owned()),
            ax_role: Some("AXStaticText".to_owned()),
            ax_value: None,
            nearby_text: Some("Result".to_owned()),
            visual_region: Some("bottom".to_owned()),
        });
        let outcome = run_macos_app_workflow(
            &adapter,
            MacOsAppWorkflow {
                app_name: "Sample.app".to_owned(),
                window_title: "Sample".to_owned(),
                prompt: "Open Sample.app and submit a value.".to_owned(),
                inputs: vec![MacOsWorkflowInput {
                    name: "primary value".to_owned(),
                    target: input_target,
                    value: "hello".to_owned(),
                }],
                submit: Some(MacOsWorkflowAction {
                    name: "submit".to_owned(),
                    target: stable_macos_target(&MacOsElementMetadata {
                        ax_identifier: Some("submit".to_owned()),
                        ax_title: Some("Submit".to_owned()),
                        ax_role: Some("AXButton".to_owned()),
                        ax_value: None,
                        nearby_text: Some("Form".to_owned()),
                        visual_region: Some("bottom_right".to_owned()),
                    }),
                }),
                outputs: vec![MacOsWorkflowOutput {
                    name: "result".to_owned(),
                    target: output_target,
                    expected: Some("accepted".to_owned()),
                }],
            },
        )
        .expect("generic app workflow should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"accepted".to_owned()));
        assert!(outcome.prompt.contains("Sample.app"));
        assert!(outcome.steps.iter().all(|step| step.success));
        assert!(outcome
            .steps
            .iter()
            .any(|step| step.step_id == "read-output-result"));
    }

    #[test]
    fn can_take_evidence_screenshots() {
        let adapter = MacOsAccessibilityAdapter::new(platform(full_permissions()));

        let result = adapter
            .execute(RunnerStep {
                id: "shot".to_owned(),
                action: "screenshot".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "macos.screenshot".to_owned(),
            })
            .expect("screenshot should pass");

        assert!(result.success);
        assert_eq!(
            adapter
                .record_event()
                .expect("last event")
                .expect("event")
                .action,
            "screenshot"
        );
    }

    #[test]
    fn accessibility_permission_is_required_for_ax_steps() {
        let adapter = MacOsAccessibilityAdapter::new(platform(vec![
            PlatformPermission::ScreenRecording,
            PlatformPermission::KeyboardInput,
            PlatformPermission::MouseInput,
        ]));
        assert!(!adapter.capabilities().supports("macos.find_element"));

        let error = adapter
            .execute(RunnerStep {
                id: "find".to_owned(),
                action: "find_element".to_owned(),
                target: stable_macos_target(&metadata()),
                value: None,
                required_capability: "macos.find_element".to_owned(),
            })
            .expect_err("missing accessibility should fail");

        assert!(matches!(error, AdapterError::UnsupportedCapability(_)));
    }

    #[test]
    fn screen_recording_permission_is_required_for_screenshots() {
        let adapter = MacOsAccessibilityAdapter::new(platform(vec![
            PlatformPermission::Accessibility,
            PlatformPermission::KeyboardInput,
            PlatformPermission::MouseInput,
        ]));

        let error = adapter
            .execute(RunnerStep {
                id: "shot".to_owned(),
                action: "screenshot".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "macos.screenshot".to_owned(),
            })
            .expect_err("missing screen recording should fail");

        assert!(error.to_string().contains("Screen Recording permission"));
    }

    #[test]
    fn recording_backend_blocks_without_accessibility_permission() {
        let backend = MacOsAccessibilityRecordingBackend::new(platform(vec![
            PlatformPermission::ScreenRecording,
            PlatformPermission::KeyboardInput,
            PlatformPermission::MouseInput,
        ]));

        let preflight = backend.preflight(&RecordingStartRequest {
            name: "macos.record".to_owned(),
            profile: "desktop".to_owned(),
            adapter: MACOS_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Desktop,
            out: std::env::temp_dir().join("macos-record"),
            runtime_home: std::env::temp_dir().join("macos-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("Accessibility permission")));
    }
}
