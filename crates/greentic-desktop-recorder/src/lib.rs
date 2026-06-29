use greentic_desktop_adapter::{LocatorTarget, RecordedEvent, RunnerStep};
use greentic_desktop_workflow::{
    compile_workflow, DesktopWorkflow, NativePlatform, WorkflowAction, WorkflowActionKind,
    WorkflowAssertion, WorkflowEvidencePolicy, WorkflowInput, WorkflowOutput,
    WorkflowOutputExtractor, WorkflowRisk, WorkflowTarget, WorkflowValueType,
};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    AssistedPrompt,
    HumanDemonstration,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingSessionState {
    Starting,
    Recording,
    Paused,
    Blocked,
    Stopping,
    Normalising,
    Completed,
    Cancelled,
    Failed,
}

impl RecordingSessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Recording => "recording",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Stopping => "stopping",
            Self::Normalising => "normalising",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "starting" => Some(Self::Starting),
            "recording" => Some(Self::Recording),
            "paused" => Some(Self::Paused),
            "blocked" => Some(Self::Blocked),
            "stopping" => Some(Self::Stopping),
            "normalising" => Some(Self::Normalising),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingTargetKind {
    Web,
    Desktop,
    Java,
    Terminal,
    Remote,
}

impl RecordingTargetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Desktop => "desktop",
            Self::Java => "java",
            Self::Terminal => "terminal",
            Self::Remote => "remote",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "desktop" => Self::Desktop,
            "java" => Self::Java,
            "terminal" => Self::Terminal,
            "remote" => Self::Remote,
            _ => Self::Web,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingCaptureState {
    Starting,
    Recording,
    Paused,
    Blocked,
    Stopped,
    Failed,
}

impl RecordingCaptureState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Recording => "recording",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "starting" => Some(Self::Starting),
            "recording" => Some(Self::Recording),
            "paused" => Some(Self::Paused),
            "blocked" => Some(Self::Blocked),
            "stopped" => Some(Self::Stopped),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingPreflight {
    pub available: bool,
    pub blocked_reasons: Vec<String>,
}

impl RecordingPreflight {
    pub fn ready() -> Self {
        Self {
            available: true,
            blocked_reasons: Vec::new(),
        }
    }

    pub fn blocked(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            blocked_reasons: vec![reason.into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingEventEnvelope {
    pub session_id: String,
    pub backend: String,
    pub target_kind: RecordingTargetKind,
    pub timestamp: String,
    pub sequence: u64,
    pub event_kind: String,
    pub target_json: String,
    pub value: Option<String>,
    pub redaction: String,
    pub screenshot_ref: Option<String>,
    pub dom_snapshot_ref: Option<String>,
    pub ui_tree_ref: Option<String>,
    pub terminal_buffer_ref: Option<String>,
}

impl RecordingEventEnvelope {
    pub fn new(
        session_id: impl Into<String>,
        backend: impl Into<String>,
        target_kind: RecordingTargetKind,
        sequence: u64,
        event_kind: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            backend: backend.into(),
            target_kind,
            timestamp: unix_timestamp().to_string(),
            sequence,
            event_kind: event_kind.into(),
            target_json: "{}".to_owned(),
            value: None,
            redaction: "none".to_owned(),
            screenshot_ref: None,
            dom_snapshot_ref: None,
            ui_tree_ref: None,
            terminal_buffer_ref: None,
        }
    }

    pub fn render_json(&self) -> String {
        format!(
            r#"{{"schema_version":"recording.event.v1","session_id":"{}","backend":"{}","target_kind":"{}","timestamp":"{}","sequence":{},"event":{{"kind":"{}","target":{},"value":{},"redaction":"{}"}},"evidence":{{"screenshot_ref":{},"dom_snapshot_ref":{},"ui_tree_ref":{},"terminal_buffer_ref":{}}}}}"#,
            json_escape(&self.session_id),
            json_escape(&self.backend),
            self.target_kind.as_str(),
            json_escape(&self.timestamp),
            self.sequence,
            json_escape(&self.event_kind),
            if self.target_json.trim().is_empty() {
                "{}"
            } else {
                self.target_json.as_str()
            },
            json_option(&self.value),
            json_escape(&self.redaction),
            json_option(&self.screenshot_ref),
            json_option(&self.dom_snapshot_ref),
            json_option(&self.ui_tree_ref),
            json_option(&self.terminal_buffer_ref),
        )
    }
}

#[derive(Debug, Clone)]
pub struct RecordingEventSink {
    manifest: RecordingSessionManifest,
}

impl RecordingEventSink {
    pub fn new(manifest: RecordingSessionManifest) -> Self {
        Self { manifest }
    }

    pub fn append_event(
        &self,
        event: RecordingEventEnvelope,
    ) -> Result<(), RecordingLifecycleError> {
        append_raw_event(&self.manifest, &event.render_json())
    }

    pub fn session_id(&self) -> &str {
        &self.manifest.session_id
    }

    pub fn append_backend_warning(&self, warning: &str) -> Result<(), RecordingLifecycleError> {
        append_raw_event(
            &self.manifest,
            &format!(
                r#"{{"schema_version":"recording.event.v1","session_id":"{}","backend":"{}","target_kind":"{}","timestamp":"{}","sequence":0,"event":{{"kind":"backend_warning","target":{{}},"value":"{}","redaction":"none"}},"evidence":{{"screenshot_ref":null,"dom_snapshot_ref":null,"ui_tree_ref":null,"terminal_buffer_ref":null}}}}"#,
                json_escape(&self.manifest.session_id),
                json_escape(
                    self.manifest
                        .capture_backend
                        .as_deref()
                        .unwrap_or("unknown")
                ),
                self.manifest.target_kind.as_str(),
                unix_timestamp(),
                json_escape(warning)
            ),
        )
    }

    pub fn update_heartbeat(&self) -> Result<(), RecordingLifecycleError> {
        let mut manifest = self.manifest.clone();
        manifest.capture_heartbeat_at = Some(unix_timestamp().to_string());
        manifest.write()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingHandle {
    pub backend_id: String,
    pub capture_state: RecordingCaptureState,
}

pub trait RecordingBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn target_kind(&self) -> RecordingTargetKind;
    fn preflight(&self, request: &RecordingStartRequest) -> RecordingPreflight;
    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle;
}

#[derive(Debug, Clone)]
pub struct BlockingRecordingBackend {
    id: &'static str,
    target_kind: RecordingTargetKind,
    blocked_reason: String,
}

impl BlockingRecordingBackend {
    pub fn new(
        id: &'static str,
        target_kind: RecordingTargetKind,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id,
            target_kind,
            blocked_reason: reason.into(),
        }
    }
}

impl RecordingBackend for BlockingRecordingBackend {
    fn id(&self) -> &'static str {
        self.id
    }

    fn target_kind(&self) -> RecordingTargetKind {
        self.target_kind
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        RecordingPreflight::blocked(self.blocked_reason.clone())
    }

    fn start(&self, _request: RecordingStartRequest, _sink: RecordingEventSink) -> RecordingHandle {
        RecordingHandle {
            backend_id: self.id.to_owned(),
            capture_state: RecordingCaptureState::Blocked,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct FixtureRecordingBackend {
    id: &'static str,
    target_kind: RecordingTargetKind,
}

#[cfg(test)]
impl FixtureRecordingBackend {
    pub fn ready(id: &'static str, target_kind: RecordingTargetKind) -> Self {
        Self { id, target_kind }
    }
}

#[cfg(test)]
impl RecordingBackend for FixtureRecordingBackend {
    fn id(&self) -> &'static str {
        self.id
    }

    fn target_kind(&self) -> RecordingTargetKind {
        self.target_kind
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        RecordingPreflight::ready()
    }

    fn start(&self, _request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let mut event =
            RecordingEventEnvelope::new(sink.session_id(), self.id, self.target_kind, 1, "fill");
        event.value = Some("inputs.fixture_value".to_owned());
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();
        RecordingHandle {
            backend_id: self.id.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

#[derive(Clone, Default)]
pub struct RecordingBackendRegistry {
    backends: Vec<Arc<dyn RecordingBackend>>,
}

impl RecordingBackendRegistry {
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    pub fn register<B>(&mut self, backend: B)
    where
        B: RecordingBackend + 'static,
    {
        self.backends.push(Arc::new(backend));
    }

    pub fn backend_for(&self, kind: RecordingTargetKind) -> Option<Arc<dyn RecordingBackend>> {
        self.backends
            .iter()
            .find(|backend| backend.target_kind() == kind)
            .cloned()
    }

    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingSessionManifest {
    pub session_id: String,
    pub name: String,
    pub profile: String,
    pub state: RecordingSessionState,
    pub started_at: String,
    pub adapters: Vec<String>,
    pub platform_os: String,
    pub root: PathBuf,
    pub raw_events: PathBuf,
    pub screenshots: PathBuf,
    pub normalised_steps: PathBuf,
    pub draft_runner: PathBuf,
    pub redact: Vec<String>,
    pub secret_fields: Vec<String>,
    pub target_kind: RecordingTargetKind,
    pub capture_state: RecordingCaptureState,
    pub capture_backend: Option<String>,
    pub capture_heartbeat_at: Option<String>,
    pub capture_blocked_reasons: Vec<String>,
    pub observations: usize,
    pub screenshot_count: usize,
    pub last_event_summary: Option<String>,
}

#[derive(Debug)]
pub enum RecordingLifecycleError {
    Io(std::io::Error),
    InvalidSession(String),
    InvalidState(String),
}

impl std::fmt::Display for RecordingLifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::InvalidSession(message) | Self::InvalidState(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for RecordingLifecycleError {}

impl From<std::io::Error> for RecordingLifecycleError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedAction {
    pub event_type: String,
    pub timestamp: u64,
    pub adapter: String,
    pub target: LocatorTarget,
    pub value: Option<String>,
    pub screenshot_ref: Option<String>,
    pub normalized_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingStartRequest {
    pub name: String,
    pub profile: String,
    pub adapter: String,
    pub target_kind: RecordingTargetKind,
    pub out: PathBuf,
    pub runtime_home: PathBuf,
    pub redact: Vec<String>,
    pub secret_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerPackage {
    pub id: String,
    pub version: String,
    pub mode: RecordingMode,
    pub inputs: Vec<String>,
    pub secrets: Vec<String>,
    pub steps: Vec<RunnerStep>,
    pub assertions: Vec<String>,
    pub outputs: Vec<String>,
    pub open_questions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedInputCandidate {
    pub name: String,
    pub target: LocatorTarget,
    pub value: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedSecretCandidate {
    pub name: String,
    pub target: LocatorTarget,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedOutputCandidate {
    pub name: String,
    pub extractor: OutputExtractorCandidate,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedAssertionCandidate {
    pub name: String,
    pub target: LocatorTarget,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionRule {
    pub name: String,
    pub pattern: String,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputExtractorCandidate {
    VisibleText(String),
    TargetText(Box<LocatorTarget>),
    TerminalField { row: usize, col: usize, len: usize },
    Regex(String),
    VisionRegion(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenQuestion {
    pub id: String,
    pub question: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecordingAnnotations {
    pub inputs: Vec<RecordedInputCandidate>,
    pub secrets: Vec<RecordedSecretCandidate>,
    pub outputs: Vec<RecordedOutputCandidate>,
    pub assertions: Vec<RecordedAssertionCandidate>,
    pub redaction_rules: Vec<RedactionRule>,
    pub submit_actions: Vec<String>,
    pub open_questions: Vec<OpenQuestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordedPlatform {
    Windows,
    MacOS,
    LinuxX11,
    LinuxWayland,
}

impl RecordedPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOS => "macos",
            Self::LinuxX11 => "linux-x11",
            Self::LinuxWayland => "linux-wayland",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortablePlatformSupport {
    pub supported: Vec<RecordedPlatform>,
    pub preferred_adapter: BTreeMap<RecordedPlatform, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlatformLocators {
    pub windows: Option<LocatorTarget>,
    pub macos: Option<LocatorTarget>,
    pub linux_x11: Option<LocatorTarget>,
    pub linux_wayland: Option<LocatorTarget>,
}

impl PlatformLocators {
    pub fn locator_for(&self, platform: RecordedPlatform) -> Option<LocatorTarget> {
        match platform {
            RecordedPlatform::Windows => self.windows.clone(),
            RecordedPlatform::MacOS => self.macos.clone(),
            RecordedPlatform::LinuxX11 => self.linux_x11.clone(),
            RecordedPlatform::LinuxWayland => self.linux_wayland.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlatformAppLaunch {
    pub windows_executable: Option<String>,
    pub macos_bundle_id: Option<String>,
    pub linux_desktop_file: Option<String>,
}

impl PlatformAppLaunch {
    pub fn value_for(&self, platform: RecordedPlatform) -> Option<String> {
        match platform {
            RecordedPlatform::Windows => self
                .windows_executable
                .as_ref()
                .map(|value| format!("windows:exec {value}")),
            RecordedPlatform::MacOS => self
                .macos_bundle_id
                .as_ref()
                .map(|value| format!("macos:bundle-id {value}")),
            RecordedPlatform::LinuxX11 | RecordedPlatform::LinuxWayland => self
                .linux_desktop_file
                .as_ref()
                .map(|value| format!("linux:desktop-file {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableStep {
    pub id: String,
    pub action: String,
    pub required_capability: String,
    pub app: Option<PlatformAppLaunch>,
    pub locators: PlatformLocators,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableRunnerPackage {
    pub package: RunnerPackage,
    pub platforms: PortablePlatformSupport,
    pub steps: Vec<PortableStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformReplayPlan {
    pub platform: RecordedPlatform,
    pub adapter_id: String,
    pub steps: Vec<RunnerStep>,
    pub evidence_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortableReplayError {
    UnsupportedPlatform(RecordedPlatform),
    MissingPreferredAdapter(RecordedPlatform),
    MissingPlatformLocator {
        step_id: String,
        platform: RecordedPlatform,
    },
    MissingPlatformApp {
        step_id: String,
        platform: RecordedPlatform,
    },
}

#[derive(Debug, Clone)]
pub struct RecordingSession {
    id: String,
    mode: RecordingMode,
    actions: Vec<RecordedAction>,
    prompt_steps: Vec<RunnerStep>,
    annotations: RecordingAnnotations,
}

impl RecordingSession {
    pub fn new(id: impl Into<String>, mode: RecordingMode) -> Self {
        Self {
            id: id.into(),
            mode,
            actions: Vec::new(),
            prompt_steps: Vec::new(),
            annotations: RecordingAnnotations::default(),
        }
    }

    pub fn capture_human_event(
        &mut self,
        adapter: impl Into<String>,
        event: RecordedEvent,
        screenshot_ref: Option<String>,
    ) {
        self.actions.push(RecordedAction {
            event_type: event.action,
            timestamp: unix_timestamp(),
            adapter: adapter.into(),
            target: event.target,
            value: event.value.map(|value| redact_sensitive_value(&value)),
            screenshot_ref,
            normalized_summary: None,
        });
    }

    pub fn add_prompt_step(&mut self, step: RunnerStep) {
        self.prompt_steps.push(step);
    }

    pub fn mark_input(
        &mut self,
        name: impl Into<String>,
        target: LocatorTarget,
        value: Option<String>,
    ) {
        self.annotations.inputs.push(RecordedInputCandidate {
            name: normalize_field_name(name.into()),
            target,
            value,
            required: true,
        });
    }

    pub fn mark_secret(&mut self, name: impl Into<String>, target: LocatorTarget) {
        self.annotations.secrets.push(RecordedSecretCandidate {
            name: normalize_field_name(name.into()),
            target,
            required: true,
        });
    }

    pub fn mark_output(&mut self, name: impl Into<String>, extractor: OutputExtractorCandidate) {
        self.annotations.outputs.push(RecordedOutputCandidate {
            name: normalize_field_name(name.into()),
            extractor,
            required: true,
        });
    }

    pub fn mark_assertion(
        &mut self,
        name: impl Into<String>,
        target: LocatorTarget,
        expected: impl Into<String>,
    ) {
        self.annotations
            .assertions
            .push(RecordedAssertionCandidate {
                name: normalize_field_name(name.into()),
                target,
                expected: expected.into(),
            });
    }

    pub fn mark_submit_action(&mut self, action_name: impl Into<String>) {
        self.annotations.submit_actions.push(action_name.into());
    }

    pub fn add_redaction_rule(
        &mut self,
        name: impl Into<String>,
        pattern: impl Into<String>,
        replacement: impl Into<String>,
    ) {
        self.annotations.redaction_rules.push(RedactionRule {
            name: normalize_field_name(name.into()),
            pattern: pattern.into(),
            replacement: replacement.into(),
        });
    }

    pub fn add_open_question(
        &mut self,
        id: impl Into<String>,
        question: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.annotations.open_questions.push(OpenQuestion {
            id: normalize_field_name(id.into()),
            question: question.into(),
            reason: reason.into(),
        });
    }

    pub fn normalize(&mut self) {
        for action in &mut self.actions {
            action.normalized_summary = Some(normalize_action(action));
        }
    }

    pub fn into_package(mut self, version: impl Into<String>) -> RunnerPackage {
        self.normalize();
        let mut steps = self.prompt_steps;
        steps.extend(self.actions.iter().enumerate().map(|(index, action)| {
            let required_capability = format!(
                "{}.{}",
                capability_prefix(&action.adapter),
                action.event_type
            );
            RunnerStep {
                id: format!("recorded_{}", index + 1),
                action: action.event_type.clone(),
                target: action.target.clone(),
                value: action.value.clone(),
                required_capability,
            }
        }));
        let derived = derive_recording_candidates(&self.actions, &self.annotations);

        RunnerPackage {
            id: self.id,
            version: version.into(),
            mode: self.mode,
            inputs: derived
                .inputs
                .iter()
                .map(|input| format!("inputs.{}", input.name))
                .collect(),
            secrets: derived
                .secrets
                .iter()
                .map(|secret| format!("secrets.{}", secret.name))
                .collect(),
            steps,
            assertions: derived
                .assertions
                .iter()
                .map(|assertion| assertion.name.clone())
                .collect(),
            outputs: derived
                .outputs
                .iter()
                .map(|output| format!("outputs.{}", output.name))
                .collect(),
            open_questions: derived
                .open_questions
                .iter()
                .map(|question| question.question.clone())
                .collect(),
        }
    }
}

impl RecordingSessionManifest {
    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.yaml")
    }

    pub fn render_yaml(&self) -> String {
        let adapters = self
            .adapters
            .iter()
            .map(|adapter| format!("  - {adapter}\n"))
            .collect::<String>();
        format!(
            "session_id: {}\nname: {}\nprofile: {}\nstate: {}\nstarted_at: \"{}\"\nadapters:\n{}platform:\n  os: {}\npaths:\n  raw_events: {}\n  screenshots: {}\n  normalised_steps: {}\n  draft_runner: {}\nredact: [{}]\nsecret_fields: [{}]\ncapture:\n  target_kind: {}\n  state: {}\n  backend: {}\n  heartbeat_at: {}\n  blocked_reasons:\n{}  observations: {}\n  screenshots: {}\n  last_event_summary: {}\n",
            self.session_id,
            self.name,
            self.profile,
            self.state.as_str(),
            self.started_at,
            adapters,
            self.platform_os,
            self.raw_events.display(),
            self.screenshots.display(),
            self.normalised_steps.display(),
            self.draft_runner.display(),
            self.redact.join(","),
            self.secret_fields.join(","),
            self.target_kind.as_str(),
            self.capture_state.as_str(),
            self.capture_backend.as_deref().unwrap_or(""),
            self.capture_heartbeat_at.as_deref().unwrap_or(""),
            self.capture_blocked_reasons
                .iter()
                .map(|reason| format!("    - {}\n", reason))
                .collect::<String>(),
            self.observations,
            self.screenshot_count,
            self.last_event_summary.as_deref().unwrap_or("")
        )
    }

    pub fn write(&self) -> Result<(), RecordingLifecycleError> {
        fs::create_dir_all(&self.root)?;
        fs::write(self.manifest_path(), self.render_yaml())?;
        Ok(())
    }
}

pub fn start_recording_session(
    request: RecordingStartRequest,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    start_recording_session_with_registry(request, &RecordingBackendRegistry::new())
}

pub fn start_recording_session_with_registry(
    request: RecordingStartRequest,
    registry: &RecordingBackendRegistry,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    if request.name.trim().is_empty() {
        return Err(RecordingLifecycleError::InvalidSession(
            "recording name must not be empty".to_owned(),
        ));
    }
    if request.adapter.trim().is_empty() {
        return Err(RecordingLifecycleError::InvalidSession(
            "recording adapter must not be empty".to_owned(),
        ));
    }
    let session_id = format!("rec_{}", unix_timestamp());
    let mut manifest = RecordingSessionManifest {
        session_id: session_id.clone(),
        name: request.name.clone(),
        profile: request.profile.clone(),
        state: RecordingSessionState::Starting,
        started_at: unix_timestamp().to_string(),
        adapters: vec![request.adapter.clone()],
        platform_os: std::env::consts::OS.to_owned(),
        raw_events: request.out.join("raw").join("events.jsonl"),
        screenshots: request.out.join("evidence").join("screenshots"),
        normalised_steps: request.out.join("normalised").join("steps.yaml"),
        draft_runner: request.out.join("runner.draft.yaml"),
        root: request.out.clone(),
        redact: request.redact.clone(),
        secret_fields: request.secret_fields.clone(),
        target_kind: request.target_kind,
        capture_state: RecordingCaptureState::Starting,
        capture_backend: None,
        capture_heartbeat_at: None,
        capture_blocked_reasons: Vec::new(),
        observations: 0,
        screenshot_count: 0,
        last_event_summary: None,
    };
    fs::create_dir_all(manifest.raw_events.parent().ok_or_else(|| {
        RecordingLifecycleError::InvalidSession("raw event path has no parent".to_owned())
    })?)?;
    clear_generated_recording_files(&manifest)?;
    fs::create_dir_all(&manifest.screenshots)?;
    fs::create_dir_all(manifest.normalised_steps.parent().ok_or_else(|| {
        RecordingLifecycleError::InvalidSession("normalised path has no parent".to_owned())
    })?)?;
    if let Some(backend) = registry.backend_for(request.target_kind) {
        let preflight = backend.preflight(&request);
        if preflight.available {
            manifest.state = RecordingSessionState::Recording;
            manifest.capture_state = RecordingCaptureState::Recording;
            manifest.capture_backend = Some(backend.id().to_owned());
            manifest.capture_heartbeat_at = Some(unix_timestamp().to_string());
            manifest.last_event_summary = Some("capture backend started".to_owned());
            manifest.write()?;
            let sink = RecordingEventSink::new(manifest.clone());
            let handle = backend.start(request.clone(), sink.clone());
            manifest.capture_state = handle.capture_state;
            manifest.capture_backend = Some(handle.backend_id);
            manifest.capture_heartbeat_at = Some(unix_timestamp().to_string());
            manifest.write()?;
            sink.update_heartbeat()?;
        } else {
            manifest.state = RecordingSessionState::Blocked;
            manifest.capture_state = RecordingCaptureState::Blocked;
            manifest.capture_backend = Some(backend.id().to_owned());
            manifest.capture_blocked_reasons = preflight.blocked_reasons;
            manifest.last_event_summary = Some("capture blocked".to_owned());
            manifest.write()?;
        }
    } else {
        manifest.state = RecordingSessionState::Blocked;
        manifest.capture_state = RecordingCaptureState::Blocked;
        manifest.capture_blocked_reasons = vec![format!(
            "No recording backend is registered for {} targets.",
            request.target_kind.as_str()
        )];
        manifest.last_event_summary = Some("capture blocked".to_owned());
        manifest.write()?;
    }
    append_raw_event(
        &manifest,
        &format!(
            r#"{{"type":"session_started","timestamp":"{}","adapter":"{}","capture_state":"{}"}}"#,
            unix_timestamp(),
            json_escape(&request.adapter),
            manifest.capture_state.as_str()
        ),
    )?;
    write_session_index(&request.runtime_home, &session_id, &manifest.root)?;
    Ok(manifest)
}

fn clear_generated_recording_files(
    manifest: &RecordingSessionManifest,
) -> Result<(), RecordingLifecycleError> {
    for file in [
        manifest.raw_events.clone(),
        manifest.root.join("markers.jsonl"),
        manifest.draft_runner.clone(),
        manifest.normalised_steps.clone(),
    ] {
        match fs::remove_file(&file) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    for dir in [
        manifest.root.join("evidence").join("dom"),
        manifest.screenshots.clone(),
    ] {
        match fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

pub fn pause_recording_session(
    runtime_home: &Path,
    session: &str,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    transition(
        runtime_home,
        session,
        RecordingSessionState::Paused,
        "session_paused",
    )
}

pub fn resume_recording_session(
    runtime_home: &Path,
    session: &str,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    transition(
        runtime_home,
        session,
        RecordingSessionState::Recording,
        "session_resumed",
    )
}

pub fn stop_recording_session(
    runtime_home: &Path,
    session: &str,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    transition(
        runtime_home,
        session,
        RecordingSessionState::Completed,
        "session_stopped",
    )
}

pub fn cancel_recording_session(
    runtime_home: &Path,
    session: &str,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    transition(
        runtime_home,
        session,
        RecordingSessionState::Cancelled,
        "session_cancelled",
    )
}

pub fn load_recording_session(
    runtime_home: &Path,
    session: &str,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    let root = resolve_session_root(runtime_home, session)?;
    parse_manifest(&root.join("manifest.yaml"), root)
}

pub fn list_recording_sessions(
    runtime_home: &Path,
) -> Result<Vec<RecordingSessionManifest>, RecordingLifecycleError> {
    let index = session_index_dir(runtime_home);
    if !index.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(index)? {
        let entry = entry?;
        if entry.path().extension().and_then(|value| value.to_str()) == Some("path") {
            let root = fs::read_to_string(entry.path())?;
            if let Ok(manifest) = parse_manifest(
                Path::new(root.trim()).join("manifest.yaml").as_path(),
                PathBuf::from(root.trim()),
            ) {
                sessions.push(manifest);
            }
        }
    }
    sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    Ok(sessions)
}

pub fn normalise_recording(
    recording: &Path,
    out: &Path,
) -> Result<RunnerPackage, RecordingLifecycleError> {
    let raw_events = if recording.is_dir() {
        recording.join("events.jsonl")
    } else {
        recording.to_path_buf()
    };
    let mut contents = String::new();
    fs::File::open(&raw_events)?.read_to_string(&mut contents)?;
    let runner_id = raw_events
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .unwrap_or("recorded.runner")
        .to_owned();
    let workflow = normalise_recording_workflow(&runner_id, &contents)?;
    let compiled = compile_workflow(&workflow)
        .map_err(|err| RecordingLifecycleError::InvalidSession(err.to_string()))?;
    let steps = compiled.steps;
    if steps.is_empty() {
        return Err(RecordingLifecycleError::InvalidSession(
            "recording has no captured events to normalise".to_owned(),
        ));
    }
    let package = RunnerPackage {
        id: runner_id,
        version: "0.1.0-draft".to_owned(),
        mode: RecordingMode::HumanDemonstration,
        inputs: derive_raw_event_inputs(&steps),
        secrets: derive_raw_event_secrets(&steps),
        steps,
        assertions: vec!["recording completed".to_owned()],
        outputs: workflow
            .outputs
            .iter()
            .map(|output| format!("outputs.{}", output.name))
            .collect(),
        open_questions: derive_open_questions(&contents),
    };
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, package.render_yaml())?;
    Ok(package)
}

pub fn finalise_recording(
    recording: &Path,
    runner: &Path,
) -> Result<PathBuf, RecordingLifecycleError> {
    let out = recording.join("runner.draft.yaml");
    fs::copy(runner, &out)?;
    Ok(out)
}

pub fn append_recording_note(
    runtime_home: &Path,
    session: &str,
    event_type: &str,
    value: &str,
) -> Result<(), RecordingLifecycleError> {
    let manifest = load_recording_session(runtime_home, session)?;
    append_raw_event(
        &manifest,
        &format!(
            "{{\"type\":\"{}\",\"timestamp\":\"{}\",\"value\":\"{}\"}}",
            json_escape(event_type),
            unix_timestamp(),
            json_escape(&redact_sensitive_value(value))
        ),
    )
}

fn transition(
    runtime_home: &Path,
    session: &str,
    state: RecordingSessionState,
    event: &str,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    let mut manifest = load_recording_session(runtime_home, session)?;
    if matches!(
        manifest.state,
        RecordingSessionState::Completed | RecordingSessionState::Cancelled
    ) {
        return Err(RecordingLifecycleError::InvalidState(format!(
            "recording session {} is already {}",
            manifest.session_id,
            manifest.state.as_str()
        )));
    }
    manifest.state = state;
    manifest.capture_state = match state {
        RecordingSessionState::Recording => RecordingCaptureState::Recording,
        RecordingSessionState::Paused => RecordingCaptureState::Paused,
        RecordingSessionState::Blocked => RecordingCaptureState::Blocked,
        RecordingSessionState::Completed
        | RecordingSessionState::Cancelled
        | RecordingSessionState::Stopping
        | RecordingSessionState::Normalising => RecordingCaptureState::Stopped,
        RecordingSessionState::Failed => RecordingCaptureState::Failed,
        RecordingSessionState::Starting => RecordingCaptureState::Starting,
    };
    manifest.capture_heartbeat_at = Some(unix_timestamp().to_string());
    manifest.last_event_summary = Some(event.to_owned());
    manifest.write()?;
    append_raw_event(
        &manifest,
        &format!(
            "{{\"type\":\"{}\",\"timestamp\":\"{}\"}}",
            event,
            unix_timestamp()
        ),
    )?;
    Ok(manifest)
}

impl RunnerPackage {
    pub fn render_yaml(&self) -> String {
        let mut output = format!(
            "id: {}\nversion: {}\nmode: {:?}\n",
            self.id, self.version, self.mode
        );
        render_string_list(&mut output, "inputs", &self.inputs);
        render_string_list(&mut output, "secrets", &self.secrets);
        render_string_list(&mut output, "outputs", &self.outputs);
        render_string_list(&mut output, "assertions", &self.assertions);
        render_string_list(&mut output, "open_questions", &self.open_questions);
        output.push_str("steps:\n");

        for step in &self.steps {
            output.push_str(&format!(
                "  - id: {}\n    action: {}\n    required_capability: {}\n",
                step.id, step.action, step.required_capability
            ));
            if let Some(value) = &step.value {
                output.push_str(&format!("    value: \"{}\"\n", value));
            }
        }

        output
    }
}

impl PortableRunnerPackage {
    pub fn replay_plan(
        &self,
        platform: RecordedPlatform,
    ) -> Result<PlatformReplayPlan, PortableReplayError> {
        if !self.platforms.supported.contains(&platform) {
            return Err(PortableReplayError::UnsupportedPlatform(platform));
        }
        let adapter_id = self
            .platforms
            .preferred_adapter
            .get(&platform)
            .cloned()
            .ok_or(PortableReplayError::MissingPreferredAdapter(platform))?;
        let mut steps = Vec::new();
        for step in &self.steps {
            let target = step.locators.locator_for(platform).ok_or_else(|| {
                PortableReplayError::MissingPlatformLocator {
                    step_id: step.id.clone(),
                    platform,
                }
            })?;
            let value = if let Some(app) = &step.app {
                Some(app.value_for(platform).ok_or_else(|| {
                    PortableReplayError::MissingPlatformApp {
                        step_id: step.id.clone(),
                        platform,
                    }
                })?)
            } else {
                step.value.clone()
            };
            steps.push(RunnerStep {
                id: step.id.clone(),
                action: step.action.clone(),
                target,
                value,
                required_capability: step.required_capability.clone(),
            });
        }

        Ok(PlatformReplayPlan {
            platform,
            adapter_id,
            steps,
            evidence_path: format!(
                "evidence://{}/{}/platform-path.json",
                self.package.id,
                platform.as_str()
            ),
        })
    }

    pub fn render_yaml(&self) -> String {
        let mut output = self.package.render_yaml();
        output.push_str("platforms:\n  supported:\n");
        for platform in &self.platforms.supported {
            output.push_str(&format!("    - {}\n", platform.as_str()));
        }
        output.push_str("  preferred_adapter:\n");
        for (platform, adapter) in &self.platforms.preferred_adapter {
            output.push_str(&format!("    {}: {}\n", platform.as_str(), adapter));
        }
        output.push_str("portable_steps:\n");
        for step in &self.steps {
            output.push_str(&format!(
                "  - id: {}\n    action: {}\n    required_capability: {}\n",
                step.id, step.action, step.required_capability
            ));
            if let Some(app) = &step.app {
                output.push_str("    app:\n");
                if let Some(value) = &app.windows_executable {
                    output.push_str(&format!(
                        "      windows:\n        executable: \"{}\"\n",
                        value
                    ));
                }
                if let Some(value) = &app.macos_bundle_id {
                    output.push_str(&format!("      macos:\n        bundle_id: \"{}\"\n", value));
                }
                if let Some(value) = &app.linux_desktop_file {
                    output.push_str(&format!(
                        "      linux:\n        desktop_file: \"{}\"\n",
                        value
                    ));
                }
            }
        }
        output
    }
}

pub fn merge_prompt_and_recorded_steps(
    prompt_steps: Vec<RunnerStep>,
    recorded_steps: Vec<RunnerStep>,
) -> Vec<RunnerStep> {
    let mut merged = prompt_steps;
    merged.extend(recorded_steps);
    merged
}

fn derive_recording_candidates(
    actions: &[RecordedAction],
    annotations: &RecordingAnnotations,
) -> RecordingAnnotations {
    let mut derived = annotations.clone();

    for action in actions {
        let target_name = target_label(&action.target);
        if let Some(value) = &action.value {
            if value == "{{secret}}" || looks_secretish(&target_name) || looks_secretish(value) {
                if value == "{{secret}}" && !derived.secrets.is_empty() {
                    continue;
                }
                let name = if value == "{{secret}}" && !looks_secretish(&target_name) {
                    "recorded_secret".to_owned()
                } else {
                    normalize_field_name(if target_name.is_empty() {
                        "secret"
                    } else {
                        &target_name
                    })
                };
                if !derived.secrets.iter().any(|secret| secret.name == name) {
                    derived.secrets.push(RecordedSecretCandidate {
                        name,
                        target: action.target.clone(),
                        required: true,
                    });
                }
            } else if value.contains("{{inputs.") {
                let name = value
                    .trim_matches('{')
                    .trim_matches('}')
                    .strip_prefix("inputs.")
                    .unwrap_or("recorded_input");
                let name = normalize_field_name(name);
                if !derived.inputs.iter().any(|input| input.name == name) {
                    derived.inputs.push(RecordedInputCandidate {
                        name,
                        target: action.target.clone(),
                        value: None,
                        required: true,
                    });
                }
            }
        }

        if matches!(action.event_type.as_str(), "copy" | "read_text" | "observe") {
            if let Some(value) = &action.value {
                if !value.trim().is_empty() && value != "{{secret}}" {
                    let name = normalize_field_name(if target_name.is_empty() {
                        "recorded_output"
                    } else {
                        &target_name
                    });
                    if !derived.outputs.iter().any(|output| output.name == name) {
                        derived.outputs.push(RecordedOutputCandidate {
                            name,
                            extractor: OutputExtractorCandidate::VisibleText(value.clone()),
                            required: true,
                        });
                    }
                }
            }
        }
    }

    if derived.outputs.is_empty() && derived.assertions.is_empty() {
        derived.open_questions.push(OpenQuestion {
            id: "missing_output".to_owned(),
            question: "Which visible value should this runner return as output?".to_owned(),
            reason: "recording did not mark an output or assertion".to_owned(),
        });
    }

    derived
}

fn derive_raw_event_inputs(steps: &[RunnerStep]) -> Vec<String> {
    let mut inputs = Vec::new();
    for step in steps {
        if let Some(value) = &step.value {
            if value.contains("{{inputs.") {
                inputs.push(value.trim_matches('{').trim_matches('}').to_owned());
            }
        }
    }
    inputs.sort();
    inputs.dedup();
    inputs
}

fn derive_raw_event_secrets(steps: &[RunnerStep]) -> Vec<String> {
    let mut secrets = Vec::new();
    for step in steps {
        if step.value.as_deref() == Some("{{secret}}") {
            secrets.push("secrets.recorded_secret".to_owned());
        }
    }
    secrets.sort();
    secrets.dedup();
    secrets
}

pub fn normalise_recording_workflow(
    runner_id: &str,
    contents: &str,
) -> Result<DesktopWorkflow, RecordingLifecycleError> {
    let target_kind = contents
        .lines()
        .find_map(|line| json_string_value(line, "target_kind"))
        .unwrap_or_else(|| "vision".to_owned());
    let target = workflow_target_for(&target_kind, contents);
    let mut workflow = DesktopWorkflow {
        id: runner_id.to_owned(),
        summary: "Recorded desktop automation".to_owned(),
        target,
        inputs: Vec::new(),
        actions: Vec::new(),
        outputs: Vec::new(),
        assertions: Vec::new(),
        evidence_policy: WorkflowEvidencePolicy {
            capture_steps: true,
            capture_screenshots: contents.contains("screenshot_ref")
                || contents.contains("terminal_buffer_ref")
                || contents.contains("ui_tree_ref"),
        },
    };

    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty()
            || line.contains(r#""type":"session_started""#)
            || line.contains(r#""type":"session_stopped""#)
        {
            continue;
        }
        let action = raw_event_action(line);
        let value = json_string_value(line, "value").map(|value| redact_sensitive_value(&value));
        if value.as_deref() == Some("{{secret}}") {
            workflow.inputs.push(WorkflowInput {
                name: format!("secret_{}", index + 1),
                value_type: WorkflowValueType::String,
                required: true,
                secret: true,
                target: LocatorTarget::default(),
                value_template: "{{secret}}".to_owned(),
            });
        }

        if let Some(input_name) = input_name_from_value(value.as_deref()) {
            workflow.inputs.push(WorkflowInput {
                name: input_name,
                value_type: WorkflowValueType::String,
                required: true,
                secret: false,
                target: LocatorTarget::default(),
                value_template: value.clone().unwrap_or_default(),
            });
        }

        match workflow_action_for(&target_kind, &action, value.clone()) {
            Some(workflow_action) => workflow.actions.push(workflow_action),
            None if is_output_event(&action) => workflow.outputs.push(WorkflowOutput {
                name: output_name_for(line, workflow.outputs.len()),
                value_type: WorkflowValueType::String,
                extractor: output_extractor_for(&target_kind, value.clone()),
                required: true,
                expected: value.clone(),
            }),
            None => {}
        }

        if action == "assert_text" {
            workflow.assertions.push(WorkflowAssertion {
                name: format!("assertion_{}", index + 1),
                target: LocatorTarget::default(),
                expected: value.unwrap_or_default(),
                capability_hint: Some(raw_event_capability(line, &action)),
            });
        }
    }

    dedupe_workflow_inputs(&mut workflow.inputs);
    if workflow.actions.is_empty() && workflow.inputs.is_empty() && workflow.outputs.is_empty() {
        return Err(RecordingLifecycleError::InvalidSession(
            "recording has no semantic events to normalise".to_owned(),
        ));
    }
    Ok(workflow)
}

fn workflow_target_for(target_kind: &str, contents: &str) -> WorkflowTarget {
    match target_kind {
        "web" => WorkflowTarget::web(
            contents
                .lines()
                .find(|line| json_string_value(line, "kind").as_deref() == Some("navigate"))
                .and_then(|line| json_string_value(line, "value"))
                .unwrap_or_else(|| "about:blank".to_owned()),
        ),
        "desktop" => WorkflowTarget::native_app(
            NativePlatform::MacOs,
            Some("Recorded app".to_owned()),
            "Recorded window".to_owned(),
        ),
        "java" => WorkflowTarget::java_app("Recorded Java window"),
        "terminal" => WorkflowTarget::terminal("recorded-terminal"),
        "remote" => WorkflowTarget::vision(),
        _ => WorkflowTarget::vision(),
    }
}

fn workflow_action_for(
    target_kind: &str,
    action: &str,
    value: Option<String>,
) -> Option<WorkflowAction> {
    let kind = match action {
        "click" | "click_region" if target_kind == "remote" => {
            WorkflowActionKind::AdapterCapability("remote.click_region".to_owned())
        }
        "key" | "press_key" if target_kind == "remote" => {
            WorkflowActionKind::AdapterCapability("remote.press_key".to_owned())
        }
        "click" | "click_region" => WorkflowActionKind::Click,
        "key" | "press_key" | "press_shortcut" | "send_keys" => WorkflowActionKind::Key,
        "observe" | "wait_for" | "wait_for_screen" | "wait_for_text" => WorkflowActionKind::Wait,
        "screenshot" => WorkflowActionKind::Screenshot,
        "fill" | "type_text" | "send_text" if target_kind == "terminal" => {
            WorkflowActionKind::Input
        }
        "fill" | "type_text" | "send_text" => return None,
        "goto" | "find_window" | "activate_window" | "focus_session" | "connect" => return None,
        "read" | "read_text" | "extract_field" | "extract_text_region" => return None,
        "backend_warning" | "recorder_start" => return None,
        other => WorkflowActionKind::AdapterCapability(format!(
            "{}.{}",
            target_kind_adapter_prefix(target_kind),
            other
        )),
    };
    Some(WorkflowAction {
        name: action.to_owned(),
        kind,
        target: LocatorTarget::default(),
        value_template: value,
        risk: WorkflowRisk::Low,
    })
}

fn target_kind_adapter_prefix(target_kind: &str) -> &str {
    match target_kind {
        "desktop" => "desktop",
        "java" => "java",
        "terminal" => "terminal",
        "remote" => "remote",
        "web" => "web",
        _ => "vision",
    }
}

fn input_name_from_value(value: Option<&str>) -> Option<String> {
    let value = value?;
    value
        .trim_matches('{')
        .trim_matches('}')
        .strip_prefix("inputs.")
        .map(str::to_owned)
}

fn is_output_event(action: &str) -> bool {
    matches!(
        action,
        "read" | "read_text" | "extract_field" | "extract_text_region" | "observe"
    )
}

fn output_name_for(line: &str, index: usize) -> String {
    json_string_value(line, "label")
        .or_else(|| json_string_value(line, "name"))
        .map(normalize_field_name)
        .filter(|name| name != "value")
        .unwrap_or_else(|| format!("recorded_output_{}", index + 1))
}

fn output_extractor_for(target_kind: &str, value: Option<String>) -> WorkflowOutputExtractor {
    if target_kind == "terminal" {
        WorkflowOutputExtractor::VisibleText(value.unwrap_or_default())
    } else {
        WorkflowOutputExtractor::TargetText(Box::default())
    }
}

fn dedupe_workflow_inputs(inputs: &mut Vec<WorkflowInput>) {
    let mut seen = Vec::new();
    inputs.retain(|input| {
        if seen.contains(&input.name) {
            false
        } else {
            seen.push(input.name.clone());
            true
        }
    });
}

fn derive_open_questions(contents: &str) -> Vec<String> {
    let mut questions = Vec::new();
    if !contents.contains("\"role\"")
        && !contents.contains("\"label\"")
        && !contents.contains("\"accessible_name\"")
        && !contents.contains("\"test_id\"")
    {
        questions.push(
            "Some recorded targets have weak locators. Confirm the intended UI elements before publishing."
                .to_owned(),
        );
    }
    if !contents.contains("extract") && !contents.contains("read") && !contents.contains("result") {
        questions.push("Which visible value should this runner return as output?".to_owned());
    }
    questions
}

fn raw_event_action(line: &str) -> String {
    json_string_value(line, "type")
        .or_else(|| json_string_value(line, "kind"))
        .map(|kind| match kind.as_str() {
            "navigate" => "goto".to_owned(),
            "input" | "change" | "type" => "fill".to_owned(),
            "type_text" => "type_text".to_owned(),
            "submit" => "click".to_owned(),
            other => other.to_owned(),
        })
        .unwrap_or_else(|| "recorded".to_owned())
}

fn raw_event_capability(line: &str, action: &str) -> String {
    if json_string_value(line, "target_kind").as_deref() == Some("web") {
        return match action {
            "goto" => "web.goto".to_owned(),
            "fill" => "web.fill".to_owned(),
            "click" => "web.click".to_owned(),
            "select" => "web.select".to_owned(),
            "key" | "press" => "web.press".to_owned(),
            "observe" | "wait_for" => "web.wait_for".to_owned(),
            "read" | "extract_text" => "web.extract_text".to_owned(),
            other => format!("web.{other}"),
        };
    }
    if json_string_value(line, "target_kind").as_deref() == Some("desktop") {
        return match action {
            "activate_window" => "desktop.activate_window".to_owned(),
            "click" => "desktop.click".to_owned(),
            "fill" | "type_text" => "desktop.type_text".to_owned(),
            "select_menu" => "desktop.select_menu".to_owned(),
            "key" | "press_shortcut" => "desktop.press_shortcut".to_owned(),
            "read" | "read_text" => "desktop.read_text".to_owned(),
            "extract_field" => "desktop.extract_field".to_owned(),
            "assert_text" => "desktop.assert_text".to_owned(),
            other => format!("desktop.{other}"),
        };
    }
    if json_string_value(line, "target_kind").as_deref() == Some("java") {
        return match action {
            "find_window" => "java.find_window".to_owned(),
            "find_component" => "java.find_component".to_owned(),
            "click" => "java.click".to_owned(),
            "fill" | "type_text" => "java.type_text".to_owned(),
            "select" => "java.select".to_owned(),
            "read" | "read_text" => "java.read_text".to_owned(),
            "assert_text" => "java.assert_text".to_owned(),
            other => format!("java.{other}"),
        };
    }
    if json_string_value(line, "target_kind").as_deref() == Some("terminal") {
        return match action {
            "connect" => "terminal.connect".to_owned(),
            "type" | "fill" | "send_text" => "terminal.send_text".to_owned(),
            "key" | "send_keys" => "terminal.send_keys".to_owned(),
            "observe" | "terminal_screen" | "wait_for_screen" => {
                "terminal.wait_for_screen".to_owned()
            }
            "extract_field" => "terminal.extract_field".to_owned(),
            "assert_text" => "terminal.assert_text".to_owned(),
            "disconnect" => "terminal.disconnect".to_owned(),
            other => format!("terminal.{other}"),
        };
    }
    if json_string_value(line, "target_kind").as_deref() == Some("remote") {
        return match action {
            "focus_session" => "remote.focus_session".to_owned(),
            "click" | "click_region" => "remote.click_region".to_owned(),
            "fill" | "type_text" => "remote.type_text".to_owned(),
            "key" | "press_key" => "remote.press_key".to_owned(),
            "observe" | "wait_for_text" => "remote.wait_for_text".to_owned(),
            "read" | "extract_text_region" => "remote.extract_text_region".to_owned(),
            "assert_text" => "remote.assert_text".to_owned(),
            other => format!("remote.{other}"),
        };
    }
    format!("recording.{action}")
}

fn render_string_list(output: &mut String, name: &str, values: &[String]) {
    output.push_str(name);
    output.push_str(":\n");
    for value in values {
        output.push_str(&format!("  - {value}\n"));
    }
}

pub fn redact_sensitive_value(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if looks_secretish(&lower) {
        "{{secret}}".to_owned()
    } else {
        value.to_owned()
    }
}

fn looks_secretish(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("apikey")
}

fn normalize_field_name(value: impl AsRef<str>) -> String {
    let rendered = value
        .as_ref()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned();
    if rendered.is_empty() {
        "value".to_owned()
    } else {
        rendered
    }
}

fn target_label(target: &LocatorTarget) -> String {
    target
        .preferred
        .as_ref()
        .and_then(|strategy| {
            strategy
                .label
                .clone()
                .or_else(|| strategy.name.clone())
                .or_else(|| strategy.text.clone())
                .or_else(|| strategy.automation_id.clone())
        })
        .unwrap_or_default()
}

fn normalize_action(action: &RecordedAction) -> String {
    if action.event_type == "click" {
        "click stable target".to_owned()
    } else {
        format!("{} via {}", action.event_type, action.adapter)
    }
}

fn capability_prefix(adapter: &str) -> &str {
    adapter
        .strip_prefix("greentic.desktop.")
        .unwrap_or(adapter)
        .split('-')
        .next()
        .unwrap_or(adapter)
}

fn append_raw_event(
    manifest: &RecordingSessionManifest,
    line: &str,
) -> Result<(), RecordingLifecycleError> {
    if let Some(parent) = manifest.raw_events.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest.raw_events)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn write_session_index(
    runtime_home: &Path,
    session_id: &str,
    root: &Path,
) -> Result<(), RecordingLifecycleError> {
    let index = session_index_dir(runtime_home);
    fs::create_dir_all(&index)?;
    fs::write(
        index.join(format!("{session_id}.path")),
        root.display().to_string(),
    )?;
    Ok(())
}

fn session_index_dir(runtime_home: &Path) -> PathBuf {
    runtime_home.join("recording_sessions")
}

fn resolve_session_root(
    runtime_home: &Path,
    session: &str,
) -> Result<PathBuf, RecordingLifecycleError> {
    let direct = PathBuf::from(session);
    if direct.join("manifest.yaml").exists() {
        return Ok(direct);
    }
    let indexed = session_index_dir(runtime_home).join(format!("{session}.path"));
    if indexed.exists() {
        return Ok(PathBuf::from(fs::read_to_string(indexed)?.trim()));
    }
    Err(RecordingLifecycleError::InvalidSession(format!(
        "recording session {session} not found"
    )))
}

fn parse_manifest(
    path: &Path,
    root: PathBuf,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    let contents = fs::read_to_string(path)?;
    let session_id = yaml_value(&contents, "session_id").ok_or_else(|| {
        RecordingLifecycleError::InvalidSession("manifest missing session_id".to_owned())
    })?;
    let name = yaml_value(&contents, "name").unwrap_or_else(|| session_id.clone());
    let profile = yaml_value(&contents, "profile").unwrap_or_else(|| "default".to_owned());
    let state = yaml_value(&contents, "state")
        .and_then(|value| RecordingSessionState::parse(&value))
        .unwrap_or(RecordingSessionState::Failed);
    let started_at = yaml_value(&contents, "started_at")
        .map(|value| value.trim_matches('"').to_owned())
        .unwrap_or_default();
    let platform_os = yaml_nested_value(&contents, "os").unwrap_or_else(|| "unknown".to_owned());
    let raw_events = root.join(
        yaml_nested_value(&contents, "raw_events").unwrap_or_else(|| "raw/events.jsonl".to_owned()),
    );
    let screenshots = root.join(
        yaml_nested_value(&contents, "screenshots")
            .unwrap_or_else(|| "evidence/screenshots".to_owned()),
    );
    let normalised_steps = root.join(
        yaml_nested_value(&contents, "normalised_steps")
            .unwrap_or_else(|| "normalised/steps.yaml".to_owned()),
    );
    let draft_runner = root.join(
        yaml_nested_value(&contents, "draft_runner")
            .unwrap_or_else(|| "runner.draft.yaml".to_owned()),
    );
    let adapters = yaml_list_after(&contents, "adapters");
    let target_kind = yaml_nested_value(&contents, "target_kind")
        .map(|value| RecordingTargetKind::parse(&value))
        .unwrap_or_else(|| RecordingTargetKind::parse(&profile));
    let capture_state = yaml_nested_value(&contents, "state")
        .and_then(|value| RecordingCaptureState::parse(&value))
        .unwrap_or(match state {
            RecordingSessionState::Recording => RecordingCaptureState::Recording,
            RecordingSessionState::Paused => RecordingCaptureState::Paused,
            RecordingSessionState::Blocked => RecordingCaptureState::Blocked,
            RecordingSessionState::Completed | RecordingSessionState::Cancelled => {
                RecordingCaptureState::Stopped
            }
            RecordingSessionState::Failed => RecordingCaptureState::Failed,
            _ => RecordingCaptureState::Starting,
        });
    let capture_backend = yaml_nested_value(&contents, "backend").filter(|value| !value.is_empty());
    let capture_heartbeat_at =
        yaml_nested_value(&contents, "heartbeat_at").filter(|value| !value.is_empty());
    let capture_blocked_reasons = yaml_list_after(&contents, "blocked_reasons");
    let observations = yaml_nested_value(&contents, "observations")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let screenshot_count = yaml_nested_value(&contents, "screenshots")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let last_event_summary =
        yaml_nested_value(&contents, "last_event_summary").filter(|value| !value.is_empty());
    Ok(RecordingSessionManifest {
        session_id,
        name,
        profile,
        state,
        started_at,
        adapters,
        platform_os,
        root,
        raw_events,
        screenshots,
        normalised_steps,
        draft_runner,
        redact: csv_yaml_value(&contents, "redact"),
        secret_fields: csv_yaml_value(&contents, "secret_fields"),
        target_kind,
        capture_state,
        capture_backend,
        capture_heartbeat_at,
        capture_blocked_reasons,
        observations,
        screenshot_count,
        last_event_summary,
    })
}

fn yaml_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix(&format!("{key}:"))
            .map(|value| value.trim().trim_matches('"').to_owned())
    })
}

fn yaml_nested_value(contents: &str, key: &str) -> Option<String> {
    yaml_value(contents, key)
}

fn yaml_list_after(contents: &str, key: &str) -> Vec<String> {
    let mut found = false;
    let mut values = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == format!("{key}:") {
            found = true;
            continue;
        }
        if found {
            if let Some(value) = trimmed.strip_prefix("- ") {
                values.push(value.to_owned());
            } else if !trimmed.is_empty() && !line.starts_with(' ') {
                break;
            }
        }
    }
    values
}

fn csv_yaml_value(contents: &str, key: &str) -> Vec<String> {
    yaml_value(contents, key)
        .map(|value| {
            value
                .trim_matches(['[', ']'])
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn json_string_value(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn json_option(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|value| format!(r#""{}""#, json_escape(value)))
        .unwrap_or_else(|| "null".to_owned())
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorStrategy;

    fn event(action: &str, value: Option<&str>) -> RecordedEvent {
        RecordedEvent {
            action: action.to_owned(),
            target: LocatorTarget {
                preferred: Some(LocatorStrategy {
                    name: Some("Save".to_owned()),
                    ..LocatorStrategy::default()
                }),
                ..LocatorTarget::default()
            },
            value: value.map(str::to_owned),
        }
    }

    fn locator(name: &str) -> LocatorTarget {
        LocatorTarget {
            preferred: Some(LocatorStrategy {
                name: Some(name.to_owned()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        }
    }

    fn portable_package() -> PortableRunnerPackage {
        let mut preferred_adapter = BTreeMap::new();
        preferred_adapter.insert(
            RecordedPlatform::Windows,
            "greentic.desktop.windows.uia".to_owned(),
        );
        preferred_adapter.insert(
            RecordedPlatform::MacOS,
            "greentic.desktop.macos.ax".to_owned(),
        );
        preferred_adapter.insert(
            RecordedPlatform::LinuxX11,
            "greentic.desktop.linux.x11".to_owned(),
        );

        PortableRunnerPackage {
            package: RunnerPackage {
                id: "crm.create_customer".to_owned(),
                version: "1.0.0".to_owned(),
                mode: RecordingMode::Hybrid,
                inputs: Vec::new(),
                secrets: Vec::new(),
                steps: Vec::new(),
                assertions: Vec::new(),
                outputs: Vec::new(),
                open_questions: Vec::new(),
            },
            platforms: PortablePlatformSupport {
                supported: vec![
                    RecordedPlatform::Windows,
                    RecordedPlatform::MacOS,
                    RecordedPlatform::LinuxX11,
                ],
                preferred_adapter,
            },
            steps: vec![
                PortableStep {
                    id: "open_crm".to_owned(),
                    action: "desktop.open_app".to_owned(),
                    required_capability: "desktop.open_app".to_owned(),
                    app: Some(PlatformAppLaunch {
                        windows_executable: Some("C:\\Program Files\\CRM\\crm.exe".to_owned()),
                        macos_bundle_id: Some("com.vendor.crm".to_owned()),
                        linux_desktop_file: Some("crm.desktop".to_owned()),
                    }),
                    locators: PlatformLocators {
                        windows: Some(locator("CRM")),
                        macos: Some(locator("CRM")),
                        linux_x11: Some(locator("CRM")),
                        linux_wayland: None,
                    },
                    value: None,
                },
                PortableStep {
                    id: "save".to_owned(),
                    action: "desktop.click".to_owned(),
                    required_capability: "input.click".to_owned(),
                    app: None,
                    locators: PlatformLocators {
                        windows: Some(locator("SaveButton")),
                        macos: Some(locator("AXSave")),
                        linux_x11: Some(locator("Save")),
                        linux_wayland: None,
                    },
                    value: None,
                },
            ],
        }
    }

    #[test]
    fn captures_human_demonstration_events() {
        let mut session =
            RecordingSession::new("customer_create", RecordingMode::HumanDemonstration);
        session.capture_human_event(
            "greentic.desktop.playwright",
            event("click", None),
            Some("evidence://before.png".to_owned()),
        );
        session.normalize();

        assert_eq!(session.actions.len(), 1);
        assert_eq!(
            session.actions[0].normalized_summary,
            Some("click stable target".to_owned())
        );
    }

    #[test]
    fn merges_prompt_generated_and_recorded_steps() {
        let prompt = vec![RunnerStep {
            id: "prompt_1".to_owned(),
            action: "goto".to_owned(),
            target: LocatorTarget::default(),
            value: Some("https://example.test".to_owned()),
            required_capability: "web.goto".to_owned(),
        }];
        let recorded = vec![RunnerStep {
            id: "recorded_1".to_owned(),
            action: "click".to_owned(),
            target: LocatorTarget::default(),
            value: None,
            required_capability: "web.click".to_owned(),
        }];

        assert_eq!(merge_prompt_and_recorded_steps(prompt, recorded).len(), 2);
    }

    #[test]
    fn converts_recording_into_runner_yaml() {
        let mut session = RecordingSession::new("generic_form", RecordingMode::Hybrid);
        session.mark_input("account number", locator("Account"), None);
        session.mark_secret("access token", locator("Token"));
        session.mark_output(
            "confirmation",
            OutputExtractorCandidate::TargetText(Box::new(locator("Confirmation"))),
        );
        session.capture_human_event(
            "greentic.desktop.playwright",
            event("fill", Some("token=swordfish")),
            None,
        );

        let package = session.into_package("0.1.0");
        let yaml = package.render_yaml();

        assert!(yaml.contains("id: generic_form"));
        assert_eq!(package.inputs, vec!["inputs.account_number"]);
        assert_eq!(package.secrets, vec!["secrets.access_token"]);
        assert_eq!(package.outputs, vec!["outputs.confirmation"]);
        assert!(yaml.contains("{{secret}}"));
        assert!(!yaml.contains("customer_id"));
        assert!(!yaml.contains("secrets.password"));
    }

    #[test]
    fn recording_without_marked_output_emits_open_question() {
        let mut session = RecordingSession::new("generic_click", RecordingMode::HumanDemonstration);
        session.capture_human_event("greentic.desktop.macos.ax", event("click", None), None);

        let package = session.into_package("0.1.0");

        assert!(package.inputs.is_empty());
        assert!(package.secrets.is_empty());
        assert!(package.outputs.is_empty());
        assert_eq!(package.open_questions.len(), 1);
        assert!(package.open_questions[0].contains("Which visible value"));
    }

    #[test]
    fn recording_can_derive_output_from_read_event() {
        let mut session = RecordingSession::new("generic_read", RecordingMode::HumanDemonstration);
        session.capture_human_event(
            "greentic.desktop.java-accessibility",
            event("read_text", Some("Completed")),
            None,
        );

        let package = session.into_package("0.1.0");

        assert_eq!(package.outputs, vec!["outputs.save"]);
        assert!(package.open_questions.is_empty());
    }

    #[test]
    fn portable_runner_can_contain_os_specific_locators() {
        let package = portable_package();
        let plan = package
            .replay_plan(RecordedPlatform::MacOS)
            .expect("macOS plan should render");

        assert_eq!(plan.steps[1].target, locator("AXSave"));
        assert_eq!(plan.adapter_id, "greentic.desktop.macos.ax");
    }

    #[test]
    fn portable_runner_can_contain_logical_desktop_steps() {
        let yaml = portable_package().render_yaml();

        assert!(yaml.contains("platforms:"));
        assert!(yaml.contains("action: desktop.open_app"));
        assert!(yaml.contains("bundle_id: \"com.vendor.crm\""));
        assert!(yaml.contains("desktop_file: \"crm.desktop\""));
    }

    #[test]
    fn replay_chooses_correct_platform_adapter_at_runtime() {
        let package = portable_package();
        let windows = package
            .replay_plan(RecordedPlatform::Windows)
            .expect("windows plan");
        let linux = package
            .replay_plan(RecordedPlatform::LinuxX11)
            .expect("linux plan");

        assert_eq!(windows.adapter_id, "greentic.desktop.windows.uia");
        assert_eq!(linux.adapter_id, "greentic.desktop.linux.x11");
        assert_eq!(
            windows.steps[0].value,
            Some("windows:exec C:\\Program Files\\CRM\\crm.exe".to_owned())
        );
        assert_eq!(
            linux.steps[0].value,
            Some("linux:desktop-file crm.desktop".to_owned())
        );
    }

    #[test]
    fn unsupported_os_fails_before_execution() {
        let err = portable_package()
            .replay_plan(RecordedPlatform::LinuxWayland)
            .expect_err("unsupported platform should fail before execution");

        assert_eq!(
            err,
            PortableReplayError::UnsupportedPlatform(RecordedPlatform::LinuxWayland)
        );
    }

    #[test]
    fn evidence_shows_which_platform_path_was_used() {
        let plan = portable_package()
            .replay_plan(RecordedPlatform::LinuxX11)
            .expect("linux plan");

        assert_eq!(plan.platform, RecordedPlatform::LinuxX11);
        assert_eq!(
            plan.evidence_path,
            "evidence://crm.create_customer/linux-x11/platform-path.json"
        );
    }

    fn temp_dir(name: impl AsRef<str>) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!(
            "{}-{}-{}",
            name.as_ref(),
            std::process::id(),
            nanos
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("old temp dir should remove");
        }
        root
    }

    #[test]
    fn start_pause_resume_stop_recording_lifecycle() {
        let runtime_home = temp_dir("greentic-record-home");
        let out = temp_dir("greentic-recording");
        let started = start_recording_session(RecordingStartRequest {
            name: "crm.create_customer".to_owned(),
            profile: "local-crm".to_owned(),
            adapter: "greentic.desktop.playwright".to_owned(),
            target_kind: RecordingTargetKind::Web,
            out: out.clone(),
            runtime_home: runtime_home.clone(),
            redact: vec!["password".to_owned()],
            secret_fields: vec!["password".to_owned()],
        })
        .expect("recording starts");

        assert_eq!(started.state, RecordingSessionState::Blocked);
        assert_eq!(started.capture_state, RecordingCaptureState::Blocked);
        assert!(out.join("manifest.yaml").exists());
        assert!(out.join("raw/events.jsonl").exists());
        assert!(started.capture_blocked_reasons[0].contains("No recording backend"));
    }

    #[test]
    fn fixture_backend_writes_semantic_event_and_lifecycle_transitions() {
        let runtime_home = temp_dir("greentic-record-home-active");
        let out = temp_dir("greentic-recording-active");
        let mut registry = RecordingBackendRegistry::new();
        registry.register(FixtureRecordingBackend::ready(
            "greentic.recording.fixture",
            RecordingTargetKind::Web,
        ));
        let started = start_recording_session_with_registry(
            RecordingStartRequest {
                name: "crm.create_customer".to_owned(),
                profile: "local-crm".to_owned(),
                adapter: "greentic.desktop.playwright".to_owned(),
                target_kind: RecordingTargetKind::Web,
                out: out.clone(),
                runtime_home: runtime_home.clone(),
                redact: vec!["password".to_owned()],
                secret_fields: vec!["password".to_owned()],
            },
            &registry,
        )
        .expect("recording starts");

        assert_eq!(started.state, RecordingSessionState::Recording);
        assert_eq!(started.capture_state, RecordingCaptureState::Recording);
        assert_eq!(
            started.capture_backend.as_deref(),
            Some("greentic.recording.fixture")
        );
        let raw = fs::read_to_string(out.join("raw/events.jsonl")).expect("raw events");
        assert!(raw.contains("\"schema_version\":\"recording.event.v1\""));
        assert!(raw.contains("inputs.fixture_value"));

        let paused =
            pause_recording_session(&runtime_home, &started.session_id).expect("recording pauses");
        assert_eq!(paused.state, RecordingSessionState::Paused);
        assert_eq!(paused.capture_state, RecordingCaptureState::Paused);
        let resumed = resume_recording_session(&runtime_home, &started.session_id)
            .expect("recording resumes");
        assert_eq!(resumed.state, RecordingSessionState::Recording);
        assert_eq!(resumed.capture_state, RecordingCaptureState::Recording);
        let stopped =
            stop_recording_session(&runtime_home, &started.session_id).expect("recording stops");
        assert_eq!(stopped.state, RecordingSessionState::Completed);
        assert_eq!(stopped.capture_state, RecordingCaptureState::Stopped);
    }

    #[test]
    fn cancel_lifecycle_and_list_sessions() {
        let runtime_home = temp_dir("greentic-record-home-list");
        let out = temp_dir("greentic-recording-list");
        let started = start_recording_session(RecordingStartRequest {
            name: "crm.cancel".to_owned(),
            profile: "local".to_owned(),
            adapter: "greentic.desktop.vision".to_owned(),
            target_kind: RecordingTargetKind::Desktop,
            out,
            runtime_home: runtime_home.clone(),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        })
        .expect("recording starts");

        let cancelled =
            cancel_recording_session(&runtime_home, &started.session_id).expect("cancel");
        assert_eq!(cancelled.state, RecordingSessionState::Cancelled);
        assert_eq!(
            list_recording_sessions(&runtime_home).expect("list").len(),
            1
        );
    }

    #[test]
    fn normalise_raw_events_into_draft_runner_yaml_and_redacts_secrets() {
        let root = temp_dir("greentic-normalise");
        let raw = root.join("raw");
        fs::create_dir_all(&raw).expect("raw dir");
        fs::write(
            raw.join("events.jsonl"),
            "{\"type\":\"click\",\"value\":\"password=swordfish\"}\n",
        )
        .expect("raw write");
        let out = root.join("runner.draft.yaml");

        let package = normalise_recording(&raw, &out).expect("normalise");
        let yaml = fs::read_to_string(out).expect("yaml");

        assert_eq!(package.mode, RecordingMode::HumanDemonstration);
        assert!(yaml.contains("{{secret}}"));
        assert_eq!(package.secrets, vec!["secrets.recorded_secret"]);
        assert!(package.inputs.is_empty());
        assert!(!yaml.contains("customer_id"));
        assert!(!yaml.contains("secrets.password"));
    }

    #[test]
    fn normalise_web_v1_events_into_web_runner_steps() {
        let root = temp_dir("greentic-normalise-web");
        let raw = root.join("raw");
        fs::create_dir_all(&raw).expect("raw dir");
        fs::write(
            raw.join("events.jsonl"),
            "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"navigate\",\"target\":{},\"value\":\"https://example.test\",\"redaction\":\"none\"},\"evidence\":{}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"input\",\"target\":{},\"value\":\"{{inputs.number_1}}\",\"redaction\":\"input_candidate\"},\"evidence\":{}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"click\",\"target\":{},\"value\":null,\"redaction\":\"none\"},\"evidence\":{}}\n",
        )
        .expect("events should write");

        let package = normalise_recording(&raw, &root.join("runner.yaml")).expect("normalise");

        assert_eq!(package.steps[0].action, "goto");
        assert_eq!(package.steps[0].required_capability, "web.goto");
        assert_eq!(package.steps[1].action, "fill");
        assert_eq!(package.steps[1].required_capability, "web.fill");
        assert_eq!(package.steps[2].required_capability, "web.click");
        assert_eq!(package.inputs, vec!["inputs.number_1"]);
    }

    #[test]
    fn normalise_rejects_lifecycle_and_backend_warning_only_captures() {
        let root = temp_dir("greentic-normalise-non-semantic");
        let raw = root.join("raw");
        fs::create_dir_all(&raw).expect("raw dir");
        fs::write(
            raw.join("events.jsonl"),
            concat!(
                r#"{"type":"session_started","timestamp":"1","adapter":"greentic.desktop.playwright","capture_state":"recording"}"#,
                "\n",
                r#"{"schema_version":"recording.event.v1","session_id":"rec","backend":"greentic.recording.fixture","target_kind":"web","timestamp":"2","sequence":0,"event":{"kind":"backend_warning","target":{},"value":"source unavailable","redaction":"none"},"evidence":{"screenshot_ref":null,"dom_snapshot_ref":null,"ui_tree_ref":null,"terminal_buffer_ref":null}}"#,
                "\n"
            ),
        )
        .expect("events should write");

        let err = normalise_recording(&raw, &root.join("runner.yaml"))
            .expect_err("non-semantic capture should not normalise");
        assert!(err.to_string().contains("no semantic events"), "{err}");
    }

    #[test]
    fn normalise_all_recording_targets_into_semantic_capabilities() {
        let cases = [
            (
                "desktop",
                "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"desktop\",\"event\":{\"kind\":\"type_text\",\"target\":{},\"value\":\"{{inputs.amount}}\",\"redaction\":\"input_candidate\"},\"evidence\":{}}\n",
                "macos.type_text",
            ),
            (
                "java",
                "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"java\",\"event\":{\"kind\":\"type_text\",\"target\":{},\"value\":\"{{inputs.number_1}}\",\"redaction\":\"input_candidate\"},\"evidence\":{}}\n",
                "java.type_text",
            ),
            (
                "terminal",
                "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"terminal\",\"event\":{\"kind\":\"send_text\",\"target\":{},\"value\":\"{{inputs.command}}\",\"redaction\":\"input_candidate\"},\"evidence\":{}}\n",
                "terminal.type_text",
            ),
            (
                "remote",
                "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"remote\",\"event\":{\"kind\":\"click_region\",\"target\":{},\"value\":null,\"redaction\":\"none\"},\"evidence\":{\"screenshot_ref\":\"evidence://remote/1.png\"}}\n",
                "remote.click_region",
            ),
        ];

        for (name, jsonl, expected_capability) in cases {
            let root = temp_dir(format!("greentic-normalise-{name}"));
            let raw = root.join("raw");
            fs::create_dir_all(&raw).expect("raw dir");
            fs::write(raw.join("events.jsonl"), jsonl).expect("events");

            let package = normalise_recording(&raw, &root.join("runner.yaml")).expect(name);

            assert!(
                package
                    .steps
                    .iter()
                    .any(|step| step.required_capability == expected_capability),
                "{name} should include {expected_capability}: {:?}",
                package.steps
            );
        }
    }

    #[test]
    fn finalise_copies_runner_into_recording_folder() {
        let root = temp_dir("greentic-finalise");
        fs::create_dir_all(&root).expect("root");
        let runner = root.join("source.yaml");
        fs::write(&runner, "id: crm.create_customer\n").expect("runner");

        let copied = finalise_recording(&root, &runner).expect("finalise");
        assert_eq!(
            fs::read_to_string(copied).expect("copied"),
            "id: crm.create_customer\n"
        );
    }
}
