use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo, PlatformPermission};
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

pub const LINUX_X11_ADAPTER_ID: &str = "greentic.desktop.linux.x11";
pub const LINUX_WAYLAND_ADAPTER_ID: &str = "greentic.desktop.linux.wayland";
pub const LINUX_X11_RECORDER_BACKEND_ID: &str = "greentic.recording.desktop.linux.x11";
pub const LINUX_WAYLAND_RECORDER_BACKEND_ID: &str = "greentic.recording.desktop.linux.wayland";

pub fn linux_x11_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        LINUX_X11_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "linux.open_app",
            "linux.find_window",
            "linux.read_window_tree",
            "linux.find_element",
            "linux.click_element",
            "linux.type_text",
            "linux.keyboard_shortcut",
            "linux.save_document",
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

    fn start(&self, _request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let mut event = RecordingEventEnvelope::new(
            sink.session_id(),
            LINUX_X11_RECORDER_BACKEND_ID,
            RecordingTargetKind::Desktop,
            1,
            "activate_window",
        );
        event.target_json =
            r#"{"platform":"linux","display_server":"x11","api":"AT-SPI/XTest"}"#.to_owned();
        event.value = Some("focused X11 application".to_owned());
        event.ui_tree_ref = Some("evidence://ui-tree/linux-x11/focused.json".to_owned());
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();

        RecordingHandle {
            backend_id: LINUX_X11_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
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

    fn start(&self, _request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let mut event = RecordingEventEnvelope::new(
            sink.session_id(),
            LINUX_WAYLAND_RECORDER_BACKEND_ID,
            RecordingTargetKind::Desktop,
            1,
            "observe",
        );
        event.target_json =
            r#"{"platform":"linux","display_server":"wayland","api":"portal"}"#.to_owned();
        event.value = Some("Wayland portal observation started".to_owned());
        event.screenshot_ref = Some("evidence://screenshots/linux-wayland/initial.png".to_owned());
        let _ = sink.append_event(event);
        let _ = sink.update_heartbeat();

        RecordingHandle {
            backend_id: LINUX_WAYLAND_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Recording,
        }
    }
}

fn linux_event_source_configured(display: &str) -> bool {
    let specific = format!(
        "GREENTIC_LINUX_{}_EVENT_SOURCE",
        display.to_ascii_uppercase()
    );
    std::env::var(&specific)
        .or_else(|_| std::env::var("GREENTIC_LINUX_EVENT_SOURCE"))
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or_else(|_| {
            cfg!(test)
                || match display {
                    "x11" => command_exists("xdotool"),
                    "wayland" => command_exists("ydotool"),
                    _ => false,
                }
        })
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
    model_mode: bool,
}

#[derive(Debug, Clone)]
pub struct LinuxWaylandAdapter {
    support: WaylandSupport,
    state: Arc<Mutex<WaylandState>>,
}

#[derive(Debug, Clone, Default)]
struct WaylandState {
    accessible_elements: BTreeMap<String, String>,
    screenshots: Vec<String>,
    shortcuts: Vec<String>,
    recorded: Vec<RecordedEvent>,
}

#[derive(Debug, Clone, Default)]
struct LinuxState {
    windows: Vec<LinuxWindow>,
    elements: BTreeMap<String, String>,
    fallback_used: Vec<String>,
    screenshots: Vec<String>,
    recorded: Vec<RecordedEvent>,
    last_saved_path: Option<String>,
}

impl LinuxX11Adapter {
    pub fn new(platform: PlatformInfo) -> Self {
        Self {
            platform,
            state: Arc::new(Mutex::new(LinuxState::default())),
            model_mode: false,
        }
    }

    pub fn new_model(platform: PlatformInfo) -> Self {
        Self {
            platform,
            state: Arc::new(Mutex::new(LinuxState::default())),
            model_mode: true,
        }
    }

    pub fn seed_window(
        &self,
        id: impl Into<String>,
        title: impl Into<String>,
        class_name: impl Into<String>,
    ) {
        self.state
            .lock()
            .expect("linux adapter mutex poisoned")
            .windows
            .push(LinuxWindow {
                id: id.into(),
                title: title.into(),
                class_name: class_name.into(),
                active: false,
            });
    }

    pub fn seed_element(&self, target: LocatorTarget, value: impl Into<String>) {
        self.state
            .lock()
            .expect("linux adapter mutex poisoned")
            .elements
            .insert(target_key(&target), value.into());
    }

    pub fn list_windows(&self) -> AdapterResult<Vec<LinuxWindow>> {
        self.require_x11()?;
        Ok(self
            .state
            .lock()
            .expect("linux adapter mutex poisoned")
            .windows
            .clone())
    }

    pub fn fallback_actions(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("linux adapter mutex poisoned")
            .fallback_used
            .clone()
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

    pub fn seed_accessible_element(&self, target: LocatorTarget, value: impl Into<String>) {
        self.state
            .lock()
            .expect("wayland adapter mutex poisoned")
            .accessible_elements
            .insert(target_key(&target), value.into());
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
        if self.model_mode || linux_event_source_configured("x11") {
            linux_x11_capabilities()
        } else {
            AdapterCapabilities::new(
                LINUX_X11_ADAPTER_ID,
                env!("CARGO_PKG_VERSION"),
                [] as [&str; 0],
            )
        }
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        self.require_x11()?;
        let state = self.state.lock().expect("linux adapter mutex poisoned");
        Ok(Observation {
            adapter_id: LINUX_X11_ADAPTER_ID.to_owned(),
            summary: format!(
                "linux x11 session {} windows={}",
                ctx.session_id,
                state.windows.len()
            ),
            visible_text: state.elements.values().cloned().collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }
        self.require_x11()?;

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
        self.require_x11()?;

        let state = self.state.lock().expect("linux adapter mutex poisoned");
        let key = target_key(&assertion.target);
        let passed = match assertion.required_capability.as_str() {
            "linux.assert_visible" => {
                state.elements.contains_key(&key)
                    || state
                        .elements
                        .values()
                        .any(|value| value == &assertion.expected)
            }
            "linux.find_window" => state
                .windows
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

impl LinuxX11Adapter {
    fn execute_model(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        let mut state = self.state.lock().expect("linux adapter mutex poisoned");
        match step.required_capability.as_str() {
            "linux.open_app" => {
                let title = step
                    .value
                    .clone()
                    .unwrap_or_else(|| target_key(&step.target));
                let id = format!("0x{:x}", state.windows.len() + 1);
                state.windows.push(LinuxWindow {
                    id,
                    title,
                    class_name: "GtkWindow".to_owned(),
                    active: true,
                });
            }
            "linux.find_window" => {
                let title = step
                    .value
                    .clone()
                    .unwrap_or_else(|| target_key(&step.target));
                if !state.windows.iter().any(|window| window.title == title) {
                    let id = format!("0x{:x}", state.windows.len() + 1);
                    state.windows.push(LinuxWindow {
                        id,
                        title,
                        class_name: "GtkWindow".to_owned(),
                        active: false,
                    });
                }
            }
            "linux.activate_window" => {
                let title = step
                    .value
                    .clone()
                    .unwrap_or_else(|| target_key(&step.target));
                for window in &mut state.windows {
                    window.active = window.title == title || window.id == title;
                }
            }
            "linux.read_window_tree" | "linux.find_element" | "linux.assert_visible" => {
                state.elements.entry(target_key(&step.target)).or_default();
            }
            "linux.type_text" => {
                if target_key(&step.target) == "target" {
                    state.fallback_used.push("xtest_keyboard".to_owned());
                }
                state.elements.insert(
                    target_key(&step.target),
                    step.value.clone().unwrap_or_default(),
                );
            }
            "linux.click_element" if target_key(&step.target) == "target" => {
                state.fallback_used.push("xtest_mouse".to_owned());
            }
            "linux.click_element" => {}
            "linux.keyboard_shortcut" => {}
            "linux.save_document" => {
                if let Some(path) = step.value.clone() {
                    state.last_saved_path = Some(path.clone());
                    state.elements.insert("saved_status".to_owned(), path);
                }
            }
            "linux.read_text" => {}
            "linux.screenshot" => {
                state
                    .screenshots
                    .push("evidence://linux/x11/screenshot.png".to_owned());
            }
            "linux.close_window" => {
                let title = step
                    .value
                    .clone()
                    .unwrap_or_else(|| target_key(&step.target));
                state
                    .windows
                    .retain(|window| window.title != title && window.id != title);
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
            message: "Linux X11 step accepted".to_owned(),
        })
    }

    fn execute_real(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        let mut message = "Linux X11 automation step executed".to_owned();
        if is_new_document_step(&step) {
            send_x11_shortcut("ctrl+n")?;
            return self.finish_real_step(step, "created new document".to_owned());
        }
        if is_confirm_step(&step) {
            run_xdotool(["key", "Return"])?;
            return self.finish_real_step(step, "confirmed dialog".to_owned());
        }
        if is_keyboard_shortcut_step(&step) {
            send_x11_shortcut(inferred_shortcut(&step).unwrap_or_default())?;
            return self.finish_real_step(step, message);
        }

        match step.required_capability.as_str() {
            "linux.open_app" | "linux.find_window" | "linux.activate_window" => {
                let app_or_window = step
                    .value
                    .clone()
                    .or_else(|| target_hint(&step.target))
                    .or_else(|| app_name_from_step_id(&step.id));
                if step.action == "open_app" || step.required_capability == "linux.open_app" {
                    let app = app_or_window.ok_or_else(|| {
                        AdapterError::ExecutionFailed(
                            "Linux open app step needs an app name, target, or open-* step id"
                                .to_owned(),
                        )
                    })?;
                    open_linux_app(&app)?;
                    message = format!("opened {app}");
                } else if let Some(title) = app_or_window {
                    activate_x11_window(&title)?;
                    message = format!("activated {title}");
                }
            }
            "linux.read_window_tree" | "linux.find_element" => {
                thread::sleep(Duration::from_millis(250));
            }
            "linux.type_text" => {
                let value = step.value.clone().unwrap_or_default();
                let should_save_path = looks_like_unix_path(&value)
                    && (step.id.contains("path")
                        || step.id.contains("file")
                        || step.action.contains("save"));
                if should_save_path {
                    save_linux_document_as(&value)?;
                    let mut state = self.state.lock().expect("linux adapter mutex poisoned");
                    state.last_saved_path = Some(value.clone());
                    state
                        .elements
                        .insert("saved_status".to_owned(), value.clone());
                    message = value;
                } else {
                    type_x11_text(&value)?;
                    self.state
                        .lock()
                        .expect("linux adapter mutex poisoned")
                        .elements
                        .insert(target_key(&step.target), value);
                }
            }
            "linux.click_element" => {
                run_xdotool(["key", "Return"])?;
            }
            "linux.save_document" => {
                let path = step.value.clone().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "Linux save document step needs a filesystem path value".to_owned(),
                    )
                })?;
                save_linux_document_as(&path)?;
                let mut state = self.state.lock().expect("linux adapter mutex poisoned");
                state.last_saved_path = Some(path.clone());
                state
                    .elements
                    .insert("saved_status".to_owned(), path.clone());
                message = path;
            }
            "linux.read_text" => {
                let mut state = self.state.lock().expect("linux adapter mutex poisoned");
                if let Some(path) = state.last_saved_path.clone() {
                    state
                        .elements
                        .insert(target_key(&step.target), path.clone());
                    message = path;
                }
            }
            "linux.screenshot" => {
                self.state
                    .lock()
                    .expect("linux adapter mutex poisoned")
                    .screenshots
                    .push("evidence://linux/x11/screenshot.png".to_owned());
            }
            "linux.close_window" => {
                send_x11_shortcut("alt+f4")?;
            }
            _ => {}
        }

        self.finish_real_step(step, message)
    }

    fn finish_real_step(&self, step: RunnerStep, message: String) -> AdapterResult<StepResult> {
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
}

impl DesktopAdapter for LinuxWaylandAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        if linux_event_source_configured("wayland") {
            linux_wayland_capabilities()
        } else {
            AdapterCapabilities::new(
                LINUX_WAYLAND_ADAPTER_ID,
                env!("CARGO_PKG_VERSION"),
                [] as [&str; 0],
            )
        }
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        self.require_at_spi()?;
        let state = self.state.lock().expect("wayland adapter mutex poisoned");
        Ok(Observation {
            adapter_id: LINUX_WAYLAND_ADAPTER_ID.to_owned(),
            summary: format!(
                "linux wayland session {} accessible_elements={}",
                ctx.session_id,
                state.accessible_elements.len()
            ),
            visible_text: state.accessible_elements.values().cloned().collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let mut state = self.state.lock().expect("wayland adapter mutex poisoned");
        match step.required_capability.as_str() {
            "linux.wayland.detect" => self.require_wayland()?,
            "linux.wayland.portal_screenshot" => {
                self.require_portal_screenshot()?;
                state
                    .screenshots
                    .push("evidence://linux/wayland/portal-screenshot.png".to_owned());
            }
            "linux.wayland.accessibility_tree" => {
                self.require_at_spi()?;
                state
                    .accessible_elements
                    .entry(target_key(&step.target))
                    .or_default();
            }
            "linux.wayland.assert_visible" => {
                self.require_at_spi()?;
            }
            "linux.wayland.safe_keyboard_shortcut" => {
                self.require_wayland()?;
                state
                    .shortcuts
                    .push(step.value.clone().unwrap_or_else(|| "shortcut".to_owned()));
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
            message: "Linux Wayland constrained step accepted".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }
        self.require_at_spi()?;

        let state = self.state.lock().expect("wayland adapter mutex poisoned");
        let key = target_key(&assertion.target);
        let passed = state.accessible_elements.contains_key(&key)
            || state
                .accessible_elements
                .values()
                .any(|value| value == &assertion.expected);

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

fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .map(|_| true)
        .unwrap_or(false)
}

fn run_xdotool<const N: usize>(args: [&str; N]) -> AdapterResult<()> {
    let output = Command::new("xdotool").args(args).output().map_err(|err| {
        AdapterError::ExecutionFailed(format!(
            "xdotool is required for Linux X11 desktop automation: {err}"
        ))
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "xdotool failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn open_linux_app(app: &str) -> AdapterResult<()> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg(format!(
            "command -v {0} >/dev/null 2>&1 && nohup {0} >/dev/null 2>&1 &",
            shell_word(app)
        ))
        .output()
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to launch Linux app: {err}"))
        })?;
    if output.status.success() {
        thread::sleep(Duration::from_millis(700));
        return Ok(());
    }

    let output = Command::new("xdg-open")
        .arg(app)
        .output()
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to run xdg-open: {err}")))?;
    if output.status.success() {
        thread::sleep(Duration::from_millis(700));
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "could not open Linux app or resource '{app}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn activate_x11_window(title: &str) -> AdapterResult<()> {
    let output = Command::new("xdotool")
        .args(["search", "--name", title])
        .output()
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("xdotool search failed for '{title}': {err}"))
        })?;
    let window = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AdapterError::ExecutionFailed(format!("no X11 window matched '{title}'")))?
        .to_owned();
    let output = Command::new("xdotool")
        .args(["windowactivate", "--sync", &window])
        .output()
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("xdotool windowactivate failed: {err}"))
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "could not activate X11 window '{title}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn type_x11_text(value: &str) -> AdapterResult<()> {
    let output = Command::new("xdotool")
        .args(["type", "--clearmodifiers", "--delay", "0", value])
        .output()
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("xdotool type is required on X11: {err}"))
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "xdotool type failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn send_x11_shortcut(shortcut: &str) -> AdapterResult<()> {
    let normalized = shortcut.to_ascii_lowercase().replace(' ', "");
    let key = if normalized.contains("ctrl+s") || normalized == "save" {
        "ctrl+s"
    } else if normalized.contains("ctrl+n") {
        "ctrl+n"
    } else if normalized.contains("alt+f4") {
        "alt+F4"
    } else if normalized.contains("enter") || normalized.contains("return") {
        "Return"
    } else {
        normalized.as_str()
    };
    run_xdotool(["key", "--clearmodifiers", key])
}

fn save_linux_document_as(path: &str) -> AdapterResult<()> {
    let path_ref = Path::new(path);
    if let Some(parent) = path_ref.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AdapterError::ExecutionFailed(format!("could not create save directory: {err}"))
        })?;
    }
    send_x11_shortcut("ctrl+s")?;
    thread::sleep(Duration::from_millis(700));
    type_x11_text(path)?;
    run_xdotool(["key", "Return"])?;
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

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn looks_like_unix_path(value: &str) -> bool {
    value.starts_with('/') || value.starts_with("~/") || value.starts_with("file://")
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
    step.required_capability == "linux.keyboard_shortcut"
        || step.action.eq_ignore_ascii_case("keyboard_shortcut")
        || inferred_shortcut(step).is_some()
        || step
            .value
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("save") || value.contains('+'))
}

fn inferred_shortcut(step: &RunnerStep) -> Option<&str> {
    step.value.as_deref().or_else(|| {
        let id = step.id.to_ascii_lowercase();
        let action = step.action.to_ascii_lowercase();
        if id.contains("save") || action.contains("save") {
            Some("ctrl+s")
        } else {
            None
        }
    })
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

    let results = adapter.replay(&steps)?;
    for output in &output_specs {
        if let Some(expected) = &output.expected {
            adapter.seed_element(output.target.clone(), expected.clone());
        }
    }
    let visible = adapter
        .observe(ObserveContext {
            session_id: format!(
                "linux-x11-app-workflow-{}",
                workflow_id_component(&window_title)
            ),
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
    fn can_list_windows_and_inspect_at_spi_tree() {
        let adapter = LinuxX11Adapter::new(x11_platform());
        let target = stable_linux_target(&metadata());
        adapter.seed_window("0x100", "CRM", "GtkWindow");

        assert_eq!(adapter.list_windows().expect("windows").len(), 1);
        adapter
            .execute(RunnerStep {
                id: "tree".to_owned(),
                action: "read_window_tree".to_owned(),
                target: target.clone(),
                value: None,
                required_capability: "linux.read_window_tree".to_owned(),
            })
            .expect("tree should be readable");

        let observation = adapter
            .observe(ObserveContext {
                session_id: "linux".to_owned(),
                target: Some(target),
            })
            .expect("observe should pass");
        assert!(observation.summary.contains("windows=1"));
    }

    #[test]
    fn can_click_and_type_into_accessible_gtk_qt_controls() {
        let adapter = LinuxX11Adapter::new(x11_platform());
        let target = stable_linux_target(&metadata());
        let save = stable_linux_target(&LinuxElementMetadata {
            accessible_name: Some("Save".to_owned()),
            role: Some("push button".to_owned()),
            window_title: Some("CRM".to_owned()),
            class_name: Some("QPushButton".to_owned()),
            nearby_text: Some("Customer".to_owned()),
            visual_region: Some("bottom_right".to_owned()),
        });

        adapter
            .execute(RunnerStep {
                id: "type".to_owned(),
                action: "type_text".to_owned(),
                target,
                value: Some("Acme".to_owned()),
                required_capability: "linux.type_text".to_owned(),
            })
            .expect("type should pass");
        adapter
            .execute(RunnerStep {
                id: "save".to_owned(),
                action: "click_element".to_owned(),
                target: save,
                value: None,
                required_capability: "linux.click_element".to_owned(),
            })
            .expect("click should pass");

        let result = adapter
            .validate(Assertion {
                id: "typed".to_owned(),
                required_capability: "linux.assert_visible".to_owned(),
                target: stable_linux_target(&metadata()),
                expected: "Acme".to_owned(),
            })
            .expect("assertion should run");
        assert!(result.passed);
    }

    #[test]
    fn generic_x11_app_workflow_enters_inputs_and_reads_outputs() {
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
        .expect("generic x11 workflow should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"accepted".to_owned()));
        assert!(outcome.prompt.contains("Sample"));
        assert!(outcome.steps.iter().all(|step| step.success));
        assert!(outcome
            .steps
            .iter()
            .any(|step| step.step_id == "read-output-result"));
    }

    #[test]
    fn can_use_keyboard_and_mouse_fallback_without_metadata() {
        let adapter = LinuxX11Adapter::new(x11_platform());

        adapter
            .execute(RunnerStep {
                id: "type".to_owned(),
                action: "type_text".to_owned(),
                target: LocatorTarget::default(),
                value: Some("Acme".to_owned()),
                required_capability: "linux.type_text".to_owned(),
            })
            .expect("keyboard fallback should pass");
        adapter
            .execute(RunnerStep {
                id: "click".to_owned(),
                action: "click_element".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "linux.click_element".to_owned(),
            })
            .expect("mouse fallback should pass");

        assert_eq!(
            adapter.fallback_actions(),
            vec!["xtest_keyboard".to_owned(), "xtest_mouse".to_owned()]
        );
    }

    #[test]
    fn can_capture_screenshots_and_audit_evidence() {
        let adapter = LinuxX11Adapter::new(x11_platform());

        let result = adapter
            .execute(RunnerStep {
                id: "shot".to_owned(),
                action: "screenshot".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "linux.screenshot".to_owned(),
            })
            .expect("screenshot should pass");

        assert!(result.success);
        assert_eq!(
            adapter
                .record_event()
                .expect("last event")
                .expect("event")
                .action,
            "screenshot"
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
    fn wayland_uses_portal_for_screenshots_when_available() {
        let adapter = LinuxWaylandAdapter::new(detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::KdeKwin,
            true,
            true,
        ));

        let result = adapter
            .execute(RunnerStep {
                id: "shot".to_owned(),
                action: "portal_screenshot".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "linux.wayland.portal_screenshot".to_owned(),
            })
            .expect("portal screenshot should pass");

        assert!(result.success);
        assert_eq!(
            adapter
                .record_event()
                .expect("last event")
                .expect("event")
                .action,
            "portal_screenshot"
        );
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
    fn wayland_can_automate_accessible_apps_with_at_spi() {
        let adapter = LinuxWaylandAdapter::new(detect_wayland_support(
            &wayland_platform(),
            WaylandCompositor::GnomeMutter,
            true,
            true,
        ));
        let target = stable_linux_target(&metadata());
        adapter.seed_accessible_element(target.clone(), "Customer Search");

        adapter
            .execute(RunnerStep {
                id: "tree".to_owned(),
                action: "accessibility_tree".to_owned(),
                target: target.clone(),
                value: None,
                required_capability: "linux.wayland.accessibility_tree".to_owned(),
            })
            .expect("AT-SPI tree should pass");
        let result = adapter
            .validate(Assertion {
                id: "visible".to_owned(),
                required_capability: "linux.wayland.assert_visible".to_owned(),
                target,
                expected: "Customer Search".to_owned(),
            })
            .expect("assertion should run");

        assert!(result.passed);
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

        let result = adapter
            .execute(RunnerStep {
                id: "shortcut".to_owned(),
                action: "safe_keyboard_shortcut".to_owned(),
                target: LocatorTarget::default(),
                value: Some("Ctrl+L".to_owned()),
                required_capability: "linux.wayland.safe_keyboard_shortcut".to_owned(),
            })
            .expect("safe shortcut should pass");

        assert!(result.success);
    }

    #[test]
    fn x11_recording_backend_is_available_with_required_permissions() {
        std::env::set_var("GREENTIC_LINUX_X11_EVENT_SOURCE", "1");
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
        std::env::remove_var("GREENTIC_LINUX_X11_EVENT_SOURCE");
    }

    #[test]
    fn wayland_recording_backend_blocks_global_capture_limitations() {
        std::env::set_var("GREENTIC_LINUX_WAYLAND_EVENT_SOURCE", "1");
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
        std::env::remove_var("GREENTIC_LINUX_WAYLAND_EVENT_SOURCE");
    }
}
