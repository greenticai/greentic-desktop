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
        } else {
            "web.goto"
        };
        let mut inputs = Vec::new();
        if lower.contains("company") {
            inputs.push("company_name");
        }
        if lower.contains("email") {
            inputs.push("email");
        }
        if inputs.is_empty() && lower.contains("customer") {
            inputs.push("customer_name");
        }
        let outputs = if lower.contains("customer id") || lower.contains("customer_id") {
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
}
