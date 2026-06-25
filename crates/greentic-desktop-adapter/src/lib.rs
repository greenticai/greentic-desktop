use greentic_desktop_core::{Capability, RiskLevel};
use std::collections::BTreeSet;
use std::fmt;

pub type AdapterResult<T> = Result<T, AdapterError>;

/// Universal adapter contract used by the runner.
pub trait DesktopAdapter: Send + Sync {
    fn capabilities(&self) -> AdapterCapabilities;
    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation>;
    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult>;
    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult>;
    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCapabilities {
    pub adapter_id: String,
    pub version: String,
    pub capabilities: Vec<String>,
}

impl AdapterCapabilities {
    pub fn new(
        adapter_id: impl Into<String>,
        version: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let mut capabilities: Vec<String> = capabilities
            .into_iter()
            .map(Into::into)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .collect();
        capabilities.sort();
        capabilities.dedup();

        Self {
            adapter_id: adapter_id.into(),
            version: version.into(),
            capabilities,
        }
    }

    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities
            .binary_search_by(|candidate| candidate.as_str().cmp(capability))
            .is_ok()
    }

    pub fn to_core_capabilities(&self, risk: RiskLevel) -> Vec<Capability> {
        self.capabilities
            .iter()
            .map(|name| Capability {
                name: name.clone(),
                adapter: self.adapter_id.clone(),
                risk,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveContext {
    pub session_id: String,
    pub target: Option<LocatorTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub adapter_id: String,
    pub summary: String,
    pub visible_text: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerStep {
    pub id: String,
    pub action: String,
    pub target: LocatorTarget,
    pub value: Option<String>,
    pub required_capability: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub step_id: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assertion {
    pub id: String,
    pub required_capability: String,
    pub target: LocatorTarget,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionResult {
    pub assertion_id: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedEvent {
    pub action: String,
    pub target: LocatorTarget,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocatorTarget {
    pub preferred: Option<LocatorStrategy>,
    pub fallback: Option<LocatorStrategy>,
    pub visual_fallback: Option<VisualLocator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocatorStrategy {
    pub data_testid: Option<String>,
    pub role: Option<String>,
    pub name: Option<String>,
    pub automation_id: Option<String>,
    pub text: Option<String>,
    pub region: Option<String>,
    pub label: Option<String>,
    pub css: Option<String>,
    pub xpath: Option<String>,
    pub class_name: Option<String>,
    pub control_type: Option<String>,
    pub relative_position: Option<String>,
    pub keyboard_shortcut: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualLocator {
    pub image: String,
    pub region: Option<String>,
    pub nearby_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    UnsupportedCapability(String),
    ExecutionFailed(String),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCapability(capability) => {
                write!(f, "unsupported capability: {capability}")
            }
            Self::ExecutionFailed(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for AdapterError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityValidation {
    pub missing: Vec<String>,
}

impl CapabilityValidation {
    pub fn is_valid(&self) -> bool {
        self.missing.is_empty()
    }
}

pub fn validate_required_capabilities(
    installed: &[AdapterCapabilities],
    required: impl IntoIterator<Item = impl AsRef<str>>,
) -> CapabilityValidation {
    let installed_capabilities: BTreeSet<&str> = installed
        .iter()
        .flat_map(|adapter| adapter.capabilities.iter().map(String::as_str))
        .collect();

    let mut missing: Vec<String> = required
        .into_iter()
        .filter_map(|capability| {
            let capability = capability.as_ref();
            (!installed_capabilities.contains(capability)).then(|| capability.to_owned())
        })
        .collect();
    missing.sort();
    missing.dedup();

    CapabilityValidation { missing }
}

pub fn select_best_adapter(
    installed: &[AdapterCapabilities],
    required: impl IntoIterator<Item = impl AsRef<str>>,
) -> Option<&AdapterCapabilities> {
    let required: Vec<String> = required
        .into_iter()
        .map(|capability| capability.as_ref().to_owned())
        .collect();

    installed
        .iter()
        .filter(|adapter| {
            required
                .iter()
                .all(|capability| adapter.supports(capability))
        })
        .max_by_key(|adapter| adapter.capabilities.len())
}

#[derive(Debug, Clone)]
pub struct StaticAdapter {
    capabilities: AdapterCapabilities,
}

impl StaticAdapter {
    pub fn new(capabilities: AdapterCapabilities) -> Self {
        Self { capabilities }
    }
}

impl DesktopAdapter for StaticAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        self.capabilities.clone()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        Ok(Observation {
            adapter_id: self.capabilities.adapter_id.clone(),
            summary: format!("observed session {}", ctx.session_id),
            visible_text: Vec::new(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities.supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        Ok(StepResult {
            step_id: step.id,
            success: true,
            message: "step accepted".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities.supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed: true,
            message: "assertion accepted".to_owned(),
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web_adapter() -> AdapterCapabilities {
        AdapterCapabilities::new(
            "greentic.desktop.playwright",
            "1.0.0",
            [
                "web.goto",
                "web.click",
                "web.fill",
                "web.extract_text",
                "web.assert_visible",
                "evidence.screenshot",
            ],
        )
    }

    #[test]
    fn adapters_expose_stable_capability_format() {
        let capabilities = web_adapter();

        assert_eq!(capabilities.adapter_id, "greentic.desktop.playwright");
        assert_eq!(capabilities.version, "1.0.0");
        assert!(capabilities.supports("web.click"));
        assert!(capabilities.supports("evidence.screenshot"));
    }

    #[test]
    fn validation_reports_missing_capabilities_before_execution() {
        let validation =
            validate_required_capabilities(&[web_adapter()], ["web.click", "windows.click"]);

        assert!(!validation.is_valid());
        assert_eq!(validation.missing, vec!["windows.click"]);
    }

    #[test]
    fn runner_can_select_best_adapter() {
        let terminal = AdapterCapabilities::new("terminal", "1.0.0", ["terminal.send_text"]);
        let web = web_adapter();
        let adapters = vec![terminal, web];

        let selected = select_best_adapter(&adapters, ["web.click", "web.fill"])
            .expect("web adapter should satisfy required capabilities");

        assert_eq!(selected.adapter_id, "greentic.desktop.playwright");
    }

    #[test]
    fn unsupported_steps_fail_before_adapter_work() {
        let adapter = StaticAdapter::new(web_adapter());
        let step = RunnerStep {
            id: "fill_email".to_owned(),
            action: "fill".to_owned(),
            target: LocatorTarget {
                preferred: Some(LocatorStrategy {
                    label: Some("Email".to_owned()),
                    ..LocatorStrategy::default()
                }),
                ..LocatorTarget::default()
            },
            value: Some("user@example.com".to_owned()),
            required_capability: "windows.click".to_owned(),
        };

        let err = adapter
            .execute(step)
            .expect_err("unsupported capability must fail early");

        assert_eq!(
            err,
            AdapterError::UnsupportedCapability("windows.click".to_owned())
        );
    }
}
