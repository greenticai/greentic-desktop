use greentic_desktop_linux::{detect_wayland_support, detect_x11_session, WaylandCompositor};
use greentic_desktop_macos::{
    first_run_permission_check, run_macos_app_workflow, MacOsAccessibilityAdapter,
    MacOsAppWorkflow, MacOsAppWorkflowOutcome,
};
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo, PlatformPermission};
use greentic_desktop_runtime::DesktopRuntime;

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
                name: "macos-app-workflow-e2e".to_owned(),
                environment: HarnessEnvironment::MacOsGithubActions,
                command: "cargo test -p greentic-desktop-test-harness macos_app_workflow_e2e_installs_extension_and_returns_output".to_owned(),
                permission_gated: false,
            },
            HarnessJob {
                name: "ubuntu-x11-virtual-display".to_owned(),
                environment: HarnessEnvironment::UbuntuX11VirtualDisplay,
                command: "xvfb-run -a cargo test -p greentic-desktop-linux can_detect_x11_session && xvfb-run -a cargo test -p greentic-desktop-linux can_list_windows_and_inspect_at_spi_tree"
                    .to_owned(),
                permission_gated: false,
            },
            HarnessJob {
                name: "ubuntu-wayland-detection".to_owned(),
                environment: HarnessEnvironment::UbuntuWaylandDetection,
                command: "cargo test -p greentic-desktop-linux detects_wayland_and_reports_global_restrictions && cargo test -p greentic-desktop-linux wayland_requires_manual_approval_when_portal_is_missing"
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

pub fn macos_app_workflow_e2e_result(
    workflow: MacOsAppWorkflow,
) -> Result<MacOsAppWorkflowOutcome, String> {
    let root = std::env::temp_dir().join(format!(
        "greentic-macos-app-workflow-e2e-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
    }
    let mut config = greentic_desktop_config::RuntimeConfig::default();
    config.runner.home = root.clone();
    config.evidence.store = root.join("evidence");
    let runtime = DesktopRuntime::new(config);
    let manifest = runtime
        .install_extension("macos")
        .map_err(|err| err.to_string())?;
    if manifest.id != "greentic.desktop.macos.ax" {
        return Err(format!(
            "expected greentic.desktop.macos.ax, installed {}",
            manifest.id
        ));
    }

    let adapter = MacOsAccessibilityAdapter::new(PlatformInfo {
        os: DesktopPlatform::MacOS,
        version: "github-actions".to_owned(),
        desktop_environment: Some("Aqua".to_owned()),
        display_server: Some("quartz".to_owned()),
        permissions: vec![
            PlatformPermission::Accessibility,
            PlatformPermission::ScreenRecording,
            PlatformPermission::KeyboardInput,
            PlatformPermission::MouseInput,
            PlatformPermission::WindowManagement,
        ],
    });
    let outcome = run_macos_app_workflow(&adapter, workflow).map_err(|err| err.to_string());
    let _ = std::fs::remove_dir_all(root);
    outcome
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
    use greentic_desktop_macos::{
        stable_macos_target, MacOsElementMetadata, MacOsWorkflowAction, MacOsWorkflowInput,
        MacOsWorkflowOutput,
    };

    fn calculator_workflow_fixture(
        number_1: &str,
        operation: &str,
        number_2: &str,
        expected_result: &str,
    ) -> MacOsAppWorkflow {
        let calculator_display = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("calculator-display".to_owned()),
            ax_title: Some("Display".to_owned()),
            ax_role: Some("AXStaticText".to_owned()),
            ax_value: None,
            nearby_text: Some("Calculator".to_owned()),
            visual_region: Some("top".to_owned()),
        });

        MacOsAppWorkflow {
            app_name: "Calculator".to_owned(),
            window_title: "Calculator".to_owned(),
            prompt: "Open Calculator and complete the supplied operation.".to_owned(),
            inputs: vec![
                MacOsWorkflowInput {
                    name: "number 1".to_owned(),
                    target: calculator_display.clone(),
                    value: number_1.to_owned(),
                },
                MacOsWorkflowInput {
                    name: "operation".to_owned(),
                    target: calculator_display.clone(),
                    value: operation.to_owned(),
                },
                MacOsWorkflowInput {
                    name: "number 2".to_owned(),
                    target: calculator_display.clone(),
                    value: number_2.to_owned(),
                },
            ],
            submit: Some(MacOsWorkflowAction {
                name: "equals".to_owned(),
                target: stable_macos_target(&MacOsElementMetadata {
                    ax_identifier: Some("equals".to_owned()),
                    ax_title: Some("Equals".to_owned()),
                    ax_role: Some("AXButton".to_owned()),
                    ax_value: None,
                    nearby_text: Some("Calculator".to_owned()),
                    visual_region: Some("keypad".to_owned()),
                }),
            }),
            outputs: vec![MacOsWorkflowOutput {
                name: "result".to_owned(),
                target: calculator_display,
                expected: Some(expected_result.to_owned()),
            }],
        }
    }

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
        let app_workflow = plan
            .job("macos-app-workflow-e2e")
            .expect("macos app workflow e2e job");
        let manual = plan
            .job("macos-manual-permission")
            .expect("macos manual job");

        assert!(!unit.permission_gated);
        assert!(!app_workflow.permission_gated);
        assert!(app_workflow.command.contains("macos_app_workflow_e2e"));
        assert!(manual.permission_gated);
        assert!(macos_unit_harness_result()
            .iter()
            .any(|message| message.contains("Accessibility")));
    }

    #[test]
    fn macos_app_workflow_e2e_installs_extension_and_returns_output() {
        let outcome =
            macos_app_workflow_e2e_result(calculator_workflow_fixture("1", "+", "1", "2"))
                .expect("macos app workflow e2e should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"2".to_owned()));
        assert!(outcome.prompt.contains("Open Calculator"));
        assert!(outcome.steps.iter().all(|step| step.success));
    }
}
