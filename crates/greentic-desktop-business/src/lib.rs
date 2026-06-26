use greentic_desktop_mcp::{McpCallRequest, McpServerState, PublishedRunnerTool};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSource {
    RequestField(String),
    StepOutput { step_id: String, output: String },
    Literal(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusinessStep {
    CollectData {
        source: String,
    },
    ValidateInputs {
        required: Vec<String>,
    },
    CallRunner {
        step_id: String,
        runner: String,
        input: BTreeMap<String, ValueSource>,
    },
    SendConfirmationEmail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessFlow {
    pub id: String,
    pub environment: String,
    pub requires_human_approval: bool,
    pub steps: Vec<BusinessStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutcome {
    pub step_id: String,
    pub runner: Option<String>,
    pub resolved_inputs: BTreeMap<String, String>,
    pub outputs: BTreeMap<String, String>,
    pub evidence_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessFlowOutcome {
    pub success: bool,
    pub step_outcomes: Vec<StepOutcome>,
    pub evidence_uris: Vec<String>,
    pub confirmation_sent: bool,
    pub failure_reason: Option<String>,
}

pub fn run_business_flow(
    flow: BusinessFlow,
    tools: Vec<PublishedRunnerTool>,
    request: BTreeMap<String, String>,
    secrets: BTreeMap<String, String>,
    approved_by_human: bool,
) -> BusinessFlowOutcome {
    let permissions = tools
        .iter()
        .map(PublishedRunnerTool::tool_name)
        .collect::<Vec<_>>();
    let mut state = McpServerState::new(tools, permissions);
    let mut step_outcomes = Vec::new();
    let mut evidence_uris = Vec::new();
    let mut confirmation_sent = false;

    for step in flow.steps {
        match step {
            BusinessStep::CollectData { source } => {
                step_outcomes.push(StepOutcome {
                    step_id: "collect_data".to_owned(),
                    runner: None,
                    resolved_inputs: BTreeMap::from([("source".to_owned(), source)]),
                    outputs: request.clone(),
                    evidence_uri: None,
                });
            }
            BusinessStep::ValidateInputs { required } => {
                if let Some(missing) = required.iter().find(|field| !request.contains_key(*field)) {
                    return failed(
                        step_outcomes,
                        evidence_uris,
                        confirmation_sent,
                        format!("missing required input: {missing}"),
                    );
                }
                step_outcomes.push(StepOutcome {
                    step_id: "validate_inputs".to_owned(),
                    runner: None,
                    resolved_inputs: BTreeMap::new(),
                    outputs: BTreeMap::from([("valid".to_owned(), "true".to_owned())]),
                    evidence_uri: None,
                });
            }
            BusinessStep::CallRunner {
                step_id,
                runner,
                input,
            } => {
                if flow.requires_human_approval
                    && flow.environment == "production"
                    && !approved_by_human
                {
                    return failed(
                        step_outcomes,
                        evidence_uris,
                        confirmation_sent,
                        "human approval required before production submission".to_owned(),
                    );
                }
                let resolved_inputs = resolve_inputs(&input, &request, &step_outcomes);
                let result = state.call_tool(McpCallRequest {
                    tool_name: runner.clone(),
                    inputs: resolved_inputs.clone(),
                    secrets: secrets.clone(),
                    approved_by_human,
                    environment: flow.environment.clone(),
                    approvals: u8::from(approved_by_human),
                });
                let evidence_uri =
                    (!result.evidence_uri.is_empty()).then(|| result.evidence_uri.clone());
                if let Some(uri) = &evidence_uri {
                    evidence_uris.push(uri.clone());
                }
                if !result.success {
                    return failed(
                        step_outcomes,
                        evidence_uris,
                        confirmation_sent,
                        result
                            .failure
                            .map(|failure| failure.message)
                            .unwrap_or_else(|| "runner failed".to_owned()),
                    );
                }
                let outputs = parse_flat_json(&result.outputs_json);
                step_outcomes.push(StepOutcome {
                    step_id,
                    runner: Some(runner),
                    resolved_inputs,
                    outputs,
                    evidence_uri,
                });
            }
            BusinessStep::SendConfirmationEmail => {
                confirmation_sent = true;
                step_outcomes.push(StepOutcome {
                    step_id: "send_confirmation_email".to_owned(),
                    runner: None,
                    resolved_inputs: BTreeMap::new(),
                    outputs: BTreeMap::from([("sent".to_owned(), "true".to_owned())]),
                    evidence_uri: None,
                });
            }
        }
    }

    BusinessFlowOutcome {
        success: true,
        step_outcomes,
        evidence_uris,
        confirmation_sent,
        failure_reason: None,
    }
}

pub fn onboard_new_customer_flow() -> BusinessFlow {
    BusinessFlow {
        id: "onboard_new_customer".to_owned(),
        environment: "production".to_owned(),
        requires_human_approval: true,
        steps: vec![
            BusinessStep::CollectData {
                source: "web_assistant".to_owned(),
            },
            BusinessStep::ValidateInputs {
                required: vec!["company_name".to_owned(), "contact_email".to_owned()],
            },
            BusinessStep::CallRunner {
                step_id: "crm".to_owned(),
                runner: "crm.create_customer".to_owned(),
                input: BTreeMap::from([
                    (
                        "email".to_owned(),
                        ValueSource::RequestField("contact_email".to_owned()),
                    ),
                    (
                        "company_name".to_owned(),
                        ValueSource::RequestField("company_name".to_owned()),
                    ),
                ]),
            },
            BusinessStep::CallRunner {
                step_id: "billing".to_owned(),
                runner: "billing.create_account".to_owned(),
                input: BTreeMap::from([(
                    "customer_id".to_owned(),
                    ValueSource::StepOutput {
                        step_id: "crm".to_owned(),
                        output: "customer_id".to_owned(),
                    },
                )]),
            },
            BusinessStep::SendConfirmationEmail,
        ],
    }
}

fn resolve_inputs(
    bindings: &BTreeMap<String, ValueSource>,
    request: &BTreeMap<String, String>,
    outcomes: &[StepOutcome],
) -> BTreeMap<String, String> {
    bindings
        .iter()
        .map(|(key, source)| {
            let value = match source {
                ValueSource::RequestField(field) => request.get(field).cloned().unwrap_or_default(),
                ValueSource::StepOutput { step_id, output } => outcomes
                    .iter()
                    .find(|outcome| &outcome.step_id == step_id)
                    .and_then(|outcome| outcome.outputs.get(output))
                    .cloned()
                    .unwrap_or_default(),
                ValueSource::Literal(value) => value.clone(),
            };
            (key.clone(), value)
        })
        .collect()
}

fn failed(
    step_outcomes: Vec<StepOutcome>,
    evidence_uris: Vec<String>,
    confirmation_sent: bool,
    reason: String,
) -> BusinessFlowOutcome {
    BusinessFlowOutcome {
        success: false,
        step_outcomes,
        evidence_uris,
        confirmation_sent,
        failure_reason: Some(reason),
    }
}

fn parse_flat_json(value: &str) -> BTreeMap<String, String> {
    value
        .trim_matches(|ch| ch == '{' || ch == '}')
        .split(',')
        .filter_map(|part| {
            let (key, value) = part.split_once(':')?;
            Some((
                key.trim_matches('"').to_owned(),
                value.trim_matches('"').to_owned(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_mcp::example_runner_tool;

    fn tools() -> Vec<PublishedRunnerTool> {
        let mut crm = example_runner_tool();
        crm.package.id = "crm.create_customer".to_owned();
        crm.package.inputs = vec!["email".to_owned(), "company_name".to_owned()];
        crm.package.secrets = Vec::new();
        crm.package.steps[0].value = Some("{{email}}".to_owned());
        crm.package.outputs = vec!["customer_id".to_owned()];

        let mut billing = example_runner_tool();
        billing.package.id = "billing.create_account".to_owned();
        billing.package.inputs = vec!["customer_id".to_owned()];
        billing.package.secrets = Vec::new();
        billing.package.steps[0].value = Some("{{customer_id}}".to_owned());
        billing.package.outputs = vec!["billing_account_id".to_owned()];

        vec![crm, billing]
    }

    fn request() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("company_name".to_owned(), "Acme Ltd".to_owned()),
            ("contact_email".to_owned(), "buyer@example.test".to_owned()),
        ])
    }

    fn secrets() -> BTreeMap<String, String> {
        BTreeMap::from([("password".to_owned(), "secret".to_owned())])
    }

    #[test]
    fn runner_outputs_feed_downstream_flow_steps() {
        let outcome = run_business_flow(
            onboard_new_customer_flow(),
            tools(),
            request(),
            secrets(),
            true,
        );

        assert!(outcome.success);
        let billing = outcome
            .step_outcomes
            .iter()
            .find(|step| step.step_id == "billing")
            .expect("billing step");
        assert_eq!(
            billing.resolved_inputs.get("customer_id"),
            Some(&"buyer@example.test".to_owned())
        );
    }

    #[test]
    fn human_approval_can_be_required_before_production_submission() {
        let outcome = run_business_flow(
            onboard_new_customer_flow(),
            tools(),
            request(),
            secrets(),
            false,
        );

        assert!(!outcome.success);
        assert_eq!(
            outcome.failure_reason,
            Some("human approval required before production submission".to_owned())
        );
    }

    #[test]
    fn evidence_is_stored_for_compliance() {
        let outcome = run_business_flow(
            onboard_new_customer_flow(),
            tools(),
            request(),
            secrets(),
            true,
        );

        assert_eq!(outcome.evidence_uris.len(), 2);
        assert!(outcome
            .evidence_uris
            .iter()
            .any(|uri| uri.contains("run_crm.create_customer")));
        assert!(outcome.confirmation_sent);
    }

    #[test]
    fn validation_blocks_missing_required_inputs() {
        let outcome = run_business_flow(
            onboard_new_customer_flow(),
            tools(),
            BTreeMap::from([("company_name".to_owned(), "Acme Ltd".to_owned())]),
            secrets(),
            true,
        );

        assert!(!outcome.success);
        assert_eq!(
            outcome.failure_reason,
            Some("missing required input: contact_email".to_owned())
        );
    }
}
