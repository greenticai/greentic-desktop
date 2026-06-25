use greentic_desktop_adapter::{LocatorStrategy, LocatorTarget, RunnerStep};
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo};
use greentic_desktop_recorder::RunnerPackage;

pub const DESKTOP_WINDOWING_ADAPTER_ID: &str = "greentic.desktop.windowing";

pub fn desktop_windowing_capabilities() -> Vec<&'static str> {
    vec![
        "desktop.open_app",
        "desktop.find_window",
        "desktop.activate_window",
        "desktop.list_windows",
        "desktop.close_window",
        "desktop.window_screenshot",
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppLaunchTarget {
    WindowsExecutable { path: String, args: Vec<String> },
    WindowsStartMenu { app_user_model_id: String },
    WindowsPowerShell { command: String },
    MacBundleId { bundle_id: String },
    MacAppName { name: String },
    MacAppPath { path: String },
    LinuxDesktopEntry { desktop_file_id: String },
    LinuxExecutable { command: String, args: Vec<String> },
    LinuxFlatpak { app_id: String },
    LinuxSnap { snap_name: String },
    LinuxAppImage { path: String },
}

impl AppLaunchTarget {
    pub fn platform(&self) -> DesktopPlatform {
        match self {
            Self::WindowsExecutable { .. }
            | Self::WindowsStartMenu { .. }
            | Self::WindowsPowerShell { .. } => DesktopPlatform::Windows,
            Self::MacBundleId { .. } | Self::MacAppName { .. } | Self::MacAppPath { .. } => {
                DesktopPlatform::MacOS
            }
            Self::LinuxDesktopEntry { .. }
            | Self::LinuxExecutable { .. }
            | Self::LinuxFlatpak { .. }
            | Self::LinuxSnap { .. }
            | Self::LinuxAppImage { .. } => DesktopPlatform::Linux,
        }
    }

    pub fn launch_command(&self) -> String {
        match self {
            Self::WindowsExecutable { path, args } => {
                format!("windows:exec {} {}", path, args.join(" "))
                    .trim()
                    .to_owned()
            }
            Self::WindowsStartMenu { app_user_model_id } => {
                format!("windows:start-menu {app_user_model_id}")
            }
            Self::WindowsPowerShell { command } => format!("windows:powershell {command}"),
            Self::MacBundleId { bundle_id } => format!("macos:open-bundle-id {bundle_id}"),
            Self::MacAppName { name } => format!("macos:open-app-name {name}"),
            Self::MacAppPath { path } => format!("macos:open-path {path}"),
            Self::LinuxDesktopEntry { desktop_file_id } => {
                format!("linux:desktop-entry {desktop_file_id}")
            }
            Self::LinuxExecutable { command, args } => {
                format!("linux:exec {} {}", command, args.join(" "))
                    .trim()
                    .to_owned()
            }
            Self::LinuxFlatpak { app_id } => format!("linux:flatpak {app_id}"),
            Self::LinuxSnap { snap_name } => format!("linux:snap {snap_name}"),
            Self::LinuxAppImage { path } => format!("linux:appimage {path}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppLaunchRequirement {
    pub app_id: String,
    pub target: AppLaunchTarget,
    pub expected_window_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopWindow {
    pub window_id: String,
    pub app_id: String,
    pub title: String,
    pub platform: DesktopPlatform,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowScreenshot {
    pub window_id: String,
    pub evidence_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowingError {
    WrongPlatform {
        expected: DesktopPlatform,
        actual: DesktopPlatform,
    },
    WindowNotFound(String),
}

#[derive(Debug, Clone)]
pub struct DesktopWindowManager {
    platform: PlatformInfo,
    windows: Vec<DesktopWindow>,
    launch_log: Vec<String>,
}

impl DesktopWindowManager {
    pub fn new(platform: PlatformInfo) -> Self {
        Self {
            platform,
            windows: Vec::new(),
            launch_log: Vec::new(),
        }
    }

    pub fn launch_log(&self) -> &[String] {
        &self.launch_log
    }

    pub fn open_app(
        &mut self,
        requirement: &AppLaunchRequirement,
    ) -> Result<DesktopWindow, WindowingError> {
        let expected = requirement.target.platform();
        if expected != self.platform.os {
            return Err(WindowingError::WrongPlatform {
                expected,
                actual: self.platform.os,
            });
        }
        let existing = self
            .windows
            .iter()
            .find(|window| window.app_id == requirement.app_id)
            .cloned();
        if let Some(existing) = existing {
            return Ok(existing);
        }

        self.launch_log.push(requirement.target.launch_command());
        let window = DesktopWindow {
            window_id: format!("{}:{}", self.platform.os.as_str(), self.windows.len() + 1),
            app_id: requirement.app_id.clone(),
            title: requirement.expected_window_title.clone(),
            platform: self.platform.os,
            active: false,
        };
        self.windows.push(window.clone());
        Ok(window)
    }

    pub fn list_windows(&self) -> Vec<DesktopWindow> {
        self.windows.clone()
    }

    pub fn find_window(&self, title: &str) -> Result<DesktopWindow, WindowingError> {
        self.windows
            .iter()
            .find(|window| window.title.contains(title) || window.app_id == title)
            .cloned()
            .ok_or_else(|| WindowingError::WindowNotFound(title.to_owned()))
    }

    pub fn activate_window(&mut self, title: &str) -> Result<DesktopWindow, WindowingError> {
        let mut found = None;
        for window in &mut self.windows {
            let is_match = window.title.contains(title) || window.app_id == title;
            window.active = is_match;
            if is_match {
                found = Some(window.clone());
            }
        }
        found.ok_or_else(|| WindowingError::WindowNotFound(title.to_owned()))
    }

    pub fn close_window(&mut self, title: &str) -> Result<DesktopWindow, WindowingError> {
        let index = self
            .windows
            .iter()
            .position(|window| window.title.contains(title) || window.app_id == title)
            .ok_or_else(|| WindowingError::WindowNotFound(title.to_owned()))?;
        Ok(self.windows.remove(index))
    }

    pub fn window_screenshot(&self, title: &str) -> Result<WindowScreenshot, WindowingError> {
        let window = self.find_window(title)?;
        Ok(WindowScreenshot {
            window_id: window.window_id.clone(),
            evidence_uri: format!("evidence://window/{}/screenshot.png", window.window_id),
        })
    }

    pub fn restore_target_app(
        &mut self,
        requirement: &AppLaunchRequirement,
    ) -> Result<DesktopWindow, WindowingError> {
        let window = self.open_app(requirement)?;
        self.activate_window(&window.title)
    }
}

pub fn launch_step(requirement: &AppLaunchRequirement) -> RunnerStep {
    RunnerStep {
        id: format!("open_{}", requirement.app_id),
        action: "open_app".to_owned(),
        target: window_target(&requirement.expected_window_title),
        value: Some(requirement.target.launch_command()),
        required_capability: "desktop.open_app".to_owned(),
    }
}

pub fn restore_steps(requirement: &AppLaunchRequirement) -> Vec<RunnerStep> {
    vec![
        launch_step(requirement),
        RunnerStep {
            id: format!("activate_{}", requirement.app_id),
            action: "activate_window".to_owned(),
            target: window_target(&requirement.expected_window_title),
            value: Some(requirement.expected_window_title.clone()),
            required_capability: "desktop.activate_window".to_owned(),
        },
    ]
}

pub fn attach_launch_requirement(package: &mut RunnerPackage, requirement: &AppLaunchRequirement) {
    let mut restore = restore_steps(requirement);
    restore.append(&mut package.steps);
    package.steps = restore;
}

pub fn platform_specific_capability(
    platform: DesktopPlatform,
    generic: &str,
) -> Option<&'static str> {
    match (platform, generic) {
        (DesktopPlatform::Windows, "desktop.open_app") => Some("windows.open_app"),
        (DesktopPlatform::Windows, "desktop.find_window") => Some("windows.find_window"),
        (DesktopPlatform::Windows, "desktop.activate_window") => Some("windows.find_window"),
        (DesktopPlatform::Windows, "desktop.close_window") => Some("windows.close_app"),
        (DesktopPlatform::Windows, "desktop.window_screenshot") => Some("windows.screenshot"),
        (DesktopPlatform::MacOS, "desktop.open_app") => Some("macos.find_app"),
        (DesktopPlatform::MacOS, "desktop.find_window") => Some("macos.find_window"),
        (DesktopPlatform::MacOS, "desktop.activate_window") => Some("macos.activate_app"),
        (DesktopPlatform::MacOS, "desktop.close_window") => Some("macos.close_app"),
        (DesktopPlatform::MacOS, "desktop.window_screenshot") => Some("macos.screenshot"),
        (DesktopPlatform::Linux, "desktop.open_app") => Some("linux.find_window"),
        (DesktopPlatform::Linux, "desktop.find_window") => Some("linux.find_window"),
        (DesktopPlatform::Linux, "desktop.activate_window") => Some("linux.activate_window"),
        (DesktopPlatform::Linux, "desktop.close_window") => Some("linux.close_window"),
        (DesktopPlatform::Linux, "desktop.window_screenshot") => Some("linux.screenshot"),
        (_, "desktop.list_windows") => Some("platform.list_windows"),
        _ => None,
    }
}

fn window_target(title: &str) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            name: Some(title.to_owned()),
            ..LocatorStrategy::default()
        }),
        ..LocatorTarget::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_recorder::{RecordingMode, RunnerPackage};

    fn platform(os: DesktopPlatform) -> PlatformInfo {
        PlatformInfo {
            os,
            version: "test".to_owned(),
            desktop_environment: None,
            display_server: None,
            permissions: Vec::new(),
        }
    }

    fn package() -> RunnerPackage {
        RunnerPackage {
            id: "crm.create_customer".to_owned(),
            version: "1.0.0".to_owned(),
            mode: RecordingMode::Hybrid,
            inputs: Vec::new(),
            secrets: Vec::new(),
            steps: vec![RunnerStep {
                id: "fill".to_owned(),
                action: "type_text".to_owned(),
                target: LocatorTarget::default(),
                value: Some("Acme".to_owned()),
                required_capability: "web.fill".to_owned(),
            }],
            assertions: Vec::new(),
            outputs: Vec::new(),
        }
    }

    #[test]
    fn runner_packages_can_declare_portable_app_launch_requirements() {
        let requirement = AppLaunchRequirement {
            app_id: "crm".to_owned(),
            target: AppLaunchTarget::WindowsExecutable {
                path: "C:\\CRM\\crm.exe".to_owned(),
                args: vec!["--tenant".to_owned(), "acme".to_owned()],
            },
            expected_window_title: "CRM".to_owned(),
        };
        let step = launch_step(&requirement);

        assert_eq!(step.required_capability, "desktop.open_app");
        assert!(step.value.expect("command").contains("windows:exec"));
    }

    #[test]
    fn macos_bundle_ids_are_supported() {
        let requirement = AppLaunchRequirement {
            app_id: "crm".to_owned(),
            target: AppLaunchTarget::MacBundleId {
                bundle_id: "com.example.crm".to_owned(),
            },
            expected_window_title: "CRM".to_owned(),
        };
        let mut manager = DesktopWindowManager::new(platform(DesktopPlatform::MacOS));

        let window = manager.open_app(&requirement).expect("mac app opens");

        assert_eq!(window.title, "CRM");
        assert_eq!(
            manager.launch_log(),
            &["macos:open-bundle-id com.example.crm".to_owned()]
        );
    }

    #[test]
    fn linux_desktop_entries_are_supported() {
        let requirement = AppLaunchRequirement {
            app_id: "crm".to_owned(),
            target: AppLaunchTarget::LinuxDesktopEntry {
                desktop_file_id: "crm.desktop".to_owned(),
            },
            expected_window_title: "CRM".to_owned(),
        };
        let mut manager = DesktopWindowManager::new(platform(DesktopPlatform::Linux));

        let window = manager.open_app(&requirement).expect("linux app opens");

        assert_eq!(window.platform, DesktopPlatform::Linux);
        assert_eq!(manager.launch_log()[0], "linux:desktop-entry crm.desktop");
    }

    #[test]
    fn windows_behaviour_remains_compatible() {
        assert_eq!(
            platform_specific_capability(DesktopPlatform::Windows, "desktop.open_app"),
            Some("windows.open_app")
        );
        assert_eq!(
            platform_specific_capability(DesktopPlatform::Windows, "desktop.close_window"),
            Some("windows.close_app")
        );
    }

    #[test]
    fn window_manager_can_list_find_activate_close_and_screenshot() {
        let requirement = AppLaunchRequirement {
            app_id: "crm".to_owned(),
            target: AppLaunchTarget::LinuxExecutable {
                command: "crm".to_owned(),
                args: Vec::new(),
            },
            expected_window_title: "CRM".to_owned(),
        };
        let mut manager = DesktopWindowManager::new(platform(DesktopPlatform::Linux));
        manager.open_app(&requirement).expect("app opens");

        assert_eq!(manager.list_windows().len(), 1);
        assert_eq!(manager.find_window("CRM").expect("window").app_id, "crm");
        assert!(manager.activate_window("CRM").expect("active").active);
        assert!(manager
            .window_screenshot("CRM")
            .expect("shot")
            .evidence_uri
            .contains("evidence://window"));
        assert_eq!(manager.close_window("CRM").expect("closed").title, "CRM");
        assert!(manager.list_windows().is_empty());
    }

    #[test]
    fn replay_can_restore_target_app_before_steps() {
        let requirement = AppLaunchRequirement {
            app_id: "crm".to_owned(),
            target: AppLaunchTarget::MacAppName {
                name: "CRM".to_owned(),
            },
            expected_window_title: "CRM".to_owned(),
        };
        let mut package = package();
        attach_launch_requirement(&mut package, &requirement);

        assert_eq!(package.steps[0].required_capability, "desktop.open_app");
        assert_eq!(
            package.steps[1].required_capability,
            "desktop.activate_window"
        );
        assert_eq!(package.steps[2].id, "fill");

        let mut manager = DesktopWindowManager::new(platform(DesktopPlatform::MacOS));
        let restored = manager
            .restore_target_app(&requirement)
            .expect("window restored");
        assert!(restored.active);
    }
}
