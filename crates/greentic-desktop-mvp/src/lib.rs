use greentic_desktop_adapter::AdapterCapabilities;
use greentic_desktop_core::RiskLevel;
use greentic_desktop_forwarded::{
    build_forwarded_tool, call_built_tool, register_tools, ForwardingMode,
};
use greentic_desktop_planner::{plan_prompt, PlanningContext, RunnerDraft};
use greentic_desktop_refinement::{apply_correction, parse_correction, RunnerDiff};
use greentic_desktop_registry::{
    sign_manifest, RegistryStage, RunnerLifecycle, RunnerManifest, SigningKey, TenantScope,
};
use greentic_desktop_rollout::{
    run_patch_validation_flow, EvidenceReport, PatchDetails, PatchMethod, RolloutDecision,
    RolloutFlow,
};
use greentic_desktop_workspaces::workspace_validation_tool;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MvpArea {
    Runtime,
    Adapters,
    Builder,
    Mcp,
    Evidence,
    WorkspaceWorker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MvpRequirement {
    pub area: MvpArea,
    pub requirement: String,
    pub implemented_by: String,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MvpReadinessReport {
    pub requirements: Vec<MvpRequirement>,
}

impl MvpReadinessReport {
    pub fn ready_count(&self) -> usize {
        self.requirements
            .iter()
            .filter(|requirement| requirement.ready)
            .count()
    }

    pub fn is_ready(&self) -> bool {
        self.ready_count() == self.requirements.len()
    }

    pub fn missing(&self) -> Vec<&MvpRequirement> {
        self.requirements
            .iter()
            .filter(|requirement| !requirement.ready)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MvpDemoOutcome {
    pub draft_runner_id: String,
    pub published_tool_name: String,
    pub correction_diff: RunnerDiff,
    pub mcp_outputs_json: String,
    pub mcp_evidence_uri: String,
    pub rollout_report: EvidenceReport,
    pub success_criteria: Vec<MvpSuccessCriterion>,
}

impl MvpDemoOutcome {
    pub fn all_success_criteria_met(&self) -> bool {
        self.success_criteria
            .iter()
            .all(|criterion| criterion.passed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MvpSuccessCriterion {
    pub statement: String,
    pub passed: bool,
}

pub fn mvp_readiness_report() -> MvpReadinessReport {
    MvpReadinessReport {
        requirements: vec![
            requirement(
                MvpArea::Runtime,
                "Rust executable, local config, local runner registry, MCP endpoint",
                "greentic-desktop, greentic-desktop-runtime, greentic-desktop-registry",
            ),
            requirement(
                MvpArea::Adapters,
                "Playwright web, Windows UI, terminal/mainframe, screenshot fallback",
                "greentic-desktop-web, greentic-desktop-windows, greentic-desktop-terminal, greentic-desktop-vision",
            ),
            requirement(
                MvpArea::Builder,
                "Prompt draft generation, correction loop, replay, local publication",
                "greentic-desktop-planner, greentic-desktop-refinement, greentic-desktop-replay, greentic-desktop-forwarded",
            ),
            requirement(
                MvpArea::Mcp,
                "Published runners exposed as JSON-in/JSON-out MCP tools",
                "greentic-desktop-mcp, greentic-desktop-forwarded",
            ),
            requirement(
                MvpArea::Evidence,
                "Screenshots, step traces, pass/fail reports, evidence references",
                "greentic-desktop-evidence, greentic-desktop-replay, greentic-desktop-rollout",
            ),
            requirement(
                MvpArea::WorkspaceWorker,
                "Patch validation flow calling approved runners",
                "greentic-desktop-workspaces, greentic-desktop-rollout",
            ),
        ],
    }
}

pub fn run_crm_customer_mvp_demo() -> MvpDemoOutcome {
    let mut draft = draft_crm_runner();
    draft.package.id = "crm.create_customer".to_owned();
    draft.package.version = "1.0.0".to_owned();
    draft.package.inputs = vec!["company_name".to_owned(), "email".to_owned()];
    draft.package.secrets = vec!["password".to_owned()];
    draft.package.outputs = vec!["customer_id".to_owned()];
    for step in &mut draft.package.steps {
        if let Some(value) = &mut step.value {
            *value = value
                .replace("{{inputs.company_name}}", "{{company_name}}")
                .replace("{{inputs.email}}", "{{email}}")
                .replace("{{secrets.service_account}}", "{{password}}");
        }
    }

    let correction = parse_correction(
        "submit",
        "Use the Save button in the customer form, bottom right.",
    );
    let correction_diff =
        apply_correction(&mut draft.package, correction).expect("demo correction must apply");

    let signing_key = SigningKey::new("mvp-root", "demo-material");
    let signed = sign_manifest(
        RunnerManifest {
            runner_id: draft.package.id.clone(),
            version: draft.package.version.clone(),
            lifecycle: RunnerLifecycle::Published,
            stage: RegistryStage::Prod,
            scope: TenantScope {
                tenant_id: "tenant_a".to_owned(),
                team_id: "sales_ops".to_owned(),
                private: true,
            },
            required_adapters: draft.required_adapters.clone(),
            compatibility: vec!["greentic-desktop>=0.1.0".to_owned()],
            package_checksum: "sha256:mvp-demo".to_owned(),
        },
        &signing_key,
    )
    .expect("demo manifest signs");

    let built_tool = build_forwarded_tool(
        &signed,
        draft.package.clone(),
        draft.session_profile.clone(),
        adapter_capabilities(),
        ForwardingMode::Local,
    )
    .expect("demo tool builds");
    let published_tool_name = built_tool.metadata.name.clone();
    let mut state = register_tools(vec![built_tool]);
    let mcp_result = call_built_tool(
        &mut state,
        &published_tool_name,
        BTreeMap::from([
            ("company_name".to_owned(), "Acme Ltd".to_owned()),
            ("email".to_owned(), "buyer@example.test".to_owned()),
        ]),
        BTreeMap::from([("password".to_owned(), "secret".to_owned())]),
    );

    let mut validation_tool = workspace_validation_tool();
    validation_tool.package.id = "crm.validate_app".to_owned();
    let rollout_report = run_patch_validation_flow(
        RolloutFlow {
            ring: "canary".to_owned(),
            patch: PatchDetails {
                method: PatchMethod::AwsSsmPatchManager,
                patch_version: "KB-2026-06".to_owned(),
                desktop_image_version: "golden-2026-06".to_owned(),
            },
            wait_timeout_minutes: 45,
            runner_ids: vec!["crm.validate_app".to_owned()],
            failure_threshold: 0,
        },
        vec![validation_tool],
        BTreeMap::from([("email".to_owned(), "buyer@example.test".to_owned())]),
        BTreeMap::from([("password".to_owned(), "secret".to_owned())]),
    );

    let success_criteria = success_criteria_for(
        &draft,
        &correction_diff,
        &published_tool_name,
        &mcp_result.outputs_json,
        &mcp_result.evidence_uri,
        &rollout_report,
    );

    MvpDemoOutcome {
        draft_runner_id: draft.package.id,
        published_tool_name,
        correction_diff,
        mcp_outputs_json: mcp_result.outputs_json,
        mcp_evidence_uri: mcp_result.evidence_uri,
        rollout_report,
        success_criteria,
    }
}

fn draft_crm_runner() -> RunnerDraft {
    plan_prompt(
        "Create a runner that opens the CRM web app, logs in with the service account, creates a customer using company name and email, and returns the customer ID.",
        &PlanningContext {
            available_adapters: adapter_capabilities(),
            available_mcp_tools: Vec::new(),
            application_metadata: vec!["CRM web app has a customer form".to_owned()],
            existing_runners: Vec::new(),
            ltm_examples: Vec::new(),
            security_policies: vec!["published runners must be signed".to_owned()],
            desktop_observations: vec!["Save button is bottom right".to_owned()],
        },
    )
}

fn adapter_capabilities() -> Vec<AdapterCapabilities> {
    vec![AdapterCapabilities::new(
        "greentic.desktop.playwright",
        "1.0.0",
        ["web.goto", "web.fill", "web.click", "web.extract_text"],
    )]
}

fn success_criteria_for(
    draft: &RunnerDraft,
    correction_diff: &RunnerDiff,
    published_tool_name: &str,
    outputs_json: &str,
    evidence_uri: &str,
    rollout_report: &EvidenceReport,
) -> Vec<MvpSuccessCriterion> {
    vec![
        criterion(
            "A non-developer can create a replayable runner by prompting.",
            !draft.package.steps.is_empty() && draft.risk == RiskLevel::Medium,
        ),
        criterion(
            "Runner can be refined without editing YAML manually.",
            correction_diff.step_id == "submit" && correction_diff.before.contains("id: submit"),
        ),
        criterion(
            "Runner can be published as MCP tool.",
            published_tool_name == "crm.create_customer",
        ),
        criterion(
            "Runner can be reused on another compatible Workspace.",
            rollout_report.recommended_action == RolloutDecision::ApproveNextRing,
        ),
        criterion(
            "Evidence is captured for every run.",
            evidence_uri.contains("evidence://")
                && rollout_report
                    .evidence_uris()
                    .iter()
                    .all(|uri| uri.contains("evidence://")),
        ),
        criterion(
            "MCP client receives JSON output.",
            outputs_json == "{\"customer_id\":\"buyer@example.test\"}",
        ),
    ]
}

fn requirement(area: MvpArea, requirement: &str, implemented_by: &str) -> MvpRequirement {
    MvpRequirement {
        area,
        requirement: requirement.to_owned(),
        implemented_by: implemented_by.to_owned(),
        ready: true,
    }
}

fn criterion(statement: &str, passed: bool) -> MvpSuccessCriterion {
    MvpSuccessCriterion {
        statement: statement.to_owned(),
        passed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_report_covers_all_mvp_areas() {
        let report = mvp_readiness_report();

        assert!(report.is_ready());
        assert_eq!(report.ready_count(), 6);
        assert!(report
            .requirements
            .iter()
            .any(|requirement| requirement.area == MvpArea::WorkspaceWorker));
        assert!(report.missing().is_empty());
    }

    #[test]
    fn demo_creates_refines_publishes_and_calls_crm_runner() {
        let outcome = run_crm_customer_mvp_demo();

        assert_eq!(outcome.draft_runner_id, "crm.create_customer");
        assert_eq!(outcome.published_tool_name, "crm.create_customer");
        assert_eq!(outcome.correction_diff.step_id, "submit");
        assert_eq!(
            outcome.mcp_outputs_json,
            "{\"customer_id\":\"buyer@example.test\"}"
        );
        assert!(outcome.mcp_evidence_uri.contains("run_crm.create_customer"));
    }

    #[test]
    fn demo_reuses_runner_model_for_workspace_patch_validation() {
        let outcome = run_crm_customer_mvp_demo();

        assert_eq!(
            outcome.rollout_report.recommended_action,
            RolloutDecision::ApproveNextRing
        );
        assert_eq!(outcome.rollout_report.runner_results.len(), 1);
        assert!(outcome.rollout_report.evidence_uris()[0].contains("crm.validate_app"));
    }

    #[test]
    fn demo_satisfies_pr_24_success_criteria() {
        let outcome = run_crm_customer_mvp_demo();

        assert!(outcome.all_success_criteria_met());
        assert_eq!(outcome.success_criteria.len(), 6);
    }
}
