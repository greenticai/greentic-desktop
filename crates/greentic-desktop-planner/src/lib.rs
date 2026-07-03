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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedTechnology {
    ExistingRunner,
    Web,
    Excel,
    Java,
    Native,
    Terminal,
    Vision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRoute {
    pub technology: PlannedTechnology,
    pub adapter_id: String,
    pub confidence: u8,
    pub required_capabilities: Vec<String>,
    pub open_questions: Vec<String>,
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
    let max_retries = request.model_policy.max_retries;
    let mut response = client
        .complete(&request)
        .map_err(|err| diagnostic("planner.llm_unavailable", &format!("{err:?}")))?;
    let mut repair_attempt = 0;
    let mut document = loop {
        match parse_runner_draft_json(&response.content) {
            Ok(document) => break document,
            Err(err) if repair_attempt < max_retries => {
                repair_attempt += 1;
                let repair_request = LlmRequestEnvelope::repair_runner_json(
                    prompt,
                    request.context.clone(),
                    response.content,
                    format!("{}: {}", err.code, err.message),
                    repair_attempt,
                );
                response = client.complete(&repair_request).map_err(|llm_err| {
                    diagnostic("planner.llm_unavailable", &format!("{llm_err:?}"))
                })?;
            }
            Err(err) => return Err(from_schema(err)),
        }
    };
    resolve_adapter_id_steps(context, &mut document.steps);
    normalize_native_capabilities(context, &mut document.steps);
    document.required_capabilities = required_capabilities_from_steps(&document.steps);
    validate_planned_runner(&document, &options.policy)
        .map_err(|err| diagnostic(&err.code, &err.message))?;

    let required_adapters = adapters_for_capabilities(context, &document.required_capabilities);
    let risk = document.risk_level;
    let mut open_questions = document.open_questions.clone();
    open_questions.extend(missing_capability_questions(
        context,
        &document.required_capabilities,
    ));
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
    let route = route_capabilities(prompt, context);
    let adapter_id = route.adapter_id.clone();
    let risk = if lower.contains("delete") || lower.contains("payment") || lower.contains("admin") {
        RiskLevel::High
    } else if lower.contains("create") || lower.contains("update") || lower.contains("submit") {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };
    let inputs = infer_inputs(&lower);
    let outputs = infer_outputs(&lower);
    let steps = infer_steps(&lower, &route, &inputs, &outputs);

    let session_profile = if route.technology == PlannedTechnology::Web {
        SessionProfile {
            id: "planned_web_session".to_owned(),
            bootstrap: vec![BootstrapAction::OpenBrowser {
                browser: BrowserKind::Default,
                url: infer_url(prompt).unwrap_or_else(|| "about:blank".to_owned()),
            }],
            teardown: Vec::new(),
        }
    } else if route.technology == PlannedTechnology::Terminal {
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

    let mut open_questions = route.open_questions.clone();
    if !lower.contains("service account") && lower.contains("login") {
        open_questions.push("Which credentials or service account should be used?".to_owned());
    }
    if route.confidence < 50 {
        open_questions
            .push("Which application or technology should this runner target?".to_owned());
    }
    if outputs.is_empty() {
        open_questions.push("Which output should the runner return?".to_owned());
    }

    RunnerDraft {
        package: RunnerPackage {
            id: runner_id(prompt),
            version: "0.1.0".to_owned(),
            mode: RecordingMode::AssistedPrompt,
            inputs,
            secrets: infer_secrets(&lower),
            steps,
            assertions: vec!["no unexpected errors".to_owned()],
            outputs,
            open_questions: open_questions.clone(),
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

pub fn route_capabilities(prompt: &str, context: &PlanningContext) -> CapabilityRoute {
    let lower = prompt.to_ascii_lowercase();
    if let Some(existing) = context
        .existing_runners
        .iter()
        .chain(context.available_mcp_tools.iter())
        .find(|runner| prompt_matches_runner(&lower, runner))
    {
        return CapabilityRoute {
            technology: PlannedTechnology::ExistingRunner,
            adapter_id: existing.clone(),
            confidence: 95,
            required_capabilities: Vec::new(),
            open_questions: Vec::new(),
        };
    }

    let mut candidates = vec![
        route_candidate(
            PlannedTechnology::Web,
            context,
            &["web.goto", "web.fill", "web.click", "web.extract_text"],
            score_signals(&lower, context, &["http", "url", "browser", "dom", "web"]),
            "greentic.desktop.playwright",
        ),
        route_candidate(
            PlannedTechnology::Excel,
            context,
            &[
                "excel.read_range",
                "excel.search_rows",
                "excel.append_rows",
                "excel.create_workbook",
            ],
            score_signals(
                &lower,
                context,
                &["xlsx", "workbook", "spreadsheet", "worksheet", "excel file"],
            ),
            "greentic.desktop.excel",
        ),
        route_candidate(
            PlannedTechnology::Java,
            context,
            &[
                "java.find_window",
                "java.find_component",
                "java.type_text",
                "java.read_text",
            ],
            score_signals(
                &lower,
                context,
                &["java", "access bridge", "swing", "awt", "jar"],
            ),
            "greentic.desktop.java-accessibility",
        ),
        route_candidate(
            PlannedTechnology::Native,
            context,
            &[
                "windows.open_app",
                "macos.activate_app",
                "linux.find_window",
                "windows.find_element",
                "macos.find_element",
                "linux.find_element",
            ],
            score_signals(
                &lower,
                context,
                &[
                    "window",
                    "desktop app",
                    "native",
                    "windows",
                    "macos",
                    "linux",
                ],
            ),
            "greentic.desktop.native",
        ),
        route_candidate(
            PlannedTechnology::Terminal,
            context,
            &[
                "terminal.connect",
                "terminal.send_text",
                "terminal.wait_for_screen",
                "terminal.extract_field",
            ],
            score_signals(
                &lower,
                context,
                &["terminal", "screen", "host", "tn3270", "ssh", "mainframe"],
            ),
            "greentic.desktop.terminal-tn3270",
        ),
        route_candidate(
            PlannedTechnology::Vision,
            context,
            &[
                "vision.screenshot",
                "vision.find_text",
                "vision.extract_text",
            ],
            score_signals(&lower, context, &["screenshot", "image", "ocr", "visual"]),
            "greentic.desktop.vision",
        ),
    ];

    candidates.sort_by(|left, right| {
        right.confidence.cmp(&left.confidence).then_with(|| {
            technology_priority(left.technology).cmp(&technology_priority(right.technology))
        })
    });

    let mut selected = candidates.into_iter().next().unwrap_or(CapabilityRoute {
        technology: PlannedTechnology::Vision,
        adapter_id: "greentic.desktop.vision".to_owned(),
        confidence: 0,
        required_capabilities: vec!["vision.screenshot".to_owned()],
        open_questions: Vec::new(),
    });

    if selected.confidence == 0 {
        selected.open_questions.push(
            "Install a web, Java, native desktop, terminal, or vision adapter before planning."
                .to_owned(),
        );
    } else if !context
        .available_adapters
        .iter()
        .any(|adapter| adapter.adapter_id == selected.adapter_id)
    {
        selected.open_questions.push(format!(
            "Install an adapter that supports {}.",
            selected
                .required_capabilities
                .first()
                .cloned()
                .unwrap_or_else(|| "the selected technology".to_owned())
        ));
    }

    selected
}

fn route_candidate(
    technology: PlannedTechnology,
    context: &PlanningContext,
    desired_capabilities: &[&str],
    signal_score: u8,
    fallback_adapter: &str,
) -> CapabilityRoute {
    let matching = context.available_adapters.iter().find(|adapter| {
        desired_capabilities
            .iter()
            .any(|capability| adapter.supports(capability))
    });
    let capabilities = matching
        .map(|adapter| {
            desired_capabilities
                .iter()
                .filter(|capability| adapter.supports(capability))
                .map(|capability| (*capability).to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            desired_capabilities
                .iter()
                .map(|value| (*value).to_owned())
                .collect()
        });
    let installed_bonus = if matching.is_some() { 40 } else { 0 };

    CapabilityRoute {
        technology,
        adapter_id: matching
            .map(|adapter| adapter.adapter_id.clone())
            .unwrap_or_else(|| fallback_adapter.to_owned()),
        confidence: installed_bonus + signal_score,
        required_capabilities: capabilities,
        open_questions: Vec::new(),
    }
}

fn score_signals(prompt: &str, context: &PlanningContext, signals: &[&str]) -> u8 {
    let mut score = 0u8;
    for signal in signals {
        if prompt.contains(signal) {
            score = score.saturating_add(20);
        }
        if context
            .application_metadata
            .iter()
            .chain(context.desktop_observations.iter())
            .any(|value| value.to_ascii_lowercase().contains(signal))
        {
            score = score.saturating_add(25);
        }
    }
    score.min(60)
}

fn technology_priority(technology: PlannedTechnology) -> u8 {
    match technology {
        PlannedTechnology::ExistingRunner => 0,
        PlannedTechnology::Web => 1,
        PlannedTechnology::Excel => 2,
        PlannedTechnology::Java => 3,
        PlannedTechnology::Native => 4,
        PlannedTechnology::Terminal => 5,
        PlannedTechnology::Vision => 6,
    }
}

fn prompt_matches_runner(prompt: &str, runner: &str) -> bool {
    runner
        .split(['.', '_', '-'])
        .filter(|part| part.len() > 2)
        .any(|part| prompt.contains(&part.to_ascii_lowercase()))
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

fn adapters_for_capabilities(
    context: &PlanningContext,
    required_capabilities: &[String],
) -> Vec<String> {
    let mut adapters = Vec::new();
    for capability in required_capabilities {
        for adapter in &context.available_adapters {
            if adapter_matches_requirement(adapter, capability)
                && !adapters.contains(&adapter.adapter_id)
            {
                adapters.push(adapter.adapter_id.clone());
            }
        }
    }
    adapters
}

fn missing_capability_questions(
    context: &PlanningContext,
    required_capabilities: &[String],
) -> Vec<String> {
    let mut questions = Vec::new();
    for capability in required_capabilities {
        if !context
            .available_adapters
            .iter()
            .any(|adapter| adapter_matches_requirement(adapter, capability))
        {
            questions.push(format!(
                "Install an adapter that supports {capability} before testing or running this draft."
            ));
        }
    }
    questions.sort();
    questions.dedup();
    questions
}

fn adapter_matches_requirement(adapter: &AdapterCapabilities, requirement: &str) -> bool {
    adapter.adapter_id == requirement || adapter.supports(requirement)
}

fn resolve_adapter_id_steps(context: &PlanningContext, steps: &mut [RunnerStep]) {
    for step in steps {
        let Some(adapter) = context
            .available_adapters
            .iter()
            .find(|adapter| adapter.adapter_id == step.required_capability)
        else {
            continue;
        };
        if let Some(capability) = capability_for_action(adapter, &step.action) {
            step.required_capability = capability;
        }
    }
}

fn normalize_native_capabilities(context: &PlanningContext, steps: &mut [RunnerStep]) {
    let Some(preferred_prefix) = preferred_native_prefix(context) else {
        for step in steps {
            if let Some(capability) =
                canonical_native_capability(&step.required_capability, None, &step.action)
            {
                step.required_capability = capability;
            }
        }
        return;
    };

    for step in steps {
        if let Some(capability) = canonical_native_capability(
            &step.required_capability,
            Some(preferred_prefix),
            &step.action,
        ) {
            step.required_capability = capability;
        }
    }
}

fn preferred_native_prefix(context: &PlanningContext) -> Option<&'static str> {
    let observations = context
        .desktop_observations
        .iter()
        .chain(context.application_metadata.iter())
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if observations
        .iter()
        .any(|value| value.contains("current desktop platform: macos"))
    {
        return Some("macos");
    }
    if observations
        .iter()
        .any(|value| value.contains("current desktop platform: windows"))
    {
        return Some("windows");
    }
    if observations
        .iter()
        .any(|value| value.contains("current desktop platform: linux"))
    {
        return Some("linux");
    }
    if context
        .available_adapters
        .iter()
        .any(|adapter| adapter.adapter_id == "greentic.desktop.macos.ax")
    {
        return Some("macos");
    }
    if context
        .available_adapters
        .iter()
        .any(|adapter| adapter.adapter_id == "greentic.desktop.windows-ui")
    {
        return Some("windows");
    }
    if context.available_adapters.iter().any(|adapter| {
        adapter.adapter_id == "greentic.desktop.linux.x11"
            || adapter.adapter_id == "greentic.desktop.linux.wayland"
    }) {
        return Some("linux");
    }
    None
}

fn canonical_native_capability(
    capability: &str,
    preferred_prefix: Option<&str>,
    action: &str,
) -> Option<String> {
    let (prefix, suffix) = capability.split_once('.')?;
    if !matches!(prefix, "windows" | "macos" | "linux") {
        return None;
    }
    let prefix = preferred_prefix.unwrap_or(prefix);
    let normalized = normalize_capability_token(&format!("{suffix}_{action}"));
    let canonical_suffix = if normalized.contains("open_app")
        || normalized.contains("activate_app")
        || normalized.contains("launch")
        || normalized.contains("start")
    {
        match prefix {
            "windows" => "open_app",
            "macos" => "activate_app",
            "linux" => "find_window",
            _ => suffix,
        }
    } else if normalized.contains("find_window") || normalized.contains("activate_window") {
        match prefix {
            "linux" if normalized.contains("activate_window") => "activate_window",
            "linux" => "find_window",
            _ => "find_window",
        }
    } else if normalized.contains("find") {
        "find_element"
    } else if normalized.contains("press_shortcut")
        || normalized.contains("shortcut")
        || normalized.contains("hotkey")
        || (normalized.contains("press")
            && [
                "cmd", "command", "ctrl", "control", "alt", "option", "shift", "meta", "win",
            ]
            .iter()
            .any(|modifier| normalized.split('_').any(|part| part == *modifier)))
    {
        "press_shortcut"
    } else if normalized.contains("save_as")
        || normalized == "save"
        || normalized.ends_with("_save")
        || normalized.contains("_save_")
        || normalized.starts_with("save_")
    {
        "save_as"
    } else if normalized.contains("click") {
        "click_element"
    } else if normalized.contains("type")
        || normalized.contains("input")
        || normalized.contains("fill")
        || normalized.contains("enter")
    {
        "type_text"
    } else if normalized.contains("read")
        || normalized.contains("extract")
        || normalized.contains("return")
        || normalized.contains("get")
    {
        "read_text"
    } else if normalized.contains("assert") {
        "assert_visible"
    } else if normalized.contains("screenshot") {
        "screenshot"
    } else {
        suffix
    };
    Some(format!("{prefix}.{canonical_suffix}"))
}

fn required_capabilities_from_steps(steps: &[RunnerStep]) -> Vec<String> {
    let mut capabilities = steps
        .iter()
        .map(|step| step.required_capability.clone())
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn capability_for_action(adapter: &AdapterCapabilities, action: &str) -> Option<String> {
    let normalized_action = normalize_capability_token(action);
    let suffix_matches = |capability: &&String| {
        capability
            .rsplit('.')
            .next()
            .map(normalize_capability_token)
            .is_some_and(|suffix| suffix == normalized_action)
    };
    let contains_action = |capability: &&String| {
        normalize_capability_token(capability)
            .split('_')
            .any(|part| part == normalized_action)
    };

    adapter
        .capabilities
        .iter()
        .find(suffix_matches)
        .or_else(|| adapter.capabilities.iter().find(contains_action))
        .or_else(|| semantic_capability_for_action(adapter, &normalized_action))
        .or_else(|| adapter.capabilities.first())
        .cloned()
}

fn semantic_capability_for_action<'a>(
    adapter: &'a AdapterCapabilities,
    normalized_action: &str,
) -> Option<&'a String> {
    let preferred_terms: &[&str] = match normalized_action {
        "open" | "launch" | "activate" | "start" => &["open", "activate", "goto", "find"],
        "enter" | "input" | "type" | "write" | "fill" => &["type", "fill", "send"],
        "press_shortcut" | "shortcut" | "hotkey" => &["press_shortcut", "shortcut", "press"],
        "press" | "click" | "select" => &["click", "press", "select"],
        "read" | "observe" | "extract" | "return" | "get" => {
            &["read", "extract", "screenshot", "observe"]
        }
        "save" => &["save", "click", "press"],
        _ => &[],
    };
    adapter.capabilities.iter().find(|capability| {
        let normalized = normalize_capability_token(capability);
        preferred_terms
            .iter()
            .any(|term| normalized.split('_').any(|part| part == *term))
    })
}

fn normalize_capability_token(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
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
    for name in extract_named_fields(prompt, &["using", "with", "from"]) {
        inputs.push(format!("inputs.{name}"));
    }
    if prompt.contains("number 1")
        || prompt.contains("number one")
        || prompt.contains("two numbers")
    {
        inputs.push("inputs.number_1".to_owned());
    }
    if prompt.contains("number 2")
        || prompt.contains("number two")
        || prompt.contains("two numbers")
    {
        inputs.push("inputs.number_2".to_owned());
    }
    if prompt.contains("operation") {
        inputs.push("inputs.operation".to_owned());
    }
    inputs.sort();
    inputs.dedup();
    inputs
}

fn infer_outputs(prompt: &str) -> Vec<String> {
    let mut outputs = Vec::new();
    for marker in ["return ", "returns ", "read ", "extract "] {
        if let Some(index) = prompt.find(marker) {
            let name = prompt[index + marker.len()..]
                .split([',', '.', ';'])
                .next()
                .unwrap_or("output");
            let name = normalize_identifier(
                name.trim()
                    .trim_start_matches("the ")
                    .trim_start_matches("a ")
                    .trim_start_matches("an "),
            );
            if !name.is_empty() {
                outputs.push(format!("outputs.{name}"));
            }
        }
    }
    outputs.sort();
    outputs.dedup();
    outputs
}

fn infer_secrets(prompt: &str) -> Vec<String> {
    if prompt.contains("login")
        || prompt.contains("credential")
        || prompt.contains("password")
        || prompt.contains("token")
    {
        vec!["secrets.service_account_password".to_owned()]
    } else {
        Vec::new()
    }
}

fn infer_steps(
    prompt: &str,
    route: &CapabilityRoute,
    inputs: &[String],
    outputs: &[String],
) -> Vec<RunnerStep> {
    match route.technology {
        PlannedTechnology::Web => {
            let mut steps = vec![step(
                "open_target",
                "goto",
                "web.goto",
                infer_url(prompt).as_deref(),
            )];
            for input in inputs {
                let value = format!("{{{{{input}}}}}");
                steps.push(step(
                    &format!("fill_{}", input.trim_start_matches("inputs.")),
                    "fill",
                    "web.fill",
                    Some(&value),
                ));
            }
            steps.push(step("submit", "click", "web.click", None));
            for output in outputs {
                steps.push(step(
                    &format!("extract_{}", output.trim_start_matches("outputs.")),
                    "extract_text",
                    "web.extract_text",
                    None,
                ));
            }
            steps
        }
        PlannedTechnology::Excel => {
            let path = inputs
                .iter()
                .find(|input| input.contains("xlsx") || input.contains("spreadsheet"))
                .cloned()
                .unwrap_or_else(|| "inputs.xlsx_path".to_owned());
            let search = inputs
                .iter()
                .find(|input| input.contains("search"))
                .cloned()
                .unwrap_or_else(|| "inputs.search_term".to_owned());
            let mut steps = if prompt.contains("append") || prompt.contains("add row") {
                vec![step(
                    "append_rows",
                    "append_rows",
                    "excel.append_rows",
                    Some(&format!(
                        r#"{{"path":"{{{{{path}}}}}","sheet":"Sheet1","rows":[],"save_as":"{{{{{path}}}}}","overwrite":true,"allow_in_place":true}}"#
                    )),
                )]
            } else if prompt.contains("report")
                || prompt.contains("export")
                || prompt.contains("create")
            {
                vec![step(
                    "create_workbook",
                    "create_workbook",
                    "excel.create_workbook",
                    Some(
                        r#"{"path":"{{inputs.output_path}}","overwrite":false,"sheets":[{"name":"Sheet1","rows":[]}]}"#,
                    ),
                )]
            } else if prompt.contains("search")
                || prompt.contains("find")
                || prompt.contains("look up")
            {
                vec![step(
                    "search_rows",
                    "search_rows",
                    "excel.search_rows",
                    Some(&format!(
                        r#"{{"path":"{{{{{path}}}}}","sheet":"Sheet1","filters":[{{"column":"Name","op":"contains","value":"{{{{{search}}}}}"}}]}}"#
                    )),
                )]
            } else {
                vec![step(
                    "read_range",
                    "read_range",
                    "excel.read_range",
                    Some(&format!(
                        r#"{{"path":"{{{{{path}}}}}","sheet":"Sheet1","range":"A1:Z100"}}"#
                    )),
                )]
            };
            for output in outputs {
                steps.push(step(
                    &format!("extract_{}", output.trim_start_matches("outputs.")),
                    "extract_text",
                    "excel.search_rows",
                    None,
                ));
            }
            steps
        }
        PlannedTechnology::Java => structured_desktop_steps("java", inputs, outputs),
        PlannedTechnology::Native => {
            if route.adapter_id.contains("windows") {
                structured_desktop_steps("windows", inputs, outputs)
            } else if route.adapter_id.contains("macos") {
                structured_desktop_steps("macos", inputs, outputs)
            } else {
                structured_desktop_steps("linux", inputs, outputs)
            }
        }
        PlannedTechnology::Terminal => {
            let mut steps = vec![step("connect", "connect", "terminal.connect", None)];
            let value = inputs.first().map(|input| format!("{{{{{input}}}}}"));
            steps.push(step(
                "send_input",
                "send_text",
                "terminal.send_text",
                value.as_deref(),
            ));
            steps.push(step(
                "wait_for_output",
                "wait_for_screen",
                "terminal.wait_for_screen",
                None,
            ));
            steps
        }
        PlannedTechnology::Vision | PlannedTechnology::ExistingRunner => {
            vec![step("observe", "screenshot", "vision.screenshot", None)]
        }
    }
}

fn structured_desktop_steps(
    prefix: &str,
    inputs: &[String],
    outputs: &[String],
) -> Vec<RunnerStep> {
    let mut steps = Vec::new();
    match prefix {
        "windows" => steps.push(step("open_app", "open_app", "windows.open_app", None)),
        "macos" => steps.push(step(
            "activate_app",
            "activate_app",
            "macos.activate_app",
            None,
        )),
        "linux" => steps.push(step(
            "find_window",
            "find_window",
            "linux.find_window",
            None,
        )),
        "java" => steps.push(step("find_window", "find_window", "java.find_window", None)),
        _ => {}
    }
    for input in inputs {
        let name = input.trim_start_matches("inputs.");
        let find_capability = if prefix == "java" {
            "java.find_component".to_owned()
        } else {
            format!("{prefix}.find_element")
        };
        let type_capability = format!("{prefix}.type_text");
        let value = format!("{{{{{input}}}}}");
        steps.push(step(
            &format!("find_{name}"),
            "find",
            &find_capability,
            None,
        ));
        steps.push(step(
            &format!("type_{name}"),
            "type_text",
            &type_capability,
            Some(&value),
        ));
    }
    for output in outputs {
        let name = output.trim_start_matches("outputs.");
        let read_capability = format!("{prefix}.read_text");
        steps.push(step(
            &format!("read_{name}"),
            "read_text",
            &read_capability,
            None,
        ));
    }
    steps
}

fn extract_named_fields(prompt: &str, markers: &[&str]) -> Vec<String> {
    let mut fields = Vec::new();
    for marker in markers {
        if let Some(index) = prompt.find(marker) {
            let rest = &prompt[index + marker.len()..];
            let segment = split_at_first(rest, &[" and return", " then ", " to ", "."]);
            let segment = segment.replace(" and ", ",");
            for item in segment.split([',', '&']) {
                let normalized = normalize_identifier(
                    item.trim()
                        .trim_start_matches("the ")
                        .trim_start_matches("a ")
                        .trim_start_matches("an "),
                );
                if !normalized.is_empty()
                    && !matches!(normalized.as_str(), "login" | "service_account")
                {
                    fields.push(normalized);
                }
            }
        }
    }
    fields
}

fn split_at_first<'a>(value: &'a str, markers: &[&str]) -> &'a str {
    let stop = markers.iter().filter_map(|marker| value.find(marker)).min();
    stop.map(|index| &value[..index]).unwrap_or(value)
}

fn infer_url(prompt: &str) -> Option<String> {
    prompt
        .split_whitespace()
        .find(|word| word.starts_with("http://") || word.starts_with("https://"))
        .map(|word| word.trim_matches(|ch| ch == ',' || ch == '.').to_owned())
}

fn normalize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
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
    use greentic_desktop_llm::{
        LlmError, LlmRequestEnvelope, LlmResponse, OpenAiCompatibleLlmClient, StaticLlmClient,
    };
    use std::collections::VecDeque;
    use std::sync::Mutex;

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

    fn context_with(
        adapters: Vec<AdapterCapabilities>,
        observations: Vec<&str>,
    ) -> PlanningContext {
        PlanningContext {
            available_adapters: adapters,
            available_mcp_tools: Vec::new(),
            application_metadata: Vec::new(),
            existing_runners: Vec::new(),
            ltm_examples: Vec::new(),
            security_policies: Vec::new(),
            desktop_observations: observations.into_iter().map(str::to_owned).collect(),
        }
    }

    #[test]
    fn creates_draft_runner_from_prompt() {
        let draft = plan_prompt(
            "Create a runner that opens https://example.test, logs in with the service account, fills a web form using company name and email, and returns the confirmation number.",
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
            .contains(&"outputs.confirmation_number".to_owned()));
        assert!(draft.render_yaml().contains("web.extract_text"));
    }

    #[test]
    fn spreadsheet_file_prompt_routes_to_excel_adapter() {
        let context = context_with(
            vec![AdapterCapabilities::new(
                "greentic.desktop.excel",
                "1.0.0",
                ["excel.read_range", "excel.search_rows", "excel.append_rows"],
            )],
            Vec::new(),
        );
        let draft = plan_prompt(
            "Ask for an xlsx file and a search term, search the workbook, and return the matching row.",
            &context,
        );

        assert_eq!(draft.required_adapters, vec!["greentic.desktop.excel"]);
        assert!(draft
            .package
            .steps
            .iter()
            .any(|step| step.required_capability == "excel.search_rows"));
        assert!(!draft.render_yaml().contains("macos.copy_spreadsheet_row"));
    }

    #[test]
    fn calculator_prompt_with_two_numbers_derives_inputs_and_result() {
        let draft = plan_prompt(
            "Open the calculator app. Take three inputs: two numbers and one operation plus, minus, divide or multiply. Return the result.",
            &context(),
        );

        assert!(draft.package.inputs.contains(&"inputs.number_1".to_owned()));
        assert!(draft.package.inputs.contains(&"inputs.number_2".to_owned()));
        assert!(draft
            .package
            .inputs
            .contains(&"inputs.operation".to_owned()));
        assert_eq!(draft.package.outputs, vec!["outputs.result"]);
    }

    #[test]
    fn router_selects_java_from_capability_and_observation() {
        let context = context_with(
            vec![AdapterCapabilities::new(
                "greentic.desktop.java-accessibility",
                "1.0.0",
                [
                    "java.find_window",
                    "java.find_component",
                    "java.type_text",
                    "java.read_text",
                ],
            )],
            vec!["active process: Sample.jar Java Access Bridge enabled"],
        );

        let draft = plan_prompt(
            "Open the desktop form using employee id and return the status",
            &context,
        );

        assert_eq!(
            draft.required_adapters,
            vec!["greentic.desktop.java-accessibility"]
        );
        assert!(draft
            .package
            .steps
            .iter()
            .any(|step| step.required_capability == "java.find_component"));
    }

    #[test]
    fn router_selects_native_desktop_from_window_observation() {
        let context = context_with(
            vec![AdapterCapabilities::new(
                "greentic.desktop.windows-ui",
                "1.0.0",
                [
                    "windows.open_app",
                    "windows.find_element",
                    "windows.type_text",
                    "windows.read_text",
                ],
            )],
            vec!["active window: Sample native desktop app"],
        );

        let draft = plan_prompt(
            "Update the desktop app using case id and return result",
            &context,
        );

        assert_eq!(draft.required_adapters, vec!["greentic.desktop.windows-ui"]);
        assert!(draft
            .package
            .steps
            .iter()
            .any(|step| step.required_capability == "windows.open_app"));
    }

    #[test]
    fn native_shortcut_actions_keep_press_shortcut_capability() {
        assert_eq!(
            canonical_native_capability("macos.click_element", Some("macos"), "press_shortcut")
                .as_deref(),
            Some("macos.press_shortcut")
        );
        assert_eq!(
            canonical_native_capability("macos.click_element", Some("macos"), "press Cmd+N")
                .as_deref(),
            Some("macos.press_shortcut")
        );
    }

    #[test]
    fn native_save_actions_keep_save_as_capability() {
        assert_eq!(
            canonical_native_capability("macos.click_element", Some("macos"), "save_as").as_deref(),
            Some("macos.save_as")
        );
        assert_eq!(
            canonical_native_capability("macos.click_element", Some("macos"), "save document")
                .as_deref(),
            Some("macos.save_as")
        );
    }

    #[test]
    fn router_selects_terminal_from_profile_signal() {
        let context = context_with(
            vec![AdapterCapabilities::new(
                "greentic.desktop.terminal-tn3270",
                "1.0.0",
                [
                    "terminal.connect",
                    "terminal.send_text",
                    "terminal.wait_for_screen",
                ],
            )],
            vec!["terminal profile host=green-screen protocol=tn3270"],
        );

        let draft = plan_prompt(
            "Lookup a record using account number and return balance",
            &context,
        );

        assert_eq!(
            draft.required_adapters,
            vec!["greentic.desktop.terminal-tn3270"]
        );
        assert!(draft
            .package
            .steps
            .iter()
            .any(|step| step.required_capability == "terminal.connect"));
    }

    #[test]
    fn router_selects_vision_as_fallback_when_only_visual_adapter_is_available() {
        let context = context_with(
            vec![AdapterCapabilities::new(
                "greentic.desktop.vision",
                "1.0.0",
                ["vision.screenshot", "vision.extract_text"],
            )],
            vec!["screenshot only remote app"],
        );

        let draft = plan_prompt("Read the visible result from the remote app", &context);

        assert_eq!(draft.required_adapters, vec!["greentic.desktop.vision"]);
        assert_eq!(
            draft.package.steps[0].required_capability,
            "vision.screenshot"
        );
    }

    #[test]
    fn router_recommends_adapter_when_required_capability_is_missing() {
        let context = context_with(Vec::new(), vec!["browser url: https://example.test"]);

        let draft = plan_prompt("Open https://example.test and return title", &context);

        assert!(draft
            .open_questions
            .iter()
            .any(|question| question.contains("Install")));
        assert_eq!(draft.required_adapters, vec!["greentic.desktop.playwright"]);
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

    struct SequencedLlmClient {
        responses: Mutex<VecDeque<Result<LlmResponse, LlmError>>>,
        requests: Mutex<Vec<String>>,
    }

    impl SequencedLlmClient {
        fn ok(responses: Vec<&'static str>) -> Self {
            Self {
                responses: Mutex::new(
                    responses
                        .into_iter()
                        .map(|content| {
                            Ok(LlmResponse {
                                content: content.to_owned(),
                            })
                        })
                        .collect(),
                ),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().expect("requests lock").clone()
        }
    }

    impl GreenticLlmClient for SequencedLlmClient {
        fn complete(&self, request: &LlmRequestEnvelope) -> Result<LlmResponse, LlmError> {
            self.requests
                .lock()
                .expect("requests lock")
                .push(request.render_json());
            self.responses
                .lock()
                .expect("responses lock")
                .pop_front()
                .unwrap_or_else(|| Err(LlmError::Unavailable("no more mock responses".to_owned())))
        }
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
    fn invalid_structured_llm_response_triggers_repair_and_then_succeeds() {
        let client = SequencedLlmClient::ok(vec![
            r#"{
                "runner_id": "crm.create_customer",
                "version": "0.1.0-draft",
                "summary": "Create a customer",
                "risk_level": "moderate",
                "required_capabilities": ["web.goto"],
                "inputs": {"company_name": {"type": "string", "required": true}},
                "outputs": {"customer_id": {"type": "string"}},
                "steps": [{"id": "open", "action": "goto", "required_capability": "web.goto"}],
                "assertions": ["customer created"],
                "open_questions": []
            }"#,
            r#"{
                "runner_id": "crm.create_customer",
                "version": "0.1.0-draft",
                "summary": "Create a customer",
                "risk_level": "medium",
                "required_capabilities": ["web.goto"],
                "inputs": {"company_name": {"type": "string", "required": true}},
                "outputs": {"customer_id": {"type": "string"}},
                "steps": [{"id": "open", "action": "goto", "required_capability": "web.goto"}],
                "assertions": ["customer created"],
                "open_questions": []
            }"#,
        ]);

        let result = plan_prompt_with_llm(
            "Create CRM customer",
            &context(),
            &PlannerOptions::default(),
            &client,
        )
        .expect("repair should produce a valid draft");

        assert_eq!(result.draft.package.id, "crm.create_customer");
        let requests = client.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("\"task\":\"desktop.prompt_to_runner\""));
        assert!(requests[1].contains("\"task\":\"desktop.repair_runner_json\""));
        assert!(requests[1].contains("risk_level"));
        assert!(requests[1].contains("one of"));
        assert!(requests[1].contains("moderate"));
    }

    #[test]
    fn exhausted_structured_repair_returns_validation_details() {
        let client = SequencedLlmClient::ok(vec![
            r#"{"runner_id": ""}"#,
            r#"{"runner_id": ""}"#,
            r#"{"runner_id": ""}"#,
        ]);

        let err = plan_prompt_with_llm(
            "Create CRM customer",
            &context(),
            &PlannerOptions::default(),
            &client,
        )
        .expect_err("repair attempts should be exhausted");

        assert_eq!(err.code, "planner.schema_mismatch");
        assert!(
            err.message.contains("required property") || err.message.contains("runner_id"),
            "{}",
            err.message
        );
        assert_eq!(client.requests().len(), 3);
    }

    #[test]
    fn calculator_llm_response_missing_step_id_returns_schema_diagnostic() {
        let err = plan_prompt_with_llm(
            "open the local calculator app, ask the user for two numbers and an operation, introduce the first number in the calculator, press the operation, introduce the next, get the result and return it",
            &context(),
            &PlannerOptions::default(),
            &StaticLlmClient::ok(
                r#"{
                    "runner_id": "calculator.local",
                    "version": "0.1.0-draft",
                    "summary": "Use local calculator",
                    "risk_level": "low",
                    "required_capabilities": ["macos.activate_app", "macos.type_text", "macos.read_text"],
                    "inputs": {"number_1": {"type": "number"}, "number_2": {"type": "number"}, "operation": {"type": "string"}},
                    "outputs": {"result": {"type": "string"}},
                    "steps": [{"action": "activate_app", "required_capability": "macos.activate_app", "value": "Calculator"}],
                    "assertions": ["result is visible"],
                    "open_questions": []
                }"#,
            ),
        )
        .expect_err("missing step id should fail schema validation");

        assert_eq!(err.code, "planner.schema_mismatch");
        assert!(
            err.message.contains("\"id\" is a required property"),
            "{}",
            err.message
        );
    }

    #[test]
    fn unsupported_capability_response_creates_draft_with_open_question() {
        let result = plan_prompt_with_llm(
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
        .expect("unsupported runtime capability should not block drafting");

        assert!(result.draft.required_adapters.is_empty());
        assert!(result.draft.open_questions.contains(
            &"Install an adapter that supports sap.click before testing or running this draft."
                .to_owned()
        ));
    }

    #[test]
    fn llm_adapter_id_requirement_uses_installed_adapter() {
        let result = plan_prompt_with_llm(
            "Observe the desktop visually",
            &context_with(
                vec![AdapterCapabilities::new(
                    "greentic.desktop.vision",
                    "1.0.0",
                    ["vision.screenshot", "vision.extract_text"],
                )],
                Vec::new(),
            ),
            &PlannerOptions::default(),
            &StaticLlmClient::ok(
                r#"{
                    "runner_id": "desktop.observe",
                    "version": "0.1.0-draft",
                    "summary": "Observe desktop",
                    "risk_level": "low",
                    "required_capabilities": ["greentic.desktop.vision"],
                    "inputs": {},
                    "outputs": {"visible_text": {"type": "string"}},
                    "steps": [{"id": "observe", "action": "observe", "required_capability": "greentic.desktop.vision"}],
                    "assertions": [],
                    "open_questions": []
                }"#,
            ),
        )
        .expect("adapter id requirement should be accepted for draft creation");

        assert_eq!(
            result.draft.required_adapters,
            vec!["greentic.desktop.vision"]
        );
        assert!(result.draft.open_questions.is_empty());
    }

    #[test]
    fn llm_adapter_id_steps_are_normalized_to_executable_capabilities() {
        let result = plan_prompt_with_llm(
            "Create a desktop document",
            &context_with(
                vec![
                    AdapterCapabilities::new(
                        "greentic.desktop.java-accessibility",
                        "1.0.0",
                        ["java.find_window", "java.type_text"],
                    ),
                    AdapterCapabilities::new(
                        "greentic.desktop.macos.ax",
                        "1.0.0",
                        ["macos.activate_app", "macos.type_text", "macos.read_text"],
                    ),
                    AdapterCapabilities::new(
                        "greentic.desktop.playwright",
                        "1.0.0",
                        ["web.goto", "web.fill", "web.extract_text"],
                    ),
                    AdapterCapabilities::new(
                        "greentic.desktop.vision",
                        "1.0.0",
                        ["vision.screenshot", "vision.extract_text"],
                    ),
                ],
                Vec::new(),
            ),
            &PlannerOptions::default(),
            &StaticLlmClient::ok(
                r#"{
                    "runner_id": "document.create",
                    "version": "0.1.0-draft",
                    "summary": "Create document",
                    "risk_level": "medium",
                    "required_capabilities": ["greentic.desktop.java-accessibility", "greentic.desktop.macos.ax", "greentic.desktop.playwright", "greentic.desktop.vision"],
                    "inputs": {"document_path": {"type": "string"}, "text_content": {"type": "string"}},
                    "outputs": {"saved_status": {"type": "string"}},
                    "steps": [
                        {"id": "open-word", "action": "activate_app", "required_capability": "greentic.desktop.macos.ax", "value": "Word"},
                        {"id": "type-text", "action": "type_text", "required_capability": "greentic.desktop.java-accessibility", "value": "{{inputs.text_content}}"},
                        {"id": "read-state", "action": "extract_text", "required_capability": "greentic.desktop.vision"}
                    ],
                    "assertions": [],
                    "open_questions": []
                }"#,
            ),
        )
        .expect("adapter id steps should be normalized");

        assert_eq!(
            result.draft.required_adapters,
            vec![
                "greentic.desktop.java-accessibility",
                "greentic.desktop.macos.ax",
                "greentic.desktop.vision"
            ]
        );
        let step_capabilities = result
            .draft
            .package
            .steps
            .iter()
            .map(|step| step.required_capability.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            step_capabilities,
            vec![
                "macos.activate_app",
                "java.type_text",
                "vision.extract_text"
            ]
        );
    }

    #[test]
    fn llm_windows_native_steps_are_normalized_to_current_macos_platform() {
        let result = plan_prompt_with_llm(
            "Create a desktop document",
            &PlanningContext {
                available_adapters: vec![AdapterCapabilities::new(
                    "greentic.desktop.macos.ax",
                    "1.0.0",
                    [
                        "macos.activate_app",
                        "macos.find_element",
                        "macos.type_text",
                        "macos.click_element",
                        "macos.read_text",
                    ],
                )],
                available_mcp_tools: Vec::new(),
                application_metadata: vec!["current desktop platform: macos".to_owned()],
                existing_runners: Vec::new(),
                ltm_examples: Vec::new(),
                security_policies: Vec::new(),
                desktop_observations: vec!["current desktop platform: macos".to_owned()],
            },
            &PlannerOptions::default(),
            &StaticLlmClient::ok(
                r#"{
                    "runner_id": "document.create",
                    "version": "0.1.0-draft",
                    "summary": "Create document",
                    "risk_level": "medium",
                    "required_capabilities": ["windows.activate_app", "windows.click", "windows.read_text", "windows.type_text"],
                    "inputs": {"document_path": {"type": "string"}, "text_content": {"type": "string"}},
                    "outputs": {"saved_status": {"type": "string"}},
                    "steps": [
                        {"id": "open-word", "action": "activate_app", "required_capability": "windows.activate_app", "value": "Word"},
                        {"id": "type-text", "action": "type_text", "required_capability": "windows.type_text", "value": "{{inputs.text_content}}"},
                        {"id": "save", "action": "click", "required_capability": "windows.click"},
                        {"id": "read-state", "action": "read_text", "required_capability": "windows.read_text"}
                    ],
                    "assertions": [],
                    "open_questions": []
                }"#,
            ),
        )
        .expect("foreign native capabilities should normalize");

        assert_eq!(
            result.draft.required_adapters,
            vec!["greentic.desktop.macos.ax"]
        );
        let step_capabilities = result
            .draft
            .package
            .steps
            .iter()
            .map(|step| step.required_capability.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            step_capabilities,
            vec![
                "macos.activate_app",
                "macos.type_text",
                "macos.click_element",
                "macos.read_text"
            ]
        );
    }

    #[test]
    fn llm_primitive_word_prompt_accepts_app_name_aliases() {
        let result = plan_prompt_with_llm(
            "open the word app. Create a new word document in the place and the name that input parameters provide. Add the text that input parameters provide. Save the document.",
            &PlanningContext {
                available_adapters: vec![AdapterCapabilities::new(
                    "greentic.desktop.macos.ax",
                    "1.0.0",
                    [
                        "macos.activate_app",
                        "macos.find_element",
                        "macos.type_text",
                        "macos.click_element",
                    ],
                )],
                available_mcp_tools: Vec::new(),
                application_metadata: vec!["current desktop platform: macos".to_owned()],
                existing_runners: Vec::new(),
                ltm_examples: Vec::new(),
                security_policies: Vec::new(),
                desktop_observations: vec!["current desktop platform: macos".to_owned()],
            },
            &PlannerOptions::default(),
            &StaticLlmClient::ok(
                r#"{
                    "runner_id": "word.document.create",
                    "version": "0.1.0-draft",
                    "summary": "Create a Word document",
                    "risk_level": "medium",
                    "required_capabilities": ["macos.activate_app", "macos.type_text", "macos.click_element"],
                    "inputs": {"document_path": {"type": "string"}, "text_content": {"type": "string"}},
                    "outputs": {"saved_status": {"type": "string"}},
                    "primitive_workflow": {
                        "id": "word.document.create",
                        "summary": "Create a Word document",
                        "target": {"kind": {"NativeApp": "MacOs"}, "open": {"App": {"app_name": "Word", "window_title": "Word"}}},
                        "inputs": [],
                        "primitives": [
                            {"kind": "open_app", "app": {"app_name": "Word", "window_title": "Word"}},
                            {"kind": "enter_text", "target": {"label": "active document", "role": "document"}, "value_template": "{{inputs.text_content}}"},
                            {"kind": "save_resource", "path_template": "{{inputs.document_path}}", "policy": "Create"}
                        ],
                        "outputs": [],
                        "assertions": [],
                        "evidence_policy": {"capture_steps": true, "capture_screenshots": true}
                    },
                    "steps": [],
                    "assertions": [],
                    "open_questions": []
                }"#,
            ),
        )
        .expect("primitive Word draft should parse and plan");

        assert_eq!(result.draft.package.id, "word.document.create");
        assert_eq!(result.draft.package.steps[0].value.as_deref(), Some("Word"));
        assert_eq!(
            result.draft.required_adapters,
            vec!["greentic.desktop.macos.ax"]
        );
    }

    #[test]
    #[ignore = "requires network and DEEPSEEK_API_KEY"]
    fn deepseek_live_word_prompt_returns_parseable_runner() {
        let api_key = std::env::var("DEEPSEEK_API_KEY")
            .expect("set DEEPSEEK_API_KEY to run the live DeepSeek planner test");
        let client = OpenAiCompatibleLlmClient::new(
            "deepseek",
            "https://api.deepseek.com",
            "deepseek-chat",
            Some(api_key),
        );
        let result = plan_prompt_with_llm(
            "open the word app. Create a new word document in the place and the name that input parameters provide. Add the text that input parameters provide. Save the document.",
            &PlanningContext {
                available_adapters: vec![AdapterCapabilities::new(
                    "greentic.desktop.macos.ax",
                    "1.0.0",
                    [
                        "macos.activate_app",
                        "macos.find_element",
                        "macos.type_text",
                        "macos.click_element",
                    ],
                )],
                available_mcp_tools: Vec::new(),
                application_metadata: vec!["current desktop platform: macos".to_owned()],
                existing_runners: Vec::new(),
                ltm_examples: Vec::new(),
                security_policies: Vec::new(),
                desktop_observations: vec!["current desktop platform: macos".to_owned()],
            },
            &PlannerOptions::default(),
            &client,
        )
        .expect("DeepSeek response should validate as a runner draft");

        assert!(!result.draft.package.steps.is_empty());
        assert!(result
            .draft
            .package
            .inputs
            .iter()
            .any(|input| input.contains("document") || input.contains("text")));
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
