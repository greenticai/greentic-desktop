use greentic_desktop_adapter::{AdapterCapabilities, LocatorTarget, RunnerStep};
use greentic_desktop_core::RiskLevel;
use greentic_desktop_recorder::{RecordingMode, RunnerPackage};
use greentic_desktop_replay::{replay, ReplayRequest};
use greentic_desktop_security::{
    enforce_policy, ActionRequest, PolicyContext, PolicyDecision, SecurityPolicy,
};
use greentic_desktop_session::SessionProfile;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema_ref: String,
    pub output_schema_ref: String,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedRunnerTool {
    pub package: RunnerPackage,
    pub session_profile: SessionProfile,
    pub adapters: Vec<AdapterCapabilities>,
    pub risk: RiskLevel,
    pub allowed: bool,
    pub requires_human_approval: bool,
    pub rate_limit_per_minute: u16,
    pub security_policy: SecurityPolicy,
    pub actions: ActionRequest,
}

impl PublishedRunnerTool {
    pub fn tool_name(&self) -> String {
        stable_tool_name(&self.package.id)
    }

    pub fn forwarded_tool_name(&self) -> String {
        format!("forwarded___{}", self.tool_name().replace('.', "_"))
    }

    pub fn descriptor(&self) -> McpTool {
        McpTool {
            name: self.tool_name(),
            description: format!("Run desktop runner {}", self.package.id),
            input_schema_ref: "inputs.schema.json".to_owned(),
            output_schema_ref: "outputs.schema.json".to_owned(),
            risk: self.risk,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCallRequest {
    pub tool_name: String,
    pub inputs: BTreeMap<String, String>,
    pub secrets: BTreeMap<String, String>,
    pub approved_by_human: bool,
    pub environment: String,
    pub approvals: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCallResult {
    pub success: bool,
    pub outputs_json: String,
    pub failure: Option<StructuredFailure>,
    pub evidence_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredFailure {
    pub code: String,
    pub message: String,
    pub evidence_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerState {
    tools: Vec<PublishedRunnerTool>,
    permissions: BTreeSet<String>,
    call_counts: BTreeMap<String, u16>,
}

impl McpServerState {
    pub fn new(
        tools: Vec<PublishedRunnerTool>,
        permissions: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            tools,
            permissions: permissions.into_iter().collect(),
            call_counts: BTreeMap::new(),
        }
    }

    pub fn list_tools(&self) -> Vec<McpTool> {
        let mut tools = self
            .tools
            .iter()
            .filter(|tool| tool.allowed && self.permissions.contains(&tool.tool_name()))
            .map(PublishedRunnerTool::descriptor)
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    pub fn call_tool(&mut self, request: McpCallRequest) -> McpCallResult {
        let Some(tool) = self
            .tools
            .iter()
            .find(|tool| tool.tool_name() == request.tool_name)
            .cloned()
        else {
            return failure("not_found", "tool is not published", None);
        };

        if !tool.allowed || !self.permissions.contains(&request.tool_name) {
            return failure("permission_denied", "tool is not allowed", None);
        }
        if tool.requires_human_approval && !request.approved_by_human {
            return failure(
                "human_approval_required",
                "tool requires human approval",
                None,
            );
        }
        let policy_decision = enforce_policy(
            &tool.security_policy,
            &PolicyContext {
                environment: request.environment.clone(),
                approvals: request.approvals + u8::from(request.approved_by_human),
                actions: tool.actions.clone(),
                signed_published_runner: true,
            },
        );
        if let PolicyDecision::Denied { code, message } = policy_decision {
            return failure(&code, &message, None);
        }
        if !inputs_cover_schema(&tool.package, &request.inputs, &request.secrets) {
            return failure("invalid_input", "required input or secret is missing", None);
        }

        let count = self
            .call_counts
            .entry(request.tool_name.clone())
            .or_default();
        *count += 1;
        if *count > tool.rate_limit_per_minute {
            return failure("rate_limited", "tool rate limit exceeded", None);
        }

        let outcome = replay(ReplayRequest {
            package: tool.package,
            session_profile: tool.session_profile,
            inputs: request.inputs,
            secrets: request.secrets,
            adapters: tool.adapters,
        });
        let evidence_uri = outcome.evidence_ref.uri.clone();
        if outcome.passed {
            McpCallResult {
                success: true,
                outputs_json: outcome.outputs_json(),
                failure: None,
                evidence_uri,
            }
        } else {
            failure(
                "runner_failed",
                outcome.failure_reason.as_deref().unwrap_or("runner failed"),
                Some(evidence_uri),
            )
        }
    }

    pub fn render_tools_list_json(&self) -> String {
        let tools = self
            .list_tools()
            .iter()
            .map(|tool| {
                format!(
                    "{{\"name\":\"{}\",\"description\":\"{}\",\"input_schema\":\"{}\",\"output_schema\":\"{}\"}}",
                    escape_json(&tool.name),
                    escape_json(&tool.description),
                    escape_json(&tool.input_schema_ref),
                    escape_json(&tool.output_schema_ref)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"tools\":[{tools}]}}")
    }
}

pub fn stable_tool_name(runner_id: &str) -> String {
    runner_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn example_runner_tool() -> PublishedRunnerTool {
    PublishedRunnerTool {
        package: RunnerPackage {
            id: "crm.create_customer".to_owned(),
            version: "1.2.0".to_owned(),
            mode: RecordingMode::Hybrid,
            inputs: vec!["email".to_owned()],
            secrets: vec!["password".to_owned()],
            steps: vec![RunnerStep {
                id: "fill_email".to_owned(),
                action: "fill".to_owned(),
                target: LocatorTarget::default(),
                value: Some("{{email}}".to_owned()),
                required_capability: "web.fill".to_owned(),
            }],
            assertions: vec!["success visible".to_owned()],
            outputs: vec!["customer_id".to_owned()],
        },
        session_profile: SessionProfile {
            id: "empty".to_owned(),
            bootstrap: Vec::new(),
            teardown: Vec::new(),
        },
        adapters: vec![AdapterCapabilities::new(
            "greentic.desktop.playwright",
            "1.0.0",
            ["web.fill"],
        )],
        risk: RiskLevel::Medium,
        allowed: true,
        requires_human_approval: false,
        rate_limit_per_minute: 10,
        security_policy: SecurityPolicy::medium_default(),
        actions: ActionRequest {
            read_screen: true,
            type_text: true,
            submit_forms: true,
            ..ActionRequest::default()
        },
    }
}

fn inputs_cover_schema(
    package: &RunnerPackage,
    inputs: &BTreeMap<String, String>,
    secrets: &BTreeMap<String, String>,
) -> bool {
    package.inputs.iter().all(|key| inputs.contains_key(key))
        && package.secrets.iter().all(|key| secrets.contains_key(key))
}

fn failure(code: &str, message: &str, evidence_uri: Option<String>) -> McpCallResult {
    McpCallResult {
        success: false,
        outputs_json: "{}".to_owned(),
        failure: Some(StructuredFailure {
            code: code.to_owned(),
            message: message.to_owned(),
            evidence_uri: evidence_uri.clone(),
        }),
        evidence_uri: evidence_uri.unwrap_or_default(),
    }
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> McpServerState {
        McpServerState::new(
            vec![example_runner_tool()],
            ["crm.create_customer".to_owned()],
        )
    }

    #[test]
    fn tools_list_returns_published_allowed_runners() {
        let tools = state().list_tools();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "crm.create_customer");
        assert_eq!(tools[0].input_schema_ref, "inputs.schema.json");
    }

    #[test]
    fn tools_call_executes_runner_and_returns_outputs_with_evidence() {
        let mut state = state();
        let result = state.call_tool(McpCallRequest {
            tool_name: "crm.create_customer".to_owned(),
            inputs: BTreeMap::from([("email".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::from([("password".to_owned(), "secret".to_owned())]),
            approved_by_human: false,
            environment: "dev".to_owned(),
            approvals: 0,
        });

        assert!(result.success);
        assert_eq!(result.outputs_json, "{\"customer_id\":\"resolved\"}");
        assert_eq!(
            result.evidence_uri,
            "evidence://run_crm.create_customer/bundle.json"
        );
    }

    #[test]
    fn failed_calls_return_structured_failure_and_evidence_reference() {
        let mut tool = example_runner_tool();
        tool.adapters.clear();
        let mut state = McpServerState::new(vec![tool], ["crm.create_customer".to_owned()]);

        let result = state.call_tool(McpCallRequest {
            tool_name: "crm.create_customer".to_owned(),
            inputs: BTreeMap::from([("email".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::from([("password".to_owned(), "secret".to_owned())]),
            approved_by_human: false,
            environment: "dev".to_owned(),
            approvals: 0,
        });

        assert!(!result.success);
        assert_eq!(result.failure.expect("failure").code, "runner_failed");
        assert!(result.evidence_uri.contains("run_crm.create_customer"));
    }

    #[test]
    fn forwarded_tool_names_are_stable() {
        let tool = example_runner_tool();

        assert_eq!(
            tool.forwarded_tool_name(),
            "forwarded___crm_create_customer"
        );
        assert_eq!(
            stable_tool_name("workspace validate-after patch"),
            "workspace_validate_after_patch"
        );
    }

    #[test]
    fn risk_policy_is_enforced_at_call_time() {
        let mut tool = example_runner_tool();
        tool.risk = RiskLevel::High;
        tool.security_policy.risk_level = RiskLevel::High;
        let mut state = McpServerState::new(vec![tool], ["crm.create_customer".to_owned()]);

        let result = state.call_tool(McpCallRequest {
            tool_name: "crm.create_customer".to_owned(),
            inputs: BTreeMap::from([("email".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::from([("password".to_owned(), "secret".to_owned())]),
            approved_by_human: false,
            environment: "dev".to_owned(),
            approvals: 0,
        });

        assert!(!result.success);
        assert_eq!(result.failure.expect("failure").code, "approval_required");
    }

    #[test]
    fn dangerous_actions_are_blocked_at_call_time() {
        let mut tool = example_runner_tool();
        tool.actions.delete_records = true;
        let mut state = McpServerState::new(vec![tool], ["crm.create_customer".to_owned()]);

        let result = state.call_tool(McpCallRequest {
            tool_name: "crm.create_customer".to_owned(),
            inputs: BTreeMap::from([("email".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::from([("password".to_owned(), "secret".to_owned())]),
            approved_by_human: true,
            environment: "dev".to_owned(),
            approvals: 1,
        });

        assert!(!result.success);
        assert_eq!(result.failure.expect("failure").code, "permission_denied");
    }

    #[test]
    fn tools_list_can_render_json_response() {
        let json = state().render_tools_list_json();

        assert!(json.contains("\"tools\""));
        assert!(json.contains("\"name\":\"crm.create_customer\""));
    }
}
