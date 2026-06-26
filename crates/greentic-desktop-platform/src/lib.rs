use greentic_desktop_recorder::RunnerPackage;
use std::collections::BTreeSet;
use std::env;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DesktopPlatform {
    Windows,
    MacOS,
    Linux,
}

impl DesktopPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOS => "macos",
            Self::Linux => "linux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlatformPermission {
    Accessibility,
    ScreenRecording,
    WindowManagement,
    AppLaunch,
    KeyboardInput,
    MouseInput,
    Screenshot,
}

impl PlatformPermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::ScreenRecording => "screen_recording",
            Self::WindowManagement => "window_management",
            Self::AppLaunch => "app_launch",
            Self::KeyboardInput => "keyboard_input",
            Self::MouseInput => "mouse_input",
            Self::Screenshot => "screenshot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformInfo {
    pub os: DesktopPlatform,
    pub version: String,
    pub desktop_environment: Option<String>,
    pub display_server: Option<String>,
    pub permissions: Vec<PlatformPermission>,
}

impl PlatformInfo {
    pub fn has_permission(&self, permission: PlatformPermission) -> bool {
        self.permissions.contains(&permission)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCapability {
    pub name: String,
    pub required_permission: Option<PlatformPermission>,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPermissionExplanation {
    pub permission: PlatformPermission,
    pub granted: bool,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformSupportReport {
    pub supported: bool,
    pub required: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    UnsupportedCapabilities(Vec<String>),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCapabilities(missing) => {
                write!(
                    f,
                    "desktop platform is missing capabilities: {}",
                    missing.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for PlatformError {}

pub fn detect_platform() -> PlatformInfo {
    let os = if cfg!(target_os = "windows") {
        DesktopPlatform::Windows
    } else if cfg!(target_os = "macos") {
        DesktopPlatform::MacOS
    } else {
        DesktopPlatform::Linux
    };
    let display_server = env::var("XDG_SESSION_TYPE")
        .ok()
        .or_else(|| cfg!(target_os = "macos").then(|| "quartz".to_owned()))
        .or_else(|| cfg!(target_os = "windows").then(|| "desktop".to_owned()));
    let desktop_environment = env::var("XDG_CURRENT_DESKTOP").ok();

    let mut permissions = vec![
        PlatformPermission::AppLaunch,
        PlatformPermission::KeyboardInput,
        PlatformPermission::MouseInput,
    ];
    if os == DesktopPlatform::Windows {
        permissions.extend([
            PlatformPermission::Accessibility,
            PlatformPermission::WindowManagement,
            PlatformPermission::Screenshot,
        ]);
    } else if os == DesktopPlatform::MacOS {
        permissions.extend([
            PlatformPermission::Accessibility,
            PlatformPermission::ScreenRecording,
            PlatformPermission::WindowManagement,
            PlatformPermission::Screenshot,
        ]);
    } else if display_server.as_deref() == Some("x11") {
        permissions.extend([
            PlatformPermission::WindowManagement,
            PlatformPermission::Screenshot,
        ]);
    }

    PlatformInfo {
        os,
        version: env::consts::OS.to_owned(),
        desktop_environment,
        display_server,
        permissions,
    }
}

pub fn list_platform_capabilities(info: &PlatformInfo) -> Vec<PlatformCapability> {
    let names = [
        (
            "platform.detect",
            None,
            "platform detection is always available",
        ),
        (
            "platform.permissions.check",
            None,
            "permission state can be inspected from the platform model",
        ),
        (
            "platform.permissions.explain",
            None,
            "permission explanations are generated from the platform model",
        ),
        (
            "platform.open_app",
            Some(PlatformPermission::AppLaunch),
            "opening applications requires app launch permission",
        ),
        (
            "platform.activate_window",
            Some(PlatformPermission::WindowManagement),
            "activating windows requires window management permission",
        ),
        (
            "platform.list_windows",
            Some(PlatformPermission::WindowManagement),
            "listing windows requires window management permission",
        ),
        (
            "platform.screenshot",
            Some(PlatformPermission::Screenshot),
            "screenshots require screenshot or screen recording permission",
        ),
        (
            "platform.input.keyboard",
            Some(PlatformPermission::KeyboardInput),
            "keyboard input requires input permission",
        ),
        (
            "platform.input.mouse",
            Some(PlatformPermission::MouseInput),
            "mouse input requires input permission",
        ),
    ];

    names
        .into_iter()
        .map(|(name, permission, explanation)| {
            let available = permission
                .map(|permission| permission_available(info, permission))
                .unwrap_or(true);
            PlatformCapability {
                name: name.to_owned(),
                required_permission: permission,
                available,
                reason: (!available).then(|| restricted_reason(info, explanation)),
            }
        })
        .collect()
}

pub fn check_platform_permissions(info: &PlatformInfo) -> Vec<PlatformPermissionExplanation> {
    [
        PlatformPermission::Accessibility,
        PlatformPermission::ScreenRecording,
        PlatformPermission::WindowManagement,
        PlatformPermission::AppLaunch,
        PlatformPermission::KeyboardInput,
        PlatformPermission::MouseInput,
        PlatformPermission::Screenshot,
    ]
    .into_iter()
    .map(|permission| explain_platform_permission(info, permission))
    .collect()
}

pub fn explain_platform_permission(
    info: &PlatformInfo,
    permission: PlatformPermission,
) -> PlatformPermissionExplanation {
    let granted = permission_available(info, permission);
    let platform_hint = match (info.os, permission) {
        (DesktopPlatform::MacOS, PlatformPermission::Accessibility) => {
            "macOS requires Accessibility approval for UI automation"
        }
        (DesktopPlatform::MacOS, PlatformPermission::ScreenRecording) => {
            "macOS requires Screen Recording approval for screenshots"
        }
        (DesktopPlatform::Linux, PlatformPermission::Screenshot)
            if info.display_server.as_deref() == Some("wayland") =>
        {
            "Wayland usually restricts global screenshots without a portal"
        }
        (DesktopPlatform::Linux, PlatformPermission::WindowManagement)
            if info.display_server.as_deref() == Some("wayland") =>
        {
            "Wayland usually restricts global window control"
        }
        (DesktopPlatform::Windows, PlatformPermission::Accessibility) => {
            "Windows UI Automation supports accessibility-driven controls"
        }
        _ => "permission is represented by the platform model",
    };
    PlatformPermissionExplanation {
        permission,
        granted,
        explanation: format!(
            "{}: {}",
            if granted { "granted" } else { "missing" },
            platform_hint
        ),
    }
}

pub fn platform_support_report(
    info: &PlatformInfo,
    package: &RunnerPackage,
) -> PlatformSupportReport {
    let available = list_platform_capabilities(info)
        .into_iter()
        .filter(|capability| capability.available)
        .map(|capability| capability.name)
        .collect::<BTreeSet<_>>();
    let required = required_platform_capabilities(package);
    let missing = required
        .iter()
        .filter(|capability| !available.contains(*capability))
        .cloned()
        .collect::<Vec<_>>();

    PlatformSupportReport {
        supported: missing.is_empty(),
        required,
        missing,
    }
}

pub fn reject_unsupported_runner(
    info: &PlatformInfo,
    package: &RunnerPackage,
) -> Result<(), PlatformError> {
    let report = platform_support_report(info, package);
    if report.supported {
        Ok(())
    } else {
        Err(PlatformError::UnsupportedCapabilities(report.missing))
    }
}

pub fn required_platform_capabilities(package: &RunnerPackage) -> Vec<String> {
    let mut capabilities = BTreeSet::new();
    for step in &package.steps {
        for capability in map_runner_capability(&step.required_capability, &step.action) {
            capabilities.insert(capability);
        }
    }
    capabilities.into_iter().collect()
}

fn map_runner_capability(required_capability: &str, action: &str) -> Vec<String> {
    let mut capabilities = Vec::new();
    if required_capability.contains("open_app") || action.contains("open_app") {
        capabilities.push("platform.open_app".to_owned());
    }
    if required_capability.contains("find_window")
        || required_capability.contains("read_window_tree")
        || action.contains("activate")
    {
        capabilities.push("platform.list_windows".to_owned());
        capabilities.push("platform.activate_window".to_owned());
    }
    if required_capability.contains("screenshot")
        || required_capability.contains("capture")
        || action.contains("screenshot")
    {
        capabilities.push("platform.screenshot".to_owned());
    }
    if required_capability.contains("type")
        || required_capability.contains("send_keys")
        || action.contains("type")
    {
        capabilities.push("platform.input.keyboard".to_owned());
    }
    if required_capability.contains("click") || action.contains("click") || action.contains("mouse")
    {
        capabilities.push("platform.input.mouse".to_owned());
    }
    capabilities
}

fn permission_available(info: &PlatformInfo, permission: PlatformPermission) -> bool {
    if permission == PlatformPermission::Screenshot
        && info.has_permission(PlatformPermission::ScreenRecording)
    {
        return true;
    }
    info.has_permission(permission)
}

fn restricted_reason(info: &PlatformInfo, explanation: &str) -> String {
    let display = info.display_server.as_deref().unwrap_or("unknown-display");
    format!("{} on {} ({display})", explanation, info.os.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::{LocatorTarget, RunnerStep};
    use greentic_desktop_recorder::{RecordingMode, RunnerPackage};

    fn info(
        os: DesktopPlatform,
        display_server: Option<&str>,
        permissions: Vec<PlatformPermission>,
    ) -> PlatformInfo {
        PlatformInfo {
            os,
            version: "test".to_owned(),
            desktop_environment: None,
            display_server: display_server.map(str::to_owned),
            permissions,
        }
    }

    fn package(required_capabilities: Vec<&str>) -> RunnerPackage {
        RunnerPackage {
            id: "desktop.runner".to_owned(),
            version: "1.0.0".to_owned(),
            mode: RecordingMode::Hybrid,
            inputs: Vec::new(),
            secrets: Vec::new(),
            steps: required_capabilities
                .into_iter()
                .enumerate()
                .map(|(index, capability)| RunnerStep {
                    id: format!("step_{index}"),
                    action: capability
                        .rsplit('.')
                        .next()
                        .unwrap_or(capability)
                        .to_owned(),
                    target: LocatorTarget::default(),
                    value: None,
                    required_capability: capability.to_owned(),
                })
                .collect(),
            assertions: Vec::new(),
            outputs: Vec::new(),
            open_questions: Vec::new(),
        }
    }

    #[test]
    fn runner_can_detect_current_platform_family() {
        let detected = detect_platform();

        assert!(matches!(
            detected.os,
            DesktopPlatform::Windows | DesktopPlatform::MacOS | DesktopPlatform::Linux
        ));
        assert!(!detected.version.is_empty());
    }

    #[test]
    fn windows_roadmap_capabilities_remain_supported() {
        let windows = info(
            DesktopPlatform::Windows,
            Some("desktop"),
            vec![
                PlatformPermission::Accessibility,
                PlatformPermission::WindowManagement,
                PlatformPermission::AppLaunch,
                PlatformPermission::KeyboardInput,
                PlatformPermission::MouseInput,
                PlatformPermission::Screenshot,
            ],
        );
        let report = platform_support_report(
            &windows,
            &package(vec![
                "windows.open_app",
                "windows.find_window",
                "windows.click_element",
                "windows.type_text",
                "windows.screenshot",
            ]),
        );

        assert!(report.supported);
        assert!(report.required.contains(&"platform.open_app".to_owned()));
        assert!(report
            .required
            .contains(&"platform.activate_window".to_owned()));
    }

    #[test]
    fn macos_explains_accessibility_and_screen_recording_permissions() {
        let macos = info(
            DesktopPlatform::MacOS,
            Some("quartz"),
            vec![
                PlatformPermission::AppLaunch,
                PlatformPermission::KeyboardInput,
            ],
        );

        let explanations = check_platform_permissions(&macos);
        assert!(explanations.iter().any(|entry| {
            entry.permission == PlatformPermission::Accessibility
                && !entry.granted
                && entry.explanation.contains("Accessibility")
        }));
        assert!(explanations.iter().any(|entry| {
            entry.permission == PlatformPermission::ScreenRecording
                && !entry.granted
                && entry.explanation.contains("Screen Recording")
        }));
    }

    #[test]
    fn x11_can_offer_global_window_and_screenshot_control() {
        let x11 = info(
            DesktopPlatform::Linux,
            Some("x11"),
            vec![
                PlatformPermission::WindowManagement,
                PlatformPermission::Screenshot,
                PlatformPermission::KeyboardInput,
                PlatformPermission::MouseInput,
                PlatformPermission::AppLaunch,
            ],
        );

        let capabilities = list_platform_capabilities(&x11);
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "platform.screenshot" && capability.available));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "platform.list_windows" && capability.available));
    }

    #[test]
    fn wayland_rejects_missing_global_screenshot_support() {
        let wayland = info(
            DesktopPlatform::Linux,
            Some("wayland"),
            vec![
                PlatformPermission::KeyboardInput,
                PlatformPermission::MouseInput,
                PlatformPermission::AppLaunch,
            ],
        );

        let err = reject_unsupported_runner(&wayland, &package(vec!["vision.screenshot"]))
            .expect_err("Wayland without screenshot portal should be rejected");

        assert_eq!(
            err,
            PlatformError::UnsupportedCapabilities(vec!["platform.screenshot".to_owned()])
        );
    }

    #[test]
    fn remote_desktops_can_fall_back_to_keyboard_and_mouse_only() {
        let remote = info(
            DesktopPlatform::Linux,
            Some("rdp"),
            vec![
                PlatformPermission::KeyboardInput,
                PlatformPermission::MouseInput,
                PlatformPermission::AppLaunch,
            ],
        );

        let supported = platform_support_report(
            &remote,
            &package(vec!["terminal.send_keys", "windows.click_element"]),
        );
        let unsupported = platform_support_report(&remote, &package(vec!["windows.find_window"]));

        assert!(supported.supported);
        assert_eq!(
            unsupported.missing,
            vec![
                "platform.activate_window".to_owned(),
                "platform.list_windows".to_owned()
            ]
        );
    }
}
