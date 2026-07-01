use greentic_desktop_adapter::{AdapterCapabilities, LocatorTarget, RunnerStep};
use greentic_desktop_core::RiskLevel;
use greentic_desktop_recorder::{RecordingMode, RunnerPackage};
use greentic_desktop_replay::{replay, ReplayRequest};
use greentic_desktop_runner_schema::{McpInputSchema, McpOutputSchema, RunnerSchemaField};
use greentic_desktop_security::{
    enforce_policy, ActionRequest, PolicyContext, PolicyDecision, SecurityPolicy,
};
use greentic_desktop_session::SessionProfile;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    Meta, ProtocolVersion, ServerCapabilities, Tool,
};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema_ref: String,
    pub output_schema_ref: String,
    pub input_schema_json: String,
    pub output_schema_json: String,
    pub availability_diagnostics: Vec<String>,
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
        let input_schema = McpInputSchema {
            fields: schema_fields(&self.package.inputs, false)
                .into_iter()
                .chain(schema_fields(&self.package.secrets, true))
                .collect(),
        };
        let output_schema = McpOutputSchema {
            fields: schema_fields(&self.package.outputs, false),
        };
        McpTool {
            name: self.tool_name(),
            description: format!("Run desktop runner {}", self.package.id),
            input_schema_ref: "inputs.schema.json".to_owned(),
            output_schema_ref: "outputs.schema.json".to_owned(),
            input_schema_json: input_schema.to_json_schema(),
            output_schema_json: output_schema.to_json_schema(),
            availability_diagnostics: adapter_availability_diagnostics(
                &self.package,
                &self.adapters,
            ),
            risk: self.risk,
        }
    }
}

pub fn mcp_tool_descriptor_for_package(
    package: &RunnerPackage,
    adapters: &[AdapterCapabilities],
    risk: RiskLevel,
    name: String,
    description: String,
) -> McpTool {
    let input_schema = McpInputSchema {
        fields: schema_fields(&package.inputs, false)
            .into_iter()
            .chain(schema_fields(&package.secrets, true))
            .collect(),
    };
    let output_schema = McpOutputSchema {
        fields: schema_fields(&package.outputs, false),
    };
    McpTool {
        name,
        description,
        input_schema_ref: "inputs.schema.json".to_owned(),
        output_schema_ref: "outputs.schema.json".to_owned(),
        input_schema_json: input_schema.to_json_schema(),
        output_schema_json: output_schema.to_json_schema(),
        availability_diagnostics: adapter_availability_diagnostics(package, adapters),
        risk,
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

    pub fn call_tool_with_arguments(
        &mut self,
        tool_name: String,
        arguments: BTreeMap<String, String>,
    ) -> McpCallResult {
        let (inputs, secrets) = self.partition_call_arguments(&tool_name, arguments);
        self.call_tool(McpCallRequest {
            tool_name,
            inputs,
            secrets,
            approved_by_human: false,
            environment: "local".to_owned(),
            approvals: 0,
        })
    }

    pub fn render_tools_list_json(&self) -> String {
        let tools = self
            .list_tools()
            .iter()
            .map(|tool| {
                format!(
                    "{{\"name\":\"{}\",\"description\":\"{}\",\"input_schema\":\"{}\",\"output_schema\":\"{}\",\"availability_diagnostics\":{}}}",
                    escape_json(&tool.name),
                    escape_json(&tool.description),
                    escape_json(&tool.input_schema_json),
                    escape_json(&tool.output_schema_json),
                    string_array_json(&tool.availability_diagnostics)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"tools\":[{tools}]}}")
    }

    pub fn handle_jsonrpc(&mut self, body: &str) -> String {
        match parse_jsonrpc_request(body) {
            Ok(request) => match request.method.as_str() {
                "initialize" => render_initialize_response(request.id.as_ref()),
                "notifications/initialized" => render_empty_response(request.id.as_ref()),
                "tools/list" => render_tools_list_response(request.id.as_ref(), &self.list_tools()),
                "tools/call" => {
                    let Some(name) = request.tool_name else {
                        return render_jsonrpc_error(
                            request.id.as_ref(),
                            -32602,
                            "tools/call params.name is required",
                        );
                    };
                    let (inputs, secrets) = self.partition_call_arguments(&name, request.arguments);
                    let result = self.call_tool(McpCallRequest {
                        tool_name: name,
                        inputs,
                        secrets,
                        approved_by_human: false,
                        environment: "local".to_owned(),
                        approvals: 0,
                    });
                    render_tool_call_response(request.id.as_ref(), &result)
                }
                _ => render_jsonrpc_error(request.id.as_ref(), -32601, "method not found"),
            },
            Err(err) => render_jsonrpc_error(None, err.code, &err.message),
        }
    }

    fn partition_call_arguments(
        &self,
        tool_name: &str,
        arguments: BTreeMap<String, String>,
    ) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let secret_names = self
            .tools
            .iter()
            .find(|tool| tool.tool_name() == tool_name)
            .map(|tool| {
                tool.package
                    .secrets
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        arguments
            .into_iter()
            .partition(|(name, _)| !secret_names.contains(name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpJsonRpcRequest {
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub tool_name: Option<String>,
    pub arguments: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpJsonRpcError {
    pub code: i64,
    pub message: String,
}

pub fn parse_jsonrpc_request(body: &str) -> Result<McpJsonRpcRequest, McpJsonRpcError> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|err| McpJsonRpcError {
        code: -32700,
        message: format!("parse error: {err}"),
    })?;
    let method = value
        .get("method")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| McpJsonRpcError {
            code: -32600,
            message: "JSON-RPC method is required".to_owned(),
        })?
        .to_owned();
    let params = value.get("params").unwrap_or(&serde_json::Value::Null);
    let tool_name = params
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let arguments = params
        .get("arguments")
        .and_then(serde_json::Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string()),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    Ok(McpJsonRpcRequest {
        id: value.get("id").cloned(),
        method,
        tool_name,
        arguments,
    })
}

pub fn render_initialize_response(id: Option<&serde_json::Value>) -> String {
    let result = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
        .with_protocol_version(ProtocolVersion::V_2025_06_18)
        .with_server_info(Implementation::new(
            "greentic-desktop",
            env!("CARGO_PKG_VERSION"),
        ));
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": json_id(id),
        "result": result
    })
    .to_string()
}

pub fn render_empty_response(id: Option<&serde_json::Value>) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": json_id(id),
        "result": {}
    })
    .to_string()
}

pub fn render_tools_list_response(id: Option<&serde_json::Value>, tools: &[McpTool]) -> String {
    let tools = rmcp_list_tools_result(tools);
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": json_id(id),
        "result": tools
    })
    .to_string()
}

pub fn render_tool_call_response(id: Option<&serde_json::Value>, result: &McpCallResult) -> String {
    if result.success {
        let call_result = rmcp_call_tool_result(result);
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": json_id(id),
            "result": call_result
        })
        .to_string()
    } else {
        let failure = result.failure.as_ref();
        render_jsonrpc_error(
            id,
            -32005,
            failure
                .map(|failure| failure.message.as_str())
                .unwrap_or("runner failed"),
        )
    }
}

pub fn render_jsonrpc_error(id: Option<&serde_json::Value>, code: i64, message: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": json_id(id),
        "error": {
            "code": code,
            "message": message
        }
    })
    .to_string()
}

pub fn rmcp_list_tools_result(tools: &[McpTool]) -> ListToolsResult {
    ListToolsResult::with_all_items(tools.iter().map(rmcp_tool).collect::<Vec<_>>())
}

pub fn rmcp_call_tool_result(result: &McpCallResult) -> CallToolResult {
    if result.success {
        let raw_outputs = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
            &result.outputs_json,
        )
        .unwrap_or_default();
        let outputs = normalise_mcp_outputs(raw_outputs);
        let structured = serde_json::Map::from_iter([
            (
                "status".to_owned(),
                serde_json::Value::String("passed".to_owned()),
            ),
            (
                "evidenceRef".to_owned(),
                serde_json::Value::String(result.evidence_uri.clone()),
            ),
            ("outputs".to_owned(), serde_json::Value::Object(outputs)),
        ]);
        let mut call_result = CallToolResult::structured(serde_json::Value::Object(structured));
        call_result.content = vec![ContentBlock::text(format!(
            "Runner completed. Evidence: {}",
            result.evidence_uri
        ))];
        call_result
    } else {
        let message = result
            .failure
            .as_ref()
            .map(|failure| failure.message.as_str())
            .unwrap_or("runner failed");
        CallToolResult::error(vec![ContentBlock::text(message.to_owned())])
    }
}

fn normalise_mcp_outputs(
    outputs: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    outputs
        .into_iter()
        .map(|(name, value)| {
            (
                name.strip_prefix("outputs.")
                    .map(str::to_owned)
                    .unwrap_or(name),
                value,
            )
        })
        .collect()
}

pub fn rmcp_tool(tool: &McpTool) -> Tool {
    let input_schema = json_object_or_default(&tool.input_schema_json);
    let output_schema = json_object_or_default(&tool.output_schema_json);
    let mut rmcp_tool = Tool::new(
        Cow::Owned(tool.name.clone()),
        Cow::Owned(tool.description.clone()),
        Arc::new(input_schema),
    )
    .with_raw_output_schema(Arc::new(output_schema));
    rmcp_tool.meta = Some(Meta(JsonObject::from_iter([
        (
            "greenticRisk".to_owned(),
            serde_json::Value::String(format!("{:?}", tool.risk)),
        ),
        (
            "greenticAvailabilityDiagnostics".to_owned(),
            serde_json::Value::Array(
                tool.availability_diagnostics
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        ),
    ])));
    rmcp_tool
}

fn json_object_or_default(raw: &str) -> JsonObject {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_else(|| {
            JsonObject::from_iter([(
                "type".to_owned(),
                serde_json::Value::String("object".to_owned()),
            )])
        })
}

fn json_id(id: Option<&serde_json::Value>) -> serde_json::Value {
    id.cloned().unwrap_or(serde_json::Value::Null)
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
    published_runner_tool_for_web_form()
}

pub fn published_runner_tool_for_web_form() -> PublishedRunnerTool {
    published_runner_tool(
        "web.submit_form",
        vec!["form_value"],
        vec!["session_token"],
        vec![
            RunnerStep {
                id: "fill_value".to_owned(),
                action: "fill".to_owned(),
                target: LocatorTarget::default(),
                value: Some("{{form_value}}".to_owned()),
                required_capability: "web.fill".to_owned(),
            },
            RunnerStep {
                id: "submit".to_owned(),
                action: "click".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "web.click".to_owned(),
            },
        ],
        vec!["confirmation"],
        vec!["web.fill", "web.click"],
        RiskLevel::Medium,
    )
}

pub fn published_runner_tool_for_native_app() -> PublishedRunnerTool {
    published_runner_tool(
        "native.update_record",
        vec!["record_value"],
        Vec::new(),
        vec![
            RunnerStep {
                id: "open_app".to_owned(),
                action: "open_app".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "windows.open_app".to_owned(),
            },
            RunnerStep {
                id: "type_value".to_owned(),
                action: "type_text".to_owned(),
                target: LocatorTarget::default(),
                value: Some("{{record_value}}".to_owned()),
                required_capability: "windows.type_text".to_owned(),
            },
        ],
        vec!["result"],
        vec!["windows.open_app", "windows.type_text"],
        RiskLevel::Medium,
    )
}

pub fn published_runner_tool_for_java_form() -> PublishedRunnerTool {
    published_runner_tool(
        "java.submit_form",
        vec!["field_value"],
        Vec::new(),
        vec![
            RunnerStep {
                id: "find_field".to_owned(),
                action: "find_component".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "java.find_component".to_owned(),
            },
            RunnerStep {
                id: "type_field".to_owned(),
                action: "type_text".to_owned(),
                target: LocatorTarget::default(),
                value: Some("{{field_value}}".to_owned()),
                required_capability: "java.type_text".to_owned(),
            },
        ],
        vec!["status"],
        vec!["java.find_component", "java.type_text"],
        RiskLevel::Low,
    )
}

pub fn published_runner_tool_for_terminal_lookup() -> PublishedRunnerTool {
    published_runner_tool(
        "terminal.lookup_record",
        vec!["lookup_key"],
        Vec::new(),
        vec![
            RunnerStep {
                id: "connect".to_owned(),
                action: "connect".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "terminal.connect".to_owned(),
            },
            RunnerStep {
                id: "send_lookup".to_owned(),
                action: "send_text".to_owned(),
                target: LocatorTarget::default(),
                value: Some("{{lookup_key}}".to_owned()),
                required_capability: "terminal.send_text".to_owned(),
            },
        ],
        vec!["lookup_result"],
        vec!["terminal.connect", "terminal.send_text"],
        RiskLevel::Low,
    )
}

pub fn published_runner_tool_for_vision_extraction() -> PublishedRunnerTool {
    published_runner_tool(
        "vision.extract_visible_text",
        Vec::new(),
        Vec::new(),
        vec![RunnerStep {
            id: "capture".to_owned(),
            action: "screenshot".to_owned(),
            target: LocatorTarget::default(),
            value: None,
            required_capability: "vision.screenshot".to_owned(),
        }],
        vec!["visible_text"],
        vec!["vision.screenshot"],
        RiskLevel::Low,
    )
}

fn published_runner_tool(
    id: &str,
    inputs: Vec<&str>,
    secrets: Vec<&str>,
    steps: Vec<RunnerStep>,
    outputs: Vec<&str>,
    capabilities: Vec<&str>,
    risk: RiskLevel,
) -> PublishedRunnerTool {
    PublishedRunnerTool {
        package: RunnerPackage {
            id: id.to_owned(),
            version: "1.2.0".to_owned(),
            mode: RecordingMode::Hybrid,
            inputs: inputs.into_iter().map(str::to_owned).collect(),
            secrets: secrets.into_iter().map(str::to_owned).collect(),
            steps,
            assertions: Vec::new(),
            outputs: outputs.into_iter().map(str::to_owned).collect(),
            open_questions: Vec::new(),
        },
        session_profile: SessionProfile {
            id: "empty".to_owned(),
            bootstrap: Vec::new(),
            teardown: Vec::new(),
        },
        adapters: vec![AdapterCapabilities::new(
            adapter_id_for_capabilities(&capabilities),
            "1.0.0",
            capabilities,
        )],
        risk,
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

fn adapter_id_for_capabilities(capabilities: &[&str]) -> &'static str {
    let first = capabilities.first().copied().unwrap_or("vision.screenshot");
    if first.starts_with("web.") {
        "greentic.desktop.playwright"
    } else if first.starts_with("windows.") {
        "greentic.desktop.windows-ui"
    } else if first.starts_with("java.") {
        "greentic.desktop.java-accessibility"
    } else if first.starts_with("terminal.") {
        "greentic.desktop.terminal-tn3270"
    } else {
        "greentic.desktop.vision"
    }
}

fn schema_fields(keys: &[String], secret: bool) -> Vec<RunnerSchemaField> {
    keys.iter()
        .map(|key| RunnerSchemaField {
            name: key.clone(),
            value_type: greentic_desktop_workflow::WorkflowValueType::String,
            required: true,
            secret,
            default_value: None,
            enum_values: Vec::new(),
            validation: None,
        })
        .collect()
}

fn inputs_cover_schema(
    package: &RunnerPackage,
    inputs: &BTreeMap<String, String>,
    secrets: &BTreeMap<String, String>,
) -> bool {
    package.inputs.iter().all(|key| inputs.contains_key(key))
        && package.secrets.iter().all(|key| secrets.contains_key(key))
}

fn adapter_availability_diagnostics(
    package: &RunnerPackage,
    adapters: &[AdapterCapabilities],
) -> Vec<String> {
    let mut missing = package
        .steps
        .iter()
        .map(|step| step.required_capability.clone())
        .filter(|capability| !adapters.iter().any(|adapter| adapter.supports(capability)))
        .collect::<Vec<_>>();
    missing.sort();
    missing.dedup();
    missing
        .into_iter()
        .map(|capability| {
            format!("No healthy adapter currently exposes required capability {capability}.")
        })
        .collect()
}

fn string_array_json(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| format!("\"{}\"", escape_json(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{values}]")
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
        McpServerState::new(vec![example_runner_tool()], ["web.submit_form".to_owned()])
    }

    #[test]
    fn tools_list_returns_published_allowed_runners() {
        let tools = state().list_tools();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "web.submit_form");
        assert_eq!(tools[0].input_schema_ref, "inputs.schema.json");
        assert!(tools[0].input_schema_json.contains("form_value"));
        assert!(tools[0].output_schema_json.contains("confirmation"));
        assert!(tools[0].availability_diagnostics.is_empty());
    }

    #[test]
    fn tools_list_reports_missing_adapter_diagnostics() {
        let mut tool = example_runner_tool();
        tool.adapters.clear();
        let state = McpServerState::new(vec![tool], ["web.submit_form".to_owned()]);

        let tools = state.list_tools();

        assert_eq!(tools.len(), 1);
        assert!(tools[0]
            .availability_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("web.fill")));
    }

    #[test]
    fn tools_call_fails_closed_without_real_replay_registry() {
        let mut state = state();
        let result = state.call_tool(McpCallRequest {
            tool_name: "web.submit_form".to_owned(),
            inputs: BTreeMap::from([("form_value".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::from([("session_token".to_owned(), "secret".to_owned())]),
            approved_by_human: false,
            environment: "dev".to_owned(),
            approvals: 0,
        });

        assert!(!result.success);
        let failure = result.failure.expect("failure");
        assert_eq!(failure.code, "runner_failed");
        assert!(failure.message.contains("real adapter registry"));
        assert_eq!(result.outputs_json, "{}");
        assert_eq!(
            result.evidence_uri,
            "evidence://run_web.submit_form/bundle.json"
        );
    }

    #[test]
    fn failed_calls_return_structured_failure_and_evidence_reference() {
        let mut tool = example_runner_tool();
        tool.adapters.clear();
        let mut state = McpServerState::new(vec![tool], ["web.submit_form".to_owned()]);

        let result = state.call_tool(McpCallRequest {
            tool_name: "web.submit_form".to_owned(),
            inputs: BTreeMap::from([("form_value".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::from([("session_token".to_owned(), "secret".to_owned())]),
            approved_by_human: false,
            environment: "dev".to_owned(),
            approvals: 0,
        });

        assert!(!result.success);
        assert_eq!(result.failure.expect("failure").code, "runner_failed");
        assert!(result.evidence_uri.contains("run_web.submit_form"));
    }

    #[test]
    fn successful_rmcp_calls_expose_outputs_as_clean_nested_object() {
        let result = McpCallResult {
            success: true,
            outputs_json: serde_json::json!({
                "outputs.result": "25"
            })
            .to_string(),
            failure: None,
            evidence_uri: "evidence://run_example.macos.calculator.operation/bundle.json"
                .to_owned(),
        };

        let call_result = rmcp_call_tool_result(&result);

        assert_eq!(call_result.is_error, Some(false));
        let structured = call_result
            .structured_content
            .as_ref()
            .expect("successful call should have structured content");
        assert_eq!(structured["status"], "passed");
        assert_eq!(structured["outputs"]["result"], "25");
        assert!(structured.get("outputs.result").is_none());
        assert_eq!(
            structured["evidenceRef"],
            "evidence://run_example.macos.calculator.operation/bundle.json"
        );
    }

    #[test]
    fn output_schema_normalises_runner_output_prefixes() {
        let tool = published_runner_tool(
            "example.demo_ecommerce.laptop.price_model",
            Vec::new(),
            Vec::new(),
            vec![RunnerStep {
                id: "read-model".to_owned(),
                action: "extract_text".to_owned(),
                target: LocatorTarget::default(),
                value: Some("outputs.model".to_owned()),
                required_capability: "web.extract_text".to_owned(),
            }],
            vec!["outputs.model", "outputs.price"],
            vec!["web.extract_text"],
            RiskLevel::Low,
        )
        .descriptor();
        let schema: serde_json::Value =
            serde_json::from_str(&tool.output_schema_json).expect("output schema should be json");

        assert_eq!(
            schema["properties"]["outputs"]["properties"]["model"]["type"],
            "string"
        );
        assert_eq!(
            schema["properties"]["outputs"]["properties"]["price"]["type"],
            "string"
        );
        assert_eq!(
            schema["properties"]["outputs"]["required"],
            serde_json::json!(["model", "price"])
        );
        assert!(schema["properties"]["outputs"]["properties"]
            .get("outputs.model")
            .is_none());
    }

    #[test]
    fn forwarded_tool_names_are_stable() {
        let tool = example_runner_tool();

        assert_eq!(tool.forwarded_tool_name(), "forwarded___web_submit_form");
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
        let mut state = McpServerState::new(vec![tool], ["web.submit_form".to_owned()]);

        let result = state.call_tool(McpCallRequest {
            tool_name: "web.submit_form".to_owned(),
            inputs: BTreeMap::from([("form_value".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::from([("session_token".to_owned(), "secret".to_owned())]),
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
        let mut state = McpServerState::new(vec![tool], ["web.submit_form".to_owned()]);

        let result = state.call_tool(McpCallRequest {
            tool_name: "web.submit_form".to_owned(),
            inputs: BTreeMap::from([("form_value".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::from([("session_token".to_owned(), "secret".to_owned())]),
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
        assert!(json.contains("\"name\":\"web.submit_form\""));
        assert!(json.contains("form_value"));
    }

    #[test]
    fn jsonrpc_initialize_returns_mcp_capabilities_with_request_id() {
        let mut state = state();
        let response =
            state.handle_jsonrpc(r#"{"jsonrpc":"2.0","id":"init-1","method":"initialize"}"#);

        let value: serde_json::Value = serde_json::from_str(&response).expect("valid json");
        assert_eq!(value["id"], "init-1");
        assert_eq!(
            value["result"]["protocolVersion"],
            serde_json::Value::String(MCP_PROTOCOL_VERSION.to_owned())
        );
        assert_eq!(
            value["result"]["capabilities"]["tools"],
            serde_json::json!({})
        );
    }

    #[test]
    fn jsonrpc_tools_list_returns_standard_input_schema() {
        let mut state = state();
        let response = state.handle_jsonrpc(r#"{"jsonrpc":"2.0","id":7,"method":"tools/list"}"#);

        let value: serde_json::Value = serde_json::from_str(&response).expect("valid json");
        assert_eq!(value["id"], 7);
        assert_eq!(value["result"]["tools"][0]["name"], "web.submit_form");
        assert_eq!(
            value["result"]["tools"][0]["inputSchema"]["properties"]["form_value"]["type"],
            "string"
        );
        assert_eq!(
            value["result"]["tools"][0]["outputSchema"]["properties"]["outputs"]["properties"]
                ["confirmation"]["type"],
            "string"
        );
        assert_eq!(
            value["result"]["tools"][0]["outputSchema"]["required"],
            serde_json::json!(["outputs"])
        );
    }

    #[test]
    fn jsonrpc_tools_call_parses_nested_arguments_and_returns_failure_json() {
        let mut state = state();
        let response = state.handle_jsonrpc(
            r#"{"jsonrpc":"2.0","id":"call-1","method":"tools/call","params":{"name":"web.submit_form","arguments":{"form_value":"Alice","session_token":"secret"}}}"#,
        );

        let value: serde_json::Value = serde_json::from_str(&response).expect("valid json");
        assert_eq!(value["id"], "call-1");
        assert_eq!(value["error"]["code"], -32005);
        assert!(!value["error"]["message"]
            .as_str()
            .expect("message")
            .contains("required input or secret is missing"));
    }

    #[test]
    fn jsonrpc_unknown_method_returns_method_not_found() {
        let mut state = state();
        let response = state.handle_jsonrpc(r#"{"jsonrpc":"2.0","id":2,"method":"bad"}"#);

        let value: serde_json::Value = serde_json::from_str(&response).expect("valid json");
        assert_eq!(value["id"], 2);
        assert_eq!(value["error"]["code"], -32601);
    }

    #[test]
    fn generic_fixture_builders_cover_all_runner_technologies() {
        let tools = [
            published_runner_tool_for_web_form(),
            published_runner_tool_for_native_app(),
            published_runner_tool_for_java_form(),
            published_runner_tool_for_terminal_lookup(),
            published_runner_tool_for_vision_extraction(),
        ];
        let names = tools
            .iter()
            .map(PublishedRunnerTool::tool_name)
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "web.submit_form",
                "native.update_record",
                "java.submit_form",
                "terminal.lookup_record",
                "vision.extract_visible_text",
            ]
        );
        assert!(tools.iter().all(|tool| {
            let descriptor = tool.descriptor();
            descriptor.input_schema_json.contains("\"type\":\"object\"")
                && descriptor
                    .output_schema_json
                    .contains("\"type\":\"object\"")
        }));
    }
}
