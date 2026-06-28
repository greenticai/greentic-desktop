use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep, StepResult,
};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventEnvelope, RecordingEventSink,
    RecordingHandle, RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use greentic_desktop_workflow::{
    compile_workflow, workflow_id_component, DesktopWorkflow, TerminalField, WorkflowAction,
    WorkflowActionKind, WorkflowEvidencePolicy, WorkflowOutput, WorkflowOutputExtractor,
    WorkflowRisk, WorkflowTarget, WorkflowValueType,
};
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub const TERMINAL_ADAPTER_ID: &str = "greentic.desktop.terminal-tn3270";
pub const TERMINAL_RECORDER_BACKEND_ID: &str = "greentic.recording.terminal.owned";

pub fn terminal_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        TERMINAL_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "terminal.connect",
            "terminal.disconnect",
            "terminal.read_screen",
            "terminal.send_keys",
            "terminal.send_text",
            "terminal.type_text",
            "terminal.wait_for_screen",
            "terminal.assert_text",
            "terminal.extract_field",
            "terminal.capture_screen",
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalProtocol {
    Vt100,
    Vt220,
    Tn3270,
    Tn5250,
    Ssh,
    Serial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalProfile {
    pub name: String,
    pub protocol: TerminalProtocol,
    pub host: String,
}

#[derive(Debug, Clone)]
pub struct TerminalRecordingBackend {
    profile: TerminalProfile,
    greentic_owned: bool,
    capture_command: Option<String>,
}

impl TerminalRecordingBackend {
    pub fn new(profile: TerminalProfile, greentic_owned: bool) -> Self {
        Self {
            profile,
            greentic_owned,
            capture_command: None,
        }
    }

    pub fn with_capture_command(
        profile: TerminalProfile,
        greentic_owned: bool,
        capture_command: impl Into<String>,
    ) -> Self {
        Self {
            profile,
            greentic_owned,
            capture_command: Some(capture_command.into()),
        }
    }
}

impl RecordingBackend for TerminalRecordingBackend {
    fn id(&self) -> &'static str {
        TERMINAL_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Terminal
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        if !self.greentic_owned {
            return RecordingPreflight::blocked(
                "Terminal recording only supports Greentic-owned PTY, SSH, or TN3270 sessions. Existing terminal windows are not recorded yet.",
            );
        }
        if self.capture_command.is_none() {
            return RecordingPreflight::blocked(
                "Terminal recording requires a configured Greentic-owned terminal event source command.",
            );
        }
        RecordingPreflight::ready()
    }

    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let mut event = RecordingEventEnvelope::new(
            sink.session_id(),
            TERMINAL_RECORDER_BACKEND_ID,
            RecordingTargetKind::Terminal,
            1,
            "connect",
        );
        event.target_json = format!(
            r#"{{"profile":"{}","protocol":"{}","host":"{}","ownership":"greentic-owned"}}"#,
            escape_json(&self.profile.name),
            terminal_protocol_name(&self.profile.protocol),
            escape_json(&self.profile.host)
        );
        event.value = Some(self.profile.host.clone());
        event.terminal_buffer_ref = Some("evidence://terminal/initial-buffer.txt".to_owned());
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();

        if let Some(command) = self.capture_command.clone() {
            run_terminal_capture_command(command, request, sink.clone());
        }

        RecordingHandle {
            backend_id: TERMINAL_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

fn run_terminal_capture_command(
    command: String,
    request: RecordingStartRequest,
    sink: RecordingEventSink,
) {
    let mut child = shell_command(&command);
    let output = child
        .env("GREENTIC_RECORDING_SESSION_ID", sink.session_id())
        .env("GREENTIC_RECORDING_ROOT", request.out.display().to_string())
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        let _ = sink.append_backend_warning("failed to run terminal capture command");
        return;
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = sink.append_backend_warning(&format!(
            "terminal capture command exited with status {:?}: {}",
            output.status.code(),
            stderr.trim()
        ));
        return;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for (index, line) in stdout.lines().enumerate() {
        let event = terminal_recording_event(
            sink.session_id(),
            index as u64 + 2,
            "read_screen",
            Some(line.to_owned()),
            Some(format!("evidence://terminal/buffer-{}.txt", index + 1)),
        );
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();
    }
}

fn shell_command(command: &str) -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-lc").arg(command);
        cmd
    }
}

pub fn terminal_recording_event(
    session_id: &str,
    sequence: u64,
    kind: &str,
    value: Option<String>,
    screen_ref: Option<String>,
) -> RecordingEventEnvelope {
    let redacted_value = value.map(|value| redact_terminal_value(kind, &value));
    let mut event = RecordingEventEnvelope::new(
        session_id,
        TERMINAL_RECORDER_BACKEND_ID,
        RecordingTargetKind::Terminal,
        sequence,
        kind,
    );
    event.target_json = r#"{"cursor":null,"prompt":null}"#.to_owned();
    event.redaction = if redacted_value.as_deref() == Some("{{secret}}") {
        "redacted".to_owned()
    } else if redacted_value.is_some() {
        "input_candidate".to_owned()
    } else {
        "none".to_owned()
    };
    event.value = redacted_value;
    event.terminal_buffer_ref = screen_ref;
    event
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenField {
    pub row: usize,
    pub col: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalAdapter {
    state: Arc<Mutex<TerminalState>>,
}

#[derive(Debug, Clone, Default)]
struct TerminalState {
    connected: bool,
    profile: Option<TerminalProfile>,
    screen: Vec<String>,
    fields: BTreeMap<String, ScreenField>,
    recorded: Vec<RecordedEvent>,
}

impl TerminalAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connect_profile(&self, profile: TerminalProfile) {
        let mut state = self.state.lock().expect("terminal adapter mutex poisoned");
        state.connected = true;
        state.profile = Some(profile);
    }

    pub fn record_screen_buffer(&self, lines: impl IntoIterator<Item = impl Into<String>>) {
        self.state
            .lock()
            .expect("terminal adapter mutex poisoned")
            .screen = lines.into_iter().map(Into::into).collect();
    }

    pub fn define_field(&self, name: impl Into<String>, field: ScreenField) {
        self.state
            .lock()
            .expect("terminal adapter mutex poisoned")
            .fields
            .insert(name.into(), field);
    }

    pub fn extract_field(&self, field: ScreenField) -> Option<String> {
        let state = self.state.lock().expect("terminal adapter mutex poisoned");
        let line = state.screen.get(field.row)?;
        Some(
            line.chars()
                .skip(field.col)
                .take(field.len)
                .collect::<String>()
                .trim()
                .to_owned(),
        )
    }

    pub fn extract_after_anchor(&self, anchor: &str) -> Option<String> {
        let state = self.state.lock().expect("terminal adapter mutex poisoned");
        state.screen.iter().find_map(|line| {
            let index = line.find(anchor)?;
            Some(line[index + anchor.len()..].trim().to_owned())
        })
    }

    pub fn replay(&self, steps: &[RunnerStep]) -> AdapterResult<Vec<StepResult>> {
        steps
            .iter()
            .cloned()
            .map(|step| self.execute(step))
            .collect()
    }
}

impl DesktopAdapter for TerminalAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        terminal_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let state = self.state.lock().expect("terminal adapter mutex poisoned");
        Ok(Observation {
            adapter_id: TERMINAL_ADAPTER_ID.to_owned(),
            summary: format!(
                "terminal session {} connected={}",
                ctx.session_id, state.connected
            ),
            visible_text: state.screen.clone(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let mut state = self.state.lock().expect("terminal adapter mutex poisoned");
        match step.required_capability.as_str() {
            "terminal.connect" => {
                state.connected = true;
            }
            "terminal.disconnect" => state.connected = false,
            "terminal.type_text" | "terminal.send_text" => {
                let value = step.value.clone().unwrap_or_default();
                if !value.is_empty() {
                    state.screen.push(format!("INPUT: {}", value));
                }
            }
            "terminal.send_keys" => {}
            "terminal.read_screen"
            | "terminal.wait_for_screen"
            | "terminal.assert_text"
            | "terminal.extract_field"
            | "terminal.capture_screen" => {}
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
            message: "terminal step accepted".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let state = self.state.lock().expect("terminal adapter mutex poisoned");
        let passed = match assertion.required_capability.as_str() {
            "terminal.assert_text" | "terminal.wait_for_screen" => state
                .screen
                .iter()
                .any(|line| line.contains(&assertion.expected)),
            "terminal.extract_field" => state.fields.contains_key(&assertion.expected),
            _ => state.connected,
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "terminal assertion passed".to_owned()
            } else {
                "terminal assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("terminal adapter mutex poisoned")
            .recorded
            .last()
            .cloned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalWorkflow {
    pub profile: TerminalProfile,
    pub prompt: String,
    pub initial_screen: Vec<String>,
    pub actions: Vec<TerminalWorkflowAction>,
    pub final_screen: Vec<String>,
    pub text_outputs: Vec<TerminalTextOutput>,
    pub field_outputs: Vec<TerminalFieldOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalWorkflowAction {
    pub name: String,
    pub required_capability: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTextOutput {
    pub name: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFieldOutput {
    pub name: String,
    pub field: ScreenField,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalWorkflowOutcome {
    pub prompt: String,
    pub outputs: BTreeMap<String, String>,
    pub steps: Vec<StepResult>,
}

fn terminal_protocol_name(protocol: &TerminalProtocol) -> &'static str {
    match protocol {
        TerminalProtocol::Vt100 => "vt100",
        TerminalProtocol::Vt220 => "vt220",
        TerminalProtocol::Tn3270 => "tn3270",
        TerminalProtocol::Tn5250 => "tn5250",
        TerminalProtocol::Ssh => "ssh",
        TerminalProtocol::Serial => "serial",
    }
}

fn redact_terminal_value(kind: &str, value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if kind.contains("password")
        || lower.contains("password")
        || lower.contains("token")
        || lower.contains("secret")
    {
        "{{secret}}".to_owned()
    } else {
        value.to_owned()
    }
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

pub fn run_terminal_workflow(
    adapter: &TerminalAdapter,
    workflow: TerminalWorkflow,
) -> AdapterResult<TerminalWorkflowOutcome> {
    let prompt = workflow.prompt.clone();
    let text_outputs = workflow.text_outputs.clone();
    let field_outputs = workflow.field_outputs.clone();
    let compiled = compile_workflow(&terminal_desktop_workflow(&workflow))
        .map_err(|err| AdapterError::ExecutionFailed(err.to_string()))?;

    adapter.connect_profile(workflow.profile);
    if !workflow.initial_screen.is_empty() {
        adapter.record_screen_buffer(workflow.initial_screen);
    }

    let steps = compiled.steps;
    let results = adapter.replay(&steps)?;
    if !workflow.final_screen.is_empty() {
        adapter.record_screen_buffer(workflow.final_screen);
    }

    let visible = adapter
        .observe(ObserveContext {
            session_id: "terminal-workflow".to_owned(),
            target: None,
        })?
        .visible_text;

    let mut outputs = BTreeMap::new();
    for output in text_outputs {
        if !visible.iter().any(|line| line.contains(&output.expected)) {
            return Err(AdapterError::ExecutionFailed(format!(
                "Expected terminal text {} was not visible",
                output.name
            )));
        }
        outputs.insert(output.name, output.expected);
    }

    for output in field_outputs {
        let value = adapter.extract_field(output.field).ok_or_else(|| {
            AdapterError::ExecutionFailed(format!(
                "No terminal field was visible for {}",
                output.name
            ))
        })?;
        if let Some(expected) = &output.expected {
            if &value != expected {
                return Err(AdapterError::ExecutionFailed(format!(
                    "Expected terminal field {} to be {}",
                    output.name, expected
                )));
            }
        }
        outputs.insert(output.name, value);
    }

    Ok(TerminalWorkflowOutcome {
        prompt,
        outputs,
        steps: results,
    })
}

fn terminal_desktop_workflow(workflow: &TerminalWorkflow) -> DesktopWorkflow {
    DesktopWorkflow {
        id: format!(
            "terminal-workflow-{}",
            workflow_id_component(&workflow.profile.name)
        ),
        summary: workflow.prompt.clone(),
        target: WorkflowTarget::terminal(workflow.profile.name.clone()),
        inputs: Vec::new(),
        actions: workflow
            .actions
            .iter()
            .map(|action| WorkflowAction {
                name: action.name.clone(),
                kind: WorkflowActionKind::AdapterCapability(action.required_capability.clone()),
                target: LocatorTarget::default(),
                value_template: action.value.clone(),
                risk: WorkflowRisk::Low,
            })
            .collect(),
        outputs: workflow
            .text_outputs
            .iter()
            .map(|output| WorkflowOutput {
                name: output.name.clone(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::VisibleText(output.expected.clone()),
                required: true,
                expected: Some(output.expected.clone()),
            })
            .chain(workflow.field_outputs.iter().map(|output| WorkflowOutput {
                name: output.name.clone(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::TerminalField(TerminalField {
                    row: output.field.row,
                    col: output.field.col,
                    len: output.field.len,
                }),
                required: true,
                expected: output.expected.clone(),
            }))
            .collect(),
        assertions: Vec::new(),
        evidence_policy: WorkflowEvidencePolicy::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_recorder::{
        start_recording_session_with_registry, RecordingBackendRegistry,
    };
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn exposes_terminal_capabilities() {
        let capabilities = terminal_capabilities();

        assert!(capabilities.supports("terminal.connect"));
        assert!(capabilities.supports("terminal.extract_field"));
        assert_eq!(capabilities.adapter_id, TERMINAL_ADAPTER_ID);
    }

    #[test]
    fn generic_terminal_workflow_sends_actions_and_reads_outputs() {
        let adapter = TerminalAdapter::new();
        let outcome = run_terminal_workflow(
            &adapter,
            TerminalWorkflow {
                profile: TerminalProfile {
                    name: "test".to_owned(),
                    protocol: TerminalProtocol::Tn3270,
                    host: "terminal.test".to_owned(),
                },
                prompt: "Connect to a terminal and complete the supplied workflow.".to_owned(),
                initial_screen: vec!["LOGIN".to_owned()],
                actions: vec![
                    TerminalWorkflowAction {
                        name: "username".to_owned(),
                        required_capability: "terminal.type_text".to_owned(),
                        value: Some("USER1".to_owned()),
                    },
                    TerminalWorkflowAction {
                        name: "enter-login".to_owned(),
                        required_capability: "terminal.send_keys".to_owned(),
                        value: Some("ENTER".to_owned()),
                    },
                    TerminalWorkflowAction {
                        name: "menu".to_owned(),
                        required_capability: "terminal.type_text".to_owned(),
                        value: Some("CUST".to_owned()),
                    },
                ],
                final_screen: vec![
                    "ACCOUNT STATUS: ACTIVE".to_owned(),
                    "BALANCE: 100.00".to_owned(),
                ],
                text_outputs: vec![TerminalTextOutput {
                    name: "status-line".to_owned(),
                    expected: "ACCOUNT STATUS".to_owned(),
                }],
                field_outputs: vec![TerminalFieldOutput {
                    name: "status".to_owned(),
                    field: ScreenField {
                        row: 0,
                        col: 16,
                        len: 6,
                    },
                    expected: Some("ACTIVE".to_owned()),
                }],
            },
        )
        .expect("generic terminal workflow should pass");

        assert_eq!(outcome.outputs.get("status"), Some(&"ACTIVE".to_owned()));
        assert!(outcome.steps.iter().all(|step| step.success));
    }

    #[test]
    fn records_and_extracts_screen_fields() {
        let adapter = TerminalAdapter::new();
        adapter.record_screen_buffer(["ACCOUNT STATUS: ACTIVE", "BALANCE: 100.00"]);
        adapter.define_field(
            "status",
            ScreenField {
                row: 0,
                col: 16,
                len: 6,
            },
        );

        assert_eq!(
            adapter.extract_field(ScreenField {
                row: 0,
                col: 16,
                len: 6,
            }),
            Some("ACTIVE".to_owned())
        );
        assert_eq!(
            adapter.extract_after_anchor("BALANCE:"),
            Some("100.00".to_owned())
        );
    }

    #[test]
    fn recording_backend_blocks_unmanaged_terminal_windows() {
        let backend = TerminalRecordingBackend::new(
            TerminalProfile {
                name: "external".to_owned(),
                protocol: TerminalProtocol::Vt220,
                host: "existing-window".to_owned(),
            },
            false,
        );
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "terminal.record".to_owned(),
            profile: "terminal".to_owned(),
            adapter: TERMINAL_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Terminal,
            out: std::env::temp_dir().join("terminal-record"),
            runtime_home: std::env::temp_dir().join("terminal-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight.blocked_reasons[0].contains("Greentic-owned"));
    }

    #[test]
    fn recording_backend_captures_real_terminal_command_output() {
        let root = temp_dir("greentic-terminal-real-recording");
        let runtime_home = temp_dir("greentic-terminal-real-home");
        let mut registry = RecordingBackendRegistry::new();
        registry.register(TerminalRecordingBackend::with_capture_command(
            TerminalProfile {
                name: "local-shell".to_owned(),
                protocol: TerminalProtocol::Vt220,
                host: "localhost".to_owned(),
            },
            true,
            "printf 'ACCOUNT STATUS: ACTIVE\\nBALANCE: 100.00\\n'",
        ));

        let manifest = start_recording_session_with_registry(
            RecordingStartRequest {
                name: "terminal.real".to_owned(),
                profile: "terminal".to_owned(),
                adapter: TERMINAL_ADAPTER_ID.to_owned(),
                target_kind: RecordingTargetKind::Terminal,
                out: root.clone(),
                runtime_home,
                redact: Vec::new(),
                secret_fields: Vec::new(),
            },
            &registry,
        )
        .expect("terminal recording should start");

        assert_eq!(manifest.capture_state, RecordingCaptureState::Recording);
        let raw_path = root.join("raw/events.jsonl");
        let mut raw = String::new();
        for _ in 0..20 {
            raw = fs::read_to_string(&raw_path).unwrap_or_default();
            if raw.contains("ACCOUNT STATUS: ACTIVE") && raw.contains(r#""kind":"read_screen""#) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        assert!(raw.contains(r#""kind":"connect""#), "{raw}");
        assert!(raw.contains(r#""kind":"read_screen""#), "{raw}");
        assert!(raw.contains("ACCOUNT STATUS: ACTIVE"), "{raw}");
    }

    #[test]
    fn terminal_recording_event_redacts_password_input() {
        let event = terminal_recording_event(
            "rec_terminal",
            2,
            "password_input",
            Some("swordfish".to_owned()),
            Some("evidence://terminal/buffer.txt".to_owned()),
        );

        let json = event.render_json();
        assert!(json.contains("\"target_kind\":\"terminal\""));
        assert!(json.contains("\"redaction\":\"redacted\""));
        assert!(json.contains("terminal/buffer.txt"));
        assert!(!json.contains("swordfish"));
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("old temp dir should remove");
        }
        root
    }
}
