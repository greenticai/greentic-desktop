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
use std::thread;
use std::time::{Duration, Instant, SystemTime};

pub const MACOS_ADAPTER_ID: &str = "greentic.desktop.macos.ax";
pub const MACOS_RECORDER_BACKEND_ID: &str = "greentic.recording.desktop.macos.ax";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsLiveModalSummary {
    pub blocking: bool,
    pub summary: Option<String>,
}

pub fn macos_live_frontmost_app() -> Option<String> {
    let output = run_osascript(
        r#"tell application "System Events" to get name of first application process whose frontmost is true"#,
    )
    .ok()?;
    let output = output.trim();
    (!output.is_empty()).then(|| output.to_owned())
}

pub fn macos_live_modal_summary() -> MacOsLiveModalSummary {
    let script = r#"
set summaries to {}
tell application "System Events"
  set frontmostProcesses to application processes whose frontmost is true
  repeat with processRef in frontmostProcesses
    try
      repeat with windowRef in windows of processRef
        set windowSummary to my describeWindow(processRef, windowRef)
        if windowSummary is not "" then set end of summaries to windowSummary
        try
          repeat with sheetRef in sheets of windowRef
            set sheetSummary to my describeWindow(processRef, sheetRef)
            if sheetSummary is not "" then set end of summaries to sheetSummary
            try
              repeat with nestedSheetRef in sheets of sheetRef
                set nestedSheetSummary to my describeWindow(processRef, nestedSheetRef)
                if nestedSheetSummary is not "" then set end of summaries to nestedSheetSummary
              end repeat
            end try
          end repeat
        end try
      end repeat
    end try
  end repeat
end tell
if (count of summaries) is 0 then return "no-modal"
return my joinList(summaries, " | ")

on describeWindow(processRef, windowRef)
  tell application "System Events"
  set isModal to false
  try
    set subroleValue to subrole of windowRef as text
    if subroleValue contains "Dialog" then set isModal to true
  end try
  try
    if (count of sheets of windowRef) > 0 then set isModal to true
  end try
  set parts to {}
  try
    set processName to name of processRef as text
    if processName is not "" then set end of parts to "app=" & processName
  end try
  try
    set windowName to name of windowRef as text
    if windowName is not "" then set end of parts to "title=" & windowName
  end try
  try
    repeat with itemRef in static texts of windowRef
      try
        set itemText to value of itemRef as text
        if itemText is not "" then set end of parts to "text=" & itemText
      end try
      try
        set itemName to name of itemRef as text
        if itemName is not "" then set end of parts to "text=" & itemName
      end try
    end repeat
  end try
  try
    set buttonsText to {}
    repeat with buttonRef in buttons of windowRef
      try
        set buttonName to name of buttonRef as text
        if buttonName is not "" then set end of buttonsText to buttonName
      end try
    end repeat
    if (count of buttonsText) > 0 then set end of parts to "buttons=" & my joinList(buttonsText, ",")
  end try
  set summary to my joinList(parts, "; ")
  if summary contains "already exists" then set isModal to true
  if summary contains "permission to save" then set isModal to true
  if summary contains "replace" then set isModal to true
  if summary contains "Do you want" then set isModal to true
  if isModal then return summary
  return ""
  end tell
end describeWindow

on joinList(listItems, delimiter)
  set oldDelimiters to AppleScript's text item delimiters
  set AppleScript's text item delimiters to delimiter
  set joined to listItems as text
  set AppleScript's text item delimiters to oldDelimiters
  return joined
end joinList
"#;
    match run_osascript(script) {
        Ok(output) => {
            let summary = output.trim().to_owned();
            if summary.is_empty() || summary == "no-modal" {
                MacOsLiveModalSummary {
                    blocking: false,
                    summary: None,
                }
            } else {
                MacOsLiveModalSummary {
                    blocking: true,
                    summary: Some(summary),
                }
            }
        }
        Err(err) => MacOsLiveModalSummary {
            blocking: true,
            summary: Some(format!("modal probe failed: {err}")),
        },
    }
}

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
            "macos.open_resource",
            "macos.type_text",
            "macos.press_shortcut",
            "macos.invoke_menu",
            "macos.focus_document",
            "macos.save_as",
            "macos.read_text",
            "macos.read_clipboard",
            "macos.copy_spreadsheet_row",
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
            "macos.open_resource" => {
                let path = step.value.as_deref().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "macos.open_resource requires the resource path in step.value.".to_owned(),
                    )
                })?;
                let app = self.active_app_or_frontmost()?;
                macos_open_resource(&app, path)?;
                Ok(format!("opened macOS resource {path}"))
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
                Ok(macos_labeled_output(step)
                    .map(|label| format!("{label}: {text}"))
                    .unwrap_or(text))
            }
            "macos.read_clipboard" => {
                let text = macos_read_clipboard()?;
                Ok(macos_labeled_output(step)
                    .map(|label| format!("{label}: {text}"))
                    .unwrap_or(text))
            }
            "macos.copy_spreadsheet_row" => {
                let app = self.active_app_or_frontmost()?;
                let (label, search_term) = macos_output_assignment(step);
                let row = macos_copy_spreadsheet_row(&app, &search_term)?;
                Ok(label.map(|label| format!("{label}: {row}")).unwrap_or(row))
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
    let app_name = macos_app_script_name(app);
    let app_path = Path::new(app);
    if app_path.exists() {
        if let Err(open_error) = run_command("open", [app]) {
            launch_macos_app_executable(app_path).map_err(|exec_error| {
                AdapterError::ExecutionFailed(format!(
                    "{open_error}; executable fallback also failed: {exec_error}"
                ))
            })?;
        }
    } else if let Err(app_error) = run_command("open", ["-a", &app_name]) {
        let bundle_result = if let Some(bundle_id) = known_macos_bundle_id(&app_name) {
            run_command("open", ["-b", bundle_id]).map_err(|bundle_error| {
                format!("{app_error}; bundle fallback {bundle_id} also failed: {bundle_error}")
            })
        } else {
            Err(app_error.to_string())
        };
        if let Err(launch_error) = bundle_result {
            let app_bundle = PathBuf::from(format!("/Applications/{app_name}.app"));
            if app_bundle.exists() {
                launch_macos_app_executable(&app_bundle).map_err(|exec_error| {
                    AdapterError::ExecutionFailed(format!(
                        "{launch_error}; executable fallback also failed: {exec_error}"
                    ))
                })?;
            } else {
                return Err(AdapterError::ExecutionFailed(launch_error));
            }
        }
    }
    let _ = run_osascript_with_timeout(
        &format!("tell application {} to activate", apple_quote(&app_name)),
        Duration::from_secs(2),
    );
    wait_for_macos_app_frontmost(&app_name, Duration::from_secs(12))?;
    Ok(())
}

fn launch_macos_app_executable(app_path: &Path) -> AdapterResult<()> {
    let executable = macos_app_executable_path(app_path)?;
    // Accepted risk: app_path comes from a local .app bundle and is executed without a shell.
    // foxguard: ignore[rs/no-command-injection]
    Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!(
                "failed to launch {}: {err}",
                executable.display()
            ))
        })?;
    Ok(())
}

fn macos_app_executable_path(app_path: &Path) -> AdapterResult<PathBuf> {
    let info_plist = app_path.join("Contents").join("Info.plist");
    let executable_name = run_command(
        "/usr/libexec/PlistBuddy",
        [
            "-c",
            "Print :CFBundleExecutable",
            info_plist.to_str().ok_or_else(|| {
                AdapterError::ExecutionFailed(format!(
                    "app Info.plist path is not valid UTF-8: {}",
                    info_plist.display()
                ))
            })?,
        ],
    )?
    .trim()
    .to_owned();
    if executable_name.is_empty() {
        return Err(AdapterError::ExecutionFailed(format!(
            "CFBundleExecutable is missing in {}",
            info_plist.display()
        )));
    }
    let executable = app_path
        .join("Contents")
        .join("MacOS")
        .join(executable_name);
    if executable.exists() {
        Ok(executable)
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "app executable does not exist: {}",
            executable.display()
        )))
    }
}

fn wait_for_macos_app_frontmost(app: &str, timeout: Duration) -> AdapterResult<()> {
    let deadline = Instant::now() + timeout;
    let mut last_state = String::new();
    while Instant::now() < deadline {
        let script = format!(
            r#"
tell application "System Events"
  if not (exists process {app}) then return "missing"
  tell process {app}
    try
      set frontmost to true
    end try
    if frontmost is true then return "frontmost"
    return "not-frontmost"
  end tell
end tell
"#,
            app = apple_quote(app)
        );
        match run_osascript_with_timeout(&script, Duration::from_secs(2)) {
            Ok(output) if output.trim() == "frontmost" => return Ok(()),
            Ok(output) => last_state = output.trim().to_owned(),
            Err(err) => last_state = err.to_string(),
        }
        if let Some(frontmost) = macos_live_frontmost_app() {
            let frontmost = frontmost.trim();
            if frontmost == app
                || frontmost
                    .to_ascii_lowercase()
                    .contains(&app.to_ascii_lowercase())
            {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(AdapterError::ExecutionFailed(format!(
        "macos.activate_app launched {app} but it did not become frontmost within {} ms; last state: {}",
        timeout.as_millis(),
        if last_state.is_empty() {
            "unknown"
        } else {
            last_state.as_str()
        }
    )))
}

fn ensure_macos_app_frontmost(app: &str) -> AdapterResult<()> {
    let app_name = macos_app_script_name(app);
    let _ = run_osascript_with_timeout(
        &format!("tell application {} to activate", apple_quote(&app_name)),
        Duration::from_secs(2),
    );
    wait_for_macos_app_frontmost(&app_name, Duration::from_secs(8))
}

fn macos_app_script_name(app: &str) -> String {
    let path = Path::new(app);
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|_| app.contains('/') || app.ends_with(".app"))
        .map(str::to_owned)
        .unwrap_or_else(|| app.trim_end_matches(".app").to_owned())
}

fn known_macos_bundle_id(app: &str) -> Option<&'static str> {
    let normalized = app
        .trim()
        .trim_end_matches(".app")
        .to_ascii_lowercase()
        .replace([' ', '-', '_'], "");
    match normalized.as_str() {
        "microsoftexcel" | "excel" => Some("com.microsoft.Excel"),
        "microsoftword" | "word" => Some("com.microsoft.Word"),
        "microsoftpowerpoint" | "powerpoint" => Some("com.microsoft.Powerpoint"),
        _ => None,
    }
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
    set frontmost to true
    try
      repeat with candidate in UI elements of front window
        try
          set output to output & my greenticElementText(candidate)
          repeat with child1 in UI elements of candidate
            set output to output & my greenticElementText(child1)
            repeat with child2 in UI elements of child1
              set output to output & my greenticElementText(child2)
              repeat with child3 in UI elements of child2
                set output to output & my greenticElementText(child3)
                repeat with child4 in UI elements of child3
                  set output to output & my greenticElementText(child4)
                  repeat with child5 in UI elements of child4
                    set output to output & my greenticElementText(child5)
                  end repeat
                end repeat
              end repeat
            end repeat
          end repeat
        end try
      end repeat
    end try
  end tell
end tell
return output

on greenticElementText(candidate)
  set candidateOutput to ""
  try
    set candidateText to ""
    try
      set candidateText to value of candidate as text
    end try
    if candidateText is "" or candidateText is "missing value" then
      try
        set candidateText to name of candidate as text
      end try
    end if
    if candidateText is "" or candidateText is "missing value" then
      try
        set candidateText to description of candidate as text
      end try
    end if
    if candidateText is not "" and candidateText is not "missing value" then set candidateOutput to candidateText & linefeed
  end try
  return candidateOutput
end greenticElementText
"#,
        app = apple_quote(app)
    );
    Ok(run_osascript(&script)?
        .lines()
        .map(normalize_macos_ax_text)
        .filter(|line| !line.is_empty())
        .collect())
}

fn macos_read_element_text(app: &str, target: &LocatorTarget) -> AdapterResult<String> {
    let predicate = macos_locator_predicate(target, None)?;
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
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
    match run_osascript(&script) {
        Ok(output) => Ok(normalize_macos_ax_text(&output)),
        Err(err) => {
            let expected = locator_expected_text(target);
            if let Some(expected) = expected {
                let expected = normalize_macos_ax_text(&expected);
                let visible = macos_read_process_text(app)?;
                if let Some(value) = visible
                    .into_iter()
                    .find(|value| value.trim().contains(&expected))
                {
                    return Ok(normalize_macos_ax_text(&value));
                }
            }
            Err(err)
        }
    }
}

fn normalize_macos_ax_text(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|ch| {
            !matches!(
                *ch,
                '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'
                    | '\u{202b}'
                    | '\u{202c}'
                    | '\u{202d}'
                    | '\u{202e}'
                    | '\u{2066}'
                    | '\u{2067}'
                    | '\u{2068}'
                    | '\u{2069}'
            )
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

fn locator_expected_text(target: &LocatorTarget) -> Option<String> {
    [target.preferred.as_ref(), target.fallback.as_ref()]
        .into_iter()
        .flatten()
        .find_map(|strategy| {
            strategy
                .text
                .as_deref()
                .or(strategy.name.as_deref())
                .or(strategy.label.as_deref())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn macos_type_text(app: &str, target: &LocatorTarget, value: &str) -> AdapterResult<()> {
    ensure_macos_app_frontmost(app)?;
    if target == &LocatorTarget::default() {
        let script = format!(
            r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    keystroke {value}
  end tell
end tell
"#,
            app = apple_quote(app),
            value = apple_quote(value)
        );
        return run_osascript(&script).map(|_| ());
    }

    if is_active_document_locator(target) {
        macos_focus_document(app, target)?;
        let script = format!(
            r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    keystroke {value}
  end tell
end tell
"#,
            app = apple_quote(app),
            value = apple_quote(value)
        );
        return run_osascript(&script).map(|_| ());
    }

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

fn macos_open_resource(app: &str, path: &str) -> AdapterResult<()> {
    if path.trim().is_empty() {
        return Err(AdapterError::ExecutionFailed(
            "macos.open_resource requires a non-empty file path.".to_owned(),
        ));
    }

    let expanded = expand_user_path(path);
    if expanded.exists() {
        let expanded_path = expanded.to_string_lossy().into_owned();
        run_command("open", ["-a", app, &expanded_path])?;
        activate_macos_app(app)?;
        return Ok(());
    }

    if let Some(parent) = expanded.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AdapterError::ExecutionFailed(format!(
                "failed to create resource parent directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    activate_macos_app(app)?;
    macos_press_shortcut(app, "Cmd+N")?;
    run_osascript("delay 0.8")?;
    Ok(())
}

fn macos_click_element(app: &str, target: &LocatorTarget) -> AdapterResult<()> {
    ensure_macos_app_frontmost(app)?;
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
    ensure_macos_app_frontmost(app)?;
    let (key, modifiers) = macos_shortcut_parts(shortcut)?;
    let using = if modifiers.is_empty() {
        String::new()
    } else {
        format!(" using {{{}}}", modifiers.join(", "))
    };
    let key_action = if let Some(key_code) = macos_special_key_code(&key) {
        format!("key code {key_code}{using}")
    } else {
        format!("keystroke {}{using}", apple_quote(&key))
    };
    let script = format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    {key_action}
  end tell
end tell
"#,
        app = apple_quote(app),
        key_action = key_action
    );
    run_osascript(&script)?;
    if shortcut_is_new_document(&key, &modifiers) {
        macos_confirm_default_new_document_if_needed(app)?;
    }
    Ok(())
}

fn shortcut_is_new_document(key: &str, modifiers: &[&str]) -> bool {
    key.eq_ignore_ascii_case("n") && modifiers.contains(&"command down")
}

fn macos_confirm_default_new_document_if_needed(app: &str) -> AdapterResult<()> {
    ensure_macos_app_frontmost(app)?;
    let script = macos_confirm_default_new_document_script(app);
    run_osascript(&script).map(|_| ())
}

fn macos_confirm_default_new_document_script(app: &str) -> String {
    format!(
        r#"
delay 0.6
tell application "System Events"
  tell process {app}
    set frontmost to true
    try
      set targetWindow to front window
    on error
      return "no-window"
    end try

    set defaultButtonNames to {{"Create", "Choose", "Open", "OK"}}
    set defaultTemplateNames to {{"Blank Workbook", "Blank Document", "Blank Presentation"}}
    repeat with defaultButtonName in defaultButtonNames
      try
        if exists button (defaultButtonName as text) of targetWindow then
          click button (defaultButtonName as text) of targetWindow
          return "confirmed"
        end if
      end try
    end repeat

    try
      repeat with candidate in (entire contents of targetWindow)
        try
          set candidateRole to role of candidate as text
          set candidateName to my accessibleLabel(candidate)
          repeat with defaultTemplateName in defaultTemplateNames
            if candidateName contains (defaultTemplateName as text) then
              click candidate
              delay 0.2
              repeat with defaultButtonName in defaultButtonNames
                try
                  if exists button (defaultButtonName as text) of targetWindow then
                    click button (defaultButtonName as text) of targetWindow
                    return "confirmed-template"
                  end if
                end try
              end repeat
              key code 36
              return "confirmed-template"
            end if
          end repeat
          if candidateRole is "AXButton" then
            repeat with defaultButtonName in defaultButtonNames
              if candidateName is (defaultButtonName as text) then
                click candidate
                return "confirmed"
              end if
            end repeat
          end if
        end try
      end repeat
    end try
  end tell
end tell
return "not-needed"

on accessibleLabel(candidate)
  try
    set candidateName to name of candidate as text
    if candidateName is not "" then return candidateName
  end try
  try
    set candidateDescription to description of candidate as text
    if candidateDescription is not "" then return candidateDescription
  end try
  try
    set candidateValue to value of candidate as text
    if candidateValue is not "" then return candidateValue
  end try
  return ""
end accessibleLabel
"#,
        app = apple_quote(app)
    )
}

fn macos_invoke_menu(app: &str, menu_path: &str) -> AdapterResult<()> {
    ensure_macos_app_frontmost(app)?;
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
    ensure_macos_app_frontmost(app)?;
    if should_use_active_document_focus(target) {
        return macos_focus_active_document(app);
    }

    let predicate = if is_active_document_locator(target) {
        macos_document_area_predicate()
    } else {
        macos_locator_predicate(target, None).unwrap_or_else(|_| macos_document_area_predicate())
    };
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

fn should_use_active_document_focus(target: &LocatorTarget) -> bool {
    target == &LocatorTarget::default() || is_active_document_locator(target)
}

fn macos_focus_active_document(app: &str) -> AdapterResult<()> {
    let script = macos_focus_active_document_script(app);
    let output = run_osascript(&script)?;
    let status = output.trim();
    if matches!(status, "focused" | "focused-window-center") {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(
            "No focusable macOS document area was available in the front window.".to_owned(),
        ))
    }
}

fn macos_focus_active_document_script(app: &str) -> String {
    format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    delay 0.2
    try
      set targetWindow to front window
    on error
      return "missing-window"
    end try

    try
      repeat with candidate in (entire contents of targetWindow)
        try
          set candidateRole to role of candidate as text
          if candidateRole is "AXTextArea" or candidateRole is "AXWebArea" or candidateRole is "AXScrollArea" then
            try
              set focused of candidate to true
            end try
            try
              click candidate
            end try
            return "focused"
          end if
        end try
      end repeat
    end try

    try
      set windowPosition to position of targetWindow
      set windowSize to size of targetWindow
      set clickX to (item 1 of windowPosition) + ((item 1 of windowSize) / 2)
      set clickY to (item 2 of windowPosition) + ((item 2 of windowSize) / 2)
      click at {{clickX, clickY}}
      return "focused-window-center"
    end try
  end tell
end tell
return "not-focused"
"#,
        app = apple_quote(app)
    )
}

fn macos_save_as(app: &str, path: &str) -> AdapterResult<()> {
    if path.trim().is_empty() {
        return Err(AdapterError::ExecutionFailed(
            "macos.save_as requires a non-empty file path.".to_owned(),
        ));
    }
    let expanded = expand_user_path(path);
    if let Some(parent) = expanded.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AdapterError::ExecutionFailed(format!(
                "failed to create save parent directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    let file_name = expanded
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            AdapterError::ExecutionFailed(format!(
                "macos.save_as path has no file name: {}",
                expanded.display()
            ))
        })?;
    let parent = expanded.parent().unwrap_or_else(|| Path::new("."));
    let parent = parent.to_string_lossy();
    let path_existed_before_save = expanded.exists();
    let previous_modified = path_modified_at(&expanded);
    let excel_default_output = excel_default_format_output_path(app, &expanded);
    let previous_excel_default_modified =
        excel_default_output.as_deref().and_then(path_modified_at);
    let _ = macos_confirm_existing_save_dialog(app, &expanded, previous_modified);
    if is_word_app(app) || (is_excel_app(app) && !expanded.exists()) {
        match macos_application_save_as_fallback(app, &expanded, previous_modified) {
            Ok(()) => return Ok(()),
            Err(err) if is_terminal_save_ui_error(&err.to_string()) => return Err(err),
            Err(_) => {}
        }
    }
    let mut ui_save_error = String::new();
    let ui_file_name = office_save_panel_file_name(app, file_name);
    let script = macos_save_as_script(app, &ui_file_name, &parent);
    match run_osascript_with_timeout(&script, Duration::from_secs(20)) {
        Ok(save_panel_result) if save_panel_result.contains("no-save-panel") => {
            ui_save_error = format!(
                "macos.save_as could not open a Save As panel for {}",
                expanded.display()
            );
        }
        Ok(_) => {}
        Err(err) => {
            ui_save_error = err.to_string();
        }
    }
    let ui_save_error = match macos_confirm_until_saved(
        app,
        &expanded,
        previous_modified,
        Duration::from_secs(3),
    ) {
        Ok(()) => return Ok(()),
        Err(reason) if ui_save_error.is_empty() => reason,
        Err(reason) => format!("{ui_save_error}; {reason}"),
    };
    if is_terminal_save_ui_error(&ui_save_error) {
        return Err(AdapterError::ExecutionFailed(format!(
            "macos.save_as did not create or update {}. {ui_save_error}",
            expanded.display()
        )));
    }
    if let Some(default_output) = excel_default_output.as_deref() {
        if saved_path_updated(default_output, previous_excel_default_modified) {
            std::fs::rename(default_output, &expanded).map_err(|err| {
                AdapterError::ExecutionFailed(format!(
                    "macos.save_as saved Excel default-format file {} but could not move it to {}: {err}",
                    default_output.display(),
                    expanded.display()
                ))
            })?;
            if saved_path_updated(&expanded, previous_modified) {
                return Ok(());
            }
        }
    }
    if is_excel_app(app) && path_existed_before_save && saved_path_exists(&expanded) {
        let remaining_dialog = run_osascript(&macos_save_confirmation_script(app))
            .map(|output| output.trim().to_owned())
            .unwrap_or_else(|err| err.to_string());
        if remaining_dialog.is_empty() || remaining_dialog == "no-dialog" {
            return Ok(());
        }
        return Err(AdapterError::ExecutionFailed(format!(
            "macos.save_as did not finish replacing {}. {remaining_dialog}",
            expanded.display()
        )));
    }
    macos_application_save_as_fallback(app, &expanded, previous_modified).and_then(|_| {
            macos_confirm_until_saved(app, &expanded, previous_modified, Duration::from_secs(4))
                .map_err(|reason| {
                    AdapterError::ExecutionFailed(format!(
                        "macos.save_as did not create or update {}. UI save did not complete: {ui_save_error}; application fallback did not complete: {reason}",
                        expanded.display(),
                    ))
                })
        })
}

fn macos_labeled_output(step: &RunnerStep) -> Option<String> {
    step.value
        .as_deref()
        .filter(|value| value.starts_with("outputs."))
        .map(|value| {
            value
                .trim_start_matches("outputs.")
                .split_once('=')
                .map(|(label, _)| label)
                .unwrap_or(value.trim_start_matches("outputs."))
                .replace('_', " ")
                .trim()
                .to_owned()
        })
        .filter(|label| !label.is_empty())
}

fn macos_output_assignment(step: &RunnerStep) -> (Option<String>, String) {
    let value = step.value.as_deref().unwrap_or_default();
    if let Some(rest) = value.strip_prefix("outputs.") {
        if let Some((label, operand)) = rest.split_once('=') {
            return (
                Some(label.replace('_', " ").trim().to_owned()),
                operand.trim().to_owned(),
            );
        }
    }
    (macos_labeled_output(step), value.trim().to_owned())
}

fn macos_read_clipboard() -> AdapterResult<String> {
    Ok(run_command("pbpaste", [] as [&str; 0])?.trim().to_owned())
}

fn macos_copy_spreadsheet_row(app: &str, search_term: &str) -> AdapterResult<String> {
    if search_term.trim().is_empty() {
        return Err(AdapterError::ExecutionFailed(
            "macos.copy_spreadsheet_row requires a non-empty search term.".to_owned(),
        ));
    }
    if !is_excel_app(app) {
        return Err(AdapterError::ExecutionFailed(format!(
            "macos.copy_spreadsheet_row currently requires Microsoft Excel, got {app}."
        )));
    }
    ensure_macos_app_frontmost(app)?;
    let script = format!(
        r#"
tell application "Microsoft Excel"
  if not (exists active workbook) then error "No active Excel workbook is open."
  set searchText to {search_term}
  set activeSheetRef to active sheet
  set usedRangeRef to used range of activeSheetRef
  set rowCount to count of rows of usedRangeRef
  set columnCount to count of columns of usedRangeRef
  repeat with rowIndex from 1 to rowCount
    set rowValues to {{}}
    set rowText to ""
    repeat with columnIndex from 1 to columnCount
      set cellValue to value of cell rowIndex of column columnIndex of usedRangeRef
      if cellValue is missing value then set cellValue to ""
      set cellText to cellValue as text
      set end of rowValues to cellText
      set rowText to rowText & " " & cellText
    end repeat
    if rowText contains searchText then
      set outputRow to ""
      repeat with valueIndex from 1 to count of rowValues
        if valueIndex > 1 then set outputRow to outputRow & (character id 9)
        set outputRow to outputRow & ((item valueIndex of rowValues) as text)
      end repeat
      set the clipboard to outputRow
      return outputRow
    end if
  end repeat
end tell
error "No spreadsheet row contains " & {search_term}
"#,
        search_term = apple_quote(search_term)
    );
    Ok(run_osascript_with_timeout(&script, Duration::from_secs(8))?
        .trim()
        .to_owned())
}

fn is_excel_app(app: &str) -> bool {
    app.to_ascii_lowercase().contains("microsoft excel")
}

fn is_word_app(app: &str) -> bool {
    app.to_ascii_lowercase().contains("microsoft word")
}

fn excel_default_format_output_path(app: &str, path: &Path) -> Option<PathBuf> {
    if !is_excel_app(app) {
        return None;
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())?;
    if extension == "xlsx" {
        return None;
    }
    let file_name = path.file_name()?.to_str()?;
    Some(path.with_file_name(format!("{file_name}.xlsx")))
}

fn office_save_panel_file_name(app: &str, file_name: &str) -> String {
    if app.to_ascii_lowercase().contains("microsoft word") {
        let path = Path::new(file_name);
        if matches!(
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.to_ascii_lowercase())
                .as_deref(),
            Some("docx" | "doc")
        ) {
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                return stem.to_owned();
            }
        }
    }
    file_name.to_owned()
}

fn is_terminal_save_ui_error(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    reason.contains("permission to save")
        || reason.contains("write access")
        || reason.contains("select a different location")
        || reason.contains("additional permissions are required")
        || reason.contains("grant file access")
}

fn macos_confirm_existing_save_dialog(
    app: &str,
    path: &Path,
    previous_modified: Option<SystemTime>,
) -> Result<(), String> {
    let output =
        run_osascript(&macos_save_confirmation_script(app)).map_err(|err| err.to_string())?;
    let output = output.trim();
    if output.starts_with("clicked ") {
        return wait_for_saved_path(path, previous_modified, Duration::from_secs(2)).map_err(
            |_| {
                format!(
                    "clicked existing save confirmation but {} was not updated",
                    path.display()
                )
            },
        );
    }
    Err(output.to_owned())
}

fn macos_save_as_script(app: &str, file_name: &str, parent_folder: &str) -> String {
    format!(
        r#"
tell application "System Events"
  tell process {app}
    set frontmost to true
    keystroke "s" using {{command down, shift down}}
    set hasSavePanel to false
    repeat 20 times
      try
        if exists sheet 1 of front window then
          set hasSavePanel to true
          exit repeat
        end if
      end try
      try
        if exists text field 1 of front window then
          set hasSavePanel to true
          exit repeat
        end if
      end try
      delay 0.1
    end repeat
    if hasSavePanel is false then
      try
        click menu item "Save As..." of menu "File" of menu bar 1
      end try
      try
        click menu item "Save As…" of menu "File" of menu bar 1
      end try
      repeat 20 times
        try
          if exists sheet 1 of front window then
            set hasSavePanel to true
            exit repeat
          end if
        end try
        try
          if exists text field 1 of front window then
            set hasSavePanel to true
            exit repeat
          end if
        end try
        delay 0.1
      end repeat
    end if
    if hasSavePanel is false then return "no-save-panel"
    set didSetName to false
    try
      if exists sheet 1 of front window then
        set value of text field 1 of sheet 1 of front window to {file_name}
        set didSetName to true
      end if
    end try
    if didSetName is false then
      try
        set value of text field 1 of front window to {file_name}
        set didSetName to true
      end try
    end if
    if didSetName is false then
      keystroke "a" using {{command down}}
      keystroke {file_name}
    end if
    keystroke "g" using {{command down, shift down}}
    delay 0.2
    keystroke {parent_folder}
    key code 36
    repeat 20 times
      delay 0.1
      try
        if exists button "Save" of sheet 1 of front window then
          click button "Save" of sheet 1 of front window
          return "clicked-save"
        end if
      end try
      try
        if exists button "Save" of front window then
          click button "Save" of front window
          return "clicked-save"
        end if
      end try
    end repeat
    key code 36
    return "pressed-return-fallback"
  end tell
end tell
"#,
        app = apple_quote(app),
        file_name = apple_quote(file_name),
        parent_folder = apple_quote(parent_folder)
    )
}

fn run_osascript_with_timeout(script: &str, timeout: Duration) -> AdapterResult<String> {
    run_command_with_timeout("osascript", ["-e", script], timeout)
}

fn macos_confirm_until_saved(
    app: &str,
    path: &Path,
    previous_modified: Option<SystemTime>,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let mut last_dialog = String::new();
    while Instant::now() < deadline {
        match run_osascript(&macos_save_confirmation_script(app)) {
            Ok(output) => {
                let output = output.trim();
                if !output.is_empty() && output != "no-dialog" {
                    last_dialog = output.to_owned();
                }
            }
            Err(err) => last_dialog = err.to_string(),
        }
        if saved_path_updated(path, previous_modified) {
            let remaining_dialog = run_osascript(&macos_save_confirmation_script(app))
                .map(|output| output.trim().to_owned())
                .unwrap_or_else(|err| err.to_string());
            if remaining_dialog.is_empty() || remaining_dialog == "no-dialog" {
                return Ok(());
            }
            last_dialog = remaining_dialog;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if saved_path_updated(path, previous_modified) {
        let remaining_dialog = run_osascript(&macos_save_confirmation_script(app))
            .map(|output| output.trim().to_owned())
            .unwrap_or_else(|err| err.to_string());
        if remaining_dialog.is_empty() || remaining_dialog == "no-dialog" {
            return Ok(());
        }
        last_dialog = remaining_dialog;
    }
    if last_dialog.trim().is_empty() {
        last_dialog = "No save confirmation dialog was visible.".to_owned();
    }
    Err(last_dialog)
}

fn macos_application_save_as_fallback(
    app: &str,
    path: &Path,
    previous_modified: Option<SystemTime>,
) -> AdapterResult<()> {
    let path_display = path.to_string_lossy();
    let scripts = macos_application_save_as_fallback_scripts(app, &path_display);
    let mut errors = Vec::new();
    for script in scripts {
        match run_osascript(&script) {
            Ok(_)
                if wait_for_saved_path(path, previous_modified, Duration::from_secs(2)).is_ok() =>
            {
                return Ok(())
            }
            Ok(_) => errors.push("save command returned without creating the file".to_owned()),
            Err(err) => errors.push(err.to_string()),
        }
    }
    Err(AdapterError::ExecutionFailed(format!(
        "application-level save fallback failed: {}",
        errors.join("; ")
    )))
}

fn macos_application_save_as_fallback_scripts(app: &str, path: &str) -> Vec<String> {
    let app_name = app.to_ascii_lowercase();
    let app = apple_quote(app);
    let path = apple_quote(path);
    if app_name.contains("word") {
        return vec![
            format!(
                r#"
tell application {app}
  activate
  save as active document file name {path} file format format document
end tell
"#
            ),
            format!(
                r#"
tell application {app}
  activate
  save as document 1 file name {path} file format format document
end tell
"#
            ),
        ];
    }
    if app_name.contains("excel") {
        return vec![
            format!(
                r#"
tell application {app}
  activate
  save active workbook in POSIX file {path}
end tell
"#
            ),
            format!(
                r#"
tell application {app}
  activate
  save workbook as active workbook filename {path}
end tell
"#
            ),
        ];
    }
    vec![
        format!(
            r#"
tell application {app}
  activate
  save front document in POSIX file {path}
end tell
"#
        ),
        format!(
            r#"
tell application {app}
  activate
  save document 1 in POSIX file {path}
end tell
"#
        ),
    ]
}

fn macos_save_confirmation_script(app: &str) -> String {
    format!(
        r#"
set confirmationButtons to {{"Yes", "Continue", "OK", "Replace", "Replace File", "Overwrite", "Save", "Save File", "Keep Current Format", "Use .xls", "Use Excel 97-2004 Workbook"}}
set seenDialog to false
set dialogSummary to ""
set clickedButtons to {{}}
tell application "System Events"
  tell process {app}
    set frontmost to true
    set processRef to it
    repeat 8 times
      set clickedThisPass to false
      repeat with buttonName in confirmationButtons
        try
          if exists button (buttonName as text) of front window then
            my pressButton(button (buttonName as text) of front window)
            set end of clickedButtons to "pressed " & (buttonName as text) & " in front window"
            set clickedThisPass to true
            exit repeat
          end if
        end try
        try
          if exists sheet 1 of front window then
            if exists button (buttonName as text) of sheet 1 of front window then
              my pressButton(button (buttonName as text) of sheet 1 of front window)
              set end of clickedButtons to "pressed " & (buttonName as text) & " in sheet"
              set clickedThisPass to true
              exit repeat
            end if
            try
              if exists sheet 1 of sheet 1 of front window then
                if exists button (buttonName as text) of sheet 1 of sheet 1 of front window then
                  my pressButton(button (buttonName as text) of sheet 1 of sheet 1 of front window)
                  set end of clickedButtons to "pressed " & (buttonName as text) & " in nested sheet"
                  set clickedThisPass to true
                  exit repeat
                end if
              end if
            end try
          end if
        end try
      end repeat
      if clickedThisPass then
        delay 0.2
      else
      try
        repeat with windowRef in windows
          try
            if my isDialogLike(windowRef) then
              set windowSummary to my describeDialog(windowRef)
              set seenDialog to true
              set dialogSummary to windowSummary
              if windowSummary contains "permission to save" then
                return "blocked by save confirmation: " & windowSummary
              end if
              if windowSummary contains "write access" then
                return "blocked by save confirmation: " & windowSummary
              end if
              if windowSummary contains "select a different location" then
                return "blocked by save confirmation: " & windowSummary
              end if
              if windowSummary contains "Additional permissions are required" then
                return "blocked by save confirmation: " & windowSummary
              end if
              if windowSummary contains "Grant File Access" then
                return "blocked by save confirmation: " & windowSummary
              end if
            end if
          end try
          repeat with buttonName in confirmationButtons
            try
              set clickedLabel to my clickButtonNamed(windowRef, buttonName as text)
              if clickedLabel is not "" then
                set end of clickedButtons to clickedLabel
                set clickedThisPass to true
                exit repeat
              end if
            end try
          end repeat
          if clickedThisPass then exit repeat
          try
            repeat with sheetRef in sheets of windowRef
              try
                set sheetSummary to my describeDialog(sheetRef)
                set seenDialog to true
                set dialogSummary to sheetSummary
                if sheetSummary contains "permission to save" then
                  return "blocked by save confirmation: " & sheetSummary
                end if
                if sheetSummary contains "write access" then
                  return "blocked by save confirmation: " & sheetSummary
                end if
                if sheetSummary contains "select a different location" then
                  return "blocked by save confirmation: " & sheetSummary
                end if
                if sheetSummary contains "Additional permissions are required" then
                  return "blocked by save confirmation: " & sheetSummary
                end if
                if sheetSummary contains "Grant File Access" then
                  return "blocked by save confirmation: " & sheetSummary
                end if
              end try
              repeat with buttonName in confirmationButtons
                try
                  set clickedLabel to my clickButtonNamed(sheetRef, buttonName as text)
                  if clickedLabel is not "" then
                    set end of clickedButtons to clickedLabel
                    set clickedThisPass to true
                    exit repeat
                  end if
                end try
              end repeat
              if clickedThisPass then exit repeat
              try
                repeat with nestedSheetRef in sheets of sheetRef
                  try
                    set nestedSheetSummary to my describeDialog(nestedSheetRef)
                    set seenDialog to true
                    set dialogSummary to nestedSheetSummary
                  end try
                  repeat with buttonName in confirmationButtons
                    try
                      set clickedLabel to my clickButtonNamed(nestedSheetRef, buttonName as text)
                      if clickedLabel is not "" then
                        set end of clickedButtons to clickedLabel & " in nested sheet"
                        set clickedThisPass to true
                        exit repeat
                      end if
                    end try
                  end repeat
                  if clickedThisPass then exit repeat
                end repeat
              end try
              if clickedThisPass then exit repeat
            end repeat
          end try
          if clickedThisPass then exit repeat
        end repeat
      end try
      end if
      if clickedThisPass is false then
        repeat with buttonName in confirmationButtons
          try
            if exists button (buttonName as text) of front window then
              my pressButton(button (buttonName as text) of front window)
              set end of clickedButtons to "pressed " & (buttonName as text) & " in front window"
              set clickedThisPass to true
              exit repeat
            end if
          end try
          try
            if exists sheet 1 of front window then
              set dialogSummary to my describeDialog(sheet 1 of front window)
              if dialogSummary is not "" then set seenDialog to true
              if exists button (buttonName as text) of sheet 1 of front window then
                my pressButton(button (buttonName as text) of sheet 1 of front window)
                set end of clickedButtons to "pressed " & (buttonName as text) & " in sheet"
                set clickedThisPass to true
                exit repeat
              end if
              try
                if exists sheet 1 of sheet 1 of front window then
                  set dialogSummary to my describeDialog(sheet 1 of sheet 1 of front window)
                  if dialogSummary is not "" then set seenDialog to true
                  if exists button (buttonName as text) of sheet 1 of sheet 1 of front window then
                    my pressButton(button (buttonName as text) of sheet 1 of sheet 1 of front window)
                    set end of clickedButtons to "pressed " & (buttonName as text) & " in nested sheet"
                    set clickedThisPass to true
                    exit repeat
                  end if
                end if
              end try
            end if
          end try
        end repeat
      end if
      if clickedThisPass is false then exit repeat
      delay 0.2
    end repeat
    if (count of clickedButtons) > 0 then
      return "clicked " & my joinList(clickedButtons, "; ")
    end if
    try
      repeat with windowRef in windows
        if my isDialogLike(windowRef) then
          set dialogSummary to my describeDialog(windowRef)
          if dialogSummary is not "" then set seenDialog to true
        end if
        try
          repeat with sheetRef in sheets of windowRef
            set dialogSummary to my describeDialog(sheetRef)
            if dialogSummary is not "" then set seenDialog to true
            try
              repeat with nestedSheetRef in sheets of sheetRef
                set dialogSummary to my describeDialog(nestedSheetRef)
                if dialogSummary is not "" then set seenDialog to true
              end repeat
            end try
          end repeat
        end try
      end repeat
    end try
  end tell
end tell
if seenDialog and dialogSummary is not "" then return "blocked by save confirmation: " & dialogSummary
return "no-dialog"

on pressButton(buttonRef)
  try
    tell application "System Events" to perform action "AXPress" of buttonRef
    return
  end try
  tell application "System Events" to click buttonRef
end pressButton

on clickButtonNamed(containerRef, buttonName)
  tell application "System Events"
  try
    if exists button (buttonName as text) of containerRef then
      my pressButton(button (buttonName as text) of containerRef)
      return "pressed " & buttonName
    end if
  end try
  return ""
  end tell
end clickButtonNamed

on isDialogLike(windowRef)
  tell application "System Events"
  try
    if (count of sheets of windowRef) > 0 then return true
  end try
  try
    repeat with buttonRef in buttons of windowRef
      set buttonName to my accessibleLabel(buttonRef)
      if buttonName is "Yes" then return true
      if buttonName is "Replace" then return true
      if buttonName is "OK" then return true
      if buttonName is "Continue" then return true
      if buttonName is "Save" then return true
    end repeat
  end try
  return false
  end tell
end isDialogLike

on accessibleLabel(candidate)
  tell application "System Events"
  try
    set candidateName to name of candidate as text
    if candidateName is not "" and candidateName is not "missing value" then return candidateName
  end try
  try
    set candidateDescription to description of candidate as text
    if candidateDescription is not "" and candidateDescription is not "missing value" then return candidateDescription
  end try
  try
    set candidateValue to value of candidate as text
    if candidateValue is not "" and candidateValue is not "missing value" then return candidateValue
  end try
  return ""
  end tell
end accessibleLabel

on describeDialog(dialogObject)
  tell application "System Events"
  set parts to {{}}
  try
    set dialogName to name of dialogObject as text
    if dialogName is not "" and dialogName is not "missing value" then set end of parts to "title=" & dialogName
  end try
  try
    set staticTexts to static texts of dialogObject
    repeat with itemRef in staticTexts
      try
        set itemText to value of itemRef as text
        if itemText is not "" then set end of parts to "text=" & itemText
      end try
      try
        set itemName to name of itemRef as text
        if itemName is not "" then set end of parts to "text=" & itemName
      end try
    end repeat
  end try
  try
    set buttonNames to {{}}
    repeat with buttonRef in buttons of dialogObject
      try
        set end of buttonNames to name of buttonRef as text
      end try
    end repeat
    if (count of buttonNames) > 0 then set end of parts to "buttons=" & my joinList(buttonNames, ",")
  end try
  return my joinList(parts, "; ")
  end tell
end describeDialog

on normalizedText(rawText)
  set textValue to rawText as text
  set textValue to my replaceText(textValue, "…", "...")
  return textValue
end normalizedText

on replaceText(rawText, searchText, replacementText)
  set oldDelimiters to AppleScript's text item delimiters
  set AppleScript's text item delimiters to searchText
  set textItems to text items of rawText
  set AppleScript's text item delimiters to replacementText
  set replacedText to textItems as text
  set AppleScript's text item delimiters to oldDelimiters
  return replacedText
end replaceText

on joinList(listItems, delimiter)
  set oldDelimiters to AppleScript's text item delimiters
  set AppleScript's text item delimiters to delimiter
  set joined to listItems as text
  set AppleScript's text item delimiters to oldDelimiters
  return joined
end joinList
"#,
        app = apple_quote(app)
    )
}

fn wait_for_saved_path(
    path: &Path,
    previous_modified: Option<SystemTime>,
    timeout: Duration,
) -> Result<(), ()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if saved_path_updated(path, previous_modified) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    saved_path_updated(path, previous_modified)
        .then_some(())
        .ok_or(())
}

fn saved_path_updated(path: &Path, previous_modified: Option<SystemTime>) -> bool {
    if !saved_path_exists(path) {
        return false;
    }
    match (path_modified_at(path), previous_modified) {
        (Some(current), Some(previous)) => current > previous,
        (Some(_), None) => true,
        _ => false,
    }
}

fn saved_path_exists(path: &Path) -> bool {
    path.metadata().map(|metadata| metadata.len()).unwrap_or(0) > 0
}

fn path_modified_at(path: &Path) -> Option<SystemTime> {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
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
    run_command_with_timeout("osascript", ["-e", script], Duration::from_secs(8))
}

fn run_command_with_timeout<I, S>(
    program: &str,
    args: I,
    timeout: Duration,
) -> AdapterResult<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    // Accepted risk: this private helper is called only with adapter-owned executable literals.
    // foxguard: ignore[rs/no-command-injection]
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to run {program}: {err}")))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().map_err(|err| {
                    AdapterError::ExecutionFailed(format!("failed to read {program} output: {err}"))
                })?;
                if status.success() {
                    return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
                }
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(AdapterError::ExecutionFailed(format!(
                    "{program} failed: {}",
                    stderr.trim()
                )));
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdapterError::ExecutionFailed(format!(
                    "{program} timed out after {} ms",
                    timeout.as_millis()
                )));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdapterError::ExecutionFailed(format!(
                    "failed to poll {program}: {err}"
                )));
            }
        }
    }
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
            predicates.push(macos_role_predicate(role));
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

fn macos_role_predicate(role: &str) -> String {
    if role.eq_ignore_ascii_case("document") {
        return macos_document_area_predicate();
    }
    format!("its role is {}", apple_quote(role))
}

fn macos_document_area_predicate() -> String {
    "(its role is \"AXTextArea\") or (its role is \"AXWebArea\") or (its role is \"AXScrollArea\") or (its role is \"AXGroup\")"
        .to_owned()
}

fn is_active_document_locator(target: &LocatorTarget) -> bool {
    [target.preferred.as_ref(), target.fallback.as_ref()]
        .into_iter()
        .flatten()
        .any(|strategy| {
            strategy
                .name
                .as_deref()
                .map(|name| name.eq_ignore_ascii_case("active document"))
                .unwrap_or(false)
                || strategy
                    .label
                    .as_deref()
                    .map(|label| label.eq_ignore_ascii_case("active document"))
                    .unwrap_or(false)
                || strategy
                    .role
                    .as_deref()
                    .map(|role| role.eq_ignore_ascii_case("document"))
                    .unwrap_or(false)
        })
}

fn expand_user_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
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

fn macos_special_key_code(key: &str) -> Option<u16> {
    match key
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-', '_'], "")
        .as_str()
    {
        "return" | "enter" => Some(36),
        "tab" => Some(48),
        "escape" | "esc" => Some(53),
        "delete" | "backspace" => Some(51),
        "forwarddelete" => Some(117),
        "home" => Some(115),
        "end" => Some(119),
        "pageup" | "pgup" => Some(116),
        "pagedown" | "pgdn" => Some(121),
        "left" | "leftarrow" => Some(123),
        "right" | "rightarrow" => Some(124),
        "down" | "downarrow" => Some(125),
        "up" | "uparrow" => Some(126),
        "f1" => Some(122),
        "f2" => Some(120),
        "f3" => Some(99),
        "f4" => Some(118),
        "f5" => Some(96),
        "f6" => Some(97),
        "f7" => Some(98),
        "f8" => Some(100),
        "f9" => Some(101),
        "f10" => Some(109),
        "f11" => Some(103),
        "f12" => Some(111),
        _ => None,
    }
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

    #[test]
    fn capabilities_include_spreadsheet_row_copy() {
        let capabilities = macos_capabilities();

        assert!(capabilities.supports("macos.copy_spreadsheet_row"));
    }

    #[test]
    fn output_assignment_splits_output_label_from_search_term() {
        let step = RunnerStep {
            id: "copy-row".to_owned(),
            action: "copy_spreadsheet_row".to_owned(),
            target: LocatorTarget::default(),
            value: Some("outputs.product_row=Wireless Mouse".to_owned()),
            required_capability: "macos.copy_spreadsheet_row".to_owned(),
        };

        let (label, search_term) = macos_output_assignment(&step);

        assert_eq!(label.as_deref(), Some("product row"));
        assert_eq!(search_term, "Wireless Mouse");
        assert_eq!(macos_labeled_output(&step).as_deref(), Some("product row"));
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
    fn live_validation_probes_return_safe_summaries() {
        let _ = macos_live_frontmost_app();
        let summary = macos_live_modal_summary();

        if summary.blocking {
            assert!(summary.summary.is_some());
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
        assert!(capabilities.supports("macos.open_resource"));
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
        assert!(ready.supports("macos.open_resource"));
        assert!(ready.supports("macos.save_as"));
    }

    #[test]
    fn parses_macos_shortcut_modifiers() {
        let (key, modifiers) = macos_shortcut_parts("Cmd+Shift+N").expect("shortcut");

        assert_eq!(key, "N");
        assert_eq!(modifiers, vec!["command down", "shift down"]);
        assert!(shortcut_is_new_document("N", &["command down"]));
        assert!(!shortcut_is_new_document("S", &["command down"]));
        let err = macos_shortcut_parts("Cmd+Hyper+N").expect_err("unsupported modifier");
        assert!(format!("{err}").contains("unsupported macOS shortcut modifier"));
    }

    #[test]
    fn maps_named_shortcut_keys_to_macos_key_codes() {
        let (key, modifiers) = macos_shortcut_parts("Return").expect("return shortcut");
        assert_eq!(key, "Return");
        assert!(modifiers.is_empty());
        assert_eq!(macos_special_key_code(&key), Some(36));

        let (key, modifiers) = macos_shortcut_parts("Control+PageUp").expect("page shortcut");
        assert_eq!(key, "PageUp");
        assert_eq!(modifiers, vec!["control down"]);
        assert_eq!(macos_special_key_code(&key), Some(116));
    }

    #[test]
    fn new_document_confirmation_script_clicks_generic_default_buttons() {
        let script = macos_confirm_default_new_document_script("Microsoft Word");

        assert!(script.contains("\"Create\""), "{script}");
        assert!(script.contains("\"Choose\""), "{script}");
        assert!(script.contains("candidateRole is \"AXButton\""), "{script}");
        assert!(!script.contains("whose role"), "{script}");
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
    fn generic_document_role_maps_to_macos_document_area_roles() {
        let target = LocatorTarget {
            preferred: Some(LocatorStrategy {
                role: Some("document".to_owned()),
                name: Some("active document".to_owned()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        };

        let predicate = macos_locator_predicate(&target, None).expect("predicate");

        assert!(predicate.contains("AXTextArea"), "{predicate}");
        assert!(predicate.contains("AXWebArea"), "{predicate}");
        assert!(is_active_document_locator(&target));
    }

    #[test]
    fn active_document_focus_script_avoids_brittle_whose_specifier() {
        let script = macos_focus_active_document_script("Microsoft Word");

        assert!(script.contains("repeat with candidate in (entire contents of targetWindow)"));
        assert!(script.contains("focused-window-center"));
        assert!(!script.contains("whose role"));
    }

    #[test]
    fn default_focus_target_uses_active_document_fallback() {
        assert!(should_use_active_document_focus(&LocatorTarget::default()));

        let target = LocatorTarget {
            preferred: Some(LocatorStrategy {
                role: Some("document".to_owned()),
                name: Some("active document".to_owned()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        };
        assert!(should_use_active_document_focus(&target));
    }

    #[test]
    fn save_dialog_confirmation_covers_common_office_buttons() {
        let script = macos_save_confirmation_script("Microsoft Excel");

        assert!(script.contains("Keep Current Format"));
        assert!(script.contains("Use .xls"));
        assert!(script.contains("Replace"));
        assert!(script.contains("Replace File"));
        assert!(script.contains("Overwrite"));
        assert!(script.contains("Grant File Access"));
        assert!(script.contains("Additional permissions are required"));
        assert!(script.contains("sheet 1 of front window"));
        assert!(script.contains("blocked by save confirmation"));
        assert!(script.contains("buttons="));
        assert!(script.contains("button (buttonName as text) of front window"));
        assert!(!script.contains("entire contents of containerRef"));
        assert!(!script.contains("my clickButtonNamed(processRef"));
        assert!(!script.contains("key code 36"));
    }

    #[test]
    fn save_dialog_confirmation_script_compiles_on_macos() {
        let script = macos_save_confirmation_script("Microsoft Excel");
        if !cfg!(target_os = "macos") {
            assert!(script.contains("on clickButtonNamed(containerRef, buttonName)"));
            return;
        }

        let path = std::env::temp_dir().join(format!(
            "greentic-save-confirmation-{}.applescript",
            std::process::id()
        ));
        let output = path.with_extension("scpt");
        std::fs::write(&path, script).expect("script should write");

        let compile = Command::new("osacompile")
            .arg("-o")
            .arg(&output)
            .arg(&path)
            .output()
            .expect("osacompile should run");

        assert!(
            compile.status.success(),
            "{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn save_as_script_splits_filename_from_parent_folder() {
        let script = macos_save_as_script("Microsoft Excel", "test.xls", "/Users/maarten");

        assert!(script.contains("to \"test.xls\""), "{script}");
        assert!(script.contains("keystroke \"/Users/maarten\""), "{script}");
        assert!(!script.contains("/Users/maarten/test.xls"), "{script}");
    }

    #[test]
    fn word_save_panel_uses_stem_to_avoid_double_extension() {
        assert_eq!(
            office_save_panel_file_name("Microsoft Word", "report.docx"),
            "report"
        );
        assert_eq!(
            office_save_panel_file_name("Microsoft Word", "legacy.doc"),
            "legacy"
        );
        assert_eq!(
            office_save_panel_file_name("Microsoft Excel", "book.xlsx"),
            "book.xlsx"
        );
    }

    #[test]
    fn excel_default_format_output_tracks_appended_xlsx_paths() {
        let requested = Path::new("/tmp/report.xls");
        assert_eq!(
            excel_default_format_output_path("Microsoft Excel", requested),
            Some(PathBuf::from("/tmp/report.xls.xlsx"))
        );
        assert_eq!(
            excel_default_format_output_path("Microsoft Excel", Path::new("/tmp/report.xlsx")),
            None
        );
        assert_eq!(
            excel_default_format_output_path("Microsoft Word", requested),
            None
        );
    }

    #[test]
    fn application_save_fallback_includes_generic_and_workbook_forms() {
        let word_scripts = macos_application_save_as_fallback_scripts(
            "Microsoft Word",
            "/Users/maarten/test.docx",
        );
        let word_joined = word_scripts.join("\n");
        assert!(word_joined.contains("save as active document file name"));
        assert!(word_joined.contains("file format format document"));
        assert!(word_joined.contains("save as document 1 file name"));
        assert!(!word_joined.contains("save workbook as"));

        let excel_scripts = macos_application_save_as_fallback_scripts(
            "Microsoft Excel",
            "/Users/maarten/test.xls",
        );
        let excel_joined = excel_scripts.join("\n");
        assert!(excel_joined.contains("save active workbook in POSIX file"));
        assert!(excel_joined.contains("save workbook as active workbook filename"));
        assert!(!excel_joined.contains("save as active document"));

        let generic_scripts =
            macos_application_save_as_fallback_scripts("Preview", "/Users/maarten/test.pdf");
        let generic_joined = generic_scripts.join("\n");
        assert!(generic_joined.contains("save front document in POSIX file"));
        assert!(generic_joined.contains("save document 1 in POSIX file"));
    }

    #[test]
    fn existing_file_does_not_count_as_saved_until_modified() {
        let path =
            std::env::temp_dir().join(format!("greentic-existing-save-{}", std::process::id()));
        std::fs::write(&path, "old").expect("write temp file");
        let modified = path_modified_at(&path).expect("modified time");

        assert!(!saved_path_updated(&path, Some(modified)));
        assert!(saved_path_updated(&path, None));
        std::fs::write(&path, "").expect("write empty temp file");
        assert!(!saved_path_updated(&path, None));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn app_activation_resolves_paths_and_known_bundle_ids() {
        assert_eq!(
            macos_app_script_name("/Applications/Microsoft Excel.app"),
            "Microsoft Excel"
        );
        assert_eq!(
            known_macos_bundle_id("Microsoft Excel"),
            Some("com.microsoft.Excel")
        );
        assert_eq!(known_macos_bundle_id("Word"), Some("com.microsoft.Word"));
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
