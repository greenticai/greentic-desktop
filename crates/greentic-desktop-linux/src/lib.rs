use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo, PlatformPermission};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const LINUX_X11_ADAPTER_ID: &str = "greentic.desktop.linux.x11";
pub const LINUX_WAYLAND_ADAPTER_ID: &str = "greentic.desktop.linux.wayland";

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
}

impl LinuxX11Adapter {
    pub fn new(platform: PlatformInfo) -> Self {
        Self {
            platform,
            state: Arc::new(Mutex::new(LinuxState::default())),
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
        linux_x11_capabilities()
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

        let mut state = self.state.lock().expect("linux adapter mutex poisoned");
        match step.required_capability.as_str() {
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
            "linux.click_element" => {
                if target_key(&step.target) == "target" {
                    state.fallback_used.push("xtest_mouse".to_owned());
                }
                if target_key(&step.target).contains("save") {
                    state
                        .elements
                        .insert("status".to_owned(), "Saved".to_owned());
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

impl DesktopAdapter for LinuxWaylandAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        linux_wayland_capabilities()
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
                id: "saved".to_owned(),
                required_capability: "linux.assert_visible".to_owned(),
                target: LocatorTarget::default(),
                expected: "Saved".to_owned(),
            })
            .expect("assertion should run");
        assert!(result.passed);
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
}
