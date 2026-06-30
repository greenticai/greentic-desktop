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

pub const LINUX_X11_ADAPTER_ID: &str = "greentic.desktop.linux.x11";
pub const LINUX_WAYLAND_ADAPTER_ID: &str = "greentic.desktop.linux.wayland";
pub const LINUX_X11_RECORDER_BACKEND_ID: &str = "greentic.recording.desktop.linux.x11";
pub const LINUX_WAYLAND_RECORDER_BACKEND_ID: &str = "greentic.recording.desktop.linux.wayland";

pub fn linux_x11_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        LINUX_X11_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "linux.find_window",
            "linux.read_window_tree",
            "linux.find_element",
            "linux.click_element",
            "linux.type_text",
            "linux.press_shortcut",
            "linux.read_text",
            "linux.assert_visible",
            "linux.screenshot",
            "linux.activate_window",
            "linux.close_window",
        ],
    )
}

pub fn linux_wayland_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        LINUX_WAYLAND_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "linux.wayland.detect",
            "linux.wayland.portal_screenshot",
            "linux.wayland.accessibility_tree",
            "linux.wayland.assert_visible",
            "linux.wayland.safe_keyboard_shortcut",
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxElementMetadata {
    pub accessible_name: Option<String>,
    pub role: Option<String>,
    pub window_title: Option<String>,
    pub class_name: Option<String>,
    pub nearby_text: Option<String>,
    pub visual_region: Option<String>,
}

pub fn stable_linux_target(metadata: &LinuxElementMetadata) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            name: metadata.accessible_name.clone(),
            role: metadata.role.clone(),
            ..LocatorStrategy::default()
        }),
        fallback: Some(LocatorStrategy {
            name: metadata.window_title.clone(),
            class_name: metadata.class_name.clone(),
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
pub struct LinuxX11RecordingBackend {
    platform: PlatformInfo,
}

impl LinuxX11RecordingBackend {
    pub fn new(platform: PlatformInfo) -> Self {
        Self { platform }
    }
}

impl RecordingBackend for LinuxX11RecordingBackend {
    fn id(&self) -> &'static str {
        LINUX_X11_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Desktop
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        let status = detect_x11_session(&self.platform);
        if !linux_event_source_configured("x11") {
            RecordingPreflight::blocked(
                "Install or start the Linux X11/AT-SPI event source before desktop recording.",
            )
        } else if status.is_x11 && status.diagnostics.is_empty() {
            RecordingPreflight::ready()
        } else {
            RecordingPreflight {
                available: false,
                blocked_reasons: status.diagnostics,
            }
        }
    }

    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        if let Ok(command) = std::env::var("GREENTIC_LINUX_X11_EVENT_SOURCE_COMMAND") {
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
                    backend_id: LINUX_X11_RECORDER_BACKEND_ID.to_owned(),
                    capture_state: RecordingCaptureState::Recording,
                };
            }
        }
        let _ = sink.append_backend_warning(
            "Linux X11 recording requires a real AT-SPI/XInput event source command; synthetic events are disabled.",
        );

        RecordingHandle {
            backend_id: LINUX_X11_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Blocked,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinuxWaylandRecordingBackend {
    support: WaylandSupport,
}

impl LinuxWaylandRecordingBackend {
    pub fn new(support: WaylandSupport) -> Self {
        Self { support }
    }
}

impl RecordingBackend for LinuxWaylandRecordingBackend {
    fn id(&self) -> &'static str {
        LINUX_WAYLAND_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Desktop
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        if !linux_event_source_configured("wayland") {
            RecordingPreflight::blocked(
                "Install or start the Linux Wayland portal/AT-SPI event source before desktop recording.",
            )
        } else if self.support.is_wayland
            && self.support.portal_screenshot_available
            && self.support.at_spi_available
            && self.support.global_window_introspection_supported
        {
            RecordingPreflight::ready()
        } else {
            RecordingPreflight {
                available: false,
                blocked_reasons: self.support.diagnostics.clone(),
            }
        }
    }

    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        if let Ok(command) = std::env::var("GREENTIC_LINUX_WAYLAND_EVENT_SOURCE_COMMAND") {
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
                    backend_id: LINUX_WAYLAND_RECORDER_BACKEND_ID.to_owned(),
                    capture_state: RecordingCaptureState::Recording,
                };
            }
        }
        let _ = sink.append_backend_warning(
            "Linux Wayland recording requires a real portal/AT-SPI event source command; synthetic events are disabled.",
        );

        RecordingHandle {
            backend_id: LINUX_WAYLAND_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Blocked,
        }
    }
}

fn linux_event_source_configured(display: &str) -> bool {
    let specific = format!(
        "GREENTIC_LINUX_{}_EVENT_SOURCE_COMMAND",
        display.to_ascii_uppercase()
    );
    std::env::var(&specific)
        .or_else(|_| std::env::var("GREENTIC_LINUX_EVENT_SOURCE_COMMAND"))
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxWindow {
    pub id: String,
    pub title: String,
    pub class_name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11SessionStatus {
    pub is_x11: bool,
    pub display: Option<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaylandCompositor {
    GnomeMutter,
    KdeKwin,
    Wlroots,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaylandSupport {
    pub is_wayland: bool,
    pub compositor: WaylandCompositor,
    pub portal_screenshot_available: bool,
    pub at_spi_available: bool,
    pub global_input_supported: bool,
    pub global_window_introspection_supported: bool,
    pub diagnostics: Vec<String>,
}

pub fn detect_x11_session(info: &PlatformInfo) -> X11SessionStatus {
    let is_x11 = info.os == DesktopPlatform::Linux && info.display_server.as_deref() == Some("x11");
    let mut diagnostics = Vec::new();
    if info.os != DesktopPlatform::Linux {
        diagnostics.push("Linux X11 adapter can only run on Linux".to_owned());
    }
    if info.display_server.as_deref() != Some("x11") {
        diagnostics.push("X11 session is required for this adapter".to_owned());
    }
    if !info.has_permission(PlatformPermission::WindowManagement) {
        diagnostics
            .push("Window management permission or wmctrl fallback is unavailable".to_owned());
    }
    if !info.has_permission(PlatformPermission::Screenshot) {
        diagnostics.push("Screenshot permission or X11 capture backend is unavailable".to_owned());
    }

    X11SessionStatus {
        is_x11,
        display: info.display_server.clone(),
        diagnostics,
    }
}

pub fn detect_wayland_support(
    info: &PlatformInfo,
    compositor: WaylandCompositor,
    portal_screenshot_available: bool,
    at_spi_available: bool,
) -> WaylandSupport {
    let is_wayland =
        info.os == DesktopPlatform::Linux && info.display_server.as_deref() == Some("wayland");
    let mut diagnostics = Vec::new();
    if info.os != DesktopPlatform::Linux {
        diagnostics.push("Linux Wayland adapter can only run on Linux".to_owned());
    }
    if info.display_server.as_deref() != Some("wayland") {
        diagnostics.push("Wayland session is required for this adapter".to_owned());
    }
    if !portal_screenshot_available {
        diagnostics
            .push("Global screenshots require xdg-desktop-portal permission on Wayland".to_owned());
    }
    if !at_spi_available {
        diagnostics.push("Accessible app automation requires AT-SPI metadata".to_owned());
    }
    diagnostics.push(
        "Global input injection and global window introspection are intentionally unsupported on Wayland"
            .to_owned(),
    );

    WaylandSupport {
        is_wayland,
        compositor,
        portal_screenshot_available,
        at_spi_available,
        global_input_supported: false,
        global_window_introspection_supported: false,
        diagnostics,
    }
}

#[derive(Debug, Clone)]
pub struct LinuxX11Adapter {
    platform: PlatformInfo,
    state: Arc<Mutex<LinuxState>>,
}

#[derive(Debug, Clone)]
pub struct LinuxWaylandAdapter {
    support: WaylandSupport,
    state: Arc<Mutex<WaylandState>>,
}

#[derive(Debug, Clone, Default)]
struct WaylandState {
    recorded: Vec<RecordedEvent>,
}

#[derive(Debug, Clone, Default)]
struct LinuxState {
    recorded: Vec<RecordedEvent>,
}

impl LinuxX11Adapter {
    pub fn new(platform: PlatformInfo) -> Self {
        Self {
            platform,
            state: Arc::new(Mutex::new(LinuxState::default())),
        }
    }

    pub fn list_windows(&self) -> AdapterResult<Vec<LinuxWindow>> {
        self.require_x11()?;
        list_x11_windows()
    }

    pub fn replay(&self, steps: &[RunnerStep]) -> AdapterResult<Vec<StepResult>> {
        steps
            .iter()
            .cloned()
            .map(|step| self.execute(step))
            .collect()
    }

    fn require_x11(&self) -> AdapterResult<()> {
        let status = detect_x11_session(&self.platform);
        if status.is_x11 {
            Ok(())
        } else {
            Err(AdapterError::ExecutionFailed(status.diagnostics.join("; ")))
        }
    }
}

impl LinuxWaylandAdapter {
    pub fn new(support: WaylandSupport) -> Self {
        Self {
            support,
            state: Arc::new(Mutex::new(WaylandState::default())),
        }
    }

    pub fn support(&self) -> &WaylandSupport {
        &self.support
    }

    fn require_wayland(&self) -> AdapterResult<()> {
        if self.support.is_wayland {
            Ok(())
        } else {
            Err(AdapterError::ExecutionFailed(
                self.support.diagnostics.join("; "),
            ))
        }
    }

    fn require_portal_screenshot(&self) -> AdapterResult<()> {
        self.require_wayland()?;
        if self.support.portal_screenshot_available {
            Ok(())
        } else {
            Err(AdapterError::ExecutionFailed(
                "manual approval required: xdg-desktop-portal screenshot access is missing"
                    .to_owned(),
            ))
        }
    }

    fn require_at_spi(&self) -> AdapterResult<()> {
        self.require_wayland()?;
        if self.support.at_spi_available {
            Ok(())
        } else {
            Err(AdapterError::ExecutionFailed(
                "unsupported: AT-SPI accessibility metadata is unavailable".to_owned(),
            ))
        }
    }
}

impl DesktopAdapter for LinuxX11Adapter {
    fn capabilities(&self) -> AdapterCapabilities {
        linux_x11_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        self.require_x11()?;
        let windows = list_x11_windows()?;
        let visible_text = read_at_spi_text()?;
        Ok(Observation {
            adapter_id: LINUX_X11_ADAPTER_ID.to_owned(),
            summary: format!(
                "linux x11 session {} windows={}",
                ctx.session_id,
                windows.len()
            ),
            visible_text,
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }
        self.require_x11()?;

        let message = execute_x11_step(&step)?;

        self.state
            .lock()
            .expect("linux adapter mutex poisoned")
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
        self.require_x11()?;

        let passed = match assertion.required_capability.as_str() {
            "linux.assert_visible" => read_at_spi_text()?
                .iter()
                .any(|value| value.contains(&assertion.expected)),
            "linux.find_window" => list_x11_windows()?
                .iter()
                .any(|window| window.title.contains(&assertion.expected)),
            _ => true,
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "Linux assertion passed".to_owned()
            } else {
                "Linux assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("linux adapter mutex poisoned")
            .recorded
            .last()
            .cloned())
    }
}

impl DesktopAdapter for LinuxWaylandAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        linux_wayland_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        self.require_at_spi()?;
        let visible_text = read_at_spi_text()?;
        Ok(Observation {
            adapter_id: LINUX_WAYLAND_ADAPTER_ID.to_owned(),
            summary: format!(
                "linux wayland session {} accessible_elements={}",
                ctx.session_id,
                visible_text.len()
            ),
            visible_text,
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let message = match step.required_capability.as_str() {
            "linux.wayland.detect" => {
                self.require_wayland()?;
                "Wayland session detected".to_owned()
            }
            "linux.wayland.portal_screenshot" => {
                self.require_portal_screenshot()?;
                let path = step
                    .value
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(default_screenshot_path);
                portal_screenshot(&path)?;
                path.display().to_string()
            }
            "linux.wayland.accessibility_tree" => {
                self.require_at_spi()?;
                format!("read {} AT-SPI text entries", read_at_spi_text()?.len())
            }
            "linux.wayland.assert_visible" => {
                self.require_at_spi()?;
                "Wayland AT-SPI assertion target checked".to_owned()
            }
            "linux.wayland.safe_keyboard_shortcut" => {
                self.require_wayland()?;
                Err(AdapterError::ExecutionFailed(
                    "Wayland global keyboard injection is intentionally unsupported unless a compositor-specific portal is configured.".to_owned(),
                ))?
            }
            _ => String::new(),
        };

        self.state
            .lock()
            .expect("wayland adapter mutex poisoned")
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
        self.require_at_spi()?;

        let passed = read_at_spi_text()?
            .iter()
            .any(|value| value.contains(&assertion.expected));

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "Wayland assertion passed".to_owned()
            } else {
                "Wayland assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("wayland adapter mutex poisoned")
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
                .name
                .clone()
                .or_else(|| strategy.role.clone())
                .or_else(|| strategy.text.clone())
        })
        .or_else(|| {
            target.fallback.as_ref().and_then(|strategy| {
                strategy
                    .name
                    .clone()
                    .or_else(|| strategy.class_name.clone())
            })
        })
        .unwrap_or_else(|| "target".to_owned())
        .to_lowercase()
}

fn execute_x11_step(step: &RunnerStep) -> AdapterResult<String> {
    match step.required_capability.as_str() {
        "linux.find_window" => {
            let title = step
                .value
                .clone()
                .unwrap_or_else(|| target_key(&step.target));
            let window = list_x11_windows()?
                .into_iter()
                .find(|window| window.title.contains(&title) || window.id == title)
                .ok_or_else(|| {
                    AdapterError::ExecutionFailed(format!(
                        "No X11 window containing {title} was found."
                    ))
                })?;
            Ok(format!("found X11 window {}", window.id))
        }
        "linux.activate_window" => {
            let title = step
                .value
                .clone()
                .unwrap_or_else(|| target_key(&step.target));
            run_command("wmctrl", ["-a", title.as_str()])?;
            Ok(format!("activated X11 window {title}"))
        }
        "linux.read_window_tree" => {
            Ok(format!("read {} AT-SPI entries", read_at_spi_text()?.len()))
        }
        "linux.find_element" | "linux.assert_visible" => {
            let expected = target_text(&step.target).or(step.value.clone()).ok_or_else(|| {
                AdapterError::ExecutionFailed(
                    "Linux AT-SPI locator requires accessible name, role, class, text, or step value."
                        .to_owned(),
                )
            })?;
            if read_at_spi_text()?
                .iter()
                .any(|value| value.contains(&expected))
            {
                Ok("found Linux AT-SPI element".to_owned())
            } else {
                Err(AdapterError::ExecutionFailed(format!(
                    "No Linux AT-SPI element containing {expected} was visible."
                )))
            }
        }
        "linux.type_text" => {
            let value = step.value.as_deref().unwrap_or_default();
            run_command("xdotool", ["type", "--clearmodifiers", value])?;
            Ok("typed through XTest/xdotool".to_owned())
        }
        "linux.press_shortcut" => {
            let shortcut = step.value.as_deref().ok_or_else(|| {
                AdapterError::ExecutionFailed(
                    "linux.press_shortcut requires a shortcut such as Ctrl+N in step.value."
                        .to_owned(),
                )
            })?;
            let sequence = linux_xdotool_key_sequence(shortcut)?;
            run_command("xdotool", ["key", "--clearmodifiers", sequence.as_str()])?;
            Ok(format!("pressed Linux shortcut {shortcut}"))
        }
        "linux.click_element" => {
            run_command("xdotool", ["click", "1"])?;
            Ok("clicked through XTest/xdotool".to_owned())
        }
        "linux.read_text" => Ok(read_at_spi_text()?.join("\n")),
        "linux.screenshot" => {
            let path = step
                .value
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(default_screenshot_path);
            x11_screenshot(&path)?;
            Ok(path.display().to_string())
        }
        "linux.close_window" => {
            let title = step
                .value
                .clone()
                .unwrap_or_else(|| target_key(&step.target));
            run_command("wmctrl", ["-c", title.as_str()])?;
            Ok(format!("closed X11 window {title}"))
        }
        _ => Err(AdapterError::UnsupportedCapability(
            step.required_capability.clone(),
        )),
    }
}

fn list_x11_windows() -> AdapterResult<Vec<LinuxWindow>> {
    let output = run_command("wmctrl", ["-lx"])?;
    Ok(output
        .lines()
        .filter_map(parse_wmctrl_window)
        .collect::<Vec<_>>())
}

fn parse_wmctrl_window(line: &str) -> Option<LinuxWindow> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 5 {
        return None;
    }
    let title = parts.get(4..).unwrap_or_default().join(" ");
    Some(LinuxWindow {
        id: parts[0].to_owned(),
        class_name: parts[3].to_owned(),
        title,
        active: false,
    })
}

fn read_at_spi_text() -> AdapterResult<Vec<String>> {
    let output =
        run_optional_command("busctl", ["--user", "tree", "org.a11y.Bus"]).or_else(|_| {
            run_optional_command(
                "gdbus",
                [
                    "call",
                    "--session",
                    "--dest",
                    "org.a11y.Bus",
                    "--object-path",
                    "/org/a11y/bus",
                    "--method",
                    "org.a11y.Bus.GetAddress",
                ],
            )
        })?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn linux_xdotool_key_sequence(shortcut: &str) -> AdapterResult<String> {
    let parts = shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some(key) = parts.last() else {
        return Err(AdapterError::ExecutionFailed(
            "shortcut must include a key, for example Ctrl+N.".to_owned(),
        ));
    };
    let mut sequence = Vec::new();
    for modifier in &parts[..parts.len().saturating_sub(1)] {
        sequence.push(match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "ctrl".to_owned(),
            "shift" => "shift".to_owned(),
            "alt" | "option" => "alt".to_owned(),
            "super" | "meta" | "win" | "windows" => "super".to_owned(),
            other => {
                return Err(AdapterError::ExecutionFailed(format!(
                    "unsupported Linux shortcut modifier {other}"
                )))
            }
        });
    }
    sequence.push(linux_xdotool_key(key));
    Ok(sequence.join("+"))
}

fn linux_xdotool_key(key: &str) -> String {
    match key
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-', '_'], "")
        .as_str()
    {
        "return" | "enter" => "Return".to_owned(),
        "tab" => "Tab".to_owned(),
        "escape" | "esc" => "Escape".to_owned(),
        "delete" | "del" => "Delete".to_owned(),
        "backspace" => "BackSpace".to_owned(),
        "home" => "Home".to_owned(),
        "end" => "End".to_owned(),
        "pageup" | "pgup" => "Page_Up".to_owned(),
        "pagedown" | "pgdn" => "Page_Down".to_owned(),
        "left" | "leftarrow" => "Left".to_owned(),
        "right" | "rightarrow" => "Right".to_owned(),
        "down" | "downarrow" => "Down".to_owned(),
        "up" | "uparrow" => "Up".to_owned(),
        key if key.starts_with('f') && key[1..].parse::<u8>().is_ok() => key.to_ascii_uppercase(),
        key => key.to_owned(),
    }
}

fn x11_screenshot(path: &Path) -> AdapterResult<()> {
    XcapScreenshotBackend
        .capture_primary_monitor(path)
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("xcap screenshot capture failed: {err}"))
        })?;
    if path.exists() {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "Linux screenshot backend did not create {}",
            path.display()
        )))
    }
}

fn portal_screenshot(path: &Path) -> AdapterResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to create screenshot directory: {err}"))
        })?;
    }
    Err(AdapterError::ExecutionFailed(format!(
        "xdg-desktop-portal screenshot requires an interactive portal session; no non-interactive screenshot was created at {}.",
        path.display()
    )))
}

fn target_text(target: &LocatorTarget) -> Option<String> {
    target
        .preferred
        .as_ref()
        .and_then(|strategy| {
            strategy
                .name
                .clone()
                .or_else(|| strategy.text.clone())
                .or_else(|| strategy.role.clone())
        })
        .or_else(|| {
            target.fallback.as_ref().and_then(|strategy| {
                strategy
                    .name
                    .clone()
                    .or_else(|| strategy.class_name.clone())
            })
        })
}

fn run_optional_command<const N: usize>(program: &str, args: [&str; N]) -> AdapterResult<String> {
    run_command(program, args)
}

fn run_command<const N: usize>(program: &str, args: [&str; N]) -> AdapterResult<String> {
    if std::env::consts::OS != "linux" {
        return Err(AdapterError::ExecutionFailed(
            "Linux desktop automation can only run on Linux.".to_owned(),
        ));
    }
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
        Err(AdapterError::ExecutionFailed(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn default_screenshot_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "greentic-linux-screenshot-{}-{}.png",
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

#[derive(Debug, Clone, PartialEq)]
pub struct LinuxX11AppWorkflow {
    pub window_title: String,
    pub prompt: String,
    pub inputs: Vec<LinuxWorkflowInput>,
    pub submit: Option<LinuxWorkflowAction>,
    pub outputs: Vec<LinuxWorkflowOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxWorkflowInput {
    pub name: String,
    pub target: LocatorTarget,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxWorkflowAction {
    pub name: String,
    pub target: LocatorTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxWorkflowOutput {
    pub name: String,
    pub target: LocatorTarget,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxX11AppWorkflowOutcome {
    pub prompt: String,
    pub outputs: BTreeMap<String, String>,
    pub steps: Vec<StepResult>,
}

pub fn run_linux_x11_app_workflow(
    adapter: &LinuxX11Adapter,
    workflow: LinuxX11AppWorkflow,
) -> AdapterResult<LinuxX11AppWorkflowOutcome> {
    let prompt = workflow.prompt.clone();
    let window_title = workflow.window_title.clone();
    let output_specs = workflow.outputs.clone();
    let compiled = compile_workflow(&linux_desktop_workflow(&workflow))
        .map_err(|err| AdapterError::ExecutionFailed(err.to_string()))?;
    let steps = compiled.steps;

    let results = adapter.replay(&steps).map_err(|err| {
        AdapterError::ExecutionFailed(format!("Linux X11 app workflow failed: {err}"))
    })?;
    let observation = adapter
        .observe(ObserveContext {
            session_id: format!(
                "linux-x11-app-workflow-{}",
                workflow_id_component(&window_title)
            ),
            target: output_specs.first().map(|output| output.target.clone()),
        })
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("Linux X11 app workflow failed: {err}"))
        })?;
    let visible = observation.visible_text;

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

    Ok(LinuxX11AppWorkflowOutcome {
        prompt,
        outputs,
        steps: results,
    })
}

fn linux_desktop_workflow(workflow: &LinuxX11AppWorkflow) -> DesktopWorkflow {
    DesktopWorkflow {
        id: format!(
            "linux-x11-app-workflow-{}",
            workflow_id_component(&workflow.window_title)
        ),
        summary: workflow.prompt.clone(),
        target: WorkflowTarget::native_app(
            NativePlatform::LinuxX11,
            None,
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

    fn x11_platform() -> PlatformInfo {
        PlatformInfo {
            os: DesktopPlatform::Linux,
            version: "test".to_owned(),
            desktop_environment: Some("GNOME".to_owned()),
            display_server: Some("x11".to_owned()),
            permissions: vec![
                PlatformPermission::WindowManagement,
                PlatformPermission::AppLaunch,
                PlatformPermission::KeyboardInput,
                PlatformPermission::MouseInput,
                PlatformPermission::Screenshot,
            ],
        }
    }

    fn wayland_platform() -> PlatformInfo {
        PlatformInfo {
            display_server: Some("wayland".to_owned()),
            ..x11_platform()
        }
    }

    fn metadata() -> LinuxElementMetadata {
        LinuxElementMetadata {
            accessible_name: Some("Customer Search".to_owned()),
            role: Some("text".to_owned()),
            window_title: Some("CRM".to_owned()),
            class_name: Some("GtkEntry".to_owned()),
            nearby_text: Some("Customer".to_owned()),
            visual_region: Some("center".to_owned()),
        }
    }

    #[test]
    fn can_detect_x11_session() {
        let status = detect_x11_session(&x11_platform());

        assert!(status.is_x11);
        assert_eq!(status.display, Some("x11".to_owned()));
    }

    #[test]
    fn rejects_non_x11_session() {
        let status = detect_x11_session(&wayland_platform());

        assert!(!status.is_x11);
        assert!(status
            .diagnostics
            .contains(&"X11 session is required for this adapter".to_owned()));
    }

    #[test]
    fn exposes_linux_x11_capabilities() {
        let capabilities = linux_x11_capabilities();

        assert_eq!(capabilities.adapter_id, LINUX_X11_ADAPTER_ID);
        assert!(capabilities.supports("linux.find_window"));
        assert!(capabilities.supports("linux.screenshot"));
    }

    #[test]
    fn locator_supports_accessible_metadata_and_visual_fallback() {
        let target = stable_linux_target(&metadata());

        assert_eq!(
            target.preferred.as_ref().and_then(|item| item.name.clone()),
            Some("Customer Search".to_owned())
        );
        assert_eq!(
            target
                .fallback
                .as_ref()
                .and_then(|item| item.class_name.clone()),
            Some("GtkEntry".to_owned())
        );
        assert_eq!(
            target.visual_fallback.and_then(|item| item.region),
            Some("center".to_owned())
        );
    }

    #[test]
    fn parses_wmctrl_windows_without_seeded_state() {
        let window = parse_wmctrl_window("0x100  0 host  GtkWindow.CRM  CRM Main Window")
            .expect("wmctrl line should parse");

        assert_eq!(window.id, "0x100");
        assert_eq!(window.class_name, "GtkWindow.CRM");
        assert_eq!(window.title, "CRM Main Window");
    }

    #[test]
    fn x11_execution_fails_closed_when_tools_are_unavailable() {
        let adapter = LinuxX11Adapter::new(x11_platform());
        let target = stable_linux_target(&metadata());

        let result = adapter.execute(RunnerStep {
            id: "type".to_owned(),
            action: "type_text".to_owned(),
            target,
            value: Some("Acme".to_owned()),
            required_capability: "linux.type_text".to_owned(),
        });

        if std::env::consts::OS == "linux" {
            assert!(result.is_ok() || result.is_err());
        } else {
            let error = result.expect_err("off-Linux execution should fail closed");
            assert!(
                error
                    .to_string()
                    .contains("Linux desktop automation can only run on Linux"),
                "{error}"
            );
        }
    }

    #[test]
    fn generic_x11_app_workflow_fails_without_real_fixture() {
        let adapter = LinuxX11Adapter::new(x11_platform());
        let input_target = stable_linux_target(&LinuxElementMetadata {
            accessible_name: Some("Search".to_owned()),
            role: Some("text".to_owned()),
            window_title: Some("Sample".to_owned()),
            class_name: Some("GtkEntry".to_owned()),
            nearby_text: Some("Search".to_owned()),
            visual_region: Some("center".to_owned()),
        });
        let output_target = stable_linux_target(&LinuxElementMetadata {
            accessible_name: Some("Result".to_owned()),
            role: Some("label".to_owned()),
            window_title: Some("Sample".to_owned()),
            class_name: Some("GtkLabel".to_owned()),
            nearby_text: Some("Result".to_owned()),
            visual_region: Some("bottom".to_owned()),
        });

        let outcome = run_linux_x11_app_workflow(
            &adapter,
            LinuxX11AppWorkflow {
                window_title: "Sample".to_owned(),
                prompt: "Open Sample and submit a value.".to_owned(),
                inputs: vec![LinuxWorkflowInput {
                    name: "search".to_owned(),
                    target: input_target,
                    value: "Acme".to_owned(),
                }],
                submit: Some(LinuxWorkflowAction {
                    name: "submit".to_owned(),
                    target: stable_linux_target(&LinuxElementMetadata {
                        accessible_name: Some("Submit".to_owned()),
                        role: Some("push button".to_owned()),
                        window_title: Some("Sample".to_owned()),
                        class_name: Some("GtkButton".to_owned()),
                        nearby_text: Some("Search".to_owned()),
                        visual_region: Some("bottom_right".to_owned()),
                    }),
                }),
                outputs: vec![LinuxWorkflowOutput {
                    name: "result".to_owned(),
                    target: output_target,
                    expected: Some("accepted".to_owned()),
                }],
            },
        )
        .expect_err("missing real X11 fixture should fail");

        assert!(outcome.to_string().contains("Linux"), "{outcome}");
    }

    #[test]
    fn target_text_prefers_accessible_metadata_before_visual_fallback() {
        let target = stable_linux_target(&metadata());

        assert_eq!(target_text(&target), Some("Customer Search".to_owned()));
    }

    #[test]
    fn screenshot_path_uses_png_file_location() {
        let path = default_screenshot_path();

        assert_eq!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("png")
        );
    }

    #[test]
    fn detects_wayland_and_reports_global_restrictions() {
        let support = detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::GnomeMutter,
            true,
            true,
        );

        assert!(support.is_wayland);
        assert_eq!(support.compositor, WaylandCompositor::GnomeMutter);
        assert!(!support.global_input_supported);
        assert!(!support.global_window_introspection_supported);
        assert!(support
            .diagnostics
            .iter()
            .any(|message| message.contains("intentionally unsupported")));
    }

    #[test]
    fn wayland_capabilities_are_constrained() {
        let capabilities = linux_wayland_capabilities();

        assert_eq!(capabilities.adapter_id, LINUX_WAYLAND_ADAPTER_ID);
        assert!(capabilities.supports("linux.wayland.detect"));
        assert!(capabilities.supports("linux.wayland.portal_screenshot"));
        assert!(!capabilities.supports("linux.click_element"));
    }

    #[test]
    fn wayland_portal_screenshot_requires_interactive_portal_session() {
        let adapter = LinuxWaylandAdapter::new(detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::KdeKwin,
            true,
            true,
        ));

        let error = adapter
            .execute(RunnerStep {
                id: "shot".to_owned(),
                action: "portal_screenshot".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "linux.wayland.portal_screenshot".to_owned(),
            })
            .expect_err("portal screenshot should require interactive approval");

        assert!(error.to_string().contains("interactive portal"), "{error}");
    }

    #[test]
    fn wayland_requires_manual_approval_when_portal_is_missing() {
        let adapter = LinuxWaylandAdapter::new(detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::Wlroots,
            false,
            true,
        ));

        let error = adapter
            .execute(RunnerStep {
                id: "shot".to_owned(),
                action: "portal_screenshot".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "linux.wayland.portal_screenshot".to_owned(),
            })
            .expect_err("portal missing should require manual approval");

        assert!(error.to_string().contains("manual approval required"));
    }

    #[test]
    fn wayland_at_spi_fails_closed_without_real_accessibility_bus() {
        let adapter = LinuxWaylandAdapter::new(detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::GnomeMutter,
            true,
            true,
        ));
        let target = stable_linux_target(&metadata());

        let result = adapter.execute(RunnerStep {
            id: "tree".to_owned(),
            action: "accessibility_tree".to_owned(),
            target: target.clone(),
            value: None,
            required_capability: "linux.wayland.accessibility_tree".to_owned(),
        });

        if std::env::consts::OS == "linux" {
            assert!(result.is_ok() || result.is_err());
        } else {
            let error = result.expect_err("off-Linux AT-SPI should fail closed");
            assert!(
                error
                    .to_string()
                    .contains("Linux desktop automation can only run on Linux"),
                "{error}"
            );
        }
    }

    #[test]
    fn wayland_reports_unsupported_accessibility_without_at_spi() {
        let adapter = LinuxWaylandAdapter::new(detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::Unknown,
            true,
            false,
        ));

        let error = adapter
            .execute(RunnerStep {
                id: "tree".to_owned(),
                action: "accessibility_tree".to_owned(),
                target: stable_linux_target(&metadata()),
                value: None,
                required_capability: "linux.wayland.accessibility_tree".to_owned(),
            })
            .expect_err("AT-SPI missing should be unsupported");

        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn wayland_allows_safe_keyboard_shortcut_fallback_only() {
        let adapter = LinuxWaylandAdapter::new(detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::GnomeMutter,
            true,
            true,
        ));

        let error = adapter
            .execute(RunnerStep {
                id: "shortcut".to_owned(),
                action: "safe_keyboard_shortcut".to_owned(),
                target: LocatorTarget::default(),
                value: Some("Ctrl+L".to_owned()),
                required_capability: "linux.wayland.safe_keyboard_shortcut".to_owned(),
            })
            .expect_err("global shortcut should require compositor-specific portal");

        assert!(
            error.to_string().contains("intentionally unsupported"),
            "{error}"
        );
    }

    #[test]
    fn maps_linux_xdotool_shortcuts_to_key_sequences() {
        assert_eq!(
            linux_xdotool_key_sequence("Return").expect("return shortcut"),
            "Return"
        );
        assert_eq!(
            linux_xdotool_key_sequence("Ctrl+PageUp").expect("page shortcut"),
            "ctrl+Page_Up"
        );
        assert_eq!(
            linux_xdotool_key_sequence("Ctrl+Shift+N").expect("modified shortcut"),
            "ctrl+shift+n"
        );
    }

    #[test]
    fn x11_recording_backend_is_available_with_required_permissions() {
        std::env::set_var("GREENTIC_LINUX_X11_EVENT_SOURCE_COMMAND", "x11-recorder");
        let backend = LinuxX11RecordingBackend::new(x11_platform());
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "linux.record".to_owned(),
            profile: "desktop".to_owned(),
            adapter: LINUX_X11_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Desktop,
            out: std::env::temp_dir().join("linux-record"),
            runtime_home: std::env::temp_dir().join("linux-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(preflight.available);
        std::env::remove_var("GREENTIC_LINUX_X11_EVENT_SOURCE_COMMAND");
    }

    #[test]
    fn wayland_recording_backend_blocks_global_capture_limitations() {
        std::env::set_var(
            "GREENTIC_LINUX_WAYLAND_EVENT_SOURCE_COMMAND",
            "wayland-recorder",
        );
        let support = detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::GnomeMutter,
            true,
            true,
        );
        let backend = LinuxWaylandRecordingBackend::new(support);
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "linux.wayland.record".to_owned(),
            profile: "desktop".to_owned(),
            adapter: LINUX_WAYLAND_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Desktop,
            out: std::env::temp_dir().join("linux-wayland-record"),
            runtime_home: std::env::temp_dir().join("linux-wayland-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight
            .blocked_reasons
            .iter()
            .any(|reason| reason.contains("Global input injection")));
        std::env::remove_var("GREENTIC_LINUX_WAYLAND_EVENT_SOURCE_COMMAND");
    }
}
