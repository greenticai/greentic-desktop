use greentic_desktop_adapter::{LocatorTarget, RecordedEvent, RunnerStep};
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
}

impl RecordingSession {
    pub fn new(id: impl Into<String>, mode: RecordingMode) -> Self {
        Self {
            id: id.into(),
            mode,
            actions: Vec::new(),
            prompt_steps: Vec::new(),
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

        RunnerPackage {
            id: self.id,
            version: version.into(),
            mode: self.mode,
            inputs: vec!["inputs.customer_id".to_owned()],
            secrets: vec!["secrets.password".to_owned()],
            steps,
            assertions: Vec::new(),
            outputs: Vec::new(),
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
    append_raw_event(
        &manifest,
        &format!(
            "{{\"type\":\"session_started\",\"timestamp\":\"{}\",\"adapter\":\"{}\"}}",
            unix_timestamp(),
            json_escape(&request.adapter)
        ),
    )?;
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
    let mut steps = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        if line.contains("session_") || line.trim().is_empty() {
            continue;
        }
        let action = json_string_value(line, "type").unwrap_or_else(|| "recorded".to_owned());
        let value = json_string_value(line, "value").map(|value| redact_sensitive_value(&value));
        steps.push(RunnerStep {
            id: format!("recorded_{}", index + 1),
            action: action.clone(),
            target: LocatorTarget::default(),
            value,
            required_capability: format!("recording.{action}"),
        });
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
        inputs: vec!["inputs.recorded_value".to_owned()],
        secrets: vec!["secrets.password".to_owned()],
        steps,
        assertions: vec!["recording completed".to_owned()],
        outputs: Vec::new(),
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
            "id: {}\nversion: {}\nmode: {:?}\nsteps:\n",
            self.id, self.version, self.mode
        );

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

pub fn redact_sensitive_value(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("password=") || lower.contains("token=") || lower.contains("secret=") {
        "{{secret}}".to_owned()
    } else {
        value.to_owned()
    }
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

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
        let mut session = RecordingSession::new("customer_create", RecordingMode::Hybrid);
        session.capture_human_event(
            "greentic.desktop.playwright",
            event("fill", Some("password=swordfish")),
            None,
        );

        let package = session.into_package("0.1.0");
        let yaml = package.render_yaml();

        assert!(yaml.contains("id: customer_create"));
        assert!(yaml.contains("{{secret}}"));
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
        let root = std::env::temp_dir().join(format!("{name}-{}", unix_timestamp()));
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

        let paused =
            pause_recording_session(&runtime_home, &started.session_id).expect("recording pauses");
        assert_eq!(paused.state, RecordingSessionState::Paused);
        let resumed = resume_recording_session(&runtime_home, &started.session_id)
            .expect("recording resumes");
        assert_eq!(resumed.state, RecordingSessionState::Recording);
        let stopped =
            stop_recording_session(&runtime_home, &started.session_id).expect("recording stops");
        assert_eq!(stopped.state, RecordingSessionState::Completed);
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
