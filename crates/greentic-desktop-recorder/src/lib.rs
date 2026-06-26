use greentic_desktop_adapter::{LocatorTarget, RecordedEvent, RunnerStep};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
    Stopping,
    Normalising,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingCaptureState {
    Inactive,
    Starting,
    Active,
    Paused,
    Blocked,
    Failed,
    Stopped,
}

impl RecordingCaptureState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Starting => "starting",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub capture_state: RecordingCaptureState,
    pub backend_id: String,
    pub event_count: usize,
    pub lifecycle_events: usize,
    pub screenshot_count: usize,
    pub last_event_at: Option<u64>,
    pub blocked_reasons: Vec<String>,
}

impl RecordingStatus {
    pub fn new(
        capture_state: RecordingCaptureState,
        backend_id: impl Into<String>,
        blocked_reasons: Vec<String>,
    ) -> Self {
        Self {
            capture_state,
            backend_id: backend_id.into(),
            event_count: 0,
            lifecycle_events: 0,
            screenshot_count: 0,
            last_event_at: None,
            blocked_reasons,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingProbe {
    pub capture_state: RecordingCaptureState,
    pub blocked_reasons: Vec<String>,
}

impl RecordingProbe {
    pub fn active() -> Self {
        Self {
            capture_state: RecordingCaptureState::Active,
            blocked_reasons: Vec::new(),
        }
    }

    pub fn blocked(reason: impl Into<String>) -> Self {
        Self {
            capture_state: RecordingCaptureState::Blocked,
            blocked_reasons: vec![reason.into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingContext {
    pub session_id: String,
    pub profile: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingStopSummary {
    pub events_drained: usize,
}

pub trait RecordingHandle: Send {
    fn pause(&mut self) -> Result<(), RecordingLifecycleError>;
    fn resume(&mut self) -> Result<(), RecordingLifecycleError>;
    fn poll(&mut self) -> Result<Vec<RawRecordingEvent>, RecordingLifecycleError>;
    fn stop(&mut self) -> Result<RecordingStopSummary, RecordingLifecycleError>;
}

pub trait RecordingBackend: Send + Sync {
    fn backend_id(&self) -> &'static str;
    fn capabilities(&self) -> Vec<String>;
    fn probe(&self) -> RecordingProbe;
    fn start(
        &self,
        ctx: RecordingContext,
    ) -> Result<Box<dyn RecordingHandle>, RecordingLifecycleError>;
}

pub struct RecordingRuntime {
    manifest: RecordingSessionManifest,
    handle: Box<dyn RecordingHandle>,
}

impl RecordingRuntime {
    pub fn start(
        manifest: RecordingSessionManifest,
        backend: &impl RecordingBackend,
    ) -> Result<Self, RecordingLifecycleError> {
        let probe = backend.probe();
        if probe.capture_state == RecordingCaptureState::Blocked {
            let mut status = refreshed_recording_status(&manifest)?;
            status.capture_state = RecordingCaptureState::Blocked;
            status.backend_id = backend.backend_id().to_owned();
            status.blocked_reasons = probe.blocked_reasons;
            write_recording_status(&manifest, &status)?;
            return Err(RecordingLifecycleError::InvalidState(
                "recording backend is blocked".to_owned(),
            ));
        }
        let ctx = RecordingContext {
            session_id: manifest.session_id.clone(),
            profile: manifest.profile.clone(),
            root: manifest.root.clone(),
        };
        let handle = backend.start(ctx)?;
        let mut status = refreshed_recording_status(&manifest)?;
        status.capture_state = RecordingCaptureState::Active;
        status.backend_id = backend.backend_id().to_owned();
        status.blocked_reasons.clear();
        write_recording_status(&manifest, &status)?;
        Ok(Self { manifest, handle })
    }

    pub fn poll_once(&mut self) -> Result<RecordingStatus, RecordingLifecycleError> {
        for event in self.handle.poll()? {
            append_recording_event(&self.manifest, &event)?;
        }
        let mut status = refreshed_recording_status(&self.manifest)?;
        status.capture_state = RecordingCaptureState::Active;
        write_recording_status(&self.manifest, &status)?;
        Ok(status)
    }

    pub fn pause(&mut self) -> Result<RecordingStatus, RecordingLifecycleError> {
        self.handle.pause()?;
        let mut status = refreshed_recording_status(&self.manifest)?;
        status.capture_state = RecordingCaptureState::Paused;
        write_recording_status(&self.manifest, &status)?;
        Ok(status)
    }

    pub fn resume(&mut self) -> Result<RecordingStatus, RecordingLifecycleError> {
        self.handle.resume()?;
        let mut status = refreshed_recording_status(&self.manifest)?;
        status.capture_state = RecordingCaptureState::Active;
        write_recording_status(&self.manifest, &status)?;
        Ok(status)
    }

    pub fn stop(mut self) -> Result<RecordingStatus, RecordingLifecycleError> {
        let _ = self.handle.stop()?;
        let mut status = refreshed_recording_status(&self.manifest)?;
        status.capture_state = RecordingCaptureState::Stopped;
        write_recording_status(&self.manifest, &status)?;
        Ok(status)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawRecordingEvent {
    SessionStarted {
        sequence: u64,
        timestamp: u64,
        adapter: String,
    },
    SessionPaused {
        sequence: u64,
        timestamp: u64,
    },
    SessionResumed {
        sequence: u64,
        timestamp: u64,
    },
    SessionStopped {
        sequence: u64,
        timestamp: u64,
    },
    SessionCancelled {
        sequence: u64,
        timestamp: u64,
    },
    AppActivated {
        sequence: u64,
        timestamp: u64,
        adapter: String,
        app: String,
    },
    WindowFocused {
        sequence: u64,
        timestamp: u64,
        adapter: String,
        title: String,
    },
    Click {
        sequence: u64,
        timestamp: u64,
        adapter: String,
        target: LocatorTarget,
        value: Option<String>,
        evidence_ref: Option<String>,
    },
    TextCommitted {
        sequence: u64,
        timestamp: u64,
        adapter: String,
        target: LocatorTarget,
        value: Option<String>,
        evidence_ref: Option<String>,
    },
    OutputObserved {
        sequence: u64,
        timestamp: u64,
        adapter: String,
        target: LocatorTarget,
        value: Option<String>,
        evidence_ref: Option<String>,
    },
    ScreenshotCaptured {
        sequence: u64,
        timestamp: u64,
        adapter: String,
        evidence_ref: String,
    },
    Marker {
        sequence: u64,
        timestamp: u64,
        marker_type: String,
        value: String,
    },
    Error {
        sequence: u64,
        timestamp: u64,
        backend_id: String,
        message: String,
    },
}

impl RawRecordingEvent {
    pub fn timestamp(&self) -> u64 {
        match self {
            Self::SessionStarted { timestamp, .. }
            | Self::SessionPaused { timestamp, .. }
            | Self::SessionResumed { timestamp, .. }
            | Self::SessionStopped { timestamp, .. }
            | Self::SessionCancelled { timestamp, .. }
            | Self::AppActivated { timestamp, .. }
            | Self::WindowFocused { timestamp, .. }
            | Self::Click { timestamp, .. }
            | Self::TextCommitted { timestamp, .. }
            | Self::OutputObserved { timestamp, .. }
            | Self::ScreenshotCaptured { timestamp, .. }
            | Self::Marker { timestamp, .. }
            | Self::Error { timestamp, .. } => *timestamp,
        }
    }

    pub fn is_capture_event(&self) -> bool {
        matches!(
            self,
            Self::AppActivated { .. }
                | Self::WindowFocused { .. }
                | Self::Click { .. }
                | Self::TextCommitted { .. }
                | Self::OutputObserved { .. }
                | Self::ScreenshotCaptured { .. }
        )
    }

    pub fn is_screenshot(&self) -> bool {
        matches!(self, Self::ScreenshotCaptured { .. })
    }
}

impl RecordingSessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Recording => "recording",
            Self::Paused => "paused",
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
            "stopping" => Some(Self::Stopping),
            "normalising" => Some(Self::Normalising),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
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

    pub fn status_path(&self) -> PathBuf {
        self.root.join("capture_status.json")
    }

    pub fn render_yaml(&self) -> String {
        let adapters = self
            .adapters
            .iter()
            .map(|adapter| format!("  - {adapter}\n"))
            .collect::<String>();
        format!(
            "session_id: {}\nname: {}\nprofile: {}\nstate: {}\nstarted_at: \"{}\"\nadapters:\n{}platform:\n  os: {}\npaths:\n  raw_events: {}\n  screenshots: {}\n  normalised_steps: {}\n  draft_runner: {}\nredact: [{}]\nsecret_fields: [{}]\n",
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
            self.secret_fields.join(",")
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
    let manifest = RecordingSessionManifest {
        session_id: session_id.clone(),
        name: request.name,
        profile: request.profile,
        state: RecordingSessionState::Recording,
        started_at: unix_timestamp().to_string(),
        adapters: vec![request.adapter.clone()],
        platform_os: std::env::consts::OS.to_owned(),
        raw_events: request.out.join("raw").join("events.jsonl"),
        screenshots: request.out.join("evidence").join("screenshots"),
        normalised_steps: request.out.join("normalised").join("steps.yaml"),
        draft_runner: request.out.join("runner.draft.yaml"),
        root: request.out,
        redact: request.redact,
        secret_fields: request.secret_fields,
    };
    fs::create_dir_all(manifest.raw_events.parent().ok_or_else(|| {
        RecordingLifecycleError::InvalidSession("raw event path has no parent".to_owned())
    })?)?;
    fs::create_dir_all(&manifest.screenshots)?;
    fs::create_dir_all(manifest.normalised_steps.parent().ok_or_else(|| {
        RecordingLifecycleError::InvalidSession("normalised path has no parent".to_owned())
    })?)?;
    manifest.write()?;
    append_recording_event(
        &manifest,
        &RawRecordingEvent::SessionStarted {
            sequence: next_event_sequence(&manifest),
            timestamp: unix_timestamp(),
            adapter: request.adapter.clone(),
        },
    )?;
    let status = RecordingStatus::new(
        RecordingCaptureState::Inactive,
        request.adapter,
        vec!["native capture backend has not been attached to this session".to_owned()],
    );
    write_recording_status(&manifest, &status)?;
    write_session_index(&request.runtime_home, &session_id, &manifest.root)?;
    Ok(manifest)
}

pub fn pause_recording_session(
    runtime_home: &Path,
    session: &str,
) -> Result<RecordingSessionManifest, RecordingLifecycleError> {
    transition(
        runtime_home,
        session,
        RecordingSessionState::Paused,
        |sequence, timestamp| RawRecordingEvent::SessionPaused {
            sequence,
            timestamp,
        },
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
        |sequence, timestamp| RawRecordingEvent::SessionResumed {
            sequence,
            timestamp,
        },
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
        |sequence, timestamp| RawRecordingEvent::SessionStopped {
            sequence,
            timestamp,
        },
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
        |sequence, timestamp| RawRecordingEvent::SessionCancelled {
            sequence,
            timestamp,
        },
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
    let mut steps = Vec::new();
    let mut evidence_refs = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some((step, evidence_ref)) = normalise_event_line(line, index + 1) {
            if let Some(evidence_ref) = evidence_ref {
                evidence_refs.push(evidence_ref);
            }
            steps.push(step);
        }
    }
    if steps.is_empty() {
        steps.push(RunnerStep {
            id: "recorded_1".to_owned(),
            action: "observe".to_owned(),
            target: LocatorTarget::default(),
            value: None,
            required_capability: "vision.screenshot".to_owned(),
        });
    }
    let package = RunnerPackage {
        id: runner_id,
        version: "0.1.0-draft".to_owned(),
        mode: RecordingMode::HumanDemonstration,
        inputs: derive_raw_event_inputs(&steps),
        secrets: derive_raw_event_secrets(&steps),
        steps,
        assertions: vec!["recording completed".to_owned()],
        outputs: derive_raw_event_outputs(&contents),
        open_questions: vec![
            "Mark the reusable inputs, secrets, and outputs before publishing this recording."
                .to_owned(),
        ],
    };
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, package.render_yaml())?;
    write_evidence_manifest(&raw_events, &evidence_refs)?;
    Ok(package)
}

pub fn finalise_recording(
    recording: &Path,
    runner: &Path,
) -> Result<PathBuf, RecordingLifecycleError> {
    let out = recording.join("runner.draft.yaml");
    if runner != out {
        fs::copy(runner, &out)?;
    }
    Ok(out)
}

pub fn append_recording_note(
    runtime_home: &Path,
    session: &str,
    event_type: &str,
    value: &str,
) -> Result<(), RecordingLifecycleError> {
    let manifest = load_recording_session(runtime_home, session)?;
    append_recording_event(
        &manifest,
        &RawRecordingEvent::Marker {
            sequence: next_event_sequence(&manifest),
            timestamp: unix_timestamp(),
            marker_type: event_type.to_owned(),
            value: redact_sensitive_value(value),
        },
    )?;
    refresh_and_write_recording_status(&manifest, manifest_state_capture_state(manifest.state))?;
    Ok(())
}

fn transition(
    runtime_home: &Path,
    session: &str,
    state: RecordingSessionState,
    event: impl FnOnce(u64, u64) -> RawRecordingEvent,
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
    manifest.write()?;
    append_recording_event(
        &manifest,
        &event(next_event_sequence(&manifest), unix_timestamp()),
    )?;
    refresh_and_write_recording_status(&manifest, manifest_state_capture_state(state))?;
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

fn normalise_event_line(line: &str, index: usize) -> Option<(RunnerStep, Option<String>)> {
    if let Ok(event) = serde_json::from_str::<RawRecordingEvent>(line) {
        return typed_event_to_step(event, index);
    }

    if line.contains("session_") {
        return None;
    }
    let action = json_string_value(line, "type").unwrap_or_else(|| "recorded".to_owned());
    let value = json_string_value(line, "value").map(|value| redact_sensitive_value(&value));
    Some((
        RunnerStep {
            id: format!("recorded_{index}"),
            action: action.clone(),
            target: LocatorTarget::default(),
            value,
            required_capability: format!("recording.{action}"),
        },
        None,
    ))
}

fn typed_event_to_step(
    event: RawRecordingEvent,
    index: usize,
) -> Option<(RunnerStep, Option<String>)> {
    match event {
        RawRecordingEvent::SessionStarted { .. }
        | RawRecordingEvent::SessionPaused { .. }
        | RawRecordingEvent::SessionResumed { .. }
        | RawRecordingEvent::SessionStopped { .. }
        | RawRecordingEvent::SessionCancelled { .. }
        | RawRecordingEvent::Marker { .. }
        | RawRecordingEvent::Error { .. } => None,
        RawRecordingEvent::AppActivated { adapter, app, .. } => Some((
            RunnerStep {
                id: format!("recorded_{index}"),
                action: "activate_app".to_owned(),
                target: LocatorTarget::default(),
                value: Some(app),
                required_capability: format!("{}.activate_app", capability_prefix(&adapter)),
            },
            None,
        )),
        RawRecordingEvent::WindowFocused { adapter, title, .. } => Some((
            RunnerStep {
                id: format!("recorded_{index}"),
                action: "find_window".to_owned(),
                target: LocatorTarget::default(),
                value: Some(title),
                required_capability: format!("{}.find_window", capability_prefix(&adapter)),
            },
            None,
        )),
        RawRecordingEvent::Click {
            adapter,
            target,
            value,
            evidence_ref,
            ..
        } => Some((
            RunnerStep {
                id: format!("recorded_{index}"),
                action: "click".to_owned(),
                target,
                value: value.map(|value| redact_sensitive_value(&value)),
                required_capability: format!("{}.click", capability_prefix(&adapter)),
            },
            evidence_ref,
        )),
        RawRecordingEvent::TextCommitted {
            adapter,
            target,
            value,
            evidence_ref,
            ..
        } => Some((
            RunnerStep {
                id: format!("recorded_{index}"),
                action: "input".to_owned(),
                target,
                value: value.map(|value| redact_sensitive_value(&value)),
                required_capability: format!("{}.type_text", capability_prefix(&adapter)),
            },
            evidence_ref,
        )),
        RawRecordingEvent::OutputObserved {
            adapter,
            target,
            value,
            evidence_ref,
            ..
        } => Some((
            RunnerStep {
                id: format!("recorded_{index}"),
                action: "extract".to_owned(),
                target,
                value: value.map(|value| redact_sensitive_value(&value)),
                required_capability: format!("{}.read_text", capability_prefix(&adapter)),
            },
            evidence_ref,
        )),
        RawRecordingEvent::ScreenshotCaptured {
            adapter,
            evidence_ref,
            ..
        } => Some((
            RunnerStep {
                id: format!("recorded_{index}"),
                action: "screenshot".to_owned(),
                target: LocatorTarget::default(),
                value: Some(evidence_ref.clone()),
                required_capability: format!("{}.screenshot", capability_prefix(&adapter)),
            },
            Some(evidence_ref),
        )),
    }
}

fn write_evidence_manifest(
    raw_events: &Path,
    evidence_refs: &[String],
) -> Result<(), RecordingLifecycleError> {
    let Some(recording_root) = raw_events.parent().and_then(Path::parent) else {
        return Ok(());
    };
    let manifest = recording_root.join("evidence").join("manifest.json");
    if let Some(parent) = manifest.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::json!({
        "rawEvents": raw_events.display().to_string(),
        "evidenceRefs": evidence_refs,
        "redacted": true,
    });
    fs::write(
        manifest,
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    )?;
    Ok(())
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

fn derive_raw_event_outputs(contents: &str) -> Vec<String> {
    let mut outputs = Vec::new();
    for line in contents.lines() {
        let action = json_string_value(line, "type").unwrap_or_default();
        if matches!(
            action.as_str(),
            "copy" | "read_text" | "observe" | "output_observed" | "extract"
        ) {
            if let Some(value) = json_string_value(line, "value") {
                if !value.trim().is_empty() && !looks_secretish(&value) {
                    outputs.push(format!("outputs.{}", normalize_field_name(value)));
                }
            }
        }
    }
    outputs.sort();
    outputs.dedup();
    outputs
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
    if adapter.contains("playwright") {
        return "web";
    }
    if adapter.contains("windows") {
        return "windows";
    }
    if adapter.contains("macos") {
        return "macos";
    }
    if adapter.contains("linux") {
        return "linux";
    }
    if adapter.contains("terminal") {
        return "terminal";
    }
    if adapter.contains("vision") {
        return "vision";
    }
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

fn append_recording_event(
    manifest: &RecordingSessionManifest,
    event: &RawRecordingEvent,
) -> Result<(), RecordingLifecycleError> {
    let json = serde_json::to_string(event)
        .map_err(|err| RecordingLifecycleError::InvalidState(err.to_string()))?;
    append_raw_event(manifest, &json)
}

fn next_event_sequence(manifest: &RecordingSessionManifest) -> u64 {
    fs::read_to_string(&manifest.raw_events)
        .map(|contents| contents.lines().count() as u64 + 1)
        .unwrap_or(1)
}

pub fn load_recording_status(
    manifest: &RecordingSessionManifest,
) -> Result<RecordingStatus, RecordingLifecycleError> {
    let contents = fs::read_to_string(manifest.status_path())?;
    serde_json::from_str(&contents)
        .map_err(|err| RecordingLifecycleError::InvalidState(err.to_string()))
}

pub fn write_recording_status(
    manifest: &RecordingSessionManifest,
    status: &RecordingStatus,
) -> Result<(), RecordingLifecycleError> {
    let json = serde_json::to_string_pretty(status)
        .map_err(|err| RecordingLifecycleError::InvalidState(err.to_string()))?;
    fs::write(manifest.status_path(), json)?;
    Ok(())
}

pub fn refreshed_recording_status(
    manifest: &RecordingSessionManifest,
) -> Result<RecordingStatus, RecordingLifecycleError> {
    let mut status = load_recording_status(manifest).unwrap_or_else(|_| {
        RecordingStatus::new(
            manifest_state_capture_state(manifest.state),
            manifest.adapters.first().cloned().unwrap_or_default(),
            Vec::new(),
        )
    });
    let mut event_count = 0usize;
    let mut lifecycle_events = 0usize;
    let mut screenshot_count = 0usize;
    let mut last_event_at = None;
    let contents = fs::read_to_string(&manifest.raw_events).unwrap_or_default();
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str::<RawRecordingEvent>(line) {
            Ok(event) => {
                if event.is_capture_event() {
                    event_count += 1;
                } else {
                    lifecycle_events += 1;
                }
                if event.is_screenshot() {
                    screenshot_count += 1;
                }
                last_event_at = Some(event.timestamp());
            }
            Err(_) => lifecycle_events += 1,
        }
    }
    status.event_count = event_count;
    status.lifecycle_events = lifecycle_events;
    status.screenshot_count = screenshot_count;
    status.last_event_at = last_event_at;
    Ok(status)
}

fn refresh_and_write_recording_status(
    manifest: &RecordingSessionManifest,
    capture_state: RecordingCaptureState,
) -> Result<(), RecordingLifecycleError> {
    let mut status = refreshed_recording_status(manifest)?;
    status.capture_state = capture_state;
    if capture_state != RecordingCaptureState::Blocked {
        status.blocked_reasons.clear();
    }
    write_recording_status(manifest, &status)
}

fn manifest_state_capture_state(state: RecordingSessionState) -> RecordingCaptureState {
    match state {
        RecordingSessionState::Starting => RecordingCaptureState::Starting,
        RecordingSessionState::Recording => RecordingCaptureState::Inactive,
        RecordingSessionState::Paused => RecordingCaptureState::Paused,
        RecordingSessionState::Stopping
        | RecordingSessionState::Normalising
        | RecordingSessionState::Completed
        | RecordingSessionState::Cancelled => RecordingCaptureState::Stopped,
        RecordingSessionState::Failed => RecordingCaptureState::Failed,
    }
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

    struct FakeBackend {
        probe: RecordingProbe,
        events: Vec<RawRecordingEvent>,
    }

    impl RecordingBackend for FakeBackend {
        fn backend_id(&self) -> &'static str {
            "fake.recorder"
        }

        fn capabilities(&self) -> Vec<String> {
            vec!["recording.fake".to_owned()]
        }

        fn probe(&self) -> RecordingProbe {
            self.probe.clone()
        }

        fn start(
            &self,
            _ctx: RecordingContext,
        ) -> Result<Box<dyn RecordingHandle>, RecordingLifecycleError> {
            Ok(Box::new(FakeHandle {
                events: self.events.clone(),
                paused: false,
            }))
        }
    }

    struct FakeHandle {
        events: Vec<RawRecordingEvent>,
        paused: bool,
    }

    impl RecordingHandle for FakeHandle {
        fn pause(&mut self) -> Result<(), RecordingLifecycleError> {
            self.paused = true;
            Ok(())
        }

        fn resume(&mut self) -> Result<(), RecordingLifecycleError> {
            self.paused = false;
            Ok(())
        }

        fn poll(&mut self) -> Result<Vec<RawRecordingEvent>, RecordingLifecycleError> {
            if self.paused {
                return Ok(Vec::new());
            }
            Ok(std::mem::take(&mut self.events))
        }

        fn stop(&mut self) -> Result<RecordingStopSummary, RecordingLifecycleError> {
            Ok(RecordingStopSummary {
                events_drained: self.events.len(),
            })
        }
    }

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

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!("{name}-{}-{}", std::process::id(), nanos));
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
            out: out.clone(),
            runtime_home: runtime_home.clone(),
            redact: vec!["password".to_owned()],
            secret_fields: vec!["password".to_owned()],
        })
        .expect("recording starts");

        assert_eq!(started.state, RecordingSessionState::Recording);
        assert!(out.join("manifest.yaml").exists());
        assert!(out.join("raw/events.jsonl").exists());
        let status = load_recording_status(&started).expect("status");
        assert_eq!(status.capture_state, RecordingCaptureState::Inactive);
        assert_eq!(status.event_count, 0);

        let paused =
            pause_recording_session(&runtime_home, &started.session_id).expect("recording pauses");
        assert_eq!(paused.state, RecordingSessionState::Paused);
        let resumed = resume_recording_session(&runtime_home, &started.session_id)
            .expect("recording resumes");
        assert_eq!(resumed.state, RecordingSessionState::Recording);
        let stopped =
            stop_recording_session(&runtime_home, &started.session_id).expect("recording stops");
        assert_eq!(stopped.state, RecordingSessionState::Completed);
        let status = load_recording_status(&stopped).expect("status");
        assert_eq!(status.capture_state, RecordingCaptureState::Stopped);
    }

    #[test]
    fn runtime_drains_backend_events_and_updates_status() {
        let runtime_home = temp_dir("greentic-record-runtime-home");
        let out = temp_dir("greentic-record-runtime");
        let started = start_recording_session(RecordingStartRequest {
            name: "desktop.note".to_owned(),
            profile: "desktop".to_owned(),
            adapter: "fake.recorder".to_owned(),
            out,
            runtime_home,
            redact: Vec::new(),
            secret_fields: Vec::new(),
        })
        .expect("recording starts");

        let backend = FakeBackend {
            probe: RecordingProbe::active(),
            events: vec![
                RawRecordingEvent::AppActivated {
                    sequence: 2,
                    timestamp: 100,
                    adapter: "fake.recorder".to_owned(),
                    app: "Notes".to_owned(),
                },
                RawRecordingEvent::ScreenshotCaptured {
                    sequence: 3,
                    timestamp: 101,
                    adapter: "fake.recorder".to_owned(),
                    evidence_ref: "evidence://shot.png".to_owned(),
                },
            ],
        };

        let mut runtime = RecordingRuntime::start(started.clone(), &backend).expect("runtime");
        let status = runtime.poll_once().expect("poll");

        assert_eq!(status.capture_state, RecordingCaptureState::Active);
        assert_eq!(status.backend_id, "fake.recorder");
        assert_eq!(status.event_count, 2);
        assert_eq!(status.screenshot_count, 1);
        assert_eq!(status.last_event_at, Some(101));
        assert!(fs::read_to_string(&started.raw_events)
            .expect("events")
            .contains("\"type\":\"app_activated\""));
    }

    #[test]
    fn blocked_backend_writes_blocked_status() {
        let runtime_home = temp_dir("greentic-record-blocked-home");
        let out = temp_dir("greentic-record-blocked");
        let started = start_recording_session(RecordingStartRequest {
            name: "desktop.blocked".to_owned(),
            profile: "desktop".to_owned(),
            adapter: "fake.recorder".to_owned(),
            out,
            runtime_home,
            redact: Vec::new(),
            secret_fields: Vec::new(),
        })
        .expect("recording starts");
        let backend = FakeBackend {
            probe: RecordingProbe::blocked("permission missing"),
            events: Vec::new(),
        };

        let err = match RecordingRuntime::start(started.clone(), &backend) {
            Ok(_) => panic!("blocked backend should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("blocked"));
        let status = load_recording_status(&started).expect("status");
        assert_eq!(status.capture_state, RecordingCaptureState::Blocked);
        assert_eq!(status.blocked_reasons, vec!["permission missing"]);
    }

    #[test]
    fn cancel_lifecycle_and_list_sessions() {
        let runtime_home = temp_dir("greentic-record-home-list");
        let out = temp_dir("greentic-recording-list");
        let started = start_recording_session(RecordingStartRequest {
            name: "crm.cancel".to_owned(),
            profile: "local".to_owned(),
            adapter: "greentic.desktop.vision".to_owned(),
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
    fn normalise_typed_events_into_semantic_steps_and_evidence_manifest() {
        let root = temp_dir("greentic-normalise-typed");
        let raw = root.join("raw");
        fs::create_dir_all(&raw).expect("raw dir");
        let target = locator("Result");
        let events = [
            serde_json::to_string(&RawRecordingEvent::Click {
                sequence: 1,
                timestamp: 100,
                adapter: "greentic.desktop.playwright".to_owned(),
                target: locator("Submit"),
                value: None,
                evidence_ref: Some("evidence://web/click.png".to_owned()),
            })
            .expect("click json"),
            serde_json::to_string(&RawRecordingEvent::OutputObserved {
                sequence: 2,
                timestamp: 101,
                adapter: "greentic.desktop.playwright".to_owned(),
                target,
                value: Some("confirmation 123".to_owned()),
                evidence_ref: Some("evidence://web/output.png".to_owned()),
            })
            .expect("output json"),
            serde_json::to_string(&RawRecordingEvent::ScreenshotCaptured {
                sequence: 3,
                timestamp: 102,
                adapter: "greentic.desktop.playwright".to_owned(),
                evidence_ref: "evidence://web/screen.png".to_owned(),
            })
            .expect("screenshot json"),
        ];
        fs::write(raw.join("events.jsonl"), events.join("\n")).expect("raw write");
        let out = root.join("runner.draft.yaml");

        let package = normalise_recording(&raw, &out).expect("normalise");

        assert_eq!(package.steps[0].action, "click");
        assert_eq!(package.steps[0].required_capability, "web.click");
        assert_eq!(package.steps[1].action, "extract");
        assert_eq!(package.outputs, vec!["outputs.confirmation_123"]);
        let evidence = fs::read_to_string(root.join("evidence/manifest.json")).expect("evidence");
        assert!(evidence.contains("evidence://web/output.png"));
        assert!(evidence.contains("evidence://web/screen.png"));
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
