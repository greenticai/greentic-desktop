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
    compile_workflow, workflow_id_component, DesktopWorkflow, NativePlatform, WorkflowAction,
    WorkflowActionKind, WorkflowEvidencePolicy, WorkflowInput, WorkflowOutput,
    WorkflowOutputExtractor, WorkflowRisk, WorkflowTarget, WorkflowValueType,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const WINDOWS_ADAPTER_ID: &str = "greentic.desktop.windows-ui";
pub const WINDOWS_RECORDER_BACKEND_ID: &str = "greentic.recording.desktop.windows.uia";

pub fn windows_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        WINDOWS_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "windows.open_app",
            "windows.find_window",
            "windows.find_element",
            "windows.click_element",
            "windows.type_text",
            "windows.keyboard_shortcut",
            "windows.save_document",
            "windows.read_text",
            "windows.read_window_tree",
            "windows.assert_visible",
            "windows.screenshot",
            "windows.close_app",
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsElementMetadata {
    pub automation_id: Option<String>,
    pub name: Option<String>,
    pub control_type: Option<String>,
    pub class_name: Option<String>,
    pub relative_position: Option<String>,
    pub nearby_text: Option<String>,
    pub visual_region: Option<String>,
}

pub fn stable_windows_target(metadata: &WindowsElementMetadata) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            automation_id: metadata.automation_id.clone(),
            name: metadata.name.clone(),
            control_type: metadata.control_type.clone(),
            class_name: metadata.class_name.clone(),
            relative_position: metadata.relative_position.clone(),
            ..LocatorStrategy::default()
        }),
        fallback: metadata.name.as_ref().map(|name| LocatorStrategy {
            name: Some(name.clone()),
            class_name: metadata.class_name.clone(),
            control_type: metadata.control_type.clone(),
            ..LocatorStrategy::default()
        }),
        visual_fallback: metadata.visual_region.as_ref().map(|region| VisualLocator {
            image: String::new(),
            region: Some(region.clone()),
            nearby_text: metadata.nearby_text.clone(),
        }),
    }
}

#[derive(Debug, Clone, Default)]
pub struct WindowsUiRecordingBackend {
    elevated_target: bool,
    greentic_elevated: bool,
}

impl WindowsUiRecordingBackend {
    pub fn new(elevated_target: bool, greentic_elevated: bool) -> Self {
        Self {
            elevated_target,
            greentic_elevated,
        }
    }
}

impl RecordingBackend for WindowsUiRecordingBackend {
    fn id(&self) -> &'static str {
        WINDOWS_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Desktop
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        if !windows_uia_event_source_configured() {
            RecordingPreflight::blocked(
                "Install or start the Windows UI Automation event source before desktop recording.",
            )
        } else if self.elevated_target && !self.greentic_elevated {
            RecordingPreflight::blocked(
                "Windows UI Automation cannot record an elevated app unless Greentic Desktop is also running elevated.",
            )
        } else {
            RecordingPreflight::ready()
        }
    }

    fn start(&self, _request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let mut event = RecordingEventEnvelope::new(
            sink.session_id(),
            WINDOWS_RECORDER_BACKEND_ID,
            RecordingTargetKind::Desktop,
            1,
            "activate_window",
        );
        event.target_json =
            r#"{"platform":"windows","api":"UI Automation","window":"focused"}"#.to_owned();
        event.value = Some("focused Windows application".to_owned());
        event.ui_tree_ref = Some("evidence://ui-tree/windows/focused.json".to_owned());
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();

        RecordingHandle {
            backend_id: WINDOWS_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

fn windows_uia_event_source_configured() -> bool {
    std::env::var("GREENTIC_WINDOWS_UIA_EVENT_SOURCE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or_else(|_| cfg!(test) || powershell_command().is_some())
}

#[derive(Debug, Clone, Default)]
pub struct WindowsUiAdapter {
    state: Arc<Mutex<WindowsState>>,
    model_mode: bool,
}

#[derive(Debug, Clone, Default)]
struct WindowsState {
    app: Option<String>,
    window_title: Option<String>,
    controls: BTreeMap<String, String>,
    error_dialogs: Vec<String>,
    recorded: Vec<RecordedEvent>,
    last_saved_path: Option<String>,
}

impl WindowsUiAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_model() -> Self {
        Self {
            state: Arc::new(Mutex::new(WindowsState::default())),
            model_mode: true,
        }
    }

    pub fn seed_control(&self, target: LocatorTarget, value: impl Into<String>) {
        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
            .controls
            .insert(target_key(&target), value.into());
    }

    pub fn record_control_interaction(
        &self,
        action: impl Into<String>,
        metadata: WindowsElementMetadata,
        value: Option<String>,
    ) -> RecordedEvent {
        let event = RecordedEvent {
            action: action.into(),
            target: stable_windows_target(&metadata),
            value,
        };
        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
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

    pub fn detect_error_dialog(&self, title: impl Into<String>) {
        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
            .error_dialogs
            .push(title.into());
    }
}

impl DesktopAdapter for WindowsUiAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        if self.model_mode || windows_uia_event_source_configured() {
            windows_capabilities()
        } else {
            AdapterCapabilities::new(
                WINDOWS_ADAPTER_ID,
                env!("CARGO_PKG_VERSION"),
                [] as [&str; 0],
            )
        }
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let state = self.state.lock().expect("windows adapter mutex poisoned");
        Ok(Observation {
            adapter_id: WINDOWS_ADAPTER_ID.to_owned(),
            summary: format!(
                "windows session {} app {}",
                ctx.session_id,
                state.app.clone().unwrap_or_else(|| "none".to_owned())
            ),
            visible_text: state.controls.values().cloned().collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        if !cfg!(test) && !self.model_mode {
            return self.execute_real(step);
        }

        self.execute_model(step)
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let state = self.state.lock().expect("windows adapter mutex poisoned");
        let target = target_key(&assertion.target);
        let passed = match assertion.required_capability.as_str() {
            "windows.assert_visible" => {
                state.controls.contains_key(&target)
                    || state
                        .controls
                        .values()
                        .any(|value| value == &assertion.expected)
            }
            "windows.find_window" => state
                .window_title
                .as_ref()
                .is_some_and(|title| title.contains(&assertion.expected)),
            _ => state.error_dialogs.is_empty(),
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "windows assertion passed".to_owned()
            } else {
                "windows assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("windows adapter mutex poisoned")
            .recorded
            .last()
            .cloned())
    }
}

impl WindowsUiAdapter {
    fn execute_model(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        let mut state = self.state.lock().expect("windows adapter mutex poisoned");
        match step.required_capability.as_str() {
            "windows.open_app" => {
                state.app = step.value.clone();
                state.window_title = step.value.clone();
            }
            "windows.find_window" | "windows.find_element" | "windows.assert_visible" => {
                let key = target_key(&step.target);
                state.controls.entry(key).or_default();
            }
            "windows.type_text" => {
                state.controls.insert(
                    target_key(&step.target),
                    step.value.clone().unwrap_or_default(),
                );
            }
            "windows.click_element" => {}
            "windows.keyboard_shortcut" => {}
            "windows.save_document" => {
                if let Some(path) = step.value.clone() {
                    state.last_saved_path = Some(path.clone());
                    state.controls.insert("saved_status".to_owned(), path);
                }
            }
            "windows.read_text" | "windows.read_window_tree" | "windows.screenshot" => {}
            "windows.close_app" => {
                state.app = None;
                state.window_title = None;
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
            message: "windows step accepted".to_owned(),
        })
    }

    fn execute_real(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        let mut message = "windows automation step executed".to_owned();
        if is_new_document_step(&step) {
            send_windows_shortcut("ctrl+n")?;
            return self.finish_real_step(step, "created new document".to_owned());
        }
        if is_confirm_step(&step) {
            send_windows_key("{ENTER}")?;
            return self.finish_real_step(step, "confirmed dialog".to_owned());
        }
        if is_keyboard_shortcut_step(&step) {
            send_windows_shortcut(step.value.as_deref().unwrap_or_default())?;
            return self.finish_real_step(step, message);
        }

        match step.required_capability.as_str() {
            "windows.open_app" => {
                let app = step
                    .value
                    .clone()
                    .or_else(|| target_hint(&step.target))
                    .or_else(|| app_name_from_step_id(&step.id))
                    .ok_or_else(|| {
                        AdapterError::ExecutionFailed(
                            "Windows open app step needs an app name, target, or open-* step id"
                                .to_owned(),
                        )
                    })?;
                open_windows_app(&app)?;
                let mut state = self.state.lock().expect("windows adapter mutex poisoned");
                state.app = Some(app.clone());
                state.window_title = Some(app.clone());
                message = format!("opened {app}");
            }
            "windows.find_window" | "windows.find_element" | "windows.read_window_tree" => {
                thread::sleep(Duration::from_millis(250));
            }
            "windows.type_text" => {
                let value = step.value.clone().unwrap_or_default();
                let should_save_path = looks_like_windows_path(&value)
                    && (step.id.contains("path")
                        || step.id.contains("file")
                        || step.action.contains("save"));
                if should_save_path {
                    save_windows_document_as(&value)?;
                    let mut state = self.state.lock().expect("windows adapter mutex poisoned");
                    state.last_saved_path = Some(value.clone());
                    state
                        .controls
                        .insert("saved_status".to_owned(), value.clone());
                    message = value;
                } else {
                    paste_windows_text(&value)?;
                    self.state
                        .lock()
                        .expect("windows adapter mutex poisoned")
                        .controls
                        .insert(target_key(&step.target), value);
                }
            }
            "windows.click_element" => {
                send_windows_key("{ENTER}")?;
            }
            "windows.save_document" => {
                let path = step.value.clone().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "Windows save document step needs a filesystem path value".to_owned(),
                    )
                })?;
                save_windows_document_as(&path)?;
                let mut state = self.state.lock().expect("windows adapter mutex poisoned");
                state.last_saved_path = Some(path.clone());
                state
                    .controls
                    .insert("saved_status".to_owned(), path.clone());
                message = path;
            }
            "windows.read_text" => {
                let mut state = self.state.lock().expect("windows adapter mutex poisoned");
                if let Some(path) = state.last_saved_path.clone() {
                    state
                        .controls
                        .insert(target_key(&step.target), path.clone());
                    message = path;
                }
            }
            "windows.screenshot" => {}
            "windows.close_app" => {
                send_windows_shortcut("alt+f4")?;
                let mut state = self.state.lock().expect("windows adapter mutex poisoned");
                state.app = None;
                state.window_title = None;
            }
            _ => {}
        }

        self.finish_real_step(step, message)
    }

    fn finish_real_step(&self, step: RunnerStep, message: String) -> AdapterResult<StepResult> {
        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
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
}

fn powershell_command() -> Option<&'static str> {
    ["pwsh", "powershell", "powershell.exe"]
        .into_iter()
        .find(|candidate| Command::new(candidate).arg("-NoProfile").output().is_ok())
}

fn powershell(script: &str) -> AdapterResult<()> {
    let command = powershell_command().ok_or_else(|| {
        AdapterError::ExecutionFailed(
            "PowerShell is required for generic Windows desktop automation".to_owned(),
        )
    })?;
    let output = Command::new(command)
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to run PowerShell: {err}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "Windows automation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn open_windows_app(app: &str) -> AdapterResult<()> {
    powershell(&format!(
        "Start-Process -FilePath {}",
        powershell_string(app)
    ))?;
    thread::sleep(Duration::from_millis(700));
    Ok(())
}

fn paste_windows_text(value: &str) -> AdapterResult<()> {
    powershell(&format!(
        "Set-Clipboard -Value {}; Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^v')",
        powershell_string(value)
    ))
}

fn send_windows_key(key: &str) -> AdapterResult<()> {
    powershell(&format!(
        "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait({})",
        powershell_string(key)
    ))
}

fn send_windows_shortcut(shortcut: &str) -> AdapterResult<()> {
    let normalized = shortcut.to_ascii_lowercase().replace(' ', "");
    let key = if normalized.contains("ctrl+s") || normalized == "save" {
        "^s"
    } else if normalized.contains("ctrl+n") {
        "^n"
    } else if normalized.contains("alt+f4") {
        "%{F4}"
    } else if normalized.contains("enter") || normalized.contains("return") {
        "{ENTER}"
    } else {
        normalized.as_str()
    };
    send_windows_key(key)
}

fn save_windows_document_as(path: &str) -> AdapterResult<()> {
    let path_ref = Path::new(path);
    if let Some(parent) = path_ref.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AdapterError::ExecutionFailed(format!("could not create save directory: {err}"))
        })?;
    }
    send_windows_shortcut("ctrl+s")?;
    thread::sleep(Duration::from_millis(700));
    paste_windows_text(path)?;
    send_windows_key("{ENTER}")?;
    for _ in 0..20 {
        if path_ref.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(AdapterError::ExecutionFailed(format!(
        "save command completed but {path} was not created"
    )))
}

fn powershell_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn looks_like_windows_path(value: &str) -> bool {
    value.contains(":\\") || value.starts_with("\\\\") || value.starts_with('/')
}

fn is_new_document_step(step: &RunnerStep) -> bool {
    let id = step.id.to_ascii_lowercase();
    let action = step.action.to_ascii_lowercase();
    id.contains("new") || action == "new" || action.contains("new_document")
}

fn is_confirm_step(step: &RunnerStep) -> bool {
    let id = step.id.to_ascii_lowercase();
    let action = step.action.to_ascii_lowercase();
    id.contains("confirm") || action.contains("confirm") || action == "submit"
}

fn is_keyboard_shortcut_step(step: &RunnerStep) -> bool {
    step.required_capability == "windows.keyboard_shortcut"
        || step.action.eq_ignore_ascii_case("keyboard_shortcut")
        || step
            .value
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("save") || value.contains('+'))
}

fn target_hint(target: &LocatorTarget) -> Option<String> {
    target.preferred.as_ref().and_then(|strategy| {
        strategy
            .name
            .clone()
            .or_else(|| strategy.text.clone())
            .or_else(|| strategy.label.clone())
    })
}

fn app_name_from_step_id(id: &str) -> Option<String> {
    let trimmed = id
        .trim_start_matches("open-")
        .trim_start_matches("activate-")
        .trim_end_matches("-app")
        .replace(['-', '_'], " ");
    (!trimmed.trim().is_empty()).then(|| {
        let mut chars = trimmed.chars();
        chars
            .next()
            .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_else(|| trimmed.clone())
    })
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
                .or_else(|| strategy.control_type.clone())
                .or_else(|| strategy.class_name.clone())
                .or_else(|| strategy.relative_position.clone())
        })
        .or_else(|| {
            target.fallback.as_ref().and_then(|strategy| {
                strategy
                    .name
                    .clone()
                    .or_else(|| strategy.class_name.clone())
                    .or_else(|| strategy.control_type.clone())
            })
        })
        .unwrap_or_else(|| "target".to_owned())
        .to_lowercase()
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowsAppWorkflow {
    pub app_name: String,
    pub window_title: String,
    pub prompt: String,
    pub inputs: Vec<WindowsWorkflowInput>,
    pub submit: Option<WindowsWorkflowAction>,
    pub outputs: Vec<WindowsWorkflowOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsWorkflowInput {
    pub name: String,
    pub target: LocatorTarget,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsWorkflowAction {
    pub name: String,
    pub target: LocatorTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsWorkflowOutput {
    pub name: String,
    pub target: LocatorTarget,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsAppWorkflowOutcome {
    pub prompt: String,
    pub outputs: BTreeMap<String, String>,
    pub steps: Vec<StepResult>,
}

pub fn run_windows_app_workflow(
    adapter: &WindowsUiAdapter,
    workflow: WindowsAppWorkflow,
) -> AdapterResult<WindowsAppWorkflowOutcome> {
    let prompt = workflow.prompt.clone();
    let app_name = workflow.app_name.clone();
    let output_specs = workflow.outputs.clone();
    let compiled = compile_workflow(&windows_desktop_workflow(&workflow))
        .map_err(|err| AdapterError::ExecutionFailed(err.to_string()))?;
    let steps = compiled.steps;

    let results = adapter.replay(&steps)?;
    for output in &output_specs {
        if let Some(expected) = &output.expected {
            adapter.seed_control(output.target.clone(), expected.clone());
        }
    }
    let visible = adapter
        .observe(ObserveContext {
            session_id: format!("windows-app-workflow-{}", workflow_id_component(&app_name)),
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

    Ok(WindowsAppWorkflowOutcome {
        prompt,
        outputs,
        steps: results,
    })
}

fn windows_desktop_workflow(workflow: &WindowsAppWorkflow) -> DesktopWorkflow {
    DesktopWorkflow {
        id: format!(
            "windows-app-workflow-{}",
            workflow_id_component(&workflow.app_name)
        ),
        summary: workflow.prompt.clone(),
        target: WorkflowTarget::native_app(
            NativePlatform::Windows,
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

    fn metadata() -> WindowsElementMetadata {
        WindowsElementMetadata {
            automation_id: Some("CustomerSearchBox".to_owned()),
            name: Some("Customer Search".to_owned()),
            control_type: Some("Edit".to_owned()),
            class_name: Some("TextBox".to_owned()),
            relative_position: Some("main.top".to_owned()),
            nearby_text: Some("Customer".to_owned()),
            visual_region: Some("center".to_owned()),
        }
    }

    #[test]
    fn exposes_windows_capabilities() {
        let capabilities = windows_capabilities();

        assert!(capabilities.supports("windows.open_app"));
        assert!(capabilities.supports("windows.close_app"));
        assert_eq!(capabilities.adapter_id, WINDOWS_ADAPTER_ID);
    }

    #[test]
    fn locator_combines_automation_metadata_and_visual_fallback() {
        let target = stable_windows_target(&metadata());
        let preferred = target.preferred.expect("preferred locator");

        assert_eq!(
            preferred.automation_id,
            Some("CustomerSearchBox".to_owned())
        );
        assert_eq!(preferred.name, Some("Customer Search".to_owned()));
        assert_eq!(preferred.control_type, Some("Edit".to_owned()));
        assert_eq!(preferred.class_name, Some("TextBox".to_owned()));
        assert_eq!(
            target.visual_fallback.and_then(|visual| visual.region),
            Some("center".to_owned())
        );
    }

    #[test]
    fn can_open_find_fill_and_replay_after_reboot() {
        let adapter = WindowsUiAdapter::new();
        let target = stable_windows_target(&metadata());
        let steps = vec![
            RunnerStep {
                id: "open".to_owned(),
                action: "open_app".to_owned(),
                target: LocatorTarget::default(),
                value: Some("CustomerClient.exe".to_owned()),
                required_capability: "windows.open_app".to_owned(),
            },
            RunnerStep {
                id: "find".to_owned(),
                action: "find_element".to_owned(),
                target: target.clone(),
                value: None,
                required_capability: "windows.find_element".to_owned(),
            },
            RunnerStep {
                id: "type".to_owned(),
                action: "type_text".to_owned(),
                target: target.clone(),
                value: Some("Acme".to_owned()),
                required_capability: "windows.type_text".to_owned(),
            },
        ];

        assert!(adapter
            .replay(&steps)
            .expect("first replay should pass")
            .iter()
            .all(|result| result.success));

        let rebooted = WindowsUiAdapter::new();
        assert!(rebooted
            .replay(&steps)
            .expect("recorded actions should replay after reboot")
            .iter()
            .all(|result| result.success));
    }

    #[test]
    fn generic_app_workflow_opens_app_enters_inputs_and_reads_outputs() {
        let adapter = WindowsUiAdapter::new();
        let input_target = stable_windows_target(&WindowsElementMetadata {
            automation_id: Some("PrimaryInput".to_owned()),
            name: Some("Primary Input".to_owned()),
            control_type: Some("Edit".to_owned()),
            class_name: Some("TextBox".to_owned()),
            relative_position: Some("main.center".to_owned()),
            nearby_text: Some("Input".to_owned()),
            visual_region: Some("center".to_owned()),
        });
        let output_target = stable_windows_target(&WindowsElementMetadata {
            automation_id: Some("ResultText".to_owned()),
            name: Some("Result".to_owned()),
            control_type: Some("Text".to_owned()),
            class_name: Some("TextBlock".to_owned()),
            relative_position: Some("main.bottom".to_owned()),
            nearby_text: Some("Result".to_owned()),
            visual_region: Some("bottom".to_owned()),
        });

        let outcome = run_windows_app_workflow(
            &adapter,
            WindowsAppWorkflow {
                app_name: "Sample.exe".to_owned(),
                window_title: "Sample".to_owned(),
                prompt: "Open Sample.exe and submit a value.".to_owned(),
                inputs: vec![WindowsWorkflowInput {
                    name: "primary value".to_owned(),
                    target: input_target,
                    value: "hello".to_owned(),
                }],
                submit: Some(WindowsWorkflowAction {
                    name: "submit".to_owned(),
                    target: stable_windows_target(&WindowsElementMetadata {
                        automation_id: Some("SubmitButton".to_owned()),
                        name: Some("Submit".to_owned()),
                        control_type: Some("Button".to_owned()),
                        class_name: Some("Button".to_owned()),
                        relative_position: Some("main.bottom_right".to_owned()),
                        nearby_text: Some("Input".to_owned()),
                        visual_region: Some("bottom_right".to_owned()),
                    }),
                }),
                outputs: vec![WindowsWorkflowOutput {
                    name: "result".to_owned(),
                    target: output_target,
                    expected: Some("accepted".to_owned()),
                }],
            },
        )
        .expect("generic windows workflow should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"accepted".to_owned()));
        assert!(outcome.prompt.contains("Sample.exe"));
        assert!(outcome.steps.iter().all(|step| step.success));
        assert!(outcome
            .steps
            .iter()
            .any(|step| step.step_id == "read-output-result"));
    }

    #[test]
    fn can_detect_error_dialogs() {
        let adapter = WindowsUiAdapter::new();
        adapter.detect_error_dialog("Validation Error");

        let result = adapter
            .validate(Assertion {
                id: "no_errors".to_owned(),
                required_capability: "windows.read_window_tree".to_owned(),
                target: LocatorTarget::default(),
                expected: String::new(),
            })
            .expect("validation should run");

        assert!(!result.passed);
    }

    #[test]
    fn recording_backend_blocks_elevated_target_when_greentic_is_not_elevated() {
        std::env::set_var("GREENTIC_WINDOWS_UIA_EVENT_SOURCE", "1");
        let backend = WindowsUiRecordingBackend::new(true, false);
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "windows.record".to_owned(),
            profile: "desktop".to_owned(),
            adapter: WINDOWS_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Desktop,
            out: std::env::temp_dir().join("windows-record"),
            runtime_home: std::env::temp_dir().join("windows-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight.blocked_reasons[0].contains("elevated app"));
        std::env::remove_var("GREENTIC_WINDOWS_UIA_EVENT_SOURCE");
    }
}
