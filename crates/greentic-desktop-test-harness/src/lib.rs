use greentic_desktop_linux::{detect_wayland_support, detect_x11_session, WaylandCompositor};
use greentic_desktop_macos::first_run_permission_check;
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo, PlatformPermission};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessEnvironment {
    MacOsGithubActions,
    UbuntuX11VirtualDisplay,
    UbuntuWaylandDetection,
    WindowsGithubActions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleTargetKind {
    Gtk,
    Qt,
    SwiftUiAppKit,
    JavaSwing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleTarget {
    pub kind: SampleTargetKind,
    pub path: String,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessJob {
    pub name: String,
    pub environment: HarnessEnvironment,
    pub command: String,
    pub permission_gated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessPlan {
    pub jobs: Vec<HarnessJob>,
    pub sample_targets: Vec<SampleTarget>,
}

impl HarnessPlan {
    pub fn job(&self, name: &str) -> Option<&HarnessJob> {
        self.jobs.iter().find(|job| job.name == name)
    }

    pub fn has_sample(&self, kind: SampleTargetKind) -> bool {
        self.sample_targets.iter().any(|target| target.kind == kind)
    }
}

pub fn desktop_harness_plan() -> HarnessPlan {
    HarnessPlan {
        jobs: vec![
            HarnessJob {
                name: "macos-unit".to_owned(),
                environment: HarnessEnvironment::MacOsGithubActions,
                command: "cargo test -p greentic-desktop-macos -p greentic-desktop-platform"
                    .to_owned(),
                permission_gated: false,
            },
            HarnessJob {
                name: "macos-manual-permission".to_owned(),
                environment: HarnessEnvironment::MacOsGithubActions,
                command: "cargo test -p greentic-desktop-macos -- --ignored permission_gated"
                    .to_owned(),
                permission_gated: true,
            },
            HarnessJob {
                name: "ubuntu-x11-virtual-display".to_owned(),
                environment: HarnessEnvironment::UbuntuX11VirtualDisplay,
                command: "xvfb-run -a cargo test -p greentic-desktop-linux -- can_detect_x11_session can_list_windows_and_inspect_at_spi_tree"
                    .to_owned(),
                permission_gated: false,
            },
            HarnessJob {
                name: "ubuntu-wayland-detection".to_owned(),
                environment: HarnessEnvironment::UbuntuWaylandDetection,
                command: "cargo test -p greentic-desktop-linux -- detects_wayland_and_reports_global_restrictions wayland_requires_manual_approval_when_portal_is_missing"
                    .to_owned(),
                permission_gated: false,
            },
            HarnessJob {
                name: "windows-unit".to_owned(),
                environment: HarnessEnvironment::WindowsGithubActions,
                command: "cargo test -p greentic-desktop-windows -p greentic-desktop-java"
                    .to_owned(),
                permission_gated: false,
            },
        ],
        sample_targets: vec![
            SampleTarget {
                kind: SampleTargetKind::Gtk,
                path: "examples/desktop-targets/gtk".to_owned(),
                purpose: "Linux AT-SPI GTK accessibility target".to_owned(),
            },
            SampleTarget {
                kind: SampleTargetKind::Qt,
                path: "examples/desktop-targets/qt".to_owned(),
                purpose: "Linux AT-SPI Qt accessibility target".to_owned(),
            },
            SampleTarget {
                kind: SampleTargetKind::SwiftUiAppKit,
                path: "examples/desktop-targets/macos-swiftui".to_owned(),
                purpose: "macOS SwiftUI/AppKit accessibility target".to_owned(),
            },
            SampleTarget {
                kind: SampleTargetKind::JavaSwing,
                path: "examples/desktop-targets/java-swing".to_owned(),
                purpose: "Cross-platform Java Swing accessibility target".to_owned(),
            },
        ],
    }
}

pub fn macos_unit_harness_result() -> Vec<String> {
    let info = PlatformInfo {
        os: DesktopPlatform::MacOS,
        version: "github-actions".to_owned(),
        desktop_environment: Some("Aqua".to_owned()),
        display_server: Some("quartz".to_owned()),
        permissions: Vec::new(),
    };
    first_run_permission_check(&info).messages
}

pub fn ubuntu_x11_harness_detects_x11() -> bool {
    let info = PlatformInfo {
        os: DesktopPlatform::Linux,
        version: "ubuntu".to_owned(),
        desktop_environment: Some("GNOME".to_owned()),
        display_server: Some("x11".to_owned()),
        permissions: vec![
            PlatformPermission::WindowManagement,
            PlatformPermission::Screenshot,
            PlatformPermission::KeyboardInput,
            PlatformPermission::MouseInput,
        ],
    };
    detect_x11_session(&info).is_x11
}

pub fn ubuntu_wayland_harness_detects_limitations() -> Vec<String> {
    let info = PlatformInfo {
        os: DesktopPlatform::Linux,
        version: "ubuntu".to_owned(),
        desktop_environment: Some("GNOME".to_owned()),
        display_server: Some("wayland".to_owned()),
        permissions: vec![PlatformPermission::KeyboardInput],
    };
    detect_wayland_support(&info, WaylandCompositor::GnomeMutter, false, false).diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_plan_covers_windows_macos_and_linux() {
        let plan = desktop_harness_plan();

        assert!(plan
            .jobs
            .iter()
            .any(|job| job.environment == HarnessEnvironment::WindowsGithubActions));
        assert!(plan
            .jobs
            .iter()
            .any(|job| job.environment == HarnessEnvironment::MacOsGithubActions));
        assert!(plan
            .jobs
            .iter()
            .any(|job| job.environment == HarnessEnvironment::UbuntuX11VirtualDisplay));
        assert!(plan
            .jobs
            .iter()
            .any(|job| job.environment == HarnessEnvironment::UbuntuWaylandDetection));
    }

    #[test]
    fn sample_apps_are_declared_for_desktop_targets() {
        let plan = desktop_harness_plan();

        assert!(plan.has_sample(SampleTargetKind::Gtk));
        assert!(plan.has_sample(SampleTargetKind::Qt));
        assert!(plan.has_sample(SampleTargetKind::SwiftUiAppKit));
        assert!(plan.has_sample(SampleTargetKind::JavaSwing));
    }

    #[test]
    fn linux_x11_adapter_has_automated_integration_harness() {
        let plan = desktop_harness_plan();
        let job = plan
            .job("ubuntu-x11-virtual-display")
            .expect("x11 harness job");

        assert!(job.command.contains("xvfb-run"));
        assert!(ubuntu_x11_harness_detects_x11());
    }

    #[test]
    fn wayland_harness_verifies_graceful_limitation() {
        let diagnostics = ubuntu_wayland_harness_detects_limitations();

        assert!(diagnostics
            .iter()
            .any(|message| message.contains("xdg-desktop-portal")));
        assert!(diagnostics
            .iter()
            .any(|message| message.contains("intentionally unsupported")));
    }

    #[test]
    fn macos_has_unit_and_manual_permission_gated_harnesses() {
        let plan = desktop_harness_plan();
        let unit = plan.job("macos-unit").expect("macos unit job");
        let manual = plan
            .job("macos-manual-permission")
            .expect("macos manual job");

        assert!(!unit.permission_gated);
        assert!(manual.permission_gated);
        assert!(macos_unit_harness_result()
            .iter()
            .any(|message| message.contains("Accessibility")));
    }
}
