use greentic_desktop_mcp::{McpCallResult, McpServerState, PublishedRunnerTool};
use greentic_desktop_workspaces::{call_forwarded_runner, expose_workspace_tools};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchMethod {
    AwsSsmPatchManager,
    Manual(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchDetails {
    pub method: PatchMethod,
    pub patch_version: String,
    pub desktop_image_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutFlow {
    pub ring: String,
    pub patch: PatchDetails,
    pub wait_timeout_minutes: u16,
    pub runner_ids: Vec<String>,
    pub failure_threshold: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerValidationResult {
    pub runner_id: String,
    pub passed: bool,
    pub outputs_json: String,
    pub evidence_uri: String,
    pub failed_assertion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RolloutDecision {
    ApproveNextRing,
    PauseRollout,
    RollbackCanary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceReport {
    pub ring: String,
    pub patch_version: String,
    pub desktop_image_version: String,
    pub runner_results: Vec<RunnerValidationResult>,
    pub recommended_action: RolloutDecision,
}

impl EvidenceReport {
    pub fn failure_count(&self) -> usize {
        self.runner_results
            .iter()
            .filter(|result| !result.passed)
            .count()
    }

    pub fn evidence_uris(&self) -> Vec<String> {
        self.runner_results
            .iter()
            .map(|result| result.evidence_uri.clone())
            .collect()
    }
}

pub fn run_patch_validation_flow(
    flow: RolloutFlow,
    tools: Vec<PublishedRunnerTool>,
    inputs: BTreeMap<String, String>,
    secrets: BTreeMap<String, String>,
) -> EvidenceReport {
    let mut state = expose_workspace_tools(tools);
    run_with_state(flow, &mut state, inputs, secrets)
}

pub fn run_with_state(
    flow: RolloutFlow,
    state: &mut McpServerState,
    inputs: BTreeMap<String, String>,
    secrets: BTreeMap<String, String>,
) -> EvidenceReport {
    let mut runner_results = Vec::new();
    for runner_id in &flow.runner_ids {
        let result = call_forwarded_runner(state, runner_id, inputs.clone(), secrets.clone());
        runner_results.push(validation_result(runner_id, result));
    }

    let failures = runner_results
        .iter()
        .filter(|result| !result.passed)
        .count();
    let recommended_action = if failures > flow.failure_threshold {
        RolloutDecision::RollbackCanary
    } else {
        RolloutDecision::ApproveNextRing
    };

    EvidenceReport {
        ring: flow.ring,
        patch_version: flow.patch.patch_version,
        desktop_image_version: flow.patch.desktop_image_version,
        runner_results,
        recommended_action,
    }
}

pub fn failed_rollout_actions(report: &EvidenceReport) -> Vec<&'static str> {
    if report.failure_count() == 0 {
        vec!["approve_next_ring"]
    } else {
        vec![
            "pause_rollout",
            "create_ticket",
            "notify_admin",
            "rollback_canary",
        ]
    }
}

fn validation_result(runner_id: &str, result: McpCallResult) -> RunnerValidationResult {
    RunnerValidationResult {
        runner_id: runner_id.to_owned(),
        passed: result.success,
        outputs_json: result.outputs_json,
        evidence_uri: result.evidence_uri,
        failed_assertion: result.failure.map(|failure| failure.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_workspaces::workspace_validation_tool;

    fn flow(runner_ids: Vec<&str>, failure_threshold: usize) -> RolloutFlow {
        RolloutFlow {
            ring: "canary".to_owned(),
            patch: PatchDetails {
                method: PatchMethod::AwsSsmPatchManager,
                patch_version: "KB-2026-06".to_owned(),
                desktop_image_version: "golden-2026-06".to_owned(),
            },
            wait_timeout_minutes: 45,
            runner_ids: runner_ids.into_iter().map(str::to_owned).collect(),
            failure_threshold,
        }
    }

    fn inputs() -> BTreeMap<String, String> {
        BTreeMap::from([("email".to_owned(), "user@example.test".to_owned())])
    }

    fn secrets() -> BTreeMap<String, String> {
        BTreeMap::from([("password".to_owned(), "secret".to_owned())])
    }

    #[test]
    fn patch_validation_can_call_multiple_runners() {
        let mut first = workspace_validation_tool();
        first.package.id = "crm.validate_app".to_owned();
        let mut second = workspace_validation_tool();
        second.package.id = "finance.validate_invoice_app".to_owned();

        let report = run_patch_validation_flow(
            flow(vec!["crm.validate_app", "finance.validate_invoice_app"], 0),
            vec![first, second],
            inputs(),
            secrets(),
        );

        assert_eq!(report.runner_results.len(), 2);
        assert_eq!(report.failure_count(), 2);
        assert_eq!(report.recommended_action, RolloutDecision::RollbackCanary);
    }

    #[test]
    fn failed_runner_blocks_rollout() {
        let mut tool = workspace_validation_tool();
        tool.package.id = "crm.validate_app".to_owned();
        tool.adapters.clear();

        let report = run_patch_validation_flow(
            flow(vec!["crm.validate_app"], 0),
            vec![tool],
            inputs(),
            secrets(),
        );

        assert_eq!(report.failure_count(), 1);
        assert_eq!(report.recommended_action, RolloutDecision::RollbackCanary);
    }

    #[test]
    fn evidence_is_attached_to_rollout_decision() {
        let mut tool = workspace_validation_tool();
        tool.package.id = "crm.validate_app".to_owned();

        let report = run_patch_validation_flow(
            flow(vec!["crm.validate_app"], 0),
            vec![tool],
            inputs(),
            secrets(),
        );

        assert_eq!(report.patch_version, "KB-2026-06");
        assert!(report.evidence_uris()[0].contains("run_crm.validate_app"));
    }

    #[test]
    fn rollback_actions_are_recommended_for_failures() {
        let mut tool = workspace_validation_tool();
        tool.package.id = "mainframe.lookup_customer".to_owned();
        tool.adapters.clear();
        let report = run_patch_validation_flow(
            flow(vec!["mainframe.lookup_customer"], 0),
            vec![tool],
            inputs(),
            secrets(),
        );

        assert_eq!(
            failed_rollout_actions(&report),
            vec![
                "pause_rollout",
                "create_ticket",
                "notify_admin",
                "rollback_canary"
            ]
        );
    }
}
