#![recursion_limit = "256"]

use greentic_llm::{
    ChatMessage, ChatRequest, Credential, LlmProvider as GreenticProvider, ProviderKind, RigBackend,
};
use serde_json::json;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPolicy {
    pub temperature_tenths: u8,
    pub response_format: String,
    pub max_retries: u8,
}

impl Default for ModelPolicy {
    fn default() -> Self {
        Self {
            temperature_tenths: 1,
            response_format: "json_schema".to_owned(),
            max_retries: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub default_model: &'static str,
    pub mode: &'static str,
    pub endpoint: Option<&'static str>,
    pub secret_name: Option<&'static str>,
}

pub fn known_providers() -> &'static [LlmProvider] {
    &[
        LlmProvider {
            id: "local",
            name: "Local heuristic",
            default_model: "heuristic-planner",
            mode: "heuristic",
            endpoint: None,
            secret_name: None,
        },
        LlmProvider {
            id: "openai",
            name: "OpenAI",
            default_model: "gpt-4.1-mini",
            mode: "remote",
            endpoint: Some("https://api.openai.com/v1"),
            secret_name: Some("OPENAI_API_KEY"),
        },
        LlmProvider {
            id: "anthropic",
            name: "Anthropic",
            default_model: "claude-3-5-sonnet-latest",
            mode: "remote",
            endpoint: Some("https://api.anthropic.com"),
            secret_name: Some("ANTHROPIC_API_KEY"),
        },
        LlmProvider {
            id: "deepseek",
            name: "DeepSeek",
            default_model: "deepseek-chat",
            mode: "remote",
            endpoint: Some("https://api.deepseek.com"),
            secret_name: Some("DEEPSEEK_API_KEY"),
        },
        LlmProvider {
            id: "gemini",
            name: "Google Gemini",
            default_model: "gemini-1.5-flash",
            mode: "remote",
            endpoint: Some("https://generativelanguage.googleapis.com"),
            secret_name: Some("GOOGLE_API_KEY"),
        },
        LlmProvider {
            id: "cohere",
            name: "Cohere",
            default_model: "command-r-plus",
            mode: "remote",
            endpoint: Some("https://api.cohere.com"),
            secret_name: Some("COHERE_API_KEY"),
        },
        LlmProvider {
            id: "groq",
            name: "Groq",
            default_model: "llama-3.1-70b-versatile",
            mode: "remote",
            endpoint: Some("https://api.groq.com/openai/v1"),
            secret_name: Some("GROQ_API_KEY"),
        },
        LlmProvider {
            id: "perplexity",
            name: "Perplexity",
            default_model: "sonar-pro",
            mode: "remote",
            endpoint: Some("https://api.perplexity.ai"),
            secret_name: Some("PERPLEXITY_API_KEY"),
        },
        LlmProvider {
            id: "xai",
            name: "xAI",
            default_model: "grok-2-latest",
            mode: "remote",
            endpoint: Some("https://api.x.ai/v1"),
            secret_name: Some("XAI_API_KEY"),
        },
        LlmProvider {
            id: "azure",
            name: "Azure OpenAI",
            default_model: "deployment-name",
            mode: "remote",
            endpoint: None,
            secret_name: Some("AZURE_OPENAI_API_KEY"),
        },
        LlmProvider {
            id: "azure-foundry",
            name: "Azure AI Foundry",
            default_model: "DeepSeek-R1",
            mode: "remote",
            endpoint: None,
            secret_name: Some("AZURE_AI_FOUNDRY_API_KEY"),
        },
        LlmProvider {
            id: "bedrock",
            name: "Amazon Bedrock",
            default_model: "anthropic.claude-3-5-sonnet-20240620-v1:0",
            mode: "remote",
            endpoint: None,
            secret_name: None,
        },
        LlmProvider {
            id: "mistral",
            name: "Mistral",
            default_model: "mistral-small-latest",
            mode: "remote",
            endpoint: Some("https://api.mistral.ai"),
            secret_name: Some("MISTRAL_API_KEY"),
        },
        LlmProvider {
            id: "openrouter",
            name: "OpenRouter",
            default_model: "openai/gpt-4o-mini",
            mode: "remote",
            endpoint: Some("https://openrouter.ai/api/v1"),
            secret_name: Some("OPENROUTER_API_KEY"),
        },
        LlmProvider {
            id: "huggingface",
            name: "Hugging Face",
            default_model: "mistralai/Mistral-7B-Instruct-v0.3",
            mode: "remote",
            endpoint: Some("https://api-inference.huggingface.co"),
            secret_name: Some("HUGGINGFACE_API_KEY"),
        },
        LlmProvider {
            id: "together",
            name: "Together AI",
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
            mode: "remote",
            endpoint: Some("https://api.together.xyz/v1"),
            secret_name: Some("TOGETHER_API_KEY"),
        },
        LlmProvider {
            id: "moonshot",
            name: "Moonshot AI",
            default_model: "moonshot-v1-8k",
            mode: "remote",
            endpoint: Some("https://api.moonshot.ai/v1"),
            secret_name: Some("MOONSHOT_API_KEY"),
        },
        LlmProvider {
            id: "minimax",
            name: "MiniMax",
            default_model: "abab6.5s-chat",
            mode: "remote",
            endpoint: Some("https://api.minimax.chat/v1"),
            secret_name: Some("MINIMAX_API_KEY"),
        },
        LlmProvider {
            id: "hyperbolic",
            name: "Hyperbolic",
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct",
            mode: "remote",
            endpoint: Some("https://api.hyperbolic.xyz/v1"),
            secret_name: Some("HYPERBOLIC_API_KEY"),
        },
        LlmProvider {
            id: "galadriel",
            name: "Galadriel",
            default_model: "gpt-4o-mini",
            mode: "remote",
            endpoint: Some("https://api.galadriel.com/v1"),
            secret_name: Some("GALADRIEL_API_KEY"),
        },
        LlmProvider {
            id: "mira",
            name: "Mira",
            default_model: "gpt-4o-mini",
            mode: "remote",
            endpoint: Some("https://api.mira.network/v1"),
            secret_name: Some("MIRA_API_KEY"),
        },
        LlmProvider {
            id: "zai",
            name: "Z.ai",
            default_model: "glm-4",
            mode: "remote",
            endpoint: Some("https://open.bigmodel.cn/api/paas/v4"),
            secret_name: Some("ZAI_API_KEY"),
        },
        LlmProvider {
            id: "xiaomimimo",
            name: "Xiaomi MiMo",
            default_model: "mimo-vl-7b",
            mode: "remote",
            endpoint: Some("https://api.mimo.mi.com/v1"),
            secret_name: Some("XIAOMIMIMO_API_KEY"),
        },
        LlmProvider {
            id: "ollama",
            name: "Ollama",
            default_model: "llama3.1",
            mode: "remote",
            endpoint: Some("http://127.0.0.1:11434"),
            secret_name: None,
        },
        LlmProvider {
            id: "llamafile",
            name: "Llamafile",
            default_model: "local",
            mode: "remote",
            endpoint: Some("http://127.0.0.1:8080"),
            secret_name: None,
        },
    ]
}

pub fn provider_by_id(id: &str) -> Option<&'static LlmProvider> {
    known_providers().iter().find(|provider| provider.id == id)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LlmPlanningContext {
    pub available_adapters: Vec<String>,
    pub available_mcp_tools: Vec<String>,
    pub session_profiles: Vec<String>,
    pub existing_runners: Vec<String>,
    pub ltm_examples: Vec<String>,
    pub security_policy: Vec<String>,
    pub desktop_observation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRequestEnvelope {
    pub task: String,
    pub model_policy: ModelPolicy,
    pub context: LlmPlanningContext,
    pub user_prompt: String,
    pub expected_json_schema: serde_json::Value,
    pub repair: Option<LlmRepairContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRepairContext {
    pub invalid_json: String,
    pub validation_error: String,
    pub attempt: u8,
}

impl LlmRequestEnvelope {
    pub fn prompt_to_runner(prompt: impl Into<String>, context: LlmPlanningContext) -> Self {
        Self {
            task: "desktop.prompt_to_runner".to_owned(),
            model_policy: ModelPolicy::default(),
            context,
            user_prompt: prompt.into(),
            expected_json_schema: runner_draft_json_schema(),
            repair: None,
        }
    }

    pub fn repair_runner_json(
        prompt: impl Into<String>,
        context: LlmPlanningContext,
        invalid_json: impl Into<String>,
        validation_error: impl Into<String>,
        attempt: u8,
    ) -> Self {
        Self {
            task: "desktop.repair_runner_json".to_owned(),
            model_policy: ModelPolicy::default(),
            context,
            user_prompt: prompt.into(),
            expected_json_schema: runner_draft_json_schema(),
            repair: Some(LlmRepairContext {
                invalid_json: invalid_json.into(),
                validation_error: validation_error.into(),
                attempt,
            }),
        }
    }

    pub fn render_json(&self) -> String {
        json!({
            "task": self.task,
            "model_policy": {
                "temperature": f64::from(self.model_policy.temperature_tenths) / 10.0,
                "response_format": self.model_policy.response_format,
                "max_retries": self.model_policy.max_retries,
            },
            "context": {
                "available_adapters": self.context.available_adapters,
                "available_mcp_tools": self.context.available_mcp_tools,
                "session_profiles": self.context.session_profiles,
                "existing_runners": self.context.existing_runners,
                "ltm_examples": self.context.ltm_examples,
                "security_policy": self.context.security_policy,
                "desktop_observation": self.context.desktop_observation,
            },
            "user_prompt": self.user_prompt,
            "expected_json_schema": self.expected_json_schema,
            "repair": self.repair.as_ref().map(|repair| json!({
                "attempt": repair.attempt,
                "validation_error": repair.validation_error,
                "invalid_json": repair.invalid_json,
                "instruction": "Return a corrected JSON object only. Preserve user intent, fix every validation error, and validate against expected_json_schema."
            })),
            "valid_example": runner_draft_json_example(),
        })
        .to_string()
    }
}

pub fn runner_draft_json_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "runner_id",
            "version",
            "summary",
            "risk_level",
            "required_capabilities",
            "inputs",
            "outputs",
            "steps",
            "assertions",
            "open_questions"
        ],
        "properties": {
            "runner_id": {"type": "string", "minLength": 1},
            "version": {"type": "string", "minLength": 1},
            "summary": {"type": "string", "minLength": 1},
            "risk_level": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
            "required_capabilities": {
                "type": "array",
                "items": {"type": "string", "minLength": 1}
            },
            "inputs": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "properties": {
                        "type": {"type": "string"},
                        "required": {"type": "boolean"},
                        "description": {"type": "string"}
                    }
                }
            },
            "outputs": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "properties": {
                        "type": {"type": "string"},
                        "description": {"type": "string"}
                    }
                }
            },
            "primitive_workflow": {
                "type": "object",
                "description": "Preferred portable workflow plan. Use this for desktop, web, Java, terminal, remote desktop, and vision automation. Keep steps empty when primitive_workflow is present; Greentic compiles primitives to platform adapter steps.",
                "required": ["id", "summary", "target", "primitives"],
                "properties": {
                    "id": {"type": "string", "minLength": 1},
                    "summary": {"type": "string"},
                    "target": {"type": "object"},
                    "inputs": {"type": "array"},
                    "outputs": {"type": "array"},
                    "assertions": {"type": "array"},
                    "evidence_policy": {"type": "object"},
                    "primitives": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["kind"],
                            "properties": {
                                "kind": {
                                    "type": "string",
                                    "enum": [
                                        "open_app",
                                        "open_resource",
                                        "focus",
                                        "enter_text",
                                        "invoke_command",
                                        "save_resource",
                                        "observe_output",
                                        "assert_state"
                                    ]
                                },
                                "app": {"type": "object"},
                                "resource": {"type": "object"},
                                "target": {"type": "object"},
                                "command": {"type": "object"},
                                "value_template": {"type": "string"},
                                "path_template": {"type": ["string", "null"]},
                                "policy": {"type": "string"},
                                "name": {"type": "string"},
                                "extractor": {"type": "object"},
                                "condition": {"type": "object"},
                                "create_if_missing": {"type": "boolean"}
                            }
                        }
                    }
                }
            },
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "action", "required_capability"],
                    "properties": {
                        "id": {"type": "string", "minLength": 1},
                        "action": {"type": "string", "minLength": 1},
                        "required_capability": {"type": "string", "minLength": 1},
                        "value": {"type": ["string", "null"]},
                        "target": {"type": "object"}
                    }
                }
            },
            "assertions": {"type": "array", "items": {"type": "string"}},
            "open_questions": {"type": "array", "items": {"type": "string"}}
        }
    })
}

fn runner_draft_json_example() -> serde_json::Value {
    json!({
        "runner_id": "generic.resource.update",
        "version": "0.1.0-draft",
        "summary": "Ask for a resource name, open or create the resource, enter provided row values, save, and return the saved status.",
        "risk_level": "medium",
        "required_capabilities": ["macos.activate_app", "macos.type_text", "macos.read_text"],
        "inputs": {
            "resource_name": {"type": "string", "required": true, "description": "Name or path of the resource to open or create"},
            "name": {"type": "string", "required": true},
            "email": {"type": "string", "required": true}
        },
        "outputs": {
            "saved_status": {"type": "string"}
        },
        "primitive_workflow": {
            "id": "generic.resource.update",
            "summary": "Open or create a resource, enter user-provided data, save it, and prove the resource exists.",
            "target": {"kind": {"NativeApp": "MacOs"}, "open": {"App": {"app_name": "Default app", "window_title": "Default app"}}},
            "inputs": [],
            "primitives": [
                {"kind": "open_app", "app": {"name": "Default app", "bundle_id": null, "executable": null, "window_title": "Default app"}},
                {"kind": "open_resource", "resource": {"path_template": "{{inputs.resource_name}}", "resource_type": "Unknown"}, "create_if_missing": true},
                {"kind": "enter_text", "target": {"label": "active document", "role": "document", "text": null, "automation_id": null, "shortcut": null}, "value_template": "{{inputs.name}}\t{{inputs.email}}"},
                {"kind": "save_resource", "path_template": "{{inputs.resource_name}}", "policy": "CreateOrUpdate"},
                {"kind": "assert_state", "condition": {"ResourceExists": {"path_template": "{{inputs.resource_name}}"}}}
            ],
            "outputs": [],
            "assertions": [],
            "evidence_policy": {"capture_steps": true, "capture_screenshots": true}
        },
        "steps": [],
        "assertions": ["resource was saved"],
        "open_questions": ["Which application should open this resource if the OS default is not correct?"]
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmResponse {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    Unavailable(String),
}

pub trait GreenticLlmClient {
    fn complete(&self, request: &LlmRequestEnvelope) -> Result<LlmResponse, LlmError>;
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for LlmError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleLlmClient {
    pub provider_id: String,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl OpenAiCompatibleLlmClient {
    pub fn new(
        provider_id: impl Into<String>,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            endpoint: endpoint.into(),
            model: model.into(),
            api_key,
        }
    }
}

impl GreenticLlmClient for OpenAiCompatibleLlmClient {
    fn complete(&self, request: &LlmRequestEnvelope) -> Result<LlmResponse, LlmError> {
        if !is_greentic_llm_provider(&self.provider_id) {
            return Err(LlmError::Unavailable(format!(
                "LLM provider '{}' is not wired to a live request format yet.",
                self.provider_id
            )));
        }

        let kind = ProviderKind::from_str(&self.provider_id).map_err(LlmError::Unavailable)?;
        let backend = RigBackend::new(
            kind,
            &self.model,
            &Credential {
                api_key: self.api_key.clone().unwrap_or_default(),
                base_url: endpoint_override(kind, &self.endpoint),
                expires_at: None,
                api_version: None,
                aws_profile: None,
            },
        )
        .map_err(|err| LlmError::Unavailable(err.to_string()))?;
        let response = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| LlmError::Unavailable(format!("failed to start LLM runtime: {err}")))?
            .block_on(backend.chat(ChatRequest {
                messages: vec![
                    ChatMessage::system(system_prompt()),
                    ChatMessage::user(request.render_json()),
                ],
                tools: Vec::new(),
                tool_choice: None,
                max_tokens: Some(8_192),
                temperature: Some(f32::from(request.model_policy.temperature_tenths) / 10.0),
            }))
            .map_err(|err| LlmError::Unavailable(err.to_string()))?;
        let content = response.content;
        Ok(LlmResponse { content })
    }
}

fn endpoint_override(kind: ProviderKind, endpoint: &str) -> Option<String> {
    let default = provider_by_id(kind.as_str())
        .and_then(|provider| provider.endpoint)
        .unwrap_or_default()
        .trim_end_matches('/');
    let endpoint = endpoint.trim_end_matches('/');
    (!endpoint.is_empty() && endpoint != default).then(|| endpoint.to_owned())
}

fn system_prompt() -> &'static str {
    "You turn desktop automation prompts into strict runner JSON. Return only a JSON object that validates against expected_json_schema in the user message. If the task is desktop.repair_runner_json, fix the supplied invalid_json using repair.validation_error and return only the corrected JSON. Prefer primitive_workflow for portable desktop, web, Java, terminal, remote desktop, and vision automation. If primitive_workflow is present, steps may be empty because Greentic compiles primitives. If you include steps, every item MUST include id, action, and required_capability. Inputs and outputs must be JSON objects keyed by field name."
}

pub fn is_openai_compatible_provider(provider_id: &str) -> bool {
    is_greentic_llm_provider(provider_id)
}

pub fn is_greentic_llm_provider(provider_id: &str) -> bool {
    ProviderKind::from_str(provider_id).is_ok()
}

#[derive(Debug, Clone)]
pub struct StaticLlmClient {
    response: Result<LlmResponse, LlmError>,
}

impl StaticLlmClient {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            response: Ok(LlmResponse {
                content: content.into(),
            }),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            response: Err(LlmError::Unavailable(message.into())),
        }
    }
}

impl GreenticLlmClient for StaticLlmClient {
    fn complete(&self, _request: &LlmRequestEnvelope) -> Result<LlmResponse, LlmError> {
        self.response.clone()
    }
}

#[derive(Debug, Clone, Default)]
pub struct HeuristicLlmClient;

impl GreenticLlmClient for HeuristicLlmClient {
    fn complete(&self, request: &LlmRequestEnvelope) -> Result<LlmResponse, LlmError> {
        let lower = request.user_prompt.to_ascii_lowercase();
        let runner_id = runner_id(&request.user_prompt);
        let risk = if lower.contains("payment") || lower.contains("delete") {
            "critical"
        } else if lower.contains("create") || lower.contains("update") || lower.contains("submit") {
            "medium"
        } else {
            "low"
        };
        let capability = if lower.contains("xlsx")
            || lower.contains("workbook")
            || lower.contains("spreadsheet")
            || lower.contains("excel file")
        {
            if lower.contains("append")
                || lower.contains("add a new line")
                || lower.contains("add row")
            {
                "excel.append_rows"
            } else if lower.contains("report")
                || lower.contains("export")
                || lower.contains("create")
            {
                "excel.create_workbook"
            } else if lower.contains("search")
                || lower.contains("look up")
                || lower.contains("find")
            {
                "excel.search_rows"
            } else {
                "excel.read_range"
            }
        } else if lower.contains("terminal") || lower.contains("mainframe") {
            "terminal.read_screen"
        } else if lower.contains("calculator") || lower.contains("desktop") || lower.contains("app")
        {
            desktop_capability(request)
        } else {
            "web.goto"
        };
        let mut inputs = Vec::new();
        if lower.contains("spreadsheet") || lower.contains("xlsx") || lower.contains("workbook") {
            inputs.push("xlsx_path");
        }
        if lower.contains("search term") || lower.contains("look up") || lower.contains("find") {
            inputs.push("search_term");
        }
        if lower.contains("document") || lower.contains("word") {
            if lower.contains("place") || lower.contains("path") || lower.contains("location") {
                inputs.push("document_path");
            } else {
                inputs.push("document_name");
            }
        }
        if lower.contains("text") || lower.contains("content") {
            inputs.push("text_content");
        }
        if lower.contains("provided") && lower.contains("name")
            || lower.contains("user provided name")
        {
            inputs.push("name");
        }
        if lower.contains("email") {
            inputs.push("email");
        }
        if lower.contains("calculator")
            || lower.contains("two numbers")
            || lower.contains("value 1")
        {
            inputs.push("number_1");
        }
        if lower.contains("calculator")
            || lower.contains("two numbers")
            || lower.contains("value 2")
        {
            inputs.push("number_2");
        }
        if lower.contains("calculator") || lower.contains("operation") {
            inputs.push("operation");
        }
        if lower.contains("company") {
            inputs.push("company_name");
        }
        if inputs.is_empty() && lower.contains("customer") {
            inputs.push("customer_name");
        }
        inputs.sort();
        inputs.dedup();
        let outputs = if lower.contains("saved") || lower.contains("save the changes") {
            vec!["saved_status"]
        } else if lower.contains("calculator")
            || lower.contains("calculate")
            || lower.contains("result")
        {
            vec!["result"]
        } else if lower.contains("customer id") || lower.contains("customer_id") {
            vec!["customer_id"]
        } else {
            Vec::new()
        };
        let open_questions = if capability.starts_with("excel.") && !mentions_sheet_name(&lower) {
            vec!["Which worksheet should the Excel runner use?"]
        } else if lower.contains("login") && !lower.contains("service account") {
            vec!["Which credentials or service account should be used?"]
        } else if inputs.is_empty() {
            vec!["Which input values should the runner require?"]
        } else {
            Vec::new()
        };

        let primitive_workflow = primitive_workflow_json(
            &runner_id,
            &request.user_prompt,
            &lower,
            capability,
            &inputs,
        );
        let steps = if primitive_workflow.is_some() {
            "[]".to_owned()
        } else {
            format!(
                "[{{\"id\":\"draft_1\",\"action\":\"plan\",\"required_capability\":\"{}\"}}]",
                capability
            )
        };
        let primitive_property = primitive_workflow
            .map(|workflow| format!(",\"primitive_workflow\":{workflow}"))
            .unwrap_or_default();

        Ok(LlmResponse {
            content: format!(
                "{{\"runner_id\":\"{}\",\"version\":\"0.1.0-draft\",\"summary\":\"{}\",\"risk_level\":\"{}\",\"required_capabilities\":[\"{}\"],\"inputs\":{},\"outputs\":{}{},\"steps\":{},\"assertions\":[\"no unexpected errors\"],\"open_questions\":{}}}",
                escape(&runner_id),
                escape(&request.user_prompt),
                risk,
                capability,
                named_schema(&inputs),
                named_schema(&outputs),
                primitive_property,
                steps,
                string_array(&open_questions.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>())
            ),
        })
    }
}

fn primitive_workflow_json(
    runner_id: &str,
    prompt: &str,
    lower: &str,
    capability: &str,
    inputs: &[&str],
) -> Option<String> {
    let platform = if capability.starts_with("macos.") {
        ("MacOs", "macos")
    } else if capability.starts_with("windows.") {
        ("Windows", "windows")
    } else if capability.starts_with("linux.") {
        ("LinuxX11", "linux")
    } else {
        return None;
    };
    let app_name = if lower.contains("calculator") {
        "Calculator"
    } else if lower.contains("word") || lower.contains("document") {
        "Word"
    } else if lower.contains("spreadsheet") {
        "Default spreadsheet app"
    } else {
        "Default app"
    };
    let path_template = if lower.contains("spreadsheet") {
        "{{inputs.spreadsheet_name}}"
    } else if inputs.contains(&"document_path") {
        "{{inputs.document_path}}"
    } else {
        "{{inputs.resource_name}}"
    };
    let mut primitives = vec![format!(
        "{{\"kind\":\"open_app\",\"app\":{{\"name\":\"{}\",\"bundle_id\":null,\"executable\":null,\"window_title\":\"{}\"}}}}",
        escape(app_name),
        escape(app_name)
    )];
    if lower.contains("spreadsheet") || lower.contains("document") || lower.contains("resource") {
        primitives.push(format!(
            "{{\"kind\":\"open_resource\",\"resource\":{{\"path_template\":\"{}\",\"resource_type\":\"{}\"}},\"create_if_missing\":true}}",
            escape(path_template),
            if lower.contains("spreadsheet") {
                "Spreadsheet"
            } else {
                "Document"
            }
        ));
    }
    let value_template = if lower.contains("calculator") {
        "{{inputs.number_1}} {{inputs.operation}} {{inputs.number_2}}"
    } else if lower.contains("email") && lower.contains("name") {
        "{{inputs.name}}\\t{{inputs.email}}"
    } else if inputs.contains(&"text_content") {
        "{{inputs.text_content}}"
    } else {
        "{{inputs.input}}"
    };
    primitives.push(format!(
        "{{\"kind\":\"enter_text\",\"target\":{{\"label\":\"active document\",\"role\":\"document\",\"text\":null,\"automation_id\":null,\"shortcut\":null}},\"value_template\":\"{}\"}}",
        escape(value_template)
    ));
    if lower.contains("save") || lower.contains("spreadsheet") || lower.contains("document") {
        primitives.push(format!(
            "{{\"kind\":\"save_resource\",\"path_template\":\"{}\",\"policy\":\"CreateOrUpdate\"}}",
            escape(path_template)
        ));
        primitives.push(format!(
            "{{\"kind\":\"assert_state\",\"condition\":{{\"ResourceExists\":{{\"path_template\":\"{}\"}}}}}}",
            escape(path_template)
        ));
    } else {
        primitives.push(
            "{\"kind\":\"observe_output\",\"name\":\"result\",\"extractor\":{\"target\":{\"label\":\"result\",\"role\":\"status\",\"text\":null,\"automation_id\":null,\"shortcut\":null},\"pattern\":null}}"
                .to_owned(),
        );
    }

    Some(format!(
        "{{\"id\":\"{}\",\"summary\":\"{}\",\"target\":{{\"kind\":{{\"NativeApp\":\"{}\"}},\"open\":{{\"App\":{{\"app_name\":\"{}\",\"window_title\":\"{}\"}}}}}},\"inputs\":[],\"primitives\":[{}],\"outputs\":[],\"assertions\":[],\"evidence_policy\":{{\"capture_steps\":true,\"capture_screenshots\":true}},\"metadata\":{{\"platform\":\"{}\"}}}}",
        escape(runner_id),
        escape(prompt),
        platform.0,
        escape(app_name),
        escape(app_name),
        primitives.join(","),
        platform.1
    ))
}

fn desktop_capability(request: &LlmRequestEnvelope) -> &'static str {
    let adapters = &request.context.available_adapters;
    if adapters
        .iter()
        .any(|adapter| adapter.contains("greentic.desktop.macos"))
    {
        "macos.activate_app"
    } else if adapters
        .iter()
        .any(|adapter| adapter.contains("greentic.desktop.windows"))
    {
        "windows.open_app"
    } else if adapters
        .iter()
        .any(|adapter| adapter.contains("greentic.desktop.linux"))
    {
        "linux.find_window"
    } else if adapters
        .iter()
        .any(|adapter| adapter.contains("greentic.desktop.java"))
    {
        "java.find_window"
    } else {
        "web.goto"
    }
}

fn mentions_sheet_name(prompt: &str) -> bool {
    prompt
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| matches!(part, "sheet" | "worksheet" | "tab"))
}

fn named_schema(names: &[&str]) -> String {
    let entries = names
        .iter()
        .map(|name| {
            format!(
                "\"{}\":{{\"type\":\"string\",\"required\":true}}",
                escape(name)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{entries}}}")
}

fn string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", escape(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn runner_id(prompt: &str) -> String {
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("crm") && lower.contains("customer") {
        return "crm.create_customer".to_owned();
    }
    prompt
        .split_whitespace()
        .take(4)
        .map(|word| {
            word.chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn renders_prompt_to_runner_envelope() {
        let request = LlmRequestEnvelope::prompt_to_runner(
            "Create a CRM customer",
            LlmPlanningContext {
                available_adapters: vec!["greentic.desktop.playwright".to_owned()],
                ..LlmPlanningContext::default()
            },
        );

        let json = request.render_json();
        assert!(json.contains("\"task\":\"desktop.prompt_to_runner\""));
        assert!(json.contains("greentic.desktop.playwright"));
    }

    #[test]
    fn heuristic_client_returns_structured_runner_json() {
        let client = HeuristicLlmClient;
        let response = client
            .complete(&LlmRequestEnvelope::prompt_to_runner(
                "Create CRM customer with company name and email and return customer id",
                LlmPlanningContext::default(),
            ))
            .expect("heuristic response");

        assert!(response
            .content
            .contains("\"runner_id\":\"crm.create_customer\""));
        assert!(response.content.contains("company_name"));
    }

    #[test]
    fn heuristic_client_extracts_calculator_inputs_and_result() {
        let client = HeuristicLlmClient;
        let response = client
            .complete(&LlmRequestEnvelope::prompt_to_runner(
                "open the calculator, let me introduce value 1 and value 2 as well as the operation, use the calculator to calculate and retrieve the result and provide that back",
                LlmPlanningContext {
                    available_adapters: vec!["greentic.desktop.macos".to_owned()],
                    ..LlmPlanningContext::default()
                },
            ))
            .expect("heuristic response");

        assert!(response.content.contains("\"number_1\""));
        assert!(response.content.contains("\"number_2\""));
        assert!(response.content.contains("\"operation\""));
        assert!(response.content.contains("\"result\""));
        assert!(response.content.contains("\"macos.activate_app\""));
        assert!(response.content.contains("\"open_questions\":[]"));
    }

    #[test]
    fn heuristic_client_extracts_generic_spreadsheet_resource_fields() {
        let client = HeuristicLlmClient;
        let response = client
            .complete(&LlmRequestEnvelope::prompt_to_runner(
                "Ask for the name of a spreadsheet. In /tmp create the spreadsheet if it does not exist already. Otherwise open it. Add a new line to the spreadsheet with the name and email that the user provided. Save the changes.",
                LlmPlanningContext {
                    available_adapters: vec!["greentic.desktop.excel".to_owned()],
                    ..LlmPlanningContext::default()
                },
            ))
            .expect("heuristic response");

        assert!(response.content.contains("\"xlsx_path\""));
        assert!(response.content.contains("\"name\""));
        assert!(response.content.contains("\"email\""));
        assert!(response.content.contains("\"saved_status\""));
        assert!(response.content.contains("excel.append_rows"));
        assert!(!response.content.contains("\"number_1\""));
        assert!(!response.content.contains("\"operation\""));
        assert!(response
            .content
            .contains("Which worksheet should the Excel runner use?"));
    }

    #[test]
    fn known_providers_include_remote_defaults() {
        let providers = known_providers();

        assert!(providers.iter().any(
            |provider| provider.id == "local" && provider.default_model == "heuristic-planner"
        ));
        assert!(providers.iter().any(|provider| provider.id == "deepseek"
            && provider.secret_name == Some("DEEPSEEK_API_KEY")));
        assert!(providers.iter().any(|provider| provider.id == "gemini"));
        assert!(providers.iter().any(|provider| provider.id == "openrouter"));
        assert!(providers.iter().any(|provider| provider.id == "llamafile"));
        assert_eq!(
            provider_by_id("openai").map(|provider| provider.default_model),
            Some("gpt-4.1-mini")
        );
        for kind in ProviderKind::all() {
            assert!(
                providers
                    .iter()
                    .any(|provider| provider.id == kind.as_str()),
                "missing greentic-llm provider {}",
                kind.as_str()
            );
            assert!(is_greentic_llm_provider(kind.as_str()));
        }
    }

    #[test]
    fn rig_backed_client_uses_provider_http_response_for_runner_fields() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock LLM should bind");
        let addr = listener.local_addr().expect("mock addr");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).expect("request should read");
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("desktop.prompt_to_runner"));
            assert!(request.contains("open the calculator"));
            assert!(request.contains("expected_json_schema"));
            assert!(
                request.contains("authorization: Bearer test-key")
                    || request.contains("Authorization: Bearer test-key")
            );
            let content = r#"{"runner_id":"calculator.from_llm","version":"0.1.0-draft","summary":"Calculator runner from LLM","risk_level":"low","required_capabilities":["web.goto","web.fill","web.click","web.extract_text"],"inputs":{"number_1":{"type":"number"},"number_2":{"type":"number"},"operation":{"type":"string"}},"outputs":{"result":{"type":"string"}},"steps":[{"id":"open-calculator","action":"goto","required_capability":"web.goto"},{"id":"fill-number-1","action":"fill","required_capability":"web.fill","value":"{{inputs.number_1}}"},{"id":"fill-number-2","action":"fill","required_capability":"web.fill","value":"{{inputs.number_2}}"},{"id":"read-result","action":"extract_text","required_capability":"web.extract_text"}],"assertions":["result is visible"],"open_questions":[]}"#;
            let body = serde_json::json!({
                "id": "chatcmpl-test",
                "object": "chat.completion",
                "created": 0,
                "model": "test-model",
                "system_fingerprint": null,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": content
                    },
                    "logprobs": null,
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 1,
                    "total_tokens": 2
                }
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should write");
        });

        let client = OpenAiCompatibleLlmClient::new(
            "openai",
            format!("http://{addr}/v1"),
            "test-model",
            Some("test-key".to_owned()),
        );
        let response = client
            .complete(&LlmRequestEnvelope::prompt_to_runner(
                "open the calculator and take number 1, number 2 and operation and return result",
                LlmPlanningContext {
                    available_adapters: vec!["greentic.desktop.playwright".to_owned()],
                    ..LlmPlanningContext::default()
                },
            ))
            .expect("mock LLM should respond");

        server.join().expect("server should finish");
        assert!(response.content.contains("calculator.from_llm"));
        assert!(response.content.contains("\"number_1\""));
        assert!(response.content.contains("\"result\""));
    }

    #[test]
    fn request_envelope_includes_runner_schema_and_generic_resource_example() {
        let envelope = LlmRequestEnvelope::prompt_to_runner(
            "open the local calculator app and return the result",
            LlmPlanningContext {
                available_adapters: vec!["greentic.desktop.macos".to_owned()],
                ..LlmPlanningContext::default()
            },
        );
        let rendered = envelope.render_json();

        assert!(rendered.contains("\"expected_json_schema\""));
        assert!(rendered.contains("\"valid_example\""));
        assert!(rendered.contains("\"id\""));
        assert!(rendered.contains("\"required_capability\""));
        assert!(rendered.contains("\"resource_name\""));
        assert!(rendered.contains("\"saved_status\""));
        assert!(!rendered.contains("\"number_1\""));
    }
}
