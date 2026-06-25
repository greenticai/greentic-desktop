use greentic_desktop_core::RiskLevel;
use greentic_desktop_runner_schema::RunnerDraftDocument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerPolicy {
    pub allow_high_risk_drafts: bool,
    pub allow_critical_drafts: bool,
    pub require_inputs_for_destructive_actions: bool,
}

impl Default for PlannerPolicy {
    fn default() -> Self {
        Self {
            allow_high_risk_drafts: true,
            allow_critical_drafts: false,
            require_inputs_for_destructive_actions: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDiagnostic {
    pub code: String,
    pub message: String,
}

pub fn validate_planned_runner(
    draft: &RunnerDraftDocument,
    policy: &PlannerPolicy,
) -> Result<(), PolicyDiagnostic> {
    if draft.risk_level == RiskLevel::Critical && !policy.allow_critical_drafts {
        return Err(diagnostic(
            "planner.policy_denied",
            "critical desktop actions require explicit approval before a draft can be saved",
        ));
    }
    if draft.risk_level == RiskLevel::High && !policy.allow_high_risk_drafts {
        return Err(diagnostic(
            "planner.policy_denied",
            "high-risk desktop actions are blocked by planner policy",
        ));
    }
    if policy.require_inputs_for_destructive_actions
        && destructive(draft)
        && draft.inputs.is_empty()
    {
        return Err(diagnostic(
            "planner.missing_required_input",
            "destructive or submitting runners must declare required inputs",
        ));
    }
    Ok(())
}

fn destructive(draft: &RunnerDraftDocument) -> bool {
    draft.steps.iter().any(|step| {
        let text = format!("{} {}", step.action, step.required_capability).to_ascii_lowercase();
        text.contains("delete")
            || text.contains("payment")
            || text.contains("submit")
            || text.contains("click")
    })
}

fn diagnostic(code: &str, message: &str) -> PolicyDiagnostic {
    PolicyDiagnostic {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_runner_schema::parse_runner_draft_json;

    #[test]
    fn denies_critical_planner_drafts_by_default() {
        let draft = parse_runner_draft_json(
            r#"{
                "runner_id":"billing.pay",
                "version":"0.1.0-draft",
                "summary":"pay invoice",
                "risk_level":"critical",
                "required_capabilities":["web.click"],
                "inputs":{"invoice_id":{"type":"string","required":true}},
                "outputs":{},
                "steps":[{"id":"pay","action":"payment","required_capability":"web.click"}],
                "assertions":[],
                "open_questions":[]
            }"#,
        )
        .expect("schema-valid draft");

        let err = validate_planned_runner(&draft, &PlannerPolicy::default()).expect_err("denied");
        assert_eq!(err.code, "planner.policy_denied");
    }
}
