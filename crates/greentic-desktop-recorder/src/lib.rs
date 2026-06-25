use greentic_desktop_adapter::{LocatorTarget, RecordedEvent, RunnerStep};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingMode {
    AssistedPrompt,
    HumanDemonstration,
    Hybrid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedAction {
    pub event_type: String,
    pub timestamp: u64,
    pub adapter: String,
    pub target: LocatorTarget,
    pub value: Option<String>,
    pub screenshot_ref: Option<String>,
    pub normalized_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerPackage {
    pub id: String,
    pub version: String,
    pub mode: RecordingMode,
    pub inputs: Vec<String>,
    pub secrets: Vec<String>,
    pub steps: Vec<RunnerStep>,
    pub assertions: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordedPlatform {
    Windows,
    MacOS,
    LinuxX11,
    LinuxWayland,
}

impl RecordedPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOS => "macos",
            Self::LinuxX11 => "linux-x11",
            Self::LinuxWayland => "linux-wayland",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortablePlatformSupport {
    pub supported: Vec<RecordedPlatform>,
    pub preferred_adapter: BTreeMap<RecordedPlatform, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlatformLocators {
    pub windows: Option<LocatorTarget>,
    pub macos: Option<LocatorTarget>,
    pub linux_x11: Option<LocatorTarget>,
    pub linux_wayland: Option<LocatorTarget>,
}

impl PlatformLocators {
    pub fn locator_for(&self, platform: RecordedPlatform) -> Option<LocatorTarget> {
        match platform {
            RecordedPlatform::Windows => self.windows.clone(),
            RecordedPlatform::MacOS => self.macos.clone(),
            RecordedPlatform::LinuxX11 => self.linux_x11.clone(),
            RecordedPlatform::LinuxWayland => self.linux_wayland.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlatformAppLaunch {
    pub windows_executable: Option<String>,
    pub macos_bundle_id: Option<String>,
    pub linux_desktop_file: Option<String>,
}

impl PlatformAppLaunch {
    pub fn value_for(&self, platform: RecordedPlatform) -> Option<String> {
        match platform {
            RecordedPlatform::Windows => self
                .windows_executable
                .as_ref()
                .map(|value| format!("windows:exec {value}")),
            RecordedPlatform::MacOS => self
                .macos_bundle_id
                .as_ref()
                .map(|value| format!("macos:bundle-id {value}")),
            RecordedPlatform::LinuxX11 | RecordedPlatform::LinuxWayland => self
                .linux_desktop_file
                .as_ref()
                .map(|value| format!("linux:desktop-file {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableStep {
    pub id: String,
    pub action: String,
    pub required_capability: String,
    pub app: Option<PlatformAppLaunch>,
    pub locators: PlatformLocators,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableRunnerPackage {
    pub package: RunnerPackage,
    pub platforms: PortablePlatformSupport,
    pub steps: Vec<PortableStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformReplayPlan {
    pub platform: RecordedPlatform,
    pub adapter_id: String,
    pub steps: Vec<RunnerStep>,
    pub evidence_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortableReplayError {
    UnsupportedPlatform(RecordedPlatform),
    MissingPreferredAdapter(RecordedPlatform),
    MissingPlatformLocator {
        step_id: String,
        platform: RecordedPlatform,
    },
    MissingPlatformApp {
        step_id: String,
        platform: RecordedPlatform,
    },
}

#[derive(Debug, Clone)]
pub struct RecordingSession {
    id: String,
    mode: RecordingMode,
    actions: Vec<RecordedAction>,
    prompt_steps: Vec<RunnerStep>,
}

impl RecordingSession {
    pub fn new(id: impl Into<String>, mode: RecordingMode) -> Self {
        Self {
            id: id.into(),
            mode,
            actions: Vec::new(),
            prompt_steps: Vec::new(),
        }
    }

    pub fn capture_human_event(
        &mut self,
        adapter: impl Into<String>,
        event: RecordedEvent,
        screenshot_ref: Option<String>,
    ) {
        self.actions.push(RecordedAction {
            event_type: event.action,
            timestamp: unix_timestamp(),
            adapter: adapter.into(),
            target: event.target,
            value: event.value.map(|value| redact_sensitive_value(&value)),
            screenshot_ref,
            normalized_summary: None,
        });
    }

    pub fn add_prompt_step(&mut self, step: RunnerStep) {
        self.prompt_steps.push(step);
    }

    pub fn normalize(&mut self) {
        for action in &mut self.actions {
            action.normalized_summary = Some(normalize_action(action));
        }
    }

    pub fn into_package(mut self, version: impl Into<String>) -> RunnerPackage {
        self.normalize();
        let mut steps = self.prompt_steps;
        steps.extend(self.actions.iter().enumerate().map(|(index, action)| {
            let required_capability = format!(
                "{}.{}",
                capability_prefix(&action.adapter),
                action.event_type
            );
            RunnerStep {
                id: format!("recorded_{}", index + 1),
                action: action.event_type.clone(),
                target: action.target.clone(),
                value: action.value.clone(),
                required_capability,
            }
        }));

        RunnerPackage {
            id: self.id,
            version: version.into(),
            mode: self.mode,
            inputs: vec!["inputs.customer_id".to_owned()],
            secrets: vec!["secrets.password".to_owned()],
            steps,
            assertions: Vec::new(),
            outputs: Vec::new(),
        }
    }
}

impl RunnerPackage {
    pub fn render_yaml(&self) -> String {
        let mut output = format!(
            "id: {}\nversion: {}\nmode: {:?}\nsteps:\n",
            self.id, self.version, self.mode
        );

        for step in &self.steps {
            output.push_str(&format!(
                "  - id: {}\n    action: {}\n    required_capability: {}\n",
                step.id, step.action, step.required_capability
            ));
            if let Some(value) = &step.value {
                output.push_str(&format!("    value: \"{}\"\n", value));
            }
        }

        output
    }
}

impl PortableRunnerPackage {
    pub fn replay_plan(
        &self,
        platform: RecordedPlatform,
    ) -> Result<PlatformReplayPlan, PortableReplayError> {
        if !self.platforms.supported.contains(&platform) {
            return Err(PortableReplayError::UnsupportedPlatform(platform));
        }
        let adapter_id = self
            .platforms
            .preferred_adapter
            .get(&platform)
            .cloned()
            .ok_or(PortableReplayError::MissingPreferredAdapter(platform))?;
        let mut steps = Vec::new();
        for step in &self.steps {
            let target = step.locators.locator_for(platform).ok_or_else(|| {
                PortableReplayError::MissingPlatformLocator {
                    step_id: step.id.clone(),
                    platform,
                }
            })?;
            let value = if let Some(app) = &step.app {
                Some(app.value_for(platform).ok_or_else(|| {
                    PortableReplayError::MissingPlatformApp {
                        step_id: step.id.clone(),
                        platform,
                    }
                })?)
            } else {
                step.value.clone()
            };
            steps.push(RunnerStep {
                id: step.id.clone(),
                action: step.action.clone(),
                target,
                value,
                required_capability: step.required_capability.clone(),
            });
        }

        Ok(PlatformReplayPlan {
            platform,
            adapter_id,
            steps,
            evidence_path: format!(
                "evidence://{}/{}/platform-path.json",
                self.package.id,
                platform.as_str()
            ),
        })
    }

    pub fn render_yaml(&self) -> String {
        let mut output = self.package.render_yaml();
        output.push_str("platforms:\n  supported:\n");
        for platform in &self.platforms.supported {
            output.push_str(&format!("    - {}\n", platform.as_str()));
        }
        output.push_str("  preferred_adapter:\n");
        for (platform, adapter) in &self.platforms.preferred_adapter {
            output.push_str(&format!("    {}: {}\n", platform.as_str(), adapter));
        }
        output.push_str("portable_steps:\n");
        for step in &self.steps {
            output.push_str(&format!(
                "  - id: {}\n    action: {}\n    required_capability: {}\n",
                step.id, step.action, step.required_capability
            ));
            if let Some(app) = &step.app {
                output.push_str("    app:\n");
                if let Some(value) = &app.windows_executable {
                    output.push_str(&format!(
                        "      windows:\n        executable: \"{}\"\n",
                        value
                    ));
                }
                if let Some(value) = &app.macos_bundle_id {
                    output.push_str(&format!("      macos:\n        bundle_id: \"{}\"\n", value));
                }
                if let Some(value) = &app.linux_desktop_file {
                    output.push_str(&format!(
                        "      linux:\n        desktop_file: \"{}\"\n",
                        value
                    ));
                }
            }
        }
        output
    }
}

pub fn merge_prompt_and_recorded_steps(
    prompt_steps: Vec<RunnerStep>,
    recorded_steps: Vec<RunnerStep>,
) -> Vec<RunnerStep> {
    let mut merged = prompt_steps;
    merged.extend(recorded_steps);
    merged
}

pub fn redact_sensitive_value(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("password=") || lower.contains("token=") || lower.contains("secret=") {
        "{{secret}}".to_owned()
    } else {
        value.to_owned()
    }
}

fn normalize_action(action: &RecordedAction) -> String {
    if action.event_type == "click" {
        "click stable target".to_owned()
    } else {
        format!("{} via {}", action.event_type, action.adapter)
    }
}

fn capability_prefix(adapter: &str) -> &str {
    adapter
        .strip_prefix("greentic.desktop.")
        .unwrap_or(adapter)
        .split('-')
        .next()
        .unwrap_or(adapter)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorStrategy;

    fn event(action: &str, value: Option<&str>) -> RecordedEvent {
        RecordedEvent {
            action: action.to_owned(),
            target: LocatorTarget {
                preferred: Some(LocatorStrategy {
                    name: Some("Save".to_owned()),
                    ..LocatorStrategy::default()
                }),
                ..LocatorTarget::default()
            },
            value: value.map(str::to_owned),
        }
    }

    fn locator(name: &str) -> LocatorTarget {
        LocatorTarget {
            preferred: Some(LocatorStrategy {
                name: Some(name.to_owned()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        }
    }

    fn portable_package() -> PortableRunnerPackage {
        let mut preferred_adapter = BTreeMap::new();
        preferred_adapter.insert(
            RecordedPlatform::Windows,
            "greentic.desktop.windows.uia".to_owned(),
        );
        preferred_adapter.insert(
            RecordedPlatform::MacOS,
            "greentic.desktop.macos.ax".to_owned(),
        );
        preferred_adapter.insert(
            RecordedPlatform::LinuxX11,
            "greentic.desktop.linux.x11".to_owned(),
        );

        PortableRunnerPackage {
            package: RunnerPackage {
                id: "crm.create_customer".to_owned(),
                version: "1.0.0".to_owned(),
                mode: RecordingMode::Hybrid,
                inputs: Vec::new(),
                secrets: Vec::new(),
                steps: Vec::new(),
                assertions: Vec::new(),
                outputs: Vec::new(),
            },
            platforms: PortablePlatformSupport {
                supported: vec![
                    RecordedPlatform::Windows,
                    RecordedPlatform::MacOS,
                    RecordedPlatform::LinuxX11,
                ],
                preferred_adapter,
            },
            steps: vec![
                PortableStep {
                    id: "open_crm".to_owned(),
                    action: "desktop.open_app".to_owned(),
                    required_capability: "desktop.open_app".to_owned(),
                    app: Some(PlatformAppLaunch {
                        windows_executable: Some("C:\\Program Files\\CRM\\crm.exe".to_owned()),
                        macos_bundle_id: Some("com.vendor.crm".to_owned()),
                        linux_desktop_file: Some("crm.desktop".to_owned()),
                    }),
                    locators: PlatformLocators {
                        windows: Some(locator("CRM")),
                        macos: Some(locator("CRM")),
                        linux_x11: Some(locator("CRM")),
                        linux_wayland: None,
                    },
                    value: None,
                },
                PortableStep {
                    id: "save".to_owned(),
                    action: "desktop.click".to_owned(),
                    required_capability: "input.click".to_owned(),
                    app: None,
                    locators: PlatformLocators {
                        windows: Some(locator("SaveButton")),
                        macos: Some(locator("AXSave")),
                        linux_x11: Some(locator("Save")),
                        linux_wayland: None,
                    },
                    value: None,
                },
            ],
        }
    }

    #[test]
    fn captures_human_demonstration_events() {
        let mut session =
            RecordingSession::new("customer_create", RecordingMode::HumanDemonstration);
        session.capture_human_event(
            "greentic.desktop.playwright",
            event("click", None),
            Some("evidence://before.png".to_owned()),
        );
        session.normalize();

        assert_eq!(session.actions.len(), 1);
        assert_eq!(
            session.actions[0].normalized_summary,
            Some("click stable target".to_owned())
        );
    }

    #[test]
    fn merges_prompt_generated_and_recorded_steps() {
        let prompt = vec![RunnerStep {
            id: "prompt_1".to_owned(),
            action: "goto".to_owned(),
            target: LocatorTarget::default(),
            value: Some("https://example.test".to_owned()),
            required_capability: "web.goto".to_owned(),
        }];
        let recorded = vec![RunnerStep {
            id: "recorded_1".to_owned(),
            action: "click".to_owned(),
            target: LocatorTarget::default(),
            value: None,
            required_capability: "web.click".to_owned(),
        }];

        assert_eq!(merge_prompt_and_recorded_steps(prompt, recorded).len(), 2);
    }

    #[test]
    fn converts_recording_into_runner_yaml() {
        let mut session = RecordingSession::new("customer_create", RecordingMode::Hybrid);
        session.capture_human_event(
            "greentic.desktop.playwright",
            event("fill", Some("password=swordfish")),
            None,
        );

        let package = session.into_package("0.1.0");
        let yaml = package.render_yaml();

        assert!(yaml.contains("id: customer_create"));
        assert!(yaml.contains("{{secret}}"));
    }

    #[test]
    fn portable_runner_can_contain_os_specific_locators() {
        let package = portable_package();
        let plan = package
            .replay_plan(RecordedPlatform::MacOS)
            .expect("macOS plan should render");

        assert_eq!(plan.steps[1].target, locator("AXSave"));
        assert_eq!(plan.adapter_id, "greentic.desktop.macos.ax");
    }

    #[test]
    fn portable_runner_can_contain_logical_desktop_steps() {
        let yaml = portable_package().render_yaml();

        assert!(yaml.contains("platforms:"));
        assert!(yaml.contains("action: desktop.open_app"));
        assert!(yaml.contains("bundle_id: \"com.vendor.crm\""));
        assert!(yaml.contains("desktop_file: \"crm.desktop\""));
    }

    #[test]
    fn replay_chooses_correct_platform_adapter_at_runtime() {
        let package = portable_package();
        let windows = package
            .replay_plan(RecordedPlatform::Windows)
            .expect("windows plan");
        let linux = package
            .replay_plan(RecordedPlatform::LinuxX11)
            .expect("linux plan");

        assert_eq!(windows.adapter_id, "greentic.desktop.windows.uia");
        assert_eq!(linux.adapter_id, "greentic.desktop.linux.x11");
        assert_eq!(
            windows.steps[0].value,
            Some("windows:exec C:\\Program Files\\CRM\\crm.exe".to_owned())
        );
        assert_eq!(
            linux.steps[0].value,
            Some("linux:desktop-file crm.desktop".to_owned())
        );
    }

    #[test]
    fn unsupported_os_fails_before_execution() {
        let err = portable_package()
            .replay_plan(RecordedPlatform::LinuxWayland)
            .expect_err("unsupported platform should fail before execution");

        assert_eq!(
            err,
            PortableReplayError::UnsupportedPlatform(RecordedPlatform::LinuxWayland)
        );
    }

    #[test]
    fn evidence_shows_which_platform_path_was_used() {
        let plan = portable_package()
            .replay_plan(RecordedPlatform::LinuxX11)
            .expect("linux plan");

        assert_eq!(plan.platform, RecordedPlatform::LinuxX11);
        assert_eq!(
            plan.evidence_path,
            "evidence://crm.create_customer/linux-x11/platform-path.json"
        );
    }
}
