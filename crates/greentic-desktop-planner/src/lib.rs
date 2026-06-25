use greentic_desktop_adapter::{AdapterCapabilities, LocatorStrategy, LocatorTarget, RunnerStep};
use greentic_desktop_core::RiskLevel;
use greentic_desktop_llm::{
    GreenticLlmClient, HeuristicLlmClient, LlmPlanningContext, LlmRequestEnvelope,
};
use greentic_desktop_policy::{validate_planned_runner, PlannerPolicy};
use greentic_desktop_recorder::{RecordingMode, RunnerPackage};
use greentic_desktop_runner_schema::{parse_runner_draft_json, SchemaDiagnostic};
use greentic_desktop_session::{BootstrapAction, BrowserKind, SessionProfile, TeardownAction};
use std::fs;
use std::path::Path;

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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlannerOptions {
    pub profile: Option<String>,
    pub dry_run: bool,
    pub policy: PlannerPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerResult {
    pub draft: RunnerDraft,
    pub request_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerDiagnostic {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for PlannerDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PlannerDiagnostic {}

pub fn plan_prompt_with_default_llm(
    prompt: &str,
    context: &PlanningContext,
    options: &PlannerOptions,
) -> Result<PlannerResult, PlannerDiagnostic> {
    plan_prompt_with_llm(prompt, context, options, &HeuristicLlmClient)
}

pub fn plan_prompt_with_llm(
    prompt: &str,
    context: &PlanningContext,
    options: &PlannerOptions,
    client: &impl GreenticLlmClient,
) -> Result<PlannerResult, PlannerDiagnostic> {
    if prompt.trim().is_empty() {
        return Err(diagnostic(
            "planner.needs_clarification",
            "prompt must not be empty",
        ));
    }

    let llm_context = llm_context(context);
    let request = LlmRequestEnvelope::prompt_to_runner(prompt, llm_context);
    let request_json = request.render_json();
    let response = client
        .complete(&request)
        .map_err(|err| diagnostic("planner.llm_unavailable", &format!("{err:?}")))?;
    let document = parse_runner_draft_json(&response.content).map_err(from_schema)?;
    validate_capabilities(context, &document.required_capabilities)?;
    validate_planned_runner(&document, &options.policy)
        .map_err(|err| diagnostic(&err.code, &err.message))?;

    let required_adapters = adapters_for_capabilities(context, &document.required_capabilities);
    let risk = document.risk_level;
    let open_questions = document.open_questions.clone();
    let package = document.into_package();
    let session_profile = session_profile_for(
        options.profile.as_deref(),
        required_adapters.first().map(String::as_str),
    );

    Ok(PlannerResult {
        draft: RunnerDraft {
            package,
            risk,
            required_adapters,
            session_profile,
            evidence_policy: EvidencePolicy {
                screenshots: true,
                redact_secrets: true,
            },
            open_questions,
        },
        request_json,
    })
}

pub fn save_draft_runner(draft: &RunnerDraft, path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, draft.render_yaml())
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

fn llm_context(context: &PlanningContext) -> LlmPlanningContext {
    LlmPlanningContext {
        available_adapters: context
            .available_adapters
            .iter()
            .map(|adapter| adapter.adapter_id.clone())
            .collect(),
        available_mcp_tools: context.available_mcp_tools.clone(),
        session_profiles: context.application_metadata.clone(),
        existing_runners: context.existing_runners.clone(),
        ltm_examples: context.ltm_examples.clone(),
        security_policy: context.security_policies.clone(),
        desktop_observation: context.desktop_observations.clone(),
    }
}

fn validate_capabilities(
    context: &PlanningContext,
    required_capabilities: &[String],
) -> Result<(), PlannerDiagnostic> {
    for capability in required_capabilities {
        if !context
            .available_adapters
            .iter()
            .any(|adapter| adapter.supports(capability))
        {
            return Err(diagnostic(
                "planner.unsupported_capability",
                &format!("no installed adapter supports {capability}"),
            ));
        }
    }
    Ok(())
}

fn adapters_for_capabilities(
    context: &PlanningContext,
    required_capabilities: &[String],
) -> Vec<String> {
    let mut adapters = Vec::new();
    for capability in required_capabilities {
        for adapter in &context.available_adapters {
            if adapter.supports(capability) && !adapters.contains(&adapter.adapter_id) {
                adapters.push(adapter.adapter_id.clone());
            }
        }
    }
    adapters
}

fn session_profile_for(profile: Option<&str>, adapter_id: Option<&str>) -> SessionProfile {
    let id = profile.unwrap_or("planned_desktop_session").to_owned();
    if adapter_id.unwrap_or_default().contains("playwright") {
        SessionProfile {
            id,
            bootstrap: vec![BootstrapAction::OpenBrowser {
                browser: BrowserKind::Default,
                url: "about:blank".to_owned(),
            }],
            teardown: Vec::new(),
        }
    } else if adapter_id.unwrap_or_default().contains("terminal") {
        SessionProfile {
            id,
            bootstrap: vec![BootstrapAction::TerminalConnect {
                protocol: "tn3270".to_owned(),
                host: "{{secrets.mainframe_host}}".to_owned(),
                port: 23,
            }],
            teardown: vec![TeardownAction::TerminalDisconnect],
        }
    } else {
        SessionProfile {
            id,
            bootstrap: Vec::new(),
            teardown: Vec::new(),
        }
    }
}

fn from_schema(err: SchemaDiagnostic) -> PlannerDiagnostic {
    diagnostic(&err.code, &err.message)
}

fn diagnostic(code: &str, message: &str) -> PlannerDiagnostic {
    PlannerDiagnostic {
        code: code.to_owned(),
        message: message.to_owned(),
    }
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
    use greentic_desktop_llm::StaticLlmClient;

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

    fn valid_llm() -> StaticLlmClient {
        StaticLlmClient::ok(
            r#"{
                "runner_id": "crm.create_customer",
                "version": "0.1.0-draft",
                "summary": "Create a customer",
                "risk_level": "medium",
                "required_capabilities": ["web.goto", "web.fill", "web.click", "web.extract_text"],
                "inputs": {"company_name": {"type": "string", "required": true}, "email": {"type": "string", "required": true}},
                "outputs": {"customer_id": {"type": "string"}},
                "steps": [{"id": "open", "action": "goto", "required_capability": "web.goto"}],
                "assertions": ["customer created"],
                "open_questions": []
            }"#,
        )
    }

    #[test]
    fn mock_llm_valid_runner_draft_is_validated() {
        let result = plan_prompt_with_llm(
            "Create CRM customer",
            &context(),
            &PlannerOptions::default(),
            &valid_llm(),
        )
        .expect("valid draft");

        assert_eq!(result.draft.package.id, "crm.create_customer");
        assert!(result.request_json.contains("desktop.prompt_to_runner"));
        assert_eq!(
            result.draft.required_adapters,
            vec!["greentic.desktop.playwright"]
        );
    }

    #[test]
    fn invalid_json_response_returns_diagnostic() {
        let err = plan_prompt_with_llm(
            "Create CRM customer",
            &context(),
            &PlannerOptions::default(),
            &StaticLlmClient::ok("not json"),
        )
        .expect_err("invalid json");

        assert_eq!(err.code, "planner.invalid_json");
    }

    #[test]
    fn schema_invalid_response_returns_diagnostic() {
        let err = plan_prompt_with_llm(
            "Create CRM customer",
            &context(),
            &PlannerOptions::default(),
            &StaticLlmClient::ok(r#"{"runner_id": ""}"#),
        )
        .expect_err("schema invalid");

        assert_eq!(err.code, "planner.schema_mismatch");
    }

    #[test]
    fn unsupported_capability_response_fails_before_save() {
        let err = plan_prompt_with_llm(
            "Create CRM customer",
            &context(),
            &PlannerOptions::default(),
            &StaticLlmClient::ok(
                r#"{
                    "runner_id": "crm.create_customer",
                    "version": "0.1.0-draft",
                    "summary": "Create a customer",
                    "risk_level": "medium",
                    "required_capabilities": ["sap.click"],
                    "inputs": {"company_name": {"type": "string", "required": true}},
                    "outputs": {},
                    "steps": [{"id": "sap", "action": "click", "required_capability": "sap.click"}],
                    "assertions": [],
                    "open_questions": []
                }"#,
            ),
        )
        .expect_err("unsupported");

        assert_eq!(err.code, "planner.unsupported_capability");
    }

    #[test]
    fn policy_denied_response_returns_diagnostic() {
        let err = plan_prompt_with_llm(
            "Make payment",
            &context(),
            &PlannerOptions::default(),
            &StaticLlmClient::ok(
                r#"{
                    "runner_id": "billing.pay",
                    "version": "0.1.0-draft",
                    "summary": "Pay invoice",
                    "risk_level": "critical",
                    "required_capabilities": ["web.click"],
                    "inputs": {"invoice_id": {"type": "string", "required": true}},
                    "outputs": {},
                    "steps": [{"id": "pay", "action": "payment", "required_capability": "web.click"}],
                    "assertions": [],
                    "open_questions": []
                }"#,
            ),
        )
        .expect_err("policy denied");

        assert_eq!(err.code, "planner.policy_denied");
    }

    #[test]
    fn prompt_with_missing_details_can_return_open_question() {
        let result = plan_prompt_with_llm(
            "Open the CRM",
            &context(),
            &PlannerOptions::default(),
            &StaticLlmClient::ok(
                r#"{
                    "runner_id": "crm.open",
                    "version": "0.1.0-draft",
                    "summary": "Open CRM",
                    "risk_level": "low",
                    "required_capabilities": ["web.goto"],
                    "inputs": {},
                    "outputs": {},
                    "steps": [],
                    "assertions": [],
                    "open_questions": ["Which CRM URL should be opened?"]
                }"#,
            ),
        )
        .expect("clarification draft");

        assert_eq!(
            result.draft.open_questions,
            vec!["Which CRM URL should be opened?".to_owned()]
        );
    }
}
