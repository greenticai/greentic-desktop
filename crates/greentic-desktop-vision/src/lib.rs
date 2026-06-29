use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    Observation, ObserveContext, RecordedEvent, RunnerStep, StepResult,
};
use greentic_desktop_automation_foundation::{ScreenshotBackend, XcapScreenshotBackend};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventEnvelope, RecordingEventSink,
    RecordingHandle, RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const VISION_ADAPTER_ID: &str = "greentic.desktop.vision";
pub const REMOTE_RECORDER_BACKEND_ID: &str = "greentic.recording.remote.vision";
const VISION_BACKEND_COMMAND_ENV: &str = "GREENTIC_VISION_BACKEND_COMMAND";
const REMOTE_VIEWPORT_PROVIDER_COMMAND_ENV: &str = "GREENTIC_REMOTE_VIEWPORT_PROVIDER_COMMAND";

pub fn vision_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        VISION_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "vision.screenshot",
            "vision.find_text",
            "vision.find_button",
            "vision.click_region",
            "vision.compare_baseline",
            "vision.assert_visual",
            "vision.extract_text",
        ],
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisionMatch {
    pub label: String,
    pub region: Region,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualEvidence {
    pub before_screenshot: String,
    pub annotated_region: Region,
    pub confidence: f32,
    pub after_screenshot: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteViewportCalibration {
    pub origin_x: u32,
    pub origin_y: u32,
    pub width: u32,
    pub height: u32,
    pub scale_percent: u32,
}

#[derive(Debug, Clone)]
pub struct RemoteVisionRecordingBackend {
    greentic_owned_session: bool,
    screen_capture_available: bool,
    input_control_available: bool,
    calibration: Option<RemoteViewportCalibration>,
    provider_command: Option<String>,
}

impl RemoteVisionRecordingBackend {
    pub fn new(
        greentic_owned_session: bool,
        screen_capture_available: bool,
        input_control_available: bool,
        calibration: Option<RemoteViewportCalibration>,
    ) -> Self {
        Self {
            greentic_owned_session,
            screen_capture_available,
            input_control_available,
            calibration,
            provider_command: std::env::var(REMOTE_VIEWPORT_PROVIDER_COMMAND_ENV)
                .ok()
                .filter(|command| !command.trim().is_empty()),
        }
    }

    pub fn with_provider_command(
        greentic_owned_session: bool,
        screen_capture_available: bool,
        input_control_available: bool,
        calibration: Option<RemoteViewportCalibration>,
        provider_command: impl Into<String>,
    ) -> Self {
        Self {
            greentic_owned_session,
            screen_capture_available,
            input_control_available,
            calibration,
            provider_command: Some(provider_command.into()),
        }
    }
}

impl RecordingBackend for RemoteVisionRecordingBackend {
    fn id(&self) -> &'static str {
        REMOTE_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Remote
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        let mut reasons = Vec::new();
        if !self.greentic_owned_session {
            reasons.push(
                "Remote recording requires a Greentic-owned RDP/VNC/WorkSpaces/browser canvas session."
                    .to_owned(),
            );
        }
        if !self.screen_capture_available {
            reasons.push("Screen capture permission is required for remote recording.".to_owned());
        }
        if !self.input_control_available {
            reasons.push(
                "Keyboard and mouse control permission is required for replayable remote recording."
                    .to_owned(),
            );
        }
        if self.calibration.is_none() {
            reasons.push(
                "Remote viewport calibration is required before recording input coordinates."
                    .to_owned(),
            );
        }
        if self.provider_command.is_none() {
            reasons.push(format!(
                "Remote viewport provider is not configured. Set {REMOTE_VIEWPORT_PROVIDER_COMMAND_ENV}."
            ));
        }

        if reasons.is_empty() {
            RecordingPreflight::ready()
        } else {
            RecordingPreflight {
                available: false,
                blocked_reasons: reasons,
            }
        }
    }

    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        if let (Some(command), Some(calibration)) =
            (self.provider_command.clone(), self.calibration)
        {
            run_remote_viewport_provider_command(command, calibration, request, sink.clone());
        } else {
            let _ = sink.append_backend_warning(
                "remote viewport provider or calibration missing; no synthetic remote events were generated",
            );
        }

        RecordingHandle {
            backend_id: REMOTE_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

fn run_remote_viewport_provider_command(
    command: String,
    calibration: RemoteViewportCalibration,
    request: RecordingStartRequest,
    sink: RecordingEventSink,
) {
    let output = shell_command(&command)
        .env("GREENTIC_RECORDING_SESSION_ID", sink.session_id())
        .env("GREENTIC_RECORDING_ROOT", request.out.display().to_string())
        .env(
            "GREENTIC_REMOTE_VIEWPORT_X",
            calibration.origin_x.to_string(),
        )
        .env(
            "GREENTIC_REMOTE_VIEWPORT_Y",
            calibration.origin_y.to_string(),
        )
        .env(
            "GREENTIC_REMOTE_VIEWPORT_WIDTH",
            calibration.width.to_string(),
        )
        .env(
            "GREENTIC_REMOTE_VIEWPORT_HEIGHT",
            calibration.height.to_string(),
        )
        .env(
            "GREENTIC_REMOTE_VIEWPORT_SCALE_PERCENT",
            calibration.scale_percent.to_string(),
        )
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        let _ = sink.append_backend_warning("failed to run remote viewport provider command");
        return;
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = sink.append_backend_warning(&format!(
            "remote viewport provider exited with status {:?}: {}",
            output.status.code(),
            stderr.trim()
        ));
        return;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for (index, line) in stdout.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event = parse_remote_provider_event(sink.session_id(), index as u64 + 1, line);
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();
    }
}

fn parse_remote_provider_event(
    session_id: &str,
    sequence: u64,
    line: &str,
) -> RecordingEventEnvelope {
    let parts: Vec<&str> = line.split(',').collect();
    let kind = parts.first().copied().unwrap_or("viewport_event");
    let region = if parts.len() >= 5 {
        Region {
            x: parts[1].trim().parse().unwrap_or_default(),
            y: parts[2].trim().parse().unwrap_or_default(),
            width: parts[3].trim().parse().unwrap_or_default(),
            height: parts[4].trim().parse().unwrap_or_default(),
        }
    } else {
        Region {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    };
    let value = parts.get(5).map(|value| value.trim().to_owned());
    let screenshot_ref = parts.get(6).map(|value| value.trim().to_owned());
    remote_recording_event(session_id, sequence, kind, region, value, screenshot_ref)
}

pub fn remote_recording_event(
    session_id: &str,
    sequence: u64,
    kind: &str,
    region: Region,
    value: Option<String>,
    screenshot_ref: Option<String>,
) -> RecordingEventEnvelope {
    let mut event = RecordingEventEnvelope::new(
        session_id,
        REMOTE_RECORDER_BACKEND_ID,
        RecordingTargetKind::Remote,
        sequence,
        kind,
    );
    event.target_json = format!(
        r#"{{"region":{{"x":{},"y":{},"width":{},"height":{}}}}}"#,
        region.x, region.y, region.width, region.height
    );
    event.value = value;
    event.redaction = if event.value.is_some() {
        "input_candidate".to_owned()
    } else {
        "none".to_owned()
    };
    event.screenshot_ref = screenshot_ref;
    event
}

#[derive(Debug, Clone)]
pub struct VisionAdapter {
    backend_command: Option<String>,
}

impl VisionAdapter {
    pub fn new() -> Self {
        Self {
            backend_command: std::env::var(VISION_BACKEND_COMMAND_ENV)
                .ok()
                .filter(|command| !command.trim().is_empty()),
        }
    }

    pub fn with_backend_command(command: impl Into<String>) -> Self {
        Self {
            backend_command: Some(command.into()),
        }
    }
}

impl Default for VisionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopAdapter for VisionAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        vision_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let output = self.run_backend(VisionBackendAction::Observe, None, None)?;
        Ok(Observation {
            adapter_id: VISION_ADAPTER_ID.to_owned(),
            summary: output.message.unwrap_or_else(|| {
                format!(
                    "vision session {} observed through configured backend",
                    ctx.session_id
                )
            }),
            visible_text: output
                .matches
                .iter()
                .map(|item| item.label.clone())
                .chain(output.visible_text)
                .collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        if step.required_capability == "vision.screenshot" {
            let path = step
                .value
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(default_vision_screenshot_path);
            capture_vision_screenshot(&path)?;
            return Ok(StepResult {
                step_id: step.id,
                success: true,
                message: path.display().to_string(),
            });
        }

        let output = self.run_backend(VisionBackendAction::Execute, Some(&step), None)?;

        Ok(StepResult {
            step_id: step.id,
            success: true,
            message: output
                .message
                .or_else(|| output.evidence.map(|evidence| evidence.explanation))
                .unwrap_or_else(|| "vision step completed by configured backend".to_owned()),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let output = self.run_backend(VisionBackendAction::Validate, None, Some(&assertion))?;
        let passed = output.passed.unwrap_or_else(|| {
            output
                .matches
                .iter()
                .any(|item| item.label.contains(&assertion.expected) && item.confidence >= 0.70)
                || output
                    .visible_text
                    .iter()
                    .any(|text| text.contains(&assertion.expected))
        });

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: output
                .evidence
                .map(|evidence| evidence.explanation)
                .or(output.message)
                .unwrap_or_else(|| {
                    if passed {
                        "vision assertion passed".to_owned()
                    } else {
                        "vision assertion failed".to_owned()
                    }
                }),
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(None)
    }
}

fn capture_vision_screenshot(path: &Path) -> AdapterResult<()> {
    XcapScreenshotBackend
        .capture_primary_monitor(path)
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("xcap screenshot capture failed: {err}"))
        })?;
    if path.exists() {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "vision screenshot backend did not create {}",
            path.display()
        )))
    }
}

fn default_vision_screenshot_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "greentic-vision-screenshot-{}-{}.png",
        std::process::id(),
        epoch_millis()
    ))
}

fn epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

impl VisionAdapter {
    fn run_backend(
        &self,
        action: VisionBackendAction,
        step: Option<&RunnerStep>,
        assertion: Option<&Assertion>,
    ) -> AdapterResult<VisionBackendOutput> {
        let Some(command) = &self.backend_command else {
            return Err(AdapterError::ExecutionFailed(format!(
                "No vision backend is configured. Set {VISION_BACKEND_COMMAND_ENV} to a screenshot/OCR/input backend command."
            )));
        };
        run_vision_backend_command(command, action, step, assertion)
    }
}

#[derive(Debug, Clone, Copy)]
enum VisionBackendAction {
    Execute,
    Observe,
    Validate,
}

impl VisionBackendAction {
    fn as_str(self) -> &'static str {
        match self {
            VisionBackendAction::Execute => "execute",
            VisionBackendAction::Observe => "observe",
            VisionBackendAction::Validate => "validate",
        }
    }
}

#[derive(Debug, Default)]
struct VisionBackendOutput {
    visible_text: Vec<String>,
    matches: Vec<VisionMatch>,
    evidence: Option<VisualEvidence>,
    message: Option<String>,
    passed: Option<bool>,
}

fn run_vision_backend_command(
    backend_command: &str,
    action: VisionBackendAction,
    step: Option<&RunnerStep>,
    assertion: Option<&Assertion>,
) -> AdapterResult<VisionBackendOutput> {
    let mut command = shell_command(backend_command);
    command
        .env("GREENTIC_VISION_ACTION", action.as_str())
        .env("GREENTIC_VISION_ADAPTER_ID", VISION_ADAPTER_ID)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(step) = step {
        command
            .env("GREENTIC_VISION_STEP_ID", &step.id)
            .env("GREENTIC_VISION_CAPABILITY", &step.required_capability)
            .env("GREENTIC_VISION_STEP_ACTION", &step.action)
            .env("GREENTIC_VISION_VALUE", step.value.as_deref().unwrap_or(""));
    }
    if let Some(assertion) = assertion {
        command
            .env("GREENTIC_VISION_ASSERTION_ID", &assertion.id)
            .env("GREENTIC_VISION_CAPABILITY", &assertion.required_capability)
            .env("GREENTIC_VISION_EXPECTED", &assertion.expected);
    }
    let mut child = command.spawn().map_err(|err| {
        AdapterError::ExecutionFailed(format!("vision backend failed to start: {err}"))
    })?;
    if let Some(stdin) = child.stdin.as_mut() {
        let payload = vision_backend_payload(step, assertion);
        stdin.write_all(payload.as_bytes()).map_err(|err| {
            AdapterError::ExecutionFailed(format!("vision backend stdin failed: {err}"))
        })?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| AdapterError::ExecutionFailed(format!("vision backend failed: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AdapterError::ExecutionFailed(format!(
            "vision backend action {} failed with status {:?}: {}",
            action.as_str(),
            output.status.code(),
            stderr.trim()
        )));
    }
    parse_vision_backend_output(&String::from_utf8_lossy(&output.stdout))
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

fn vision_backend_payload(step: Option<&RunnerStep>, assertion: Option<&Assertion>) -> String {
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

fn parse_vision_backend_output(stdout: &str) -> AdapterResult<VisionBackendOutput> {
    let mut output = VisionBackendOutput::default();
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix("message:") {
            output.message = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("text:") {
            output.visible_text.push(value.to_owned());
        } else if let Some(value) = line.strip_prefix("match:") {
            output.matches.push(parse_vision_match(value)?);
        } else if let Some(value) = line.strip_prefix("evidence:") {
            output.evidence = Some(parse_visual_evidence(value)?);
        } else if let Some(value) = line.strip_prefix("passed:") {
            output.passed = Some(value.trim().eq_ignore_ascii_case("true"));
        } else if !line.trim().is_empty() {
            output.visible_text.push(line.to_owned());
        }
    }
    Ok(output)
}

fn parse_vision_match(value: &str) -> AdapterResult<VisionMatch> {
    let parts: Vec<&str> = delimited_parts(value);
    if parts.len() != 6 {
        return Err(AdapterError::ExecutionFailed(format!(
            "vision backend emitted invalid match record: {value}"
        )));
    }
    Ok(VisionMatch {
        label: parts[0].to_owned(),
        region: Region {
            x: parse_u32(parts[1], "match x")?,
            y: parse_u32(parts[2], "match y")?,
            width: parse_u32(parts[3], "match width")?,
            height: parse_u32(parts[4], "match height")?,
        },
        confidence: parse_f32(parts[5], "match confidence")?,
    })
}

fn parse_visual_evidence(value: &str) -> AdapterResult<VisualEvidence> {
    let parts: Vec<&str> = delimited_parts(value);
    if parts.len() != 8 {
        return Err(AdapterError::ExecutionFailed(format!(
            "vision backend emitted invalid evidence record: {value}"
        )));
    }
    Ok(VisualEvidence {
        before_screenshot: parts[0].to_owned(),
        annotated_region: Region {
            x: parse_u32(parts[1], "evidence x")?,
            y: parse_u32(parts[2], "evidence y")?,
            width: parse_u32(parts[3], "evidence width")?,
            height: parse_u32(parts[4], "evidence height")?,
        },
        confidence: parse_f32(parts[5], "evidence confidence")?,
        after_screenshot: parts[6].to_owned(),
        explanation: parts[7].to_owned(),
    })
}

fn delimited_parts(value: &str) -> Vec<&str> {
    if value.contains('|') {
        value.split('|').collect()
    } else {
        value.split(',').collect()
    }
}

fn parse_u32(value: &str, label: &str) -> AdapterResult<u32> {
    value.trim().parse::<u32>().map_err(|err| {
        AdapterError::ExecutionFailed(format!("vision backend emitted invalid {label}: {err}"))
    })
}

fn parse_f32(value: &str, label: &str) -> AdapterResult<f32> {
    value.trim().parse::<f32>().map_err(|err| {
        AdapterError::ExecutionFailed(format!("vision backend emitted invalid {label}: {err}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorTarget;
    use greentic_desktop_recorder::{
        start_recording_session_with_registry, RecordingBackendRegistry,
    };
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn exposes_vision_capabilities() {
        let capabilities = vision_capabilities();

        assert!(capabilities.supports("vision.screenshot"));
        assert!(capabilities.supports("vision.assert_visual"));
        assert_eq!(capabilities.adapter_id, VISION_ADAPTER_ID);
    }

    #[test]
    fn default_vision_screenshot_path_is_png() {
        let path = default_vision_screenshot_path();

        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("png")
        );
        assert!(path
            .to_string_lossy()
            .contains("greentic-vision-screenshot"));
    }

    #[test]
    fn parses_ocr_text_and_visual_matches_from_backend() {
        let output = parse_vision_backend_output(
            "message:captured\ntext:Submit\nmatch:Submit,20,30,80,24,0.91\npassed:true\n",
        )
        .expect("backend output should parse");

        assert_eq!(output.visible_text, vec!["Submit"]);
        assert_eq!(output.matches[0].region.x, 20);
        assert_eq!(output.matches[0].confidence, 0.91);
        assert_eq!(output.passed, Some(true));
    }

    #[test]
    fn click_region_requires_configured_backend_and_returns_evidence() {
        let adapter = VisionAdapter::with_backend_command(
            "echo message:clicked && echo evidence:before.png,20,30,80,24,0.94,after.png,clicked visually identified region",
        );
        let result = adapter
            .execute(RunnerStep {
                id: "click".to_owned(),
                action: "click_region".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "vision.click_region".to_owned(),
            })
            .expect("click should use configured backend");

        assert_eq!(result.message, "clicked");
    }

    #[test]
    fn vision_step_fails_without_backend() {
        let adapter = VisionAdapter::new();
        let err = adapter
            .execute(RunnerStep {
                id: "click".to_owned(),
                action: "click_region".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "vision.click_region".to_owned(),
            })
            .expect_err("vision should fail without backend");

        assert!(
            err.to_string().contains("No vision backend is configured"),
            "{err}"
        );
    }

    #[test]
    fn explains_visual_assertion_result_from_backend() {
        let adapter = VisionAdapter::with_backend_command(
            "echo passed:false && echo evidence:baseline.png,0,0,100,100,0.20,current.png,current screen differs from baseline",
        );
        let result = adapter
            .validate(Assertion {
                id: "baseline".to_owned(),
                required_capability: "vision.assert_visual".to_owned(),
                target: LocatorTarget::default(),
                expected: "baseline".to_owned(),
            })
            .expect("visual assertion should use configured backend");

        assert!(!result.passed);
        assert!(result.message.contains("differs"));
    }

    #[test]
    fn remote_recording_backend_blocks_missing_screen_capture_and_calibration() {
        let backend = RemoteVisionRecordingBackend::new(true, false, true, None);
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "remote.record".to_owned(),
            profile: "remote".to_owned(),
            adapter: VISION_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Remote,
            out: std::env::temp_dir().join("remote-record"),
            runtime_home: std::env::temp_dir().join("remote-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("Screen capture permission")));
        assert!(preflight
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("viewport calibration")));
        assert!(preflight
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("Remote viewport provider")));
    }

    #[test]
    fn remote_recording_uses_provider_events_instead_of_synthetic_focus() {
        let root = temp_dir("greentic-remote-provider-recording");
        let runtime_home = temp_dir("greentic-remote-provider-home");
        let mut registry = RecordingBackendRegistry::new();
        registry.register(RemoteVisionRecordingBackend::with_provider_command(
            true,
            true,
            true,
            Some(RemoteViewportCalibration {
                origin_x: 5,
                origin_y: 10,
                width: 800,
                height: 600,
                scale_percent: 100,
            }),
            "echo click_region,12,20,80,24,Submit,evidence://remote/click.png",
        ));

        let manifest = start_recording_session_with_registry(
            RecordingStartRequest {
                name: "remote.real".to_owned(),
                profile: "remote".to_owned(),
                adapter: VISION_ADAPTER_ID.to_owned(),
                target_kind: RecordingTargetKind::Remote,
                out: root.clone(),
                runtime_home,
                redact: Vec::new(),
                secret_fields: Vec::new(),
            },
            &registry,
        )
        .expect("remote recording should start");

        assert_eq!(manifest.capture_state, RecordingCaptureState::Recording);
        let raw_path = root.join("raw/events.jsonl");
        let mut raw = String::new();
        for _ in 0..20 {
            raw = fs::read_to_string(&raw_path).unwrap_or_default();
            if raw.contains("click_region") && raw.contains("evidence://remote/click.png") {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        assert!(raw.contains(r#""kind":"click_region""#), "{raw}");
        assert!(!raw.contains("focus_session"), "{raw}");
    }

    #[test]
    fn remote_recording_event_includes_region_and_screenshot_evidence() {
        let event = remote_recording_event(
            "rec_remote",
            4,
            "click_region",
            Region {
                x: 12,
                y: 20,
                width: 80,
                height: 24,
            },
            None,
            Some("evidence://remote/after-click.png".to_owned()),
        );

        let json = event.render_json();
        assert!(json.contains("\"target_kind\":\"remote\""));
        assert!(json.contains("\"x\":12"));
        assert!(json.contains("after-click.png"));
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
