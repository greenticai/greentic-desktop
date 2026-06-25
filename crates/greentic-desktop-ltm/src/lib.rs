use greentic_desktop_evidence::{EvidenceBundle, EvidenceStatus};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryCaseType {
    RunnerOutcome,
    RunnerFailure,
    PatchRca,
    RunnerImprovement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryOutcome {
    Success,
    Failed,
    Resolved,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryCase {
    pub case_type: MemoryCaseType,
    pub runner_id: String,
    pub runner_version: String,
    pub app_version: Option<String>,
    pub desktop_image_version: Option<String>,
    pub patch_version: Option<String>,
    pub inputs_hash: String,
    pub outputs: BTreeMap<String, String>,
    pub screenshots: Vec<String>,
    pub failure: Option<String>,
    pub human_correction: Option<String>,
    pub root_cause: Option<String>,
    pub fix: Option<String>,
    pub approval_decision: Option<String>,
    pub outcome: MemoryOutcome,
}

impl MemoryCase {
    pub fn from_evidence(bundle: &EvidenceBundle) -> Self {
        Self {
            case_type: if bundle.status == EvidenceStatus::Success {
                MemoryCaseType::RunnerOutcome
            } else {
                MemoryCaseType::RunnerFailure
            },
            runner_id: bundle.runner_id.clone(),
            runner_version: bundle.runner_version.clone(),
            app_version: None,
            desktop_image_version: None,
            patch_version: None,
            inputs_hash: bundle.inputs_hash.clone(),
            outputs: bundle.outputs.clone(),
            screenshots: bundle
                .artifacts
                .iter()
                .filter(|artifact| artifact.kind.as_str().contains("screenshot"))
                .map(|artifact| artifact.uri.clone())
                .collect(),
            failure: bundle
                .tool_trace
                .iter()
                .find(|entry| entry.status == EvidenceStatus::Failed)
                .and_then(|entry| entry.message.clone()),
            human_correction: None,
            root_cause: None,
            fix: None,
            approval_decision: None,
            outcome: if bundle.status == EvidenceStatus::Success {
                MemoryOutcome::Success
            } else {
                MemoryOutcome::Failed
            },
        }
    }

    pub fn with_versions(
        mut self,
        app_version: impl Into<String>,
        desktop_image_version: impl Into<String>,
        patch_version: impl Into<String>,
    ) -> Self {
        self.app_version = Some(app_version.into());
        self.desktop_image_version = Some(desktop_image_version.into());
        self.patch_version = Some(patch_version.into());
        self
    }

    pub fn with_learning(
        mut self,
        correction: impl Into<String>,
        root_cause: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        self.human_correction = Some(correction.into());
        self.root_cause = Some(root_cause.into());
        self.fix = Some(fix.into());
        self.outcome = MemoryOutcome::Resolved;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryLtmStore {
    cases: Vec<MemoryCase>,
}

impl InMemoryLtmStore {
    pub fn insert(&mut self, case: MemoryCase) {
        self.cases.push(case);
    }

    pub fn store_run_outcome(&mut self, bundle: &EvidenceBundle) -> &MemoryCase {
        self.insert(MemoryCase::from_evidence(bundle));
        self.cases.last().expect("inserted case")
    }

    pub fn similar_failures(
        &self,
        runner_id: &str,
        failure: &str,
        limit: usize,
    ) -> Vec<&MemoryCase> {
        let query = tokens(failure);
        let mut matches = self
            .cases
            .iter()
            .filter(|case| {
                case.runner_id == runner_id
                    && matches!(
                        case.case_type,
                        MemoryCaseType::RunnerFailure | MemoryCaseType::PatchRca
                    )
            })
            .map(|case| {
                (
                    case,
                    similarity(&query, &tokens(case.failure.as_deref().unwrap_or(""))),
                )
            })
            .filter(|(_, score)| *score > 0)
            .collect::<Vec<_>>();
        matches.sort_by_key(|(_, score)| Reverse(*score));
        matches
            .into_iter()
            .take(limit)
            .map(|(case, _)| case)
            .collect()
    }

    pub fn planner_context(&self, runner_id: &str) -> Vec<String> {
        self.cases
            .iter()
            .filter(|case| case.runner_id == runner_id)
            .filter_map(|case| {
                case.fix.as_ref().map(|fix| {
                    format!(
                        "Previous {} for {}: {}",
                        case.failure.as_deref().unwrap_or("issue"),
                        case.runner_id,
                        fix
                    )
                })
            })
            .collect()
    }

    pub fn rca_summary(&self, runner_id: &str, failure: &str) -> Option<String> {
        let case = self
            .similar_failures(runner_id, failure, 1)
            .into_iter()
            .next()?;
        Some(format!(
            "{} was previously caused by {}. Fix: {}.",
            failure,
            case.root_cause.as_deref().unwrap_or("an unknown cause"),
            case.fix.as_deref().unwrap_or("no fix recorded")
        ))
    }
}

fn tokens(value: &str) -> BTreeSet<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| part.len() > 2)
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

fn similarity(left: &BTreeSet<String>, right: &BTreeSet<String>) -> usize {
    left.intersection(right).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_evidence::{
        EvidenceArtifact, EvidenceArtifactKind, EvidenceStatus, ToolTraceEntry,
    };

    fn failed_bundle() -> EvidenceBundle {
        EvidenceBundle {
            run_id: "run_123".to_owned(),
            runner_id: "crm.create_customer".to_owned(),
            runner_version: "1.2.0".to_owned(),
            status: EvidenceStatus::Failed,
            inputs_hash: "hash".to_owned(),
            outputs: BTreeMap::new(),
            artifacts: vec![EvidenceArtifact::new(
                EvidenceArtifactKind::Screenshot,
                "failure.png",
                "evidence://run_123/failure.png",
            )],
            tool_trace: vec![ToolTraceEntry {
                step_id: "save".to_owned(),
                capability: "web.click".to_owned(),
                status: EvidenceStatus::Failed,
                message: Some("Save button not found".to_owned()),
            }],
            started_at: "start".to_owned(),
            completed_at: "end".to_owned(),
        }
    }

    #[test]
    fn every_run_outcome_can_be_stored() {
        let mut store = InMemoryLtmStore::default();
        let case = store.store_run_outcome(&failed_bundle());

        assert_eq!(case.runner_id, "crm.create_customer");
        assert_eq!(case.failure, Some("Save button not found".to_owned()));
        assert_eq!(case.screenshots, vec!["evidence://run_123/failure.png"]);
    }

    #[test]
    fn similar_failures_can_be_retrieved() {
        let mut store = InMemoryLtmStore::default();
        store.insert(MemoryCase::from_evidence(&failed_bundle()).with_learning(
            "Use customer form Save",
            "CRM upgrade moved the Save button",
            "Use automation_id SaveCustomerButtonV2",
        ));

        let matches = store.similar_failures("crm.create_customer", "Save button missing", 5);

        assert_eq!(matches.len(), 1);
        assert!(matches[0]
            .fix
            .as_deref()
            .unwrap()
            .contains("SaveCustomerButtonV2"));
    }

    #[test]
    fn prompt_planner_can_use_ltm_context() {
        let mut store = InMemoryLtmStore::default();
        store.insert(MemoryCase::from_evidence(&failed_bundle()).with_learning(
            "Use customer form Save",
            "selector drift",
            "Use customer_form scoped Save button",
        ));

        let context = store.planner_context("crm.create_customer");

        assert_eq!(context.len(), 1);
        assert!(context[0].contains("customer_form"));
    }

    #[test]
    fn rca_summaries_can_be_generated() {
        let mut store = InMemoryLtmStore::default();
        store.insert(MemoryCase::from_evidence(&failed_bundle()).with_learning(
            "Install WebView2",
            "missing WebView2 runtime",
            "Install Microsoft WebView2 runtime",
        ));

        let summary = store
            .rca_summary(
                "crm.create_customer",
                "Save button not found after KB update",
            )
            .expect("summary");

        assert!(summary.contains("missing WebView2 runtime"));
        assert!(summary.contains("Install Microsoft WebView2 runtime"));
    }
}
