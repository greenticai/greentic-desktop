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
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use vte::{Params, Parser, Perform};

pub const TERMINAL_ADAPTER_ID: &str = "greentic.desktop.terminal-tn3270";
pub const TERMINAL_RECORDER_BACKEND_ID: &str = "greentic.recording.terminal.owned";
const TERMINAL_RUNTIME_COMMAND_ENV: &str = "GREENTIC_TERMINAL_ADAPTER_COMMAND";

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
            "terminal.run_command",
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
        if let Some(command) = self.capture_command.clone() {
            run_terminal_capture_command(command, self.profile.clone(), request, sink.clone());
        }

        RecordingHandle {
            backend_id: TERMINAL_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

fn run_terminal_capture_command(
    command: String,
    profile: TerminalProfile,
    request: RecordingStartRequest,
    sink: RecordingEventSink,
) {
    let mut child = shell_command(&command);
    let output = child
        .env("GREENTIC_RECORDING_SESSION_ID", sink.session_id())
        .env("GREENTIC_RECORDING_ROOT", request.out.display().to_string())
        .env("GREENTIC_TERMINAL_PROFILE_NAME", &profile.name)
        .env(
            "GREENTIC_TERMINAL_PROFILE_PROTOCOL",
            terminal_protocol_name(&profile.protocol),
        )
        .env("GREENTIC_TERMINAL_PROFILE_HOST", &profile.host)
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

fn shell_program_and_args(command: &str) -> (&'static str, Vec<&str>) {
    if cfg!(windows) {
        ("cmd", vec!["/C", command])
    } else {
        ("sh", vec!["-lc", command])
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

#[derive(Debug, Clone)]
pub struct TerminalAdapter {
    runtime: Option<TerminalRuntimeConfig>,
}

#[derive(Debug, Clone)]
struct TerminalRuntimeConfig {
    command: String,
    profile: Option<TerminalProfile>,
}

impl TerminalAdapter {
    pub fn new() -> Self {
        let runtime = std::env::var(TERMINAL_RUNTIME_COMMAND_ENV)
            .ok()
            .filter(|command| !command.trim().is_empty())
            .map(|command| TerminalRuntimeConfig {
                command,
                profile: None,
            });
        Self { runtime }
    }

    pub fn with_runtime_command(command: impl Into<String>) -> Self {
        Self {
            runtime: Some(TerminalRuntimeConfig {
                command: command.into(),
                profile: None,
            }),
        }
    }

    pub fn with_profile_runtime_command(
        profile: TerminalProfile,
        command: impl Into<String>,
    ) -> Self {
        Self {
            runtime: Some(TerminalRuntimeConfig {
                command: command.into(),
                profile: Some(profile),
            }),
        }
    }

    pub fn replay(&self, steps: &[RunnerStep]) -> AdapterResult<Vec<StepResult>> {
        steps
            .iter()
            .cloned()
            .map(|step| self.execute(step))
            .collect()
    }
}

impl Default for TerminalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopAdapter for TerminalAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        terminal_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let output = self.run_runtime(TerminalRuntimeAction::Observe, None, None)?;
        Ok(Observation {
            adapter_id: TERMINAL_ADAPTER_ID.to_owned(),
            summary: output.message.unwrap_or_else(|| {
                format!(
                    "terminal session {} observed through owned runtime",
                    ctx.session_id
                )
            }),
            visible_text: output.screen,
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        if step.required_capability == "terminal.run_command" {
            let command = step.value.as_deref().ok_or_else(|| {
                AdapterError::ExecutionFailed(
                    "terminal.run_command requires a shell command in step.value.".to_owned(),
                )
            })?;
            let terminal = if cfg!(windows) {
                run_local_shell_command_with_status(command, Duration::from_secs(30))?
            } else {
                let (program, args) = shell_program_and_args(command);
                run_local_pty_command_with_status(program, &args, Duration::from_secs(30))?
            };
            let output = terminal.lines.join("\n");
            if !terminal.exit_success {
                return Err(AdapterError::ExecutionFailed(format!(
                    "terminal command failed: {output}"
                )));
            }
            let message = terminal_labeled_output(&step)
                .map(|label| format!("{label}: {output}"))
                .unwrap_or(output);
            return Ok(StepResult {
                step_id: step.id,
                success: true,
                message,
            });
        }

        let output = self.run_runtime(TerminalRuntimeAction::Execute, Some(&step), None)?;
        let success = output.passed.unwrap_or(true);

        Ok(StepResult {
            step_id: step.id,
            success,
            message: output.message.unwrap_or_else(|| {
                if success {
                    "terminal step completed by owned runtime".to_owned()
                } else {
                    "terminal runtime reported step failure".to_owned()
                }
            }),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let output = self.run_runtime(TerminalRuntimeAction::Validate, None, Some(&assertion))?;
        let passed = output.passed.unwrap_or_else(|| {
            assertion.expected.is_empty()
                || output
                    .screen
                    .iter()
                    .any(|line| line.contains(&assertion.expected))
        });

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
        Ok(None)
    }
}

fn terminal_labeled_output(step: &RunnerStep) -> Option<String> {
    [
        step.target.preferred.as_ref(),
        step.target.fallback.as_ref(),
    ]
    .into_iter()
    .flatten()
    .find_map(|locator| {
        locator
            .text
            .as_deref()
            .or(locator.name.as_deref())
            .or(locator.label.as_deref())
    })
    .map(|label| {
        label
            .trim_start_matches("outputs.")
            .replace('_', " ")
            .trim()
            .to_owned()
    })
    .filter(|label| !label.is_empty())
}

impl TerminalAdapter {
    fn run_runtime(
        &self,
        action: TerminalRuntimeAction,
        step: Option<&RunnerStep>,
        assertion: Option<&Assertion>,
    ) -> AdapterResult<TerminalCommandOutput> {
        let Some(runtime) = &self.runtime else {
            return Err(AdapterError::ExecutionFailed(format!(
                "No owned terminal runtime is configured. Set {TERMINAL_RUNTIME_COMMAND_ENV} to a PTY, SSH, or TN3270 sidecar command."
            )));
        };
        run_terminal_runtime_command(runtime, action, step, assertion)
    }
}

#[derive(Debug, Clone, Copy)]
enum TerminalRuntimeAction {
    Execute,
    Observe,
    Validate,
}

impl TerminalRuntimeAction {
    fn as_str(self) -> &'static str {
        match self {
            TerminalRuntimeAction::Execute => "execute",
            TerminalRuntimeAction::Observe => "observe",
            TerminalRuntimeAction::Validate => "validate",
        }
    }
}

#[derive(Debug, Default)]
struct TerminalCommandOutput {
    screen: Vec<String>,
    message: Option<String>,
    passed: Option<bool>,
}

fn run_terminal_runtime_command(
    runtime: &TerminalRuntimeConfig,
    action: TerminalRuntimeAction,
    step: Option<&RunnerStep>,
    assertion: Option<&Assertion>,
) -> AdapterResult<TerminalCommandOutput> {
    let mut command = shell_command(&runtime.command);
    command
        .env("GREENTIC_TERMINAL_ACTION", action.as_str())
        .env("GREENTIC_TERMINAL_ADAPTER_ID", TERMINAL_ADAPTER_ID)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(profile) = &runtime.profile {
        command
            .env("GREENTIC_TERMINAL_PROFILE_NAME", &profile.name)
            .env(
                "GREENTIC_TERMINAL_PROFILE_PROTOCOL",
                terminal_protocol_name(&profile.protocol),
            )
            .env("GREENTIC_TERMINAL_PROFILE_HOST", &profile.host);
    }
    if let Some(step) = step {
        command
            .env("GREENTIC_TERMINAL_STEP_ID", &step.id)
            .env("GREENTIC_TERMINAL_CAPABILITY", &step.required_capability)
            .env("GREENTIC_TERMINAL_STEP_ACTION", &step.action)
            .env(
                "GREENTIC_TERMINAL_VALUE",
                step.value.as_deref().unwrap_or(""),
            );
    }
    if let Some(assertion) = assertion {
        command
            .env("GREENTIC_TERMINAL_ASSERTION_ID", &assertion.id)
            .env(
                "GREENTIC_TERMINAL_CAPABILITY",
                &assertion.required_capability,
            )
            .env("GREENTIC_TERMINAL_EXPECTED", &assertion.expected);
    }

    let mut child = command.spawn().map_err(|err| {
        AdapterError::ExecutionFailed(format!("terminal runtime failed to start: {err}"))
    })?;
    if let Some(stdin) = child.stdin.as_mut() {
        let payload = terminal_runtime_payload(step, assertion);
        stdin.write_all(payload.as_bytes()).map_err(|err| {
            AdapterError::ExecutionFailed(format!("terminal runtime stdin failed: {err}"))
        })?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| AdapterError::ExecutionFailed(format!("terminal runtime failed: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AdapterError::ExecutionFailed(format!(
            "terminal runtime action {} failed with status {:?}: {}",
            action.as_str(),
            output.status.code(),
            stderr.trim()
        )));
    }
    Ok(parse_terminal_command_output(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn terminal_runtime_payload(step: Option<&RunnerStep>, assertion: Option<&Assertion>) -> String {
    if let Some(step) = step {
        return format!(
            "kind=step\nid={}\ncapability={}\naction={}\nvalue={}\n",
            step.id,
            step.required_capability,
            step.action,
            step.value.as_deref().unwrap_or("")
        );
    }
    if let Some(assertion) = assertion {
        return format!(
            "kind=assertion\nid={}\ncapability={}\nexpected={}\n",
            assertion.id, assertion.required_capability, assertion.expected
        );
    }
    "kind=observe\n".to_owned()
}

fn parse_terminal_command_output(stdout: &str) -> TerminalCommandOutput {
    let mut output = TerminalCommandOutput::default();
    for line in parse_terminal_screen(stdout) {
        if let Some(value) = line.strip_prefix("message:") {
            output.message = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("screen:") {
            output.screen.push(value.to_owned());
        } else if let Some(value) = line.strip_prefix("passed:") {
            output.passed = Some(value.trim().eq_ignore_ascii_case("true"));
        } else if !line.trim().is_empty() {
            output.screen.push(line.to_owned());
        }
    }
    output
}

pub fn run_local_pty_command(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> AdapterResult<Vec<String>> {
    Ok(run_local_pty_command_with_status(program, args, timeout)?.lines)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalPtyCommandOutput {
    lines: Vec<String>,
    exit_success: bool,
}

fn run_local_shell_command_with_status(
    command_text: &str,
    timeout: Duration,
) -> AdapterResult<LocalPtyCommandOutput> {
    let mut child = shell_command(command_text)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to spawn shell command: {err}"))
        })?;
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    loop {
        if child
            .try_wait()
            .map_err(|err| {
                AdapterError::ExecutionFailed(format!("failed to poll shell command: {err}"))
            })?
            .is_some()
        {
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().map_err(|err| {
        AdapterError::ExecutionFailed(format!("failed to collect shell command output: {err}"))
    })?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    if !output.stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    Ok(LocalPtyCommandOutput {
        lines: parse_terminal_screen(&text),
        exit_success: !timed_out && output.status.success(),
    })
}

fn run_local_pty_command_with_status(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> AdapterResult<LocalPtyCommandOutput> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to open PTY: {err}")))?;
    let mut command = CommandBuilder::new(program);
    for arg in args {
        command.arg(arg);
    }
    let mut child = pair.slave.spawn_command(command).map_err(|err| {
        AdapterError::ExecutionFailed(format!("failed to spawn PTY command: {err}"))
    })?;
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to read PTY: {err}")))?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    let _ = sender.send(Ok(bytes));
                    break;
                }
                Ok(count) => bytes.extend_from_slice(&buffer[..count]),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(err) => {
                    let _ = sender.send(Err(err));
                    break;
                }
            }
        }
    });
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    let mut status = None;
    while Instant::now() < deadline {
        match receiver.try_recv() {
            Ok(Ok(output)) => {
                bytes = output;
                break;
            }
            Ok(Err(err)) => {
                return Err(AdapterError::ExecutionFailed(format!(
                    "failed to read PTY output: {err}"
                )));
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
        }
        if let Ok(Some(child_status)) = child.try_wait() {
            status = Some(child_status);
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if status.is_none() {
        let _ = child.kill();
        status = child.wait().ok();
    }
    drop(pair.master);
    if bytes.is_empty() {
        if let Ok(Ok(output)) = receiver.recv_timeout(Duration::from_millis(200)) {
            bytes = output;
        }
    }
    let output = String::from_utf8_lossy(&bytes);
    Ok(LocalPtyCommandOutput {
        lines: parse_terminal_screen(&output),
        exit_success: status.is_some_and(|status| status.success()),
    })
}

pub fn parse_terminal_screen(output: &str) -> Vec<String> {
    let mut parser = Parser::new();
    let mut screen = AnsiScreen::default();
    parser.advance(&mut screen, output.as_bytes());
    screen.lines()
}

#[derive(Debug, Default)]
struct AnsiScreen {
    row: usize,
    col: usize,
    rows: Vec<Vec<char>>,
}

impl AnsiScreen {
    fn put_char(&mut self, value: char) {
        while self.rows.len() <= self.row {
            self.rows.push(Vec::new());
        }
        let line = &mut self.rows[self.row];
        while line.len() < self.col {
            line.push(' ');
        }
        if line.len() == self.col {
            line.push(value);
        } else {
            line[self.col] = value;
        }
        self.col += 1;
    }

    fn newline(&mut self) {
        self.row += 1;
        self.col = 0;
    }

    fn carriage_return(&mut self) {
        self.col = 0;
    }

    fn lines(self) -> Vec<String> {
        self.rows
            .into_iter()
            .map(|line| line.into_iter().collect::<String>().trim_end().to_owned())
            .filter(|line| !line.is_empty())
            .collect()
    }
}

impl Perform for AnsiScreen {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => self.carriage_return(),
            0x08 => {
                self.col = self.col.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if action == 'H' || action == 'f' {
            let mut params = params.iter();
            let row = params
                .next()
                .and_then(|values| values.first().copied())
                .unwrap_or(1)
                .saturating_sub(1) as usize;
            let col = params
                .next()
                .and_then(|values| values.first().copied())
                .unwrap_or(1)
                .saturating_sub(1) as usize;
            self.row = row;
            self.col = col;
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
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

pub fn run_terminal_workflow(
    adapter: &TerminalAdapter,
    workflow: TerminalWorkflow,
) -> AdapterResult<TerminalWorkflowOutcome> {
    let prompt = workflow.prompt.clone();
    let text_outputs = workflow.text_outputs.clone();
    let field_outputs = workflow.field_outputs.clone();
    let compiled = compile_workflow(&terminal_desktop_workflow(&workflow))
        .map_err(|err| AdapterError::ExecutionFailed(err.to_string()))?;

    let steps = compiled.steps;
    let results = adapter.replay(&steps)?;

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
        let value = extract_field_from_screen(&visible, output.field).ok_or_else(|| {
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

pub fn extract_field_from_screen(lines: &[String], field: ScreenField) -> Option<String> {
    let line = lines.get(field.row)?;
    Some(
        line.chars()
            .skip(field.col)
            .take(field.len)
            .collect::<String>()
            .trim()
            .to_owned(),
    )
}

pub fn extract_after_anchor_from_screen(lines: &[String], anchor: &str) -> Option<String> {
    lines.iter().find_map(|line| {
        let index = line.find(anchor)?;
        Some(line[index + anchor.len()..].trim().to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_recorder::{
        start_recording_session_with_registry, RecordingBackendRegistry,
    };
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn coverage_instrumented_run() -> bool {
        std::env::var_os("CARGO_LLVM_COV").is_some()
    }

    #[test]
    fn exposes_terminal_capabilities() {
        let capabilities = terminal_capabilities();

        assert!(capabilities.supports("terminal.connect"));
        assert!(capabilities.supports("terminal.extract_field"));
        assert_eq!(capabilities.adapter_id, TERMINAL_ADAPTER_ID);
    }

    #[test]
    fn terminal_workflow_uses_owned_runtime_and_reads_outputs() {
        let adapter = TerminalAdapter::with_profile_runtime_command(
            TerminalProfile {
                name: "test".to_owned(),
                protocol: TerminalProtocol::Tn3270,
                host: "terminal.test".to_owned(),
            },
            "echo message:ok && echo screen:ACCOUNT STATUS: ACTIVE && echo screen:BALANCE: 100.00 && echo passed:true",
        );
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
    fn terminal_workflow_fails_without_owned_runtime() {
        let adapter = TerminalAdapter::new();
        let err = run_terminal_workflow(
            &adapter,
            TerminalWorkflow {
                profile: TerminalProfile {
                    name: "test".to_owned(),
                    protocol: TerminalProtocol::Tn3270,
                    host: "terminal.test".to_owned(),
                },
                prompt: "Connect to a terminal.".to_owned(),
                initial_screen: Vec::new(),
                actions: vec![TerminalWorkflowAction {
                    name: "connect".to_owned(),
                    required_capability: "terminal.connect".to_owned(),
                    value: None,
                }],
                final_screen: Vec::new(),
                text_outputs: Vec::new(),
                field_outputs: Vec::new(),
            },
        )
        .expect_err("terminal workflow should fail without a real runtime");

        assert!(
            err.to_string()
                .contains("No owned terminal runtime is configured"),
            "{err}"
        );
    }

    #[test]
    fn terminal_execute_propagates_runtime_failed_status() {
        let adapter = TerminalAdapter::with_runtime_command("echo passed:false");
        let result = adapter
            .execute(RunnerStep {
                id: "runtime-step".to_owned(),
                action: "send_keys".to_owned(),
                target: LocatorTarget::default(),
                value: Some("ENTER".to_owned()),
                required_capability: "terminal.send_keys".to_owned(),
            })
            .expect("runtime output should parse");

        assert!(!result.success);
        assert!(result.message.contains("reported step failure"));
    }

    #[test]
    fn terminal_run_command_uses_local_pty_and_labels_output() {
        let adapter = TerminalAdapter::new();
        let command = if cfg!(windows) {
            "echo 10K\tC:\\tmp\\example"
        } else {
            "printf '10K\\t/tmp/example\\n'"
        };
        let expected_path = if cfg!(windows) {
            "C:\\tmp\\example"
        } else {
            "/tmp/example"
        };
        let result = adapter
            .execute(RunnerStep {
                id: "largest-files".to_owned(),
                action: "run_command".to_owned(),
                target: LocatorTarget {
                    preferred: Some(greentic_desktop_adapter::LocatorStrategy {
                        text: Some("outputs.largest_files".to_owned()),
                        ..greentic_desktop_adapter::LocatorStrategy::default()
                    }),
                    ..LocatorTarget::default()
                },
                value: Some(command.to_owned()),
                required_capability: "terminal.run_command".to_owned(),
            })
            .expect("local terminal command should run");

        assert!(result.message.starts_with("largest files:"), "{result:?}");
        assert!(result.message.contains(expected_path), "{result:?}");
    }

    #[test]
    fn terminal_run_command_fails_on_non_zero_shell_exit() {
        let adapter = TerminalAdapter::new();
        let command = if cfg!(windows) {
            "echo shell failed 1>&2 && exit /B 7"
        } else {
            "echo shell failed >&2; exit 7"
        };
        let err = adapter
            .execute(RunnerStep {
                id: "bad-command".to_owned(),
                action: "run_command".to_owned(),
                target: LocatorTarget::default(),
                value: Some(command.to_owned()),
                required_capability: "terminal.run_command".to_owned(),
            })
            .expect_err("non-zero shell exit should fail the step");

        assert!(err.to_string().contains("terminal command failed"), "{err}");
    }

    #[test]
    fn terminal_pty_collects_exit_status_after_eof() {
        if cfg!(windows) || coverage_instrumented_run() {
            return;
        }
        let output = run_local_pty_command_with_status(
            "sh",
            &["-lc", "printf 'done\\n'"],
            Duration::from_secs(2),
        )
        .expect("local PTY command should run");

        assert!(output.exit_success);
        assert!(output.lines.iter().any(|line| line.contains("done")));
    }

    #[test]
    fn parses_ansi_terminal_screen_with_cursor_positioning() {
        let screen = parse_terminal_screen("first\r\n\x1b[3;5Hfield");

        assert_eq!(screen, vec!["first".to_owned(), "    field".to_owned()]);
        assert_eq!(
            extract_field_from_screen(
                &screen,
                ScreenField {
                    row: 1,
                    col: 4,
                    len: 5,
                }
            ),
            Some("field".to_owned())
        );
    }

    #[test]
    fn local_pty_fixture_runs_command_and_parses_output() {
        if cfg!(windows) || coverage_instrumented_run() {
            return;
        }
        let screen = run_local_pty_command(
            "sh",
            &[
                "-lc",
                "printf 'ACCOUNT STATUS: ACTIVE\\nBALANCE: 100.00\\n'",
            ],
            Duration::from_secs(2),
        )
        .expect("local PTY command should run");

        assert!(screen.iter().any(|line| line.contains("ACCOUNT STATUS")));
        assert_eq!(
            extract_after_anchor_from_screen(&screen, "BALANCE:"),
            Some("100.00".to_owned())
        );
    }

    #[test]
    fn extracts_screen_fields_without_stateful_adapter() {
        let screen = vec![
            "ACCOUNT STATUS: ACTIVE".to_owned(),
            "BALANCE: 100.00".to_owned(),
        ];
        assert_eq!(
            extract_field_from_screen(
                &screen,
                ScreenField {
                    row: 0,
                    col: 16,
                    len: 6,
                }
            ),
            Some("ACTIVE".to_owned())
        );
        assert_eq!(
            extract_after_anchor_from_screen(&screen, "BALANCE:"),
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
            "echo ACCOUNT STATUS: ACTIVE && echo BALANCE: 100.00",
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
