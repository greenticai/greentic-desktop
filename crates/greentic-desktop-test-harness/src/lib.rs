use greentic_desktop_java::{
    run_java_app_workflow, JavaAppWorkflow, JavaAppWorkflowOutcome, JavaDesktopAdapter,
};
use greentic_desktop_linux::{
    detect_wayland_support, detect_x11_session, run_linux_x11_app_workflow, LinuxX11Adapter,
    LinuxX11AppWorkflow, LinuxX11AppWorkflowOutcome, WaylandCompositor,
};
use greentic_desktop_macos::{
    first_run_permission_check, run_macos_app_workflow, MacOsAccessibilityAdapter,
    MacOsAppWorkflow, MacOsAppWorkflowOutcome,
};
use greentic_desktop_platform::{DesktopPlatform, PlatformInfo, PlatformPermission};
use greentic_desktop_recorder::normalise_recording;
use greentic_desktop_runtime::DesktopRuntime;
use greentic_desktop_terminal::{
    run_terminal_workflow, TerminalAdapter, TerminalWorkflow, TerminalWorkflowOutcome,
};
use greentic_desktop_windows::{
    run_windows_app_workflow, WindowsAppWorkflow, WindowsAppWorkflowOutcome, WindowsUiAdapter,
};

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
                command: "xvfb-run -a cargo test -p greentic-desktop-linux can_detect_x11_session && xvfb-run -a cargo test -p greentic-desktop-linux can_list_windows_and_inspect_at_spi_tree && cargo test -p greentic-desktop-test-harness linux_x11_app_workflow_e2e_installs_extension_and_returns_output"
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
                command: "cargo test -p greentic-desktop-windows -p greentic-desktop-java -p greentic-desktop-terminal && cargo test -p greentic-desktop-test-harness windows_app_workflow_e2e_installs_extension_and_returns_output && cargo test -p greentic-desktop-test-harness java_app_workflow_e2e_installs_extension_and_returns_output && cargo test -p greentic-desktop-test-harness terminal_workflow_e2e_installs_extension_and_returns_output"
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

    let adapter = MacOsAccessibilityAdapter::new_model(PlatformInfo {
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

pub fn linux_x11_app_workflow_e2e_result(
    workflow: LinuxX11AppWorkflow,
) -> Result<LinuxX11AppWorkflowOutcome, String> {
    let root = std::env::temp_dir().join(format!(
        "greentic-linux-x11-app-workflow-e2e-{}",
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
        .install_extension("linux-x11")
        .map_err(|err| err.to_string())?;
    if manifest.id != "greentic.desktop.linux.x11" {
        return Err(format!(
            "expected greentic.desktop.linux.x11, installed {}",
            manifest.id
        ));
    }

    let adapter = LinuxX11Adapter::new_model(PlatformInfo {
        os: DesktopPlatform::Linux,
        version: "github-actions".to_owned(),
        desktop_environment: Some("GNOME".to_owned()),
        display_server: Some("x11".to_owned()),
        permissions: vec![
            PlatformPermission::WindowManagement,
            PlatformPermission::AppLaunch,
            PlatformPermission::KeyboardInput,
            PlatformPermission::MouseInput,
            PlatformPermission::Screenshot,
        ],
    });
    let outcome = run_linux_x11_app_workflow(&adapter, workflow).map_err(|err| err.to_string());
    let _ = std::fs::remove_dir_all(root);
    outcome
}

pub fn windows_app_workflow_e2e_result(
    workflow: WindowsAppWorkflow,
) -> Result<WindowsAppWorkflowOutcome, String> {
    let root = std::env::temp_dir().join(format!(
        "greentic-windows-app-workflow-e2e-{}",
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
        .install_extension("windows")
        .map_err(|err| err.to_string())?;
    if manifest.id != "greentic.desktop.windows-ui" {
        return Err(format!(
            "expected greentic.desktop.windows-ui, installed {}",
            manifest.id
        ));
    }

    let adapter = WindowsUiAdapter::new_model();
    let outcome = run_windows_app_workflow(&adapter, workflow).map_err(|err| err.to_string());
    let _ = std::fs::remove_dir_all(root);
    outcome
}

pub fn java_app_workflow_e2e_result(
    workflow: JavaAppWorkflow,
) -> Result<JavaAppWorkflowOutcome, String> {
    let root = std::env::temp_dir().join(format!(
        "greentic-java-app-workflow-e2e-{}",
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
        .install_extension("java")
        .map_err(|err| err.to_string())?;
    if manifest.id != "greentic.desktop.java-accessibility" {
        return Err(format!(
            "expected greentic.desktop.java-accessibility, installed {}",
            manifest.id
        ));
    }

    let adapter = JavaDesktopAdapter::new(true);
    let outcome = run_java_app_workflow(&adapter, workflow).map_err(|err| err.to_string());
    let _ = std::fs::remove_dir_all(root);
    outcome
}

pub fn terminal_workflow_e2e_result(
    workflow: TerminalWorkflow,
) -> Result<TerminalWorkflowOutcome, String> {
    let root = std::env::temp_dir().join(format!(
        "greentic-terminal-workflow-e2e-{}",
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
        .install_extension("terminal")
        .map_err(|err| err.to_string())?;
    if manifest.id != "greentic.desktop.terminal-tn3270" {
        return Err(format!(
            "expected greentic.desktop.terminal-tn3270, installed {}",
            manifest.id
        ));
    }

    let adapter = TerminalAdapter::new();
    let outcome = run_terminal_workflow(&adapter, workflow).map_err(|err| err.to_string());
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingMatrixCase {
    pub target: String,
    pub fixture: String,
    pub expected_capability: String,
    pub expected_output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingMatrixResult {
    pub target: String,
    pub passed: bool,
    pub semantic_steps: usize,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

pub fn recording_e2e_matrix_cases() -> Vec<RecordingMatrixCase> {
    vec![
        RecordingMatrixCase {
            target: "web".to_owned(),
            fixture: "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"navigate\",\"target\":{},\"value\":\"http://fixture/generic-resource-table\",\"redaction\":\"none\"},\"evidence\":{}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"input\",\"target\":{\"label\":\"resource name\"},\"value\":\"{{inputs.resource_name}}\",\"redaction\":\"input_candidate\"},\"evidence\":{}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"input\",\"target\":{\"label\":\"name\"},\"value\":\"{{inputs.name}}\",\"redaction\":\"input_candidate\"},\"evidence\":{}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"input\",\"target\":{\"label\":\"email\"},\"value\":\"{{inputs.email}}\",\"redaction\":\"input_candidate\"},\"evidence\":{}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"click\",\"target\":{\"role\":\"button\",\"name\":\"Save row\"},\"value\":null,\"redaction\":\"none\"},\"evidence\":{}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"web\",\"event\":{\"kind\":\"read_text\",\"target\":{\"label\":\"saved status\"},\"value\":\"Saved row\",\"redaction\":\"none\"},\"evidence\":{}}\n".to_owned(),
            expected_capability: "web.click".to_owned(),
            expected_output: "outputs.saved_status".to_owned(),
        },
        RecordingMatrixCase {
            target: "terminal".to_owned(),
            fixture: "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"terminal\",\"event\":{\"kind\":\"send_text\",\"target\":{\"label\":\"resource name\"},\"value\":\"{{inputs.resource_name}}\",\"redaction\":\"input_candidate\"},\"evidence\":{\"terminal_buffer_ref\":\"evidence://terminal/before.txt\"}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"terminal\",\"event\":{\"kind\":\"send_text\",\"target\":{\"label\":\"name\"},\"value\":\"{{inputs.name}}\",\"redaction\":\"input_candidate\"},\"evidence\":{\"terminal_buffer_ref\":\"evidence://terminal/name.txt\"}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"terminal\",\"event\":{\"kind\":\"send_text\",\"target\":{\"label\":\"email\"},\"value\":\"{{inputs.email}}\",\"redaction\":\"input_candidate\"},\"evidence\":{\"terminal_buffer_ref\":\"evidence://terminal/email.txt\"}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"terminal\",\"event\":{\"kind\":\"extract_field\",\"target\":{\"label\":\"saved status\"},\"value\":\"Saved row\",\"redaction\":\"none\"},\"evidence\":{\"terminal_buffer_ref\":\"evidence://terminal/after.txt\"}}\n".to_owned(),
            expected_capability: "terminal.wait_for_screen".to_owned(),
            expected_output: "outputs.saved_status".to_owned(),
        },
        RecordingMatrixCase {
            target: "remote".to_owned(),
            fixture: "{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"remote\",\"event\":{\"kind\":\"click_region\",\"target\":{\"label\":\"open resource\",\"region\":{\"x\":10,\"y\":10,\"width\":120,\"height\":24}},\"value\":null,\"redaction\":\"none\"},\"evidence\":{\"screenshot_ref\":\"evidence://remote/open-resource.png\"}}\n{\"schema_version\":\"recording.event.v1\",\"target_kind\":\"remote\",\"event\":{\"kind\":\"extract_text_region\",\"target\":{\"label\":\"saved status\",\"region\":{\"x\":40,\"y\":40,\"width\":160,\"height\":24}},\"value\":\"Saved row\",\"redaction\":\"none\"},\"evidence\":{\"screenshot_ref\":\"evidence://remote/saved-status.png\"}}\n".to_owned(),
            expected_capability: "remote.click_region".to_owned(),
            expected_output: "outputs.saved_status".to_owned(),
        },
    ]
}

pub fn run_recording_e2e_matrix() -> Result<Vec<RecordingMatrixResult>, String> {
    recording_e2e_matrix_cases()
        .into_iter()
        .map(|case| {
            let root = std::env::temp_dir().join(format!(
                "greentic-recording-matrix-{}-{}",
                case.target,
                std::process::id()
            ));
            if root.exists() {
                std::fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
            }
            let raw = root.join("raw");
            std::fs::create_dir_all(&raw).map_err(|err| err.to_string())?;
            std::fs::write(raw.join("events.jsonl"), &case.fixture)
                .map_err(|err| err.to_string())?;
            let package = normalise_recording(&raw, &root.join("runner.yaml"))
                .map_err(|err| err.to_string())?;
            let yaml =
                std::fs::read_to_string(root.join("runner.yaml")).map_err(|err| err.to_string())?;
            let passed = package
                .steps
                .iter()
                .any(|step| step.required_capability == case.expected_capability)
                && package.outputs.contains(&case.expected_output)
                && !yaml.contains("sample-output")
                && !yaml.contains("recording.recorded")
                && !yaml.contains("company_name")
                && !yaml.contains("acme")
                && !yaml.contains("calculator");
            let result = RecordingMatrixResult {
                target: case.target,
                passed,
                semantic_steps: package.steps.len(),
                inputs: package.inputs,
                outputs: package.outputs,
            };
            let _ = std::fs::remove_dir_all(root);
            Ok(result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_java::{
        stable_java_target, JavaComponentMetadata, JavaWorkflowAction, JavaWorkflowInput,
        JavaWorkflowOutput,
    };
    use greentic_desktop_linux::{
        stable_linux_target, LinuxElementMetadata, LinuxWorkflowAction, LinuxWorkflowInput,
        LinuxWorkflowOutput,
    };
    use greentic_desktop_macos::{
        stable_macos_target, MacOsElementMetadata, MacOsWorkflowAction, MacOsWorkflowInput,
        MacOsWorkflowOutput,
    };
    use greentic_desktop_terminal::{
        ScreenField, TerminalFieldOutput, TerminalProfile, TerminalProtocol, TerminalTextOutput,
        TerminalWorkflowAction,
    };
    use greentic_desktop_windows::{
        stable_windows_target, WindowsElementMetadata, WindowsWorkflowAction, WindowsWorkflowInput,
        WindowsWorkflowOutput,
    };

    fn macos_generic_resource_workflow_fixture(
        resource_name: &str,
        name: &str,
        email: &str,
        expected_status: &str,
    ) -> MacOsAppWorkflow {
        let resource_field = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("resource-name-field".to_owned()),
            ax_title: Some("Resource name".to_owned()),
            ax_role: Some("AXTextField".to_owned()),
            ax_value: None,
            nearby_text: Some("Resource name".to_owned()),
            visual_region: Some("top".to_owned()),
        });
        let name_field = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("row-name-field".to_owned()),
            ax_title: Some("Name".to_owned()),
            ax_role: Some("AXTextField".to_owned()),
            ax_value: None,
            nearby_text: Some("Name".to_owned()),
            visual_region: Some("center".to_owned()),
        });
        let email_field = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("row-email-field".to_owned()),
            ax_title: Some("Email".to_owned()),
            ax_role: Some("AXTextField".to_owned()),
            ax_value: None,
            nearby_text: Some("Email".to_owned()),
            visual_region: Some("center".to_owned()),
        });
        let status_label = stable_macos_target(&MacOsElementMetadata {
            ax_identifier: Some("saved-status".to_owned()),
            ax_title: Some("Saved status".to_owned()),
            ax_role: Some("AXStaticText".to_owned()),
            ax_value: None,
            nearby_text: Some("Saved status".to_owned()),
            visual_region: Some("bottom".to_owned()),
        });

        MacOsAppWorkflow {
            app_name: "Generic Resource Fixture".to_owned(),
            window_title: "Generic Resource Fixture".to_owned(),
            prompt:
                "Open a generic resource editor, create or open the resource, append row fields, and save."
                    .to_owned(),
            inputs: vec![
                MacOsWorkflowInput {
                    name: "resource_name".to_owned(),
                    target: resource_field,
                    value: resource_name.to_owned(),
                },
                MacOsWorkflowInput {
                    name: "name".to_owned(),
                    target: name_field,
                    value: name.to_owned(),
                },
                MacOsWorkflowInput {
                    name: "email".to_owned(),
                    target: email_field,
                    value: email.to_owned(),
                },
            ],
            submit: Some(MacOsWorkflowAction {
                name: "save row".to_owned(),
                target: stable_macos_target(&MacOsElementMetadata {
                    ax_identifier: Some("save-row-button".to_owned()),
                    ax_title: Some("Save row".to_owned()),
                    ax_role: Some("AXButton".to_owned()),
                    ax_value: None,
                    nearby_text: Some("Email".to_owned()),
                    visual_region: Some("bottom_right".to_owned()),
                }),
            }),
            outputs: vec![MacOsWorkflowOutput {
                name: "saved_status".to_owned(),
                target: status_label,
                expected: Some(expected_status.to_owned()),
            }],
        }
    }

    fn linux_sample_workflow_fixture() -> LinuxX11AppWorkflow {
        let result = stable_linux_target(&LinuxElementMetadata {
            accessible_name: Some("Result".to_owned()),
            role: Some("label".to_owned()),
            window_title: Some("Sample".to_owned()),
            class_name: Some("GtkLabel".to_owned()),
            nearby_text: Some("Result".to_owned()),
            visual_region: Some("bottom".to_owned()),
        });

        LinuxX11AppWorkflow {
            window_title: "Sample".to_owned(),
            prompt: "Open Sample and complete the supplied workflow.".to_owned(),
            inputs: vec![LinuxWorkflowInput {
                name: "input".to_owned(),
                target: stable_linux_target(&LinuxElementMetadata {
                    accessible_name: Some("Input".to_owned()),
                    role: Some("text".to_owned()),
                    window_title: Some("Sample".to_owned()),
                    class_name: Some("GtkEntry".to_owned()),
                    nearby_text: Some("Input".to_owned()),
                    visual_region: Some("center".to_owned()),
                }),
                value: "hello".to_owned(),
            }],
            submit: Some(LinuxWorkflowAction {
                name: "submit".to_owned(),
                target: stable_linux_target(&LinuxElementMetadata {
                    accessible_name: Some("Submit".to_owned()),
                    role: Some("push button".to_owned()),
                    window_title: Some("Sample".to_owned()),
                    class_name: Some("GtkButton".to_owned()),
                    nearby_text: Some("Input".to_owned()),
                    visual_region: Some("bottom_right".to_owned()),
                }),
            }),
            outputs: vec![LinuxWorkflowOutput {
                name: "result".to_owned(),
                target: result,
                expected: Some("accepted".to_owned()),
            }],
        }
    }

    fn windows_sample_workflow_fixture() -> WindowsAppWorkflow {
        let result = stable_windows_target(&WindowsElementMetadata {
            automation_id: Some("ResultText".to_owned()),
            name: Some("Result".to_owned()),
            control_type: Some("Text".to_owned()),
            class_name: Some("TextBlock".to_owned()),
            relative_position: Some("main.bottom".to_owned()),
            nearby_text: Some("Result".to_owned()),
            visual_region: Some("bottom".to_owned()),
        });

        WindowsAppWorkflow {
            app_name: "Sample.exe".to_owned(),
            window_title: "Sample".to_owned(),
            prompt: "Open Sample.exe and complete the supplied workflow.".to_owned(),
            inputs: vec![WindowsWorkflowInput {
                name: "input".to_owned(),
                target: stable_windows_target(&WindowsElementMetadata {
                    automation_id: Some("InputBox".to_owned()),
                    name: Some("Input".to_owned()),
                    control_type: Some("Edit".to_owned()),
                    class_name: Some("TextBox".to_owned()),
                    relative_position: Some("main.center".to_owned()),
                    nearby_text: Some("Input".to_owned()),
                    visual_region: Some("center".to_owned()),
                }),
                value: "hello".to_owned(),
            }],
            submit: Some(WindowsWorkflowAction {
                name: "submit".to_owned(),
                target: stable_windows_target(&WindowsElementMetadata {
                    automation_id: Some("SubmitButton".to_owned()),
                    name: Some("Submit".to_owned()),
                    control_type: Some("Button".to_owned()),
                    class_name: Some("Button".to_owned()),
                    relative_position: Some("main.bottom_right".to_owned()),
                    nearby_text: Some("Input".to_owned()),
                    visual_region: Some("bottom_right".to_owned()),
                }),
            }),
            outputs: vec![WindowsWorkflowOutput {
                name: "result".to_owned(),
                target: result,
                expected: Some("accepted".to_owned()),
            }],
        }
    }

    #[test]
    fn recording_e2e_matrix_normalises_semantic_runners_without_placeholders() {
        let results = run_recording_e2e_matrix().expect("recording matrix should run");

        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.passed, "{result:?}");
            assert!(result.semantic_steps > 0, "{result:?}");
            assert!(!result.outputs.is_empty(), "{result:?}");
        }
    }

    #[test]
    fn recording_e2e_matrix_uses_generic_resource_fixtures() {
        for case in recording_e2e_matrix_cases() {
            let lower = case.fixture.to_ascii_lowercase();
            assert!(lower.contains("resource") || lower.contains("saved row"));
            assert!(!lower.contains("calculator"), "{lower}");
            assert!(!lower.contains("acme"), "{lower}");
            assert!(!lower.contains("sample-output"), "{lower}");
            assert!(
                lower.contains("{{inputs.resource_name}}") || case.target == "remote",
                "{lower}"
            );
        }
    }

    fn java_sample_workflow_fixture() -> JavaAppWorkflow {
        let result = stable_java_target(&JavaComponentMetadata {
            window_title: Some("Sample".to_owned()),
            component_name: Some("result".to_owned()),
            role: Some("label".to_owned()),
            text: Some("Result".to_owned()),
            keyboard_shortcut: None,
            visual_region: Some("bottom".to_owned()),
        });

        JavaAppWorkflow {
            window_title: "Sample".to_owned(),
            prompt: "Open Sample and complete the supplied Java workflow.".to_owned(),
            inputs: vec![JavaWorkflowInput {
                name: "input".to_owned(),
                target: stable_java_target(&JavaComponentMetadata {
                    window_title: Some("Sample".to_owned()),
                    component_name: Some("input".to_owned()),
                    role: Some("text".to_owned()),
                    text: Some("Input".to_owned()),
                    keyboard_shortcut: Some("Alt+I".to_owned()),
                    visual_region: Some("center".to_owned()),
                }),
                value: "hello".to_owned(),
            }],
            submit: Some(JavaWorkflowAction {
                name: "submit".to_owned(),
                target: stable_java_target(&JavaComponentMetadata {
                    window_title: Some("Sample".to_owned()),
                    component_name: Some("submit".to_owned()),
                    role: Some("push button".to_owned()),
                    text: Some("Submit".to_owned()),
                    keyboard_shortcut: Some("Alt+S".to_owned()),
                    visual_region: Some("bottom_right".to_owned()),
                }),
            }),
            outputs: vec![JavaWorkflowOutput {
                name: "result".to_owned(),
                target: result,
                expected: Some("accepted".to_owned()),
            }],
        }
    }

    fn terminal_sample_workflow_fixture() -> TerminalWorkflow {
        TerminalWorkflow {
            profile: TerminalProfile {
                name: "sample".to_owned(),
                protocol: TerminalProtocol::Tn3270,
                host: "terminal.test".to_owned(),
            },
            prompt: "Connect to a terminal and complete the supplied workflow.".to_owned(),
            initial_screen: vec!["LOGIN".to_owned()],
            actions: vec![
                TerminalWorkflowAction {
                    name: "username".to_owned(),
                    required_capability: "terminal.type_text".to_owned(),
                    value: Some("USER1".to_owned()),
                },
                TerminalWorkflowAction {
                    name: "enter".to_owned(),
                    required_capability: "terminal.send_keys".to_owned(),
                    value: Some("ENTER".to_owned()),
                },
            ],
            final_screen: vec![
                "ACCOUNT STATUS: ACTIVE".to_owned(),
                "BALANCE: 100.00".to_owned(),
            ],
            text_outputs: vec![TerminalTextOutput {
                name: "status-line".to_owned(),
                expected: "ACCOUNT STATUS".to_owned(),
            }],
            field_outputs: vec![TerminalFieldOutput {
                name: "status".to_owned(),
                field: ScreenField {
                    row: 0,
                    col: 16,
                    len: 6,
                },
                expected: Some("ACTIVE".to_owned()),
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
        assert!(job.command.contains("linux_x11_app_workflow_e2e"));
        assert!(ubuntu_x11_harness_detects_x11());
    }

    #[test]
    fn linux_x11_app_workflow_e2e_installs_extension_and_returns_output() {
        let outcome = linux_x11_app_workflow_e2e_result(linux_sample_workflow_fixture())
            .expect("linux x11 app workflow e2e should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"accepted".to_owned()));
        assert!(outcome.prompt.contains("Sample"));
        assert!(outcome.steps.iter().all(|step| step.success));
    }

    #[test]
    fn windows_app_workflow_e2e_installs_extension_and_returns_output() {
        let outcome = windows_app_workflow_e2e_result(windows_sample_workflow_fixture())
            .expect("windows app workflow e2e should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"accepted".to_owned()));
        assert!(outcome.prompt.contains("Sample.exe"));
        assert!(outcome.steps.iter().all(|step| step.success));
    }

    #[test]
    fn java_app_workflow_e2e_installs_extension_and_returns_output() {
        let outcome = java_app_workflow_e2e_result(java_sample_workflow_fixture())
            .expect("java app workflow e2e should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"accepted".to_owned()));
        assert!(outcome.prompt.contains("Java workflow"));
        assert!(outcome.steps.iter().all(|step| step.success));
    }

    #[test]
    fn terminal_workflow_e2e_installs_extension_and_returns_output() {
        let outcome = terminal_workflow_e2e_result(terminal_sample_workflow_fixture())
            .expect("terminal workflow e2e should pass");

        assert_eq!(outcome.outputs.get("status"), Some(&"ACTIVE".to_owned()));
        assert!(outcome.prompt.contains("terminal"));
        assert!(outcome.steps.iter().all(|step| step.success));
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
    fn windows_has_unit_and_app_workflow_harnesses() {
        let plan = desktop_harness_plan();
        let job = plan.job("windows-unit").expect("windows unit job");

        assert!(!job.permission_gated);
        assert!(job.command.contains("greentic-desktop-windows"));
        assert!(job.command.contains("windows_app_workflow_e2e"));
        assert!(job.command.contains("java_app_workflow_e2e"));
        assert!(job.command.contains("terminal_workflow_e2e"));
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
        let outcome = macos_app_workflow_e2e_result(macos_generic_resource_workflow_fixture(
            "contacts",
            "Maarten",
            "maarten@example.test",
            "Saved row",
        ))
        .expect("macos app workflow e2e should pass");

        assert_eq!(
            outcome.outputs.get("saved_status"),
            Some(&"Saved row".to_owned())
        );
        assert!(outcome.prompt.contains("generic resource editor"));
        assert!(outcome.steps.iter().all(|step| step.success));
    }
}
