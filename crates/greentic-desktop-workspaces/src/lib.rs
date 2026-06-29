use greentic_desktop_mcp::{
    example_runner_tool, McpCallRequest, McpCallResult, McpServerState, PublishedRunnerTool,
};
use greentic_desktop_registry::{
    RegistryStage, RunnerRegistry, SignedRunnerManifest, SigningKey, VersionSelector,
};
use greentic_desktop_session::{BootstrapAction, SessionProfile, TeardownAction};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkSpacesPattern {
    InstalledInsideWorkspace,
    AwsManagedMcpForwarding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSpaceTarget {
    pub workspace_id: String,
    pub image_version: String,
    pub region: String,
    pub pattern: WorkSpacesPattern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSpaceInstallPlan {
    pub workspace_id: String,
    pub runtime_binary: String,
    pub adapter_ids: Vec<String>,
    pub runner_refs: Vec<String>,
    pub mcp_bind: String,
    pub steps: Vec<String>,
}

impl WorkSpaceInstallPlan {
    pub fn session_profile(&self) -> SessionProfile {
        SessionProfile {
            id: format!("workspace_{}", self.workspace_id),
            bootstrap: vec![BootstrapAction::AttachWorkspace {
                workspace_id: self.workspace_id.clone(),
            }],
            teardown: vec![TeardownAction::DetachWorkspace {
                workspace_id: self.workspace_id.clone(),
            }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulledRunner {
    pub runner_id: String,
    pub version: String,
    pub tenant_id: String,
    pub required_adapters: Vec<String>,
}

pub fn plan_workspace_install(
    target: &WorkSpaceTarget,
    runner_refs: Vec<String>,
    adapter_ids: Vec<String>,
) -> WorkSpaceInstallPlan {
    WorkSpaceInstallPlan {
        workspace_id: target.workspace_id.clone(),
        runtime_binary: "greentic-desktop.exe".to_owned(),
        adapter_ids,
        runner_refs,
        mcp_bind: "127.0.0.1:8799".to_owned(),
        steps: vec![
            "install runtime into golden image".to_owned(),
            "pull approved runners from registry".to_owned(),
            "install required adapters".to_owned(),
            "start MCP endpoint".to_owned(),
            "register available tools".to_owned(),
        ],
    }
}

pub fn pull_approved_runner(
    registry: &RunnerRegistry,
    key: &SigningKey,
    tenant_id: &str,
    runner_id: &str,
    selector: VersionSelector,
    stage: RegistryStage,
) -> Result<PulledRunner, String> {
    let signed = registry
        .resolve(runner_id, tenant_id, selector, stage)
        .map_err(|err| err.to_string())?;
    signed.verify(key).map_err(|err| err.to_string())?;
    Ok(pulled_runner(signed))
}

pub fn expose_workspace_tools(tools: Vec<PublishedRunnerTool>) -> McpServerState {
    let permissions = tools
        .iter()
        .map(PublishedRunnerTool::tool_name)
        .collect::<Vec<_>>();
    McpServerState::new(tools, permissions)
}

pub fn call_forwarded_runner(
    state: &mut McpServerState,
    runner_id: &str,
    inputs: BTreeMap<String, String>,
    secrets: BTreeMap<String, String>,
) -> McpCallResult {
    state.call_tool(McpCallRequest {
        tool_name: runner_id.to_owned(),
        inputs,
        secrets,
        approved_by_human: true,
        environment: "staging".to_owned(),
        approvals: 1,
    })
}

pub fn workspace_validation_tool() -> PublishedRunnerTool {
    let mut tool = example_runner_tool();
    tool.package.id = "workspace.validate_after_patch".to_owned();
    tool.package.inputs = vec!["email".to_owned()];
    tool.package.secrets = vec!["password".to_owned()];
    if let Some(step) = tool
        .package
        .steps
        .iter_mut()
        .find(|step| step.value.is_some())
    {
        step.value = Some("{{email}}".to_owned());
    }
    tool.package.outputs = vec!["validation_result".to_owned()];
    tool
}

fn pulled_runner(signed: &SignedRunnerManifest) -> PulledRunner {
    PulledRunner {
        runner_id: signed.manifest.runner_id.clone(),
        version: signed.manifest.version.clone(),
        tenant_id: signed.manifest.scope.tenant_id.clone(),
        required_adapters: signed.manifest.required_adapters.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_registry::{sign_manifest, RunnerLifecycle, RunnerManifest, TenantScope};

    fn target() -> WorkSpaceTarget {
        WorkSpaceTarget {
            workspace_id: "ws-123".to_owned(),
            image_version: "golden-2026-06".to_owned(),
            region: "eu-west-2".to_owned(),
            pattern: WorkSpacesPattern::InstalledInsideWorkspace,
        }
    }

    fn signed_runner(key: &SigningKey) -> SignedRunnerManifest {
        sign_manifest(
            RunnerManifest {
                runner_id: "workspace.validate_after_patch".to_owned(),
                version: "1.2.0".to_owned(),
                lifecycle: RunnerLifecycle::Published,
                stage: RegistryStage::Prod,
                scope: TenantScope {
                    tenant_id: "tenant_a".to_owned(),
                    team_id: "platform".to_owned(),
                    private: true,
                },
                required_adapters: vec!["greentic.desktop.playwright".to_owned()],
                compatibility: vec!["greentic-desktop>=0.1.0".to_owned()],
                package_checksum: "sha256:abc123".to_owned(),
            },
            key,
        )
        .expect("signed runner")
    }

    #[test]
    fn runner_can_be_installed_in_workspace() {
        let plan = plan_workspace_install(
            &target(),
            vec!["workspace.validate_after_patch@stable".to_owned()],
            vec!["greentic.desktop.playwright".to_owned()],
        );

        assert_eq!(plan.runtime_binary, "greentic-desktop.exe");
        assert!(plan.steps.contains(&"start MCP endpoint".to_owned()));
        assert_eq!(
            plan.session_profile().bootstrap,
            vec![BootstrapAction::AttachWorkspace {
                workspace_id: "ws-123".to_owned()
            }]
        );
    }

    #[test]
    fn approved_runner_can_be_pulled_from_registry() {
        let key = SigningKey::new("local-dev", "material");
        let mut registry = RunnerRegistry::default();
        registry
            .publish(signed_runner(&key), &key)
            .expect("publish runner");

        let pulled = pull_approved_runner(
            &registry,
            &key,
            "tenant_a",
            "workspace.validate_after_patch",
            VersionSelector::Channel("stable".to_owned()),
            RegistryStage::Prod,
        )
        .expect("pulled runner");

        assert_eq!(pulled.version, "1.2.0");
        assert_eq!(
            pulled.required_adapters,
            vec!["greentic.desktop.playwright"]
        );
    }

    #[test]
    fn mcp_server_exposes_workspace_tools() {
        let state = expose_workspace_tools(vec![workspace_validation_tool()]);
        let tools = state.list_tools();

        assert_eq!(tools[0].name, "workspace.validate_after_patch");
    }

    #[test]
    fn external_mcp_client_gets_structured_failure_and_evidence_without_real_replay_registry() {
        let mut state = expose_workspace_tools(vec![workspace_validation_tool()]);
        let result = call_forwarded_runner(
            &mut state,
            "workspace.validate_after_patch",
            BTreeMap::from([("email".to_owned(), "user@example.test".to_owned())]),
            BTreeMap::from([("password".to_owned(), "secret".to_owned())]),
        );

        assert!(!result.success);
        assert!(result
            .failure
            .as_ref()
            .is_some_and(|failure| failure.message.contains("real adapter registry")));
        assert!(result
            .evidence_uri
            .contains("run_workspace.validate_after_patch"));
    }
}
