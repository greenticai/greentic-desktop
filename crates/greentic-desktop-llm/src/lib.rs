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
}

impl LlmRequestEnvelope {
    pub fn prompt_to_runner(prompt: impl Into<String>, context: LlmPlanningContext) -> Self {
        Self {
            task: "desktop.prompt_to_runner".to_owned(),
            model_policy: ModelPolicy::default(),
            context,
            user_prompt: prompt.into(),
        }
    }

    pub fn repair_prompt_to_runner(
        prompt: impl AsRef<str>,
        context: LlmPlanningContext,
        previous_output: impl AsRef<str>,
        diagnostics: impl AsRef<str>,
    ) -> Self {
        Self {
            task: "desktop.prompt_to_runner.repair".to_owned(),
            model_policy: ModelPolicy::default(),
            context,
            user_prompt: format!(
                "Original request:\n{}\n\nPrevious invalid JSON:\n{}\n\nValidation diagnostics:\n{}\n\nReturn a complete corrected JSON object matching the Greentic runner draft schema.",
                prompt.as_ref(),
                previous_output.as_ref(),
                diagnostics.as_ref()
            ),
        }
    }

    pub fn render_json(&self) -> String {
        format!(
            "{{\"task\":\"{}\",\"model_policy\":{{\"temperature\":0.{},\"response_format\":\"{}\",\"max_retries\":{}}},\"context\":{{\"available_adapters\":{},\"available_mcp_tools\":{},\"session_profiles\":{},\"existing_runners\":{},\"ltm_examples\":{},\"security_policy\":{},\"desktop_observation\":{}}},\"user_prompt\":\"{}\"}}",
            escape(&self.task),
            self.model_policy.temperature_tenths,
            escape(&self.model_policy.response_format),
            self.model_policy.max_retries,
            string_array(&self.context.available_adapters),
            string_array(&self.context.available_mcp_tools),
            string_array(&self.context.session_profiles),
            string_array(&self.context.existing_runners),
            string_array(&self.context.ltm_examples),
            string_array(&self.context.security_policy),
            string_array(&self.context.desktop_observation),
            escape(&self.user_prompt)
        )
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderProfile {
    pub id: &'static str,
    pub label: &'static str,
    pub default_model: &'static str,
    pub endpoint: Option<&'static str>,
    pub requires_api_key: bool,
}

pub fn supported_provider_profiles() -> Vec<LlmProviderProfile> {
    vec![
        LlmProviderProfile {
            id: "local",
            label: "Local heuristic",
            default_model: "heuristic-planner",
            endpoint: None,
            requires_api_key: false,
        },
        LlmProviderProfile {
            id: "openai",
            label: "OpenAI",
            default_model: "gpt-4.1-mini",
            endpoint: Some("https://api.openai.com/v1"),
            requires_api_key: true,
        },
        LlmProviderProfile {
            id: "anthropic",
            label: "Anthropic",
            default_model: "claude-3-5-sonnet-latest",
            endpoint: Some("https://api.anthropic.com"),
            requires_api_key: true,
        },
        LlmProviderProfile {
            id: "azure_openai",
            label: "Azure OpenAI",
            default_model: "gpt-4.1-mini",
            endpoint: None,
            requires_api_key: true,
        },
        LlmProviderProfile {
            id: "google",
            label: "Google Gemini",
            default_model: "gemini-1.5-pro",
            endpoint: Some("https://generativelanguage.googleapis.com"),
            requires_api_key: true,
        },
        LlmProviderProfile {
            id: "mistral",
            label: "Mistral",
            default_model: "mistral-large-latest",
            endpoint: Some("https://api.mistral.ai/v1"),
            requires_api_key: true,
        },
        LlmProviderProfile {
            id: "deepseek",
            label: "DeepSeek",
            default_model: "deepseek-chat",
            endpoint: Some("https://api.deepseek.com/v1"),
            requires_api_key: true,
        },
        LlmProviderProfile {
            id: "openai_compatible",
            label: "OpenAI compatible",
            default_model: "gpt-4o-mini",
            endpoint: Some("http://127.0.0.1:8000/v1"),
            requires_api_key: false,
        },
        LlmProviderProfile {
            id: "nvidia_nim",
            label: "NVIDIA NIM",
            default_model: "meta/llama-3.1-70b-instruct",
            endpoint: Some("https://integrate.api.nvidia.com/v1"),
            requires_api_key: true,
        },
        LlmProviderProfile {
            id: "ollama",
            label: "Ollama",
            default_model: "llama3.1",
            endpoint: Some("http://127.0.0.1:11434"),
            requires_api_key: false,
        },
    ]
}

pub fn provider_profile(id: &str) -> Option<LlmProviderProfile> {
    supported_provider_profiles()
        .into_iter()
        .find(|profile| profile.id == id)
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

#[derive(Debug)]
pub struct SequenceLlmClient {
    responses: std::sync::Mutex<Vec<Result<LlmResponse, LlmError>>>,
}

impl SequenceLlmClient {
    pub fn new(contents: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            responses: std::sync::Mutex::new(
                contents
                    .into_iter()
                    .map(|content| {
                        Ok(LlmResponse {
                            content: content.into(),
                        })
                    })
                    .collect(),
            ),
        }
    }
}

impl GreenticLlmClient for SequenceLlmClient {
    fn complete(&self, _request: &LlmRequestEnvelope) -> Result<LlmResponse, LlmError> {
        let mut responses = self.responses.lock().expect("sequence llm mutex poisoned");
        if responses.is_empty() {
            return Err(LlmError::Unavailable(
                "sequence LLM has no responses left".to_owned(),
            ));
        }
        responses.remove(0)
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
        let calculator_request = lower.contains("calculator")
            || ((lower.contains("number 1") || lower.contains("two numbers"))
                && lower.contains("operation"));
        let capability = if calculator_request {
            native_planner_capability(&request.context)
        } else if lower.contains("terminal") || lower.contains("mainframe") {
            "terminal.read_screen"
        } else {
            "web.goto"
        };
        let mut inputs = Vec::new();
        if calculator_request {
            inputs.push("number_1");
            inputs.push("number_2");
            inputs.push("operation");
        }
        if lower.contains("company") {
            inputs.push("company_name");
        }
        if lower.contains("email") {
            inputs.push("email");
        }
        if inputs.is_empty() && lower.contains("customer") {
            inputs.push("customer_name");
        }
        let outputs = if calculator_request
            || lower.contains("return result")
            || lower.contains("return the result")
        {
            vec!["result"]
        } else if lower.contains("customer id") || lower.contains("customer_id") {
            vec!["customer_id"]
        } else {
            Vec::new()
        };
        let open_questions = if lower.contains("login") && !lower.contains("service account") {
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

fn native_planner_capability(context: &LlmPlanningContext) -> &'static str {
    if context
        .available_adapters
        .iter()
        .any(|adapter| adapter.contains("macos"))
    {
        "macos.activate_app"
    } else if context
        .available_adapters
        .iter()
        .any(|adapter| adapter.contains("linux"))
    {
        "linux.find_window"
    } else if context
        .available_adapters
        .iter()
        .any(|adapter| adapter.contains("windows"))
    {
        "windows.open_app"
    } else {
        "vision.screenshot"
    }
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
    fn heuristic_client_derives_calculator_inputs_and_result_output() {
        let client = HeuristicLlmClient;
        let response = client
            .complete(&LlmRequestEnvelope::prompt_to_runner(
                "open the calculator app. Take three inputs: two numbers and one operation (plus, minus, divide or multiply) and make the calculator do the operation and return the result",
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
    }

    #[test]
    fn exposes_supported_provider_profiles() {
        let providers = supported_provider_profiles();
        assert!(providers.iter().any(|provider| provider.id == "local"));
        assert!(providers
            .iter()
            .any(|provider| provider.id == "openai" && provider.requires_api_key));
        assert!(
            providers
                .iter()
                .any(|provider| provider.id == "deepseek"
                    && provider.default_model == "deepseek-chat")
        );
        assert!(provider_profile("openai_compatible").is_some());
        assert!(provider_profile("nvidia_nim").is_some());
        assert!(provider_profile("ollama").is_some());
    }
}
