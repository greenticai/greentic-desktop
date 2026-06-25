use greentic_desktop_adapter::{AdapterCapabilities, LocatorStrategy, LocatorTarget, RunnerStep};
use greentic_desktop_core::RiskLevel;
use greentic_desktop_recorder::{RecordingMode, RunnerPackage};
use greentic_desktop_session::{BootstrapAction, BrowserKind, SessionProfile, TeardownAction};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanningContext {
    pub available_adapters: Vec<AdapterCapabilities>,
    pub available_mcp_tools: Vec<String>,
    pub application_metadata: Vec<String>,
    pub existing_runners: Vec<String>,
    pub ltm_examples: Vec<String>,
    pub security_policies: Vec<String>,
    pub desktop_observations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerDraft {
    pub package: RunnerPackage,
    pub risk: RiskLevel,
    pub required_adapters: Vec<String>,
    pub session_profile: SessionProfile,
    pub evidence_policy: EvidencePolicy,
    pub open_questions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePolicy {
    pub screenshots: bool,
    pub redact_secrets: bool,
}

pub fn plan_prompt(prompt: &str, context: &PlanningContext) -> RunnerDraft {
    let lower = prompt.to_ascii_lowercase();
    let adapter_id = select_adapter_id(&lower, &context.available_adapters);
    let risk = if lower.contains("delete") || lower.contains("payment") || lower.contains("admin") {
        RiskLevel::High
    } else if lower.contains("create") || lower.contains("update") || lower.contains("submit") {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };
    let inputs = infer_inputs(&lower);
    let outputs = infer_outputs(&lower);
    let steps = infer_steps(&lower, &adapter_id);

    let session_profile = if adapter_id.contains("playwright") || lower.contains("web") {
        SessionProfile {
            id: "planned_web_session".to_owned(),
            bootstrap: vec![BootstrapAction::OpenBrowser {
                browser: BrowserKind::Default,
                url: "about:blank".to_owned(),
            }],
            teardown: Vec::new(),
        }
    } else if adapter_id.contains("terminal") {
        SessionProfile {
            id: "planned_terminal_session".to_owned(),
            bootstrap: vec![BootstrapAction::TerminalConnect {
                protocol: "tn3270".to_owned(),
                host: "{{secrets.mainframe_host}}".to_owned(),
                port: 23,
            }],
            teardown: vec![TeardownAction::TerminalDisconnect],
        }
    } else {
        SessionProfile {
            id: "planned_desktop_session".to_owned(),
            bootstrap: Vec::new(),
            teardown: Vec::new(),
        }
    };

    let mut open_questions = Vec::new();
    if !lower.contains("service account") && lower.contains("login") {
        open_questions.push("Which credentials or service account should be used?".to_owned());
    }
    if context.available_adapters.is_empty() {
        open_questions.push("No adapters are available in the planning context.".to_owned());
    }

    RunnerDraft {
        package: RunnerPackage {
            id: runner_id(prompt),
            version: "0.1.0".to_owned(),
            mode: RecordingMode::AssistedPrompt,
            inputs,
            secrets: vec!["secrets.service_account_password".to_owned()],
            steps,
            assertions: vec!["no unexpected errors".to_owned()],
            outputs,
        },
        risk,
        required_adapters: vec![adapter_id],
        session_profile,
        evidence_policy: EvidencePolicy {
            screenshots: true,
            redact_secrets: true,
        },
        open_questions,
    }
}

impl RunnerDraft {
    pub fn render_yaml(&self) -> String {
        let mut output = self.package.render_yaml();
        output.push_str(&format!("risk: {:?}\n", self.risk));
        output.push_str("required_adapters:\n");
        for adapter in &self.required_adapters {
            output.push_str(&format!("  - {adapter}\n"));
        }
        output.push_str(&format!("session_profile: {}\n", self.session_profile.id));
        output
    }
}

fn select_adapter_id(prompt: &str, adapters: &[AdapterCapabilities]) -> String {
    let preferred = if prompt.contains("web") || prompt.contains("crm") {
        "playwright"
    } else if prompt.contains("mainframe") || prompt.contains("terminal") {
        "terminal"
    } else if prompt.contains("java") {
        "java"
    } else if prompt.contains("windows") {
        "windows"
    } else {
        "vision"
    };

    adapters
        .iter()
        .find(|adapter| adapter.adapter_id.contains(preferred))
        .map(|adapter| adapter.adapter_id.clone())
        .unwrap_or_else(|| format!("greentic.desktop.{preferred}"))
}

fn infer_inputs(prompt: &str) -> Vec<String> {
    let mut inputs = Vec::new();
    if prompt.contains("company name") {
        inputs.push("inputs.company_name".to_owned());
    }
    if prompt.contains("email") {
        inputs.push("inputs.email".to_owned());
    }
    if prompt.contains("customer id") {
        inputs.push("inputs.customer_id".to_owned());
    }
    inputs
}

fn infer_outputs(prompt: &str) -> Vec<String> {
    if prompt.contains("returns the customer id") || prompt.contains("return the customer id") {
        vec!["outputs.customer_id".to_owned()]
    } else {
        Vec::new()
    }
}

fn infer_steps(prompt: &str, adapter_id: &str) -> Vec<RunnerStep> {
    if adapter_id.contains("playwright") || adapter_id.contains("web") {
        vec![
            step("open_app", "goto", "web.goto", None),
            step(
                "login",
                "fill",
                "web.fill",
                Some("{{secrets.service_account}}"),
            ),
            step(
                "create_customer",
                "fill",
                "web.fill",
                Some("{{inputs.company_name}}"),
            ),
            step("submit", "click", "web.click", None),
            step(
                "extract_customer_id",
                "extract_text",
                "web.extract_text",
                None,
            ),
        ]
    } else if prompt.contains("mainframe") || adapter_id.contains("terminal") {
        vec![
            step("connect", "connect", "terminal.connect", None),
            step(
                "wait_login",
                "wait_for_screen",
                "terminal.wait_for_screen",
                Some("LOGIN"),
            ),
        ]
    } else {
        vec![step("observe", "screenshot", "vision.screenshot", None)]
    }
}

fn step(id: &str, action: &str, capability: &str, value: Option<&str>) -> RunnerStep {
    RunnerStep {
        id: id.to_owned(),
        action: action.to_owned(),
        target: LocatorTarget {
            preferred: Some(LocatorStrategy {
                name: Some(id.to_owned()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        },
        value: value.map(str::to_owned),
        required_capability: capability.to_owned(),
    }
}

fn runner_id(prompt: &str) -> String {
    prompt
        .split_whitespace()
        .take(6)
        .map(|word| {
            word.chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> PlanningContext {
        PlanningContext {
            available_adapters: vec![AdapterCapabilities::new(
                "greentic.desktop.playwright",
                "1.0.0",
                ["web.goto", "web.fill", "web.click", "web.extract_text"],
            )],
            available_mcp_tools: Vec::new(),
            application_metadata: Vec::new(),
            existing_runners: Vec::new(),
            ltm_examples: Vec::new(),
            security_policies: Vec::new(),
            desktop_observations: Vec::new(),
        }
    }

    #[test]
    fn creates_draft_runner_from_prompt() {
        let draft = plan_prompt(
            "Create a runner that opens the CRM web app, logs in with the service account, creates a customer using company name and email, and returns the customer ID.",
            &context(),
        );

        assert_eq!(draft.risk, RiskLevel::Medium);
        assert_eq!(draft.required_adapters, vec!["greentic.desktop.playwright"]);
        assert!(draft
            .package
            .inputs
            .contains(&"inputs.company_name".to_owned()));
        assert!(draft
            .package
            .outputs
            .contains(&"outputs.customer_id".to_owned()));
        assert!(draft.render_yaml().contains("web.extract_text"));
    }

    #[test]
    fn flags_open_questions_for_missing_login_details() {
        let draft = plan_prompt("Open the web app and login.", &context());
        assert!(!draft.open_questions.is_empty());
    }
}
