use greentic_desktop_adapter::AdapterCapabilities;
use greentic_desktop_core::RiskLevel;
use greentic_desktop_mcp::{
    McpCallRequest, McpCallResult, McpServerState, McpTool, PublishedRunnerTool,
};
use greentic_desktop_recorder::RunnerPackage;
use greentic_desktop_registry::{RegistryError, RunnerLifecycle, SignedRunnerManifest};
use greentic_desktop_security::{ActionRequest, SecurityPolicy};
use greentic_desktop_session::SessionProfile;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardingMode {
    Local,
    AwsForwarded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSchema {
    pub required: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltTool {
    pub metadata: ToolMetadata,
    pub tool: PublishedRunnerTool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub input_schema: ToolSchema,
    pub output_schema: ToolSchema,
    pub risk: RiskLevel,
    pub evidence_policy: String,
    pub required_permissions: Vec<String>,
    pub version: String,
    pub forwarded_name: Option<String>,
}

impl BuiltTool {
    pub fn descriptor(&self) -> McpTool {
        self.tool.descriptor()
    }
}

pub fn build_forwarded_tool(
    signed: &SignedRunnerManifest,
    package: RunnerPackage,
    session_profile: SessionProfile,
    adapters: Vec<AdapterCapabilities>,
    mode: ForwardingMode,
) -> Result<BuiltTool, RegistryError> {
    if signed.manifest.lifecycle != RunnerLifecycle::Published || signed.signature.trim().is_empty()
    {
        return Err(RegistryError::PublishedRunnerMustBeSigned);
    }
    let mut policy = SecurityPolicy::medium_default();
    policy.risk_level = infer_risk(&package);
    let actions = actions_for_package(&package);
    let tool = PublishedRunnerTool {
        package: package.clone(),
        session_profile,
        adapters,
        risk: policy.risk_level,
        allowed: true,
        requires_human_approval: policy.risk_level >= RiskLevel::High,
        rate_limit_per_minute: 60,
        security_policy: policy.clone(),
        actions: actions.clone(),
    };
    let forwarded_name =
        matches!(mode, ForwardingMode::AwsForwarded).then(|| tool.forwarded_tool_name());

    Ok(BuiltTool {
        metadata: ToolMetadata {
            name: tool.tool_name(),
            description: format!("Run desktop runner {}", signed.manifest.package_ref()),
            input_schema: ToolSchema {
                required: package.inputs.clone(),
            },
            output_schema: ToolSchema {
                required: package.outputs.clone(),
            },
            risk: tool.risk,
            evidence_policy: "screenshots=true,redact_secrets=true".to_owned(),
            required_permissions: permissions_for_actions(&actions),
            version: signed.manifest.version.clone(),
            forwarded_name,
        },
        tool,
    })
}

pub fn register_tools(tools: Vec<BuiltTool>) -> McpServerState {
    let published = tools.into_iter().map(|tool| tool.tool).collect::<Vec<_>>();
    let permissions = published
        .iter()
        .map(PublishedRunnerTool::tool_name)
        .collect::<Vec<_>>();
    McpServerState::new(published, permissions)
}

pub fn call_built_tool(
    state: &mut McpServerState,
    tool_name: &str,
    inputs: BTreeMap<String, String>,
    secrets: BTreeMap<String, String>,
) -> McpCallResult {
    state.call_tool(McpCallRequest {
        tool_name: tool_name.to_owned(),
        inputs,
        secrets,
        approved_by_human: true,
        environment: "staging".to_owned(),
        approvals: 1,
    })
}

fn infer_risk(package: &RunnerPackage) -> RiskLevel {
    let action_text = package
        .steps
        .iter()
        .map(|step| step.action.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if action_text.contains("delete") || action_text.contains("payment") {
        RiskLevel::Critical
    } else if action_text.contains("click") || action_text.contains("fill") {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

fn actions_for_package(package: &RunnerPackage) -> ActionRequest {
    let mut actions = ActionRequest::default();
    for step in &package.steps {
        if step.required_capability.contains("read")
            || step.required_capability.contains("screenshot")
            || step.required_capability.contains("extract")
        {
            actions.read_screen = true;
        }
        if step.required_capability.contains("fill") || step.required_capability.contains("type") {
            actions.type_text = true;
        }
        if step.action.contains("submit") || step.action.contains("click") {
            actions.submit_forms = true;
        }
        if step.action.contains("delete") {
            actions.delete_records = true;
        }
        if step.action.contains("payment") {
            actions.payments = true;
        }
    }
    actions
}

fn permissions_for_actions(actions: &ActionRequest) -> Vec<String> {
    let mut permissions = Vec::new();
    if actions.read_screen {
        permissions.push("read_screen".to_owned());
    }
    if actions.type_text {
        permissions.push("type_text".to_owned());
    }
    if actions.submit_forms {
        permissions.push("submit_forms".to_owned());
    }
    if actions.delete_records {
        permissions.push("delete_records".to_owned());
    }
    if actions.payments {
        permissions.push("payments".to_owned());
    }
    permissions
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::{LocatorTarget, RunnerStep};
    use greentic_desktop_mcp::example_runner_tool;
    use greentic_desktop_recorder::{RecordingMode, RunnerPackage};
    use greentic_desktop_registry::{
        sign_manifest, RegistryStage, RunnerManifest, SigningKey, TenantScope,
    };
    use greentic_desktop_session::SessionProfile;

    fn signed() -> SignedRunnerManifest {
        let key = SigningKey::new("local-dev", "material");
        sign_manifest(
            RunnerManifest {
                runner_id: "web.submit_form".to_owned(),
                version: "1.2.0".to_owned(),
                lifecycle: RunnerLifecycle::Published,
                stage: RegistryStage::Prod,
                scope: TenantScope {
                    tenant_id: "tenant_a".to_owned(),
                    team_id: "sales".to_owned(),
                    private: true,
                },
                required_adapters: vec!["greentic.desktop.playwright".to_owned()],
                compatibility: vec!["greentic-desktop>=0.1.0".to_owned()],
                package_checksum:
                    "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                        .to_owned(),
            },
            &key,
        )
        .expect("signed")
    }

    fn package() -> RunnerPackage {
        let base = example_runner_tool().package;
        RunnerPackage {
            id: "web.submit_form".to_owned(),
            version: "1.2.0".to_owned(),
            mode: RecordingMode::Hybrid,
            inputs: vec!["form_value".to_owned()],
            secrets: vec!["session_token".to_owned()],
            steps: vec![RunnerStep {
                id: "fill_value".to_owned(),
                action: "fill".to_owned(),
                target: LocatorTarget::default(),
                value: Some("{{form_value}}".to_owned()),
                required_capability: "web.fill".to_owned(),
            }],
            assertions: base.assertions,
            outputs: vec!["confirmation".to_owned()],
            open_questions: Vec::new(),
        }
    }

    fn built(mode: ForwardingMode) -> BuiltTool {
        build_forwarded_tool(
            &signed(),
            package(),
            SessionProfile {
                id: "empty".to_owned(),
                bootstrap: Vec::new(),
                teardown: Vec::new(),
            },
            vec![AdapterCapabilities::new(
                "greentic.desktop.playwright",
                "1.0.0",
                ["web.fill"],
            )],
            mode,
        )
        .expect("built tool")
    }

    #[test]
    fn published_runner_becomes_valid_mcp_tool() {
        let built = built(ForwardingMode::Local);

        assert_eq!(built.metadata.name, "web.submit_form");
        assert_eq!(built.descriptor().name, "web.submit_form");
    }

    #[test]
    fn tool_schema_matches_runner_input_output_schema() {
        let built = built(ForwardingMode::Local);

        assert_eq!(built.metadata.input_schema.required, vec!["form_value"]);
        assert_eq!(built.metadata.output_schema.required, vec!["confirmation"]);
        assert!(built
            .metadata
            .required_permissions
            .contains(&"type_text".to_owned()));
    }

    #[test]
    fn aws_forwarded_name_is_generated_when_requested() {
        let built = built(ForwardingMode::AwsForwarded);

        assert_eq!(
            built.metadata.forwarded_name,
            Some("forwarded___web_submit_form".to_owned())
        );
    }

    #[test]
    fn tool_call_fails_closed_without_real_replay_registry_and_returns_evidence() {
        let tool = built(ForwardingMode::Local);
        let mut state = register_tools(vec![tool]);

        let result = call_built_tool(
            &mut state,
            "web.submit_form",
            BTreeMap::from([("form_value".to_owned(), "user@example.test".to_owned())]),
            BTreeMap::from([("session_token".to_owned(), "secret".to_owned())]),
        );

        assert!(!result.success);
        assert!(result
            .failure
            .as_ref()
            .is_some_and(|failure| failure.message.contains("real adapter registry")));
        assert!(result.evidence_uri.contains("run_web.submit_form"));
    }
}
