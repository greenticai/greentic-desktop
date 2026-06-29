use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use greentic_desktop_automation_foundation::{ScreenshotBackend, XcapScreenshotBackend};
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo, PlatformPermission};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventSink, RecordingHandle,
    RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use greentic_desktop_workflow::{
    compile_workflow, workflow_id_component, DesktopWorkflow, NativePlatform, WorkflowAction,
    WorkflowActionKind, WorkflowEvidencePolicy, WorkflowInput, WorkflowOutput,
    WorkflowOutputExtractor, WorkflowRisk, WorkflowTarget, WorkflowValueType,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
            "macos.press_shortcut",
            "macos.invoke_menu",
            "macos.focus_document",
            "macos.save_as",
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
        if diagnostics.ready_for_ax() && macos_ax_event_source_available() {
            RecordingPreflight::ready()
        } else {
            let mut messages = diagnostics.messages;
            if !macos_ax_event_source_available() {
                messages.push(
                    "Swift or GREENTIC_MACOS_AX_EVENT_SOURCE_COMMAND is required for the macOS Accessibility event source."
                        .to_owned(),
                );
            }
            RecordingPreflight {
                available: false,
                blocked_reasons: messages,
            }
        }
    }

    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        if let Ok(command) = std::env::var("GREENTIC_MACOS_AX_EVENT_SOURCE_COMMAND") {
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
                    backend_id: MACOS_RECORDER_BACKEND_ID.to_owned(),
                    capture_state: RecordingCaptureState::Recording,
                };
            }
        }

        match start_builtin_macos_event_recorder(&request, &sink) {
            Ok(()) => {
                let _ = sink.update_heartbeat();
                RecordingHandle {
                    backend_id: MACOS_RECORDER_BACKEND_ID.to_owned(),
                    capture_state: RecordingCaptureState::Recording,
                }
            }
            Err(err) => {
                let _ = sink.append_backend_warning(&err);
                RecordingHandle {
                    backend_id: MACOS_RECORDER_BACKEND_ID.to_owned(),
                    capture_state: RecordingCaptureState::Blocked,
                }
            }
        }
    }
}

fn macos_ax_event_source_available() -> bool {
    std::env::var("GREENTIC_MACOS_AX_EVENT_SOURCE_COMMAND")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        || find_swift_command().is_some()
}

fn find_swift_command() -> Option<&'static str> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join("swift");
        candidate.is_file().then_some("swift")
    })
}

fn start_builtin_macos_event_recorder(
    request: &RecordingStartRequest,
    sink: &RecordingEventSink,
) -> Result<(), String> {
    find_swift_command().ok_or_else(|| {
        "Swift is not available to run the built-in macOS Accessibility event source.".to_owned()
    })?;
    std::fs::create_dir_all(request.out.join("logs")).map_err(|err| err.to_string())?;
    std::fs::create_dir_all(request.out.join("raw")).map_err(|err| err.to_string())?;
    let script = request.out.join("macos-event-recorder.swift");
    std::fs::write(&script, macos_event_recorder_swift()).map_err(|err| err.to_string())?;
    let log_path = request.out.join("logs").join("macos-event-recorder.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| err.to_string())?;
    let err = log.try_clone().map_err(|err| err.to_string())?;
    Command::new("swift")
        .arg(&script)
        .arg(&request.out)
        .arg(sink.session_id())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("failed to start macOS event recorder: {err}"))
}

fn macos_event_recorder_swift() -> &'static str {
    r#"
import ApplicationServices
import Foundation

let args = CommandLine.arguments
guard args.count >= 3 else {
  fputs("usage: macos-event-recorder.swift <session-root> <session-id>\n", stderr)
  exit(2)
}

let root = URL(fileURLWithPath: args[1])
let sessionId = args[2]
let raw = root.appendingPathComponent("raw/events.jsonl")
try? FileManager.default.createDirectory(at: raw.deletingLastPathComponent(), withIntermediateDirectories: true)
FileManager.default.createFile(atPath: raw.path, contents: nil)
let handle = try FileHandle(forWritingTo: raw)
handle.seekToEndOfFile()
var sequence: UInt64 = 1

func jsonEscape(_ value: String) -> String {
  var out = ""
  for scalar in value.unicodeScalars {
    switch scalar {
    case "\"": out += "\\\""
    case "\\": out += "\\\\"
    case "\n": out += "\\n"
    case "\r": out += "\\r"
    case "\t": out += "\\t"
    default: out.unicodeScalars.append(scalar)
    }
  }
  return out
}

func append(kind: String, value: String, x: Int64? = nil, y: Int64? = nil) {
  let target = x == nil || y == nil
    ? "{\"platform\":\"macos\",\"api\":\"CGEventTap\"}"
    : "{\"platform\":\"macos\",\"api\":\"CGEventTap\",\"x\":\(x!),\"y\":\(y!)}"
  let line = "{\"schema_version\":\"recording.event.v1\",\"session_id\":\"\(jsonEscape(sessionId))\",\"backend\":\"greentic.recording.desktop.macos.ax\",\"target_kind\":\"desktop\",\"timestamp\":\"\(Int(Date().timeIntervalSince1970))\",\"sequence\":\(sequence),\"event\":{\"kind\":\"\(jsonEscape(kind))\",\"target\":\(target),\"value\":\"\(jsonEscape(value))\",\"redaction\":\"none\"},\"evidence\":{\"screenshot_ref\":null,\"dom_snapshot_ref\":null,\"ui_tree_ref\":null,\"terminal_buffer_ref\":null}}\n"
  sequence += 1
  if let data = line.data(using: .utf8) {
    handle.write(data)
  }
}

let mask =
  (1 << CGEventType.leftMouseDown.rawValue) |
  (1 << CGEventType.rightMouseDown.rawValue) |
  (1 << CGEventType.keyDown.rawValue)

let callback: CGEventTapCallBack = { _, type, event, _ in
  if type == .leftMouseDown || type == .rightMouseDown {
    let point = event.location
    append(kind: "click", value: type == .leftMouseDown ? "left" : "right", x: Int64(point.x), y: Int64(point.y))
  } else if type == .keyDown {
    let code = event.getIntegerValueField(.keyboardEventKeycode)
    append(kind: "key", value: String(code))
  }
  return Unmanaged.passUnretained(event)
}

guard let tap = CGEvent.tapCreate(
  tap: .cgSessionEventTap,
  place: .headInsertEventTap,
  options: .listenOnly,
  eventsOfInterest: CGEventMask(mask),
  callback: callback,
  userInfo: nil
) else {
  fputs("failed to create CGEvent tap; grant Accessibility/Input Monitoring to this launcher\n", stderr)
  exit(1)
}

let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .commonModes)
CGEvent.tapEnable(tap: tap, enable: true)
append(kind: "backend_started", value: "macos CGEvent tap started")
CFRunLoopRun()
"#
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
    let is_macos = info.os == DesktopPlatform::MacOS;
    let accessibility_granted = is_macos && info.has_permission(PlatformPermission::Accessibility);
    let screen_recording_granted = is_macos
        && (info.has_permission(PlatformPermission::ScreenRecording)
            || info.has_permission(PlatformPermission::Screenshot));
    let input_monitoring_granted = is_macos
        && info.has_permission(PlatformPermission::KeyboardInput)
        && info.has_permission(PlatformPermission::MouseInput);
    let mut messages = Vec::new();
    if !is_macos {
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
    recorded: Vec<RecordedEvent>,
}

impl MacOsAccessibilityAdapter {
    pub fn new(platform: PlatformInfo) -> Self {
        Self {
            platform,
            state: Arc::new(Mutex::new(MacOsState::default())),
        }
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

    fn active_app_or_frontmost(&self) -> AdapterResult<String> {
        if let Some(app) = self
            .state
            .lock()
            .expect("macos adapter mutex poisoned")
            .active_app
            .clone()
        {
            return Ok(app);
        }
        run_osascript(frontmost_app_script())
            .map(|output| output.trim().to_owned())
            .and_then(|app| {
                if app.is_empty() {
                    Err(AdapterError::ExecutionFailed(
                        "No frontmost macOS application was reported by System Events.".to_owned(),
                    ))
                } else {
                    Ok(app)
                }
            })
    }

    fn execute_real_step(&self, step: &RunnerStep) -> AdapterResult<String> {
        match step.required_capability.as_str() {
            "macos.find_app" | "macos.activate_app" => {
                let app = step.value.as_deref().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "macos.activate_app requires an application name in step.value.".to_owned(),
                    )
                })?;
                activate_macos_app(app)?;
                self.state
                    .lock()
                    .expect("macos adapter mutex poisoned")
                    .active_app = Some(app.trim_end_matches(".app").to_owned());
                Ok(format!("activated macOS app {app}"))
            }
            "macos.find_window" => {
                let app = self.active_app_or_frontmost()?;
                let expected = step.value.as_deref().unwrap_or_default();
                if macos_window_exists(&app, expected)? {
                    Ok(format!("found macOS window containing {expected}"))
                } else {
                    Err(AdapterError::ExecutionFailed(format!(
                        "No window containing {expected} was visible for {app}."
                    )))
                }
            }
            "macos.read_window_tree" => {
                let app = self.active_app_or_frontmost()?;
                let text = macos_read_process_text(&app)?;
                Ok(format!(
                    "read {} macOS accessibility text entries",
                    text.len()
                ))
            }
            "macos.find_element" | "macos.assert_visible" => {
                let app = self.active_app_or_frontmost()?;
                if macos_element_exists(&app, &step.target, step.value.as_deref())? {
                    Ok("found macOS accessibility element".to_owned())
                } else {
                    Err(AdapterError::ExecutionFailed(
                        "No matching macOS accessibility element was visible.".to_owned(),
                    ))
                }
            }
            "macos.type_text" => {
                let app = self.active_app_or_frontmost()?;
                let value = step.value.as_deref().unwrap_or_default();
                macos_type_text(&app, &step.target, value)?;
                Ok("typed text through macOS Accessibility/System Events".to_owned())
            }
            "macos.click_element" => {
                let app = self.active_app_or_frontmost()?;
                macos_click_element(&app, &step.target)?;
                Ok("clicked macOS accessibility element".to_owned())
            }
            "macos.press_shortcut" => {
                let shortcut = step.value.as_deref().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "macos.press_shortcut requires a shortcut such as Cmd+N in step.value."
                            .to_owned(),
                    )
                })?;
                let app = self.active_app_or_frontmost()?;
                macos_press_shortcut(&app, shortcut)?;
                Ok(format!("pressed macOS shortcut {shortcut}"))
            }
            "macos.invoke_menu" => {
                let menu_path = step.value.as_deref().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "macos.invoke_menu requires a menu path such as File > Save in step.value."
                            .to_owned(),
                    )
                })?;
                let app = self.active_app_or_frontmost()?;
                macos_invoke_menu(&app, menu_path)?;
                Ok(format!("invoked macOS menu {menu_path}"))
            }
            "macos.focus_document" => {
                let app = self.active_app_or_frontmost()?;
                macos_focus_document(&app, &step.target)?;
                Ok("focused macOS document area".to_owned())
            }
            "macos.save_as" => {
                let path = step.value.as_deref().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "macos.save_as requires the target path in step.value.".to_owned(),
                    )
                })?;
                let app = self.active_app_or_frontmost()?;
                macos_save_as(&app, path)?;
                Ok(format!("saved macOS document as {path}"))
            }
            "macos.read_text" => {
                let app = self.active_app_or_frontmost()?;
                let text = macos_read_element_text(&app, &step.target)?;
                Ok(text)
            }
            "macos.screenshot" => {
                let path = step
                    .value
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(default_screenshot_path);
                take_macos_screenshot(&path)?;
                Ok(path.display().to_string())
            }
            "macos.close_app" => {
                let app = step
                    .value
                    .clone()
                    .or_else(|| {
                        self.state
                            .lock()
                            .expect("macos adapter mutex poisoned")
                            .active_app
                            .clone()
                    })
                    .ok_or_else(|| {
                        AdapterError::ExecutionFailed(
                            "macos.close_app requires an app name or active app.".to_owned(),
                        )
                    })?;
                run_osascript(&format!("tell application {} to quit", apple_quote(&app)))?;
                self.state
                    .lock()
                    .expect("macos adapter mutex poisoned")
                    .active_app = None;
                Ok(format!("closed macOS app {app}"))
            }
            _ => Err(AdapterError::UnsupportedCapability(
                step.required_capability.clone(),
            )),
        }
    }
}

impl DesktopAdapter for MacOsAccessibilityAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        let diagnostics = first_run_permission_check(&self.platform);
        if diagnostics.ready_for_ax() {
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
        let app = self.active_app_or_frontmost()?;
        let visible_text = macos_read_process_text(&app)?;
        Ok(Observation {
            adapter_id: MACOS_ADAPTER_ID.to_owned(),
            summary: format!("macos session {} active_app {}", ctx.session_id, app),
            visible_text,
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

        let message = self.execute_real_step(&step)?;

        self.state
            .lock()
            .expect("macos adapter mutex poisoned")
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
        self.require_ax()?;

        let passed = match assertion.required_capability.as_str() {
            "macos.assert_visible" => {
                let app = self.active_app_or_frontmost()?;
                macos_element_exists(&app, &assertion.target, Some(&assertion.expected))?
            }
            "macos.find_window" => {
                let app = self.active_app_or_frontmost()?;
                macos_window_exists(&app, &assertion.expected)?
            }
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

fn activate_macos_app(app: &str) -> AdapterResult<()> {
    let app_name = app.trim_end_matches(".app");
    run_command("open", ["-a", app_name])?;
    run_osascript(&format!(
        "tell application {} to activate",
        apple_quote(app_name)
    ))?;
    Ok(())
}

fn macos_window_exists(app: &str, expected: &str) -> AdapterResult<bool> {
    let script = format!(
        r#"
tell application "System Events"
  if not (exists process {app}) then return "false"
  tell process {app}
    repeat with candidate in windows
      set candidateName to ""
      try
        set candidateName to name of candidate as text
      end try
      if candidateName contains {expected} then return "true"
    end repeat
  end tell
end tell
return "false"
"#,
        app = apple_quote(app),
        expected = apple_quote(expected)
    );
    Ok(run_osascript(&script)?.trim() == "true")
}

fn macos_element_exists(
    app: &str,
    target: &LocatorTarget,
    expected_text: Option<&str>,
) -> AdapterResult<bool> {
    let predicate = macos_locator_predicate(target, expected_text)?;
    let script = format!(
        r#"
tell application "System Events"
  if not (exists process {app}) then return "false"
  tell process {app}
    try
      if exists (first UI element of entire contents of front window whose {predicate}) then return "true"
    end try
  end tell
end tell
return "false"
"#,
        app = apple_quote(app),
        predicate = predicate
    );
    Ok(run_osascript(&script)?.trim() == "true")
}

fn macos_read_process_text(app: &str) -> AdapterResult<Vec<String>> {
    let script = format!(
        r#"
set output to ""
tell application "System Events"
  if not (exists process {app}) then return output
  tell process {app}
    try
      repeat with candidate in entire contents of front window
        try
          set candidateText to ""
          try
            set candidateText to value of candidate as text
          end try
          if candidateText is "" then
            try
              set candidateText to name of candidate as text
            end try
          end if
          if candidateText is not "" then set output to output & candidateText & linefeed
        end try
      end repeat
    end try
  end tell
end tell
return output
"#,
        app = apple_quote(app)
    );
    Ok(run_osascript(&script)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn macos_read_element_text(app: &str, target: &LocatorTarget) -> AdapterResult<String> {
    let predicate = macos_locator_predicate(target, None)?;
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set candidate to first UI element of entire contents of front window whose {predicate}
    try
      return value of candidate as text
    end try
    try
      return name of candidate as text
    end try
  end tell
end tell
return ""
"#,
        app = apple_quote(app),
        predicate = predicate
    );
    Ok(run_osascript(&script)?.trim().to_owned())
}

fn macos_type_text(app: &str, target: &LocatorTarget, value: &str) -> AdapterResult<()> {
    let predicate = macos_locator_predicate(target, None)?;
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    set candidate to first UI element of entire contents of front window whose {predicate}
    try
      set focused of candidate to true
    end try
    try
      click candidate
    end try
    keystroke {value}
  end tell
end tell
"#,
        app = apple_quote(app),
        predicate = predicate,
        value = apple_quote(value)
    );
    run_osascript(&script).map(|_| ())
}

fn macos_click_element(app: &str, target: &LocatorTarget) -> AdapterResult<()> {
    let predicate = macos_locator_predicate(target, None)?;
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    click (first UI element of entire contents of front window whose {predicate})
  end tell
end tell
"#,
        app = apple_quote(app),
        predicate = predicate
    );
    run_osascript(&script).map(|_| ())
}

fn macos_press_shortcut(app: &str, shortcut: &str) -> AdapterResult<()> {
    let (key, modifiers) = macos_shortcut_parts(shortcut)?;
    let using = if modifiers.is_empty() {
        String::new()
    } else {
        format!(" using {{{}}}", modifiers.join(", "))
    };
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    keystroke {key}{using}
  end tell
end tell
"#,
        app = apple_quote(app),
        key = apple_quote(&key),
        using = using
    );
    run_osascript(&script).map(|_| ())
}

fn macos_invoke_menu(app: &str, menu_path: &str) -> AdapterResult<()> {
    let parts = menu_path
        .split('>')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(AdapterError::ExecutionFailed(
            "macos.invoke_menu requires at least a menu and item, for example File > Save."
                .to_owned(),
        ));
    }
    let menu = apple_quote(parts[0]);
    let mut expression = format!("menu item {}", apple_quote(parts[parts.len() - 1]));
    for part in parts[1..parts.len() - 1].iter().rev() {
        expression = format!(
            "menu item {} of menu 1 of {}",
            apple_quote(part),
            expression
        );
    }
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    click {expression} of menu {menu} of menu bar 1
  end tell
end tell
"#,
        app = apple_quote(app),
        expression = expression,
        menu = menu
    );
    run_osascript(&script).map(|_| ())
}

fn macos_focus_document(app: &str, target: &LocatorTarget) -> AdapterResult<()> {
    let predicate = macos_locator_predicate(target, None).unwrap_or_else(|_| {
        "(its role is \"AXTextArea\") or (its role is \"AXWebArea\") or (its role is \"AXScrollArea\")"
            .to_owned()
    });
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    set candidate to first UI element of entire contents of front window whose {predicate}
    try
      set focused of candidate to true
    end try
    try
      click candidate
    end try
  end tell
end tell
"#,
        app = apple_quote(app),
        predicate = predicate
    );
    run_osascript(&script).map(|_| ())
}

fn macos_save_as(app: &str, path: &str) -> AdapterResult<()> {
    if path.trim().is_empty() {
        return Err(AdapterError::ExecutionFailed(
            "macos.save_as requires a non-empty file path.".to_owned(),
        ));
    }
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    keystroke "s" using {{command down, shift down}}
    delay 0.5
    keystroke "g" using {{command down, shift down}}
    delay 0.2
    keystroke {path}
    key code 36
    delay 0.2
    key code 36
  end tell
end tell
"#,
        app = apple_quote(app),
        path = apple_quote(path)
    );
    run_osascript(&script).map(|_| ())
}

fn take_macos_screenshot(path: &Path) -> AdapterResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to create screenshot directory: {err}"))
        })?;
    }
    XcapScreenshotBackend
        .capture_primary_monitor(path)
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!(
                "xcap screenshot capture failed: {}",
                err.message
            ))
        })?;
    if path.exists() {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "xcap screenshot backend did not create {}",
            path.display()
        )))
    }
}

fn default_screenshot_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "greentic-macos-screenshot-{}-{}.png",
        std::process::id(),
        epoch_millis()
    ))
}

fn run_command<const N: usize>(program: &str, args: [&str; N]) -> AdapterResult<String> {
    // Program names are fixed by the adapter and arguments are passed directly without a shell.
    // foxguard: ignore[rs/no-command-injection]
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to run {program}: {err}")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(AdapterError::ExecutionFailed(format!(
            "{program} failed: {}",
            stderr.trim()
        )))
    }
}

fn run_osascript(script: &str) -> AdapterResult<String> {
    run_command("osascript", ["-e", script])
}

fn macos_locator_predicate(
    target: &LocatorTarget,
    expected_text: Option<&str>,
) -> AdapterResult<String> {
    let candidates = [target.preferred.as_ref(), target.fallback.as_ref()];
    let mut predicates = Vec::new();
    for strategy in candidates.into_iter().flatten() {
        if let Some(id) = non_empty(strategy.automation_id.as_deref()) {
            predicates.push(format!("its description is {}", apple_quote(id)));
        }
        if let Some(name) = non_empty(strategy.name.as_deref()) {
            predicates.push(format!("its name contains {}", apple_quote(name)));
        }
        if let Some(role) = non_empty(strategy.role.as_deref()) {
            predicates.push(format!("its role is {}", apple_quote(role)));
        }
        if let Some(text) = non_empty(strategy.text.as_deref()) {
            predicates.push(format!("its value as text contains {}", apple_quote(text)));
        }
    }
    if let Some(text) = expected_text {
        if !text.trim().is_empty() {
            predicates.push(format!(
                "((its value as text contains {text}) or (its name contains {text}))",
                text = apple_quote(text)
            ));
        }
    }
    if predicates.is_empty() {
        return Err(AdapterError::ExecutionFailed(
            "macOS Accessibility locator requires an automation id, name, role, text, or expected text.".to_owned(),
        ));
    }
    Ok(predicates.join(" or "))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn macos_shortcut_parts(shortcut: &str) -> AdapterResult<(String, Vec<&'static str>)> {
    let parts = shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some(key) = parts.last() else {
        return Err(AdapterError::ExecutionFailed(
            "shortcut must include a key, for example Cmd+N.".to_owned(),
        ));
    };
    let mut modifiers = Vec::new();
    for modifier in &parts[..parts.len().saturating_sub(1)] {
        match modifier.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" => modifiers.push("command down"),
            "shift" => modifiers.push("shift down"),
            "option" | "alt" => modifiers.push("option down"),
            "ctrl" | "control" => modifiers.push("control down"),
            other => {
                return Err(AdapterError::ExecutionFailed(format!(
                    "unsupported macOS shortcut modifier {other}"
                )))
            }
        }
    }
    Ok(((*key).to_owned(), modifiers))
}

fn frontmost_app_script() -> &'static str {
    r#"tell application "System Events" to get name of first application process whose frontmost is true"#
}

fn apple_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
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
        assert!(capabilities.supports("macos.press_shortcut"));
        assert!(capabilities.supports("macos.invoke_menu"));
        assert!(capabilities.supports("macos.focus_document"));
        assert!(capabilities.supports("macos.save_as"));
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
        assert!(ready.supports("macos.press_shortcut"));
        assert!(ready.supports("macos.save_as"));
    }

    #[test]
    fn parses_macos_shortcut_modifiers() {
        let (key, modifiers) = macos_shortcut_parts("Cmd+Shift+N").expect("shortcut");

        assert_eq!(key, "N");
        assert_eq!(modifiers, vec!["command down", "shift down"]);
        let err = macos_shortcut_parts("Cmd+Hyper+N").expect_err("unsupported modifier");
        assert!(format!("{err}").contains("unsupported macOS shortcut modifier"));
    }

    #[test]
    fn new_macos_actions_report_missing_values_before_touching_os_state() {
        let adapter = MacOsAccessibilityAdapter::new(platform(full_permissions()));
        for (capability, expected) in [
            ("macos.press_shortcut", "requires a shortcut"),
            ("macos.invoke_menu", "requires a menu path"),
            ("macos.save_as", "requires the target path"),
        ] {
            let err = adapter
                .execute(RunnerStep {
                    id: capability.to_owned(),
                    action: capability
                        .rsplit('.')
                        .next()
                        .unwrap_or(capability)
                        .to_owned(),
                    target: LocatorTarget::default(),
                    value: None,
                    required_capability: capability.to_owned(),
                })
                .expect_err("missing value should fail before querying frontmost app");
            assert!(format!("{err}").contains(expected), "{err}");
        }
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
    fn activation_uses_real_launchservices_and_fails_for_missing_app() {
        let adapter = MacOsAccessibilityAdapter::new(platform(full_permissions()));
        let error = adapter
            .execute(RunnerStep {
                id: "activate".to_owned(),
                action: "activate_app".to_owned(),
                target: LocatorTarget::default(),
                value: Some("DefinitelyMissingGreenticFixture.app".to_owned()),
                required_capability: "macos.activate_app".to_owned(),
            })
            .expect_err("missing application should not be accepted");

        assert!(error.to_string().contains("open failed"), "{error}");
    }

    #[test]
    fn locator_predicate_uses_ax_metadata_without_running_fake_state() {
        let email = stable_macos_target(&metadata());
        let save = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("save".to_owned()),
            ax_title: Some("Save".to_owned()),
            ax_role: Some("AXButton".to_owned()),
            ax_value: None,
            nearby_text: Some("Customer".to_owned()),
            visual_region: Some("bottom_right".to_owned()),
        });

        let email_predicate =
            macos_locator_predicate(&email, Some("buyer@example.test")).expect("predicate");
        let save_predicate = macos_locator_predicate(&save, None).expect("predicate");

        assert!(
            email_predicate.contains("customerEmail"),
            "{email_predicate}"
        );
        assert!(email_predicate.contains("AXTextField"), "{email_predicate}");
        assert!(
            email_predicate.contains("buyer@example.test"),
            "{email_predicate}"
        );
        assert!(save_predicate.contains("AXButton"), "{save_predicate}");
    }

    #[test]
    fn generic_app_workflow_fails_until_real_fixture_app_exists() {
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
        .expect_err("missing fixture app should fail through real LaunchServices");

        assert!(outcome.to_string().contains("open failed"), "{outcome}");
    }

    #[test]
    fn screenshot_path_uses_real_png_file_location() {
        let path = default_screenshot_path();

        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("png")
        );
        assert!(path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .starts_with("greentic-macos-screenshot-"));
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
