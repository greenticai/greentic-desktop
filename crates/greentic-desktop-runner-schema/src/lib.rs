use greentic_desktop_adapter::{LocatorStrategy, LocatorTarget, RunnerStep};
use greentic_desktop_core::RiskLevel;
use greentic_desktop_recorder::{RecordingMode, RunnerPackage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerDraftDocument {
    pub runner_id: String,
    pub version: String,
    pub summary: String,
    pub risk_level: RiskLevel,
    pub required_capabilities: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub steps: Vec<RunnerStep>,
    pub assertions: Vec<String>,
    pub open_questions: Vec<String>,
}

impl RunnerDraftDocument {
    pub fn into_package(self) -> RunnerPackage {
        RunnerPackage {
            id: self.runner_id,
            version: self.version,
            mode: RecordingMode::AssistedPrompt,
            inputs: self
                .inputs
                .into_iter()
                .map(|input| format!("inputs.{input}"))
                .collect(),
            secrets: Vec::new(),
            steps: self.steps,
            assertions: self.assertions,
            outputs: self
                .outputs
                .into_iter()
                .map(|output| format!("outputs.{output}"))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiagnostic {
    pub code: String,
    pub message: String,
}

pub fn parse_runner_draft_json(raw: &str) -> Result<RunnerDraftDocument, SchemaDiagnostic> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err(diagnostic(
            "planner.invalid_json",
            "LLM output is not a JSON object",
        ));
    }

    let runner_id = string_field(trimmed, "runner_id")?;
    let version = string_field(trimmed, "version")?;
    let summary = string_field(trimmed, "summary")?;
    let risk_level = parse_risk(&string_field(trimmed, "risk_level")?)?;
    let required_capabilities = string_array_field(trimmed, "required_capabilities")?;
    let inputs = object_keys_field(trimmed, "inputs")?;
    let outputs = object_keys_field(trimmed, "outputs")?;
    let assertions = string_array_field(trimmed, "assertions").unwrap_or_default();
    let open_questions = string_array_field(trimmed, "open_questions").unwrap_or_default();
    let steps = parse_steps(trimmed).unwrap_or_else(|| {
        required_capabilities
            .iter()
            .enumerate()
            .map(|(index, capability)| RunnerStep {
                id: format!("draft_{}", index + 1),
                action: capability
                    .rsplit('.')
                    .next()
                    .unwrap_or("execute")
                    .to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: capability.clone(),
            })
            .collect()
    });

    let document = RunnerDraftDocument {
        runner_id,
        version,
        summary,
        risk_level,
        required_capabilities,
        inputs,
        outputs,
        steps,
        assertions,
        open_questions,
    };
    validate_runner_draft(&document)?;
    Ok(document)
}

pub fn validate_runner_draft(document: &RunnerDraftDocument) -> Result<(), SchemaDiagnostic> {
    if document.runner_id.trim().is_empty() {
        return Err(diagnostic(
            "planner.schema_mismatch",
            "runner_id must not be empty",
        ));
    }
    if document.version.trim().is_empty() {
        return Err(diagnostic(
            "planner.schema_mismatch",
            "version must not be empty",
        ));
    }
    if document.required_capabilities.is_empty() {
        return Err(diagnostic(
            "planner.schema_mismatch",
            "required_capabilities must not be empty",
        ));
    }
    if document.steps.is_empty() && document.open_questions.is_empty() {
        return Err(diagnostic(
            "planner.needs_clarification",
            "draft must include steps or open questions",
        ));
    }
    for capability in &document.required_capabilities {
        if !capability.contains('.') {
            return Err(diagnostic(
                "planner.schema_mismatch",
                "required capabilities must be namespaced",
            ));
        }
    }
    for step in &document.steps {
        if step.id.trim().is_empty() || step.required_capability.trim().is_empty() {
            return Err(diagnostic(
                "planner.schema_mismatch",
                "steps must include id and required_capability",
            ));
        }
    }
    Ok(())
}

fn parse_risk(value: &str) -> Result<RiskLevel, SchemaDiagnostic> {
    match value {
        "low" | "Low" => Ok(RiskLevel::Low),
        "medium" | "Medium" => Ok(RiskLevel::Medium),
        "high" | "High" => Ok(RiskLevel::High),
        "critical" | "Critical" => Ok(RiskLevel::Critical),
        _ => Err(diagnostic(
            "planner.schema_mismatch",
            "risk_level must be low, medium, high, or critical",
        )),
    }
}

fn parse_steps(raw: &str) -> Option<Vec<RunnerStep>> {
    let body = array_body(raw, "steps")?;
    if body.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut steps = Vec::new();
    for item in split_objects(&body) {
        let id = string_field(&item, "id").ok()?;
        let action = string_field(&item, "action").unwrap_or_else(|_| "execute".to_owned());
        let required_capability = string_field(&item, "required_capability").ok()?;
        steps.push(RunnerStep {
            id: id.clone(),
            action,
            target: LocatorTarget {
                preferred: Some(LocatorStrategy {
                    name: Some(id),
                    ..LocatorStrategy::default()
                }),
                ..LocatorTarget::default()
            },
            value: string_field(&item, "value").ok(),
            required_capability,
        });
    }
    Some(steps)
}

fn string_field(raw: &str, field: &str) -> Result<String, SchemaDiagnostic> {
    let needle = format!("\"{field}\"");
    let start = raw
        .find(&needle)
        .ok_or_else(|| diagnostic("planner.schema_mismatch", &format!("missing {field}")))?;
    let after_key = &raw[start + needle.len()..];
    let colon = after_key
        .find(':')
        .ok_or_else(|| diagnostic("planner.invalid_json", "missing field separator"))?;
    let after_colon = after_key[colon + 1..].trim_start();
    if !after_colon.starts_with('"') {
        return Err(diagnostic(
            "planner.schema_mismatch",
            &format!("{field} must be a string"),
        ));
    }
    read_string(after_colon).ok_or_else(|| diagnostic("planner.invalid_json", "invalid string"))
}

fn string_array_field(raw: &str, field: &str) -> Result<Vec<String>, SchemaDiagnostic> {
    let body = array_body(raw, field)
        .ok_or_else(|| diagnostic("planner.schema_mismatch", &format!("missing {field}")))?;
    Ok(read_strings(&body))
}

fn object_keys_field(raw: &str, field: &str) -> Result<Vec<String>, SchemaDiagnostic> {
    let body = object_body(raw, field)
        .ok_or_else(|| diagnostic("planner.schema_mismatch", &format!("missing {field}")))?;
    let mut keys = Vec::new();
    let bytes = body.as_bytes();
    let mut index = 0;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if in_string {
            escaped = ch == '\\' && !escaped;
            if ch == '"' && !escaped {
                in_string = false;
            }
            if ch != '\\' {
                escaped = false;
            }
            index += 1;
            continue;
        }
        match ch {
            '"' if depth == 0 => {
                let rest = &body[index..];
                if let Some(key) = read_string(rest) {
                    let after_index = index + key.len() + 2;
                    if body[after_index..].trim_start().starts_with(':') {
                        keys.push(key);
                    }
                    index = after_index;
                    continue;
                }
                in_string = true;
            }
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        index += 1;
    }
    Ok(keys)
}

fn array_body(raw: &str, field: &str) -> Option<String> {
    delimited_field(raw, field, '[', ']')
}

fn object_body(raw: &str, field: &str) -> Option<String> {
    delimited_field(raw, field, '{', '}')
}

fn delimited_field(raw: &str, field: &str, open: char, close: char) -> Option<String> {
    let needle = format!("\"{field}\"");
    let start = raw.find(&needle)?;
    let after_key = &raw[start + needle.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    let offset = after_colon.find(open)?;
    let value = &after_colon[offset..];
    read_delimited(value, open, close)
}

fn read_delimited(raw: &str, open: char, close: char) -> Option<String> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in raw.char_indices() {
        if in_string {
            escaped = ch == '\\' && !escaped;
            if ch == '"' && !escaped {
                in_string = false;
            }
            if ch != '\\' {
                escaped = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
        } else if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(raw[1..index].to_owned());
            }
        }
    }
    None
}

fn read_strings(body: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = body;
    while let Some(index) = rest.find('"') {
        rest = &rest[index..];
        if let Some(value) = read_string(rest) {
            rest = &rest[value.len() + 2..];
            values.push(value);
        } else {
            break;
        }
    }
    values
}

fn read_string(raw: &str) -> Option<String> {
    let mut value = String::new();
    let mut chars = raw.chars();
    if chars.next()? != '"' {
        return None;
    }
    let mut escaped = false;
    for ch in chars {
        if escaped {
            value.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(value);
        } else {
            value.push(ch);
        }
    }
    None
}

fn split_objects(body: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find('{') {
        rest = &rest[start..];
        if let Some(object) = read_delimited(rest, '{', '}') {
            let full = format!("{{{object}}}");
            let advance = object.len() + 2;
            objects.push(full);
            rest = &rest[advance..];
        } else {
            break;
        }
    }
    objects
}

fn diagnostic(code: &str, message: &str) -> SchemaDiagnostic {
    SchemaDiagnostic {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> &'static str {
        r#"{
            "runner_id": "crm.create_customer",
            "version": "0.1.0-draft",
            "summary": "Create a customer",
            "risk_level": "medium",
            "required_capabilities": ["web.goto", "web.fill"],
            "inputs": {"company_name": {"type": "string", "required": true}},
            "outputs": {"customer_id": {"type": "string"}},
            "steps": [{"id": "open", "action": "goto", "required_capability": "web.goto"}],
            "assertions": ["customer is visible"],
            "open_questions": []
        }"#
    }

    #[test]
    fn parses_valid_runner_draft_json() {
        let draft = parse_runner_draft_json(valid_json()).expect("valid draft");

        assert_eq!(draft.runner_id, "crm.create_customer");
        assert_eq!(draft.risk_level, RiskLevel::Medium);
        assert_eq!(draft.inputs, vec!["company_name"]);
        assert_eq!(draft.steps[0].required_capability, "web.goto");
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_runner_draft_json("not json").expect_err("invalid");
        assert_eq!(err.code, "planner.invalid_json");
    }

    #[test]
    fn rejects_schema_invalid_response() {
        let err = parse_runner_draft_json(r#"{"runner_id":""}"#).expect_err("invalid");
        assert_eq!(err.code, "planner.schema_mismatch");
    }
}
