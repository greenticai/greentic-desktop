use serde_json::json;
use std::process::Command;

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
            id: "google",
            name: "Google Gemini",
            default_model: "gemini-1.5-flash",
            mode: "remote",
            endpoint: Some("https://generativelanguage.googleapis.com"),
            secret_name: Some("GOOGLE_API_KEY"),
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
            id: "ollama",
            name: "Ollama",
            default_model: "llama3.1",
            mode: "remote",
            endpoint: Some("http://127.0.0.1:11434"),
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
}

impl LlmRequestEnvelope {
    pub fn prompt_to_runner(prompt: impl Into<String>, context: LlmPlanningContext) -> Self {
        Self {
            task: "desktop.prompt_to_runner".to_owned(),
            model_policy: ModelPolicy::default(),
            context,
            user_prompt: prompt.into(),
            expected_json_schema: runner_draft_json_schema(),
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
        "steps": [
            {"id": "open-target-app", "action": "activate_app", "required_capability": "macos.activate_app", "value": "{{inputs.resource_name}}"},
            {"id": "enter-name", "action": "type_text", "required_capability": "macos.type_text", "value": "{{inputs.name}}"},
            {"id": "enter-email", "action": "type_text", "required_capability": "macos.type_text", "value": "{{inputs.email}}"},
            {"id": "save-resource", "action": "type_text", "required_capability": "macos.type_text", "value": "save"},
            {"id": "read-saved-status", "action": "read_text", "required_capability": "macos.read_text"}
        ],
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

    fn url(&self) -> String {
        format!("{}/chat/completions", self.endpoint.trim_end_matches('/'))
    }
}

impl GreenticLlmClient for OpenAiCompatibleLlmClient {
    fn complete(&self, request: &LlmRequestEnvelope) -> Result<LlmResponse, LlmError> {
        if !is_openai_compatible_provider(&self.provider_id) {
            return Err(LlmError::Unavailable(format!(
                "LLM provider '{}' is not wired to a live request format yet.",
                self.provider_id
            )));
        }

        let body = openai_compatible_body(&self.model, request)?;
        let mut command = Command::new("curl");
        command
            .arg("-fsS")
            .arg("-X")
            .arg("POST")
            .arg(self.url())
            .arg("-H")
            .arg("content-type: application/json")
            .arg("-d")
            .arg(body);
        if let Some(api_key) = &self.api_key {
            if !api_key.trim().is_empty() {
                command
                    .arg("-H")
                    .arg(format!("authorization: Bearer {api_key}"));
            }
        }
        let output = command
            .output()
            .map_err(|err| LlmError::Unavailable(format!("failed to start curl: {err}")))?;
        if !output.status.success() {
            return Err(LlmError::Unavailable(format!(
                "LLM request failed with status {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let raw = String::from_utf8_lossy(&output.stdout);
        let content = extract_openai_compatible_content(&raw)?;
        Ok(LlmResponse { content })
    }
}

pub fn is_openai_compatible_provider(provider_id: &str) -> bool {
    matches!(provider_id, "openai" | "deepseek" | "mistral" | "ollama")
}

fn openai_compatible_body(model: &str, request: &LlmRequestEnvelope) -> Result<String, LlmError> {
    serde_json::to_string(&json!({
        "model": model,
        "temperature": f64::from(request.model_policy.temperature_tenths) / 10.0,
        "response_format": {"type": "json_object"},
        "messages": [
            {
                "role": "system",
                "content": "You turn desktop automation prompts into strict runner JSON. Return only a JSON object that validates against expected_json_schema in the user message. Do not omit required fields. Every item in steps MUST include id, action, and required_capability. Inputs and outputs must be JSON objects keyed by field name."
            },
            {
                "role": "user",
                "content": request.render_json()
            }
        ]
    }))
    .map_err(|err| LlmError::Unavailable(format!("failed to render LLM request JSON: {err}")))
}

fn extract_openai_compatible_content(raw: &str) -> Result<String, LlmError> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|err| {
        LlmError::Unavailable(format!("LLM response was not valid JSON: {err}: {raw}"))
    })?;
    value
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            LlmError::Unavailable(format!(
                "LLM response did not contain choices[0].message.content: {raw}"
            ))
        })
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
        let capability = if lower.contains("terminal") || lower.contains("mainframe") {
            "terminal.read_screen"
        } else if lower.contains("calculator") || lower.contains("desktop") || lower.contains("app")
        {
            desktop_capability(request)
        } else {
            "web.goto"
        };
        let mut inputs = Vec::new();
        if lower.contains("spreadsheet") {
            inputs.push("spreadsheet_name");
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
        let open_questions = if lower.contains("spreadsheet") && !mentions_application(&lower) {
            vec!["Which application should open the spreadsheet if the OS default is not correct?"]
        } else if lower.contains("login") && !lower.contains("service account") {
            vec!["Which credentials or service account should be used?"]
        } else if inputs.is_empty() {
            vec!["Which input values should the runner require?"]
        } else {
            Vec::new()
        };

        Ok(LlmResponse {
            content: format!(
                "{{\"runner_id\":\"{}\",\"version\":\"0.1.0-draft\",\"summary\":\"{}\",\"risk_level\":\"{}\",\"required_capabilities\":[\"{}\"],\"inputs\":{},\"outputs\":{},\"steps\":[{{\"id\":\"draft_1\",\"action\":\"plan\",\"required_capability\":\"{}\"}}],\"assertions\":[\"no unexpected errors\"],\"open_questions\":{}}}",
                escape(&runner_id),
                escape(&request.user_prompt),
                risk,
                capability,
                named_schema(&inputs),
                named_schema(&outputs),
                capability,
                string_array(&open_questions.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>())
            ),
        })
    }
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

fn mentions_application(prompt: &str) -> bool {
    prompt.contains("excel")
        || prompt.contains("libreoffice")
        || prompt.contains("numbers")
        || prompt.contains("application")
        || prompt.contains(" app")
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
                    available_adapters: vec!["greentic.desktop.macos".to_owned()],
                    ..LlmPlanningContext::default()
                },
            ))
            .expect("heuristic response");

        assert!(response.content.contains("\"spreadsheet_name\""));
        assert!(response.content.contains("\"name\""));
        assert!(response.content.contains("\"email\""));
        assert!(response.content.contains("\"saved_status\""));
        assert!(!response.content.contains("\"number_1\""));
        assert!(!response.content.contains("\"operation\""));
        assert!(response
            .content
            .contains("Which application should open the spreadsheet"));
    }

    #[test]
    fn known_providers_include_remote_defaults() {
        let providers = known_providers();

        assert!(providers.iter().any(
            |provider| provider.id == "local" && provider.default_model == "heuristic-planner"
        ));
        assert!(providers.iter().any(|provider| provider.id == "deepseek"
            && provider.secret_name == Some("DEEPSEEK_API_KEY")));
        assert_eq!(
            provider_by_id("openai").map(|provider| provider.default_model),
            Some("gpt-4.1-mini")
        );
    }

    #[test]
    fn openai_compatible_client_uses_http_response_for_runner_fields() {
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
            assert!(request.contains(
                "\\\"required\\\":[\\\"id\\\",\\\"action\\\",\\\"required_capability\\\"]"
            ));
            let content = r#"{"runner_id":"calculator.from_llm","version":"0.1.0-draft","summary":"Calculator runner from LLM","risk_level":"low","required_capabilities":["web.goto","web.fill","web.click","web.extract_text"],"inputs":{"number_1":{"type":"number"},"number_2":{"type":"number"},"operation":{"type":"string"}},"outputs":{"result":{"type":"string"}},"steps":[{"id":"open-calculator","action":"goto","required_capability":"web.goto"},{"id":"fill-number-1","action":"fill","required_capability":"web.fill","value":"{{inputs.number_1}}"},{"id":"fill-number-2","action":"fill","required_capability":"web.fill","value":"{{inputs.number_2}}"},{"id":"read-result","action":"extract_text","required_capability":"web.extract_text"}],"assertions":["result is visible"],"open_questions":[]}"#;
            let body = serde_json::json!({
                "choices": [{"message": {"content": content}}]
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
