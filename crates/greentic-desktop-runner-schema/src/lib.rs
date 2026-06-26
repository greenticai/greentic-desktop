use greentic_desktop_adapter::{LocatorStrategy, LocatorTarget, RunnerStep};
use greentic_desktop_core::RiskLevel;
use greentic_desktop_recorder::{RecordingMode, RunnerPackage};
use greentic_desktop_workflow::{
    compile_workflow, CompiledWorkflowOutput, DesktopWorkflow, WorkflowActionKind,
    WorkflowCompileError, WorkflowOutputExtractor, WorkflowRisk, WorkflowValueType,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerDefinition {
    pub runner_id: String,
    pub version: String,
    pub summary: String,
    pub intent: String,
    pub risk: RunnerRisk,
    pub target_technologies: Vec<TargetTechnology>,
    pub inputs: Vec<RunnerInput>,
    pub secrets: Vec<RunnerSecret>,
    pub workflow: DesktopWorkflow,
    pub outputs: Vec<RunnerOutput>,
    pub assertions: Vec<RunnerAssertion>,
    pub evidence_policy: RunnerEvidencePolicy,
    pub approval_policy: RunnerApprovalPolicy,
    pub compiled_steps: Vec<RunnerStep>,
    pub compiled_outputs: Vec<CompiledWorkflowOutput>,
}

impl RunnerDefinition {
    pub fn from_workflow(
        runner_id: impl Into<String>,
        version: impl Into<String>,
        summary: impl Into<String>,
        intent: impl Into<String>,
        risk: RunnerRisk,
        target_technologies: Vec<TargetTechnology>,
        workflow: DesktopWorkflow,
    ) -> Result<Self, WorkflowCompileError> {
        let compiled = compile_workflow(&workflow)?;
        let inputs = workflow
            .inputs
            .iter()
            .filter(|input| !input.secret)
            .map(RunnerInput::from_workflow_input)
            .collect();
        let secrets = workflow
            .inputs
            .iter()
            .filter(|input| input.secret)
            .map(RunnerSecret::from_workflow_input)
            .collect();
        let outputs = workflow
            .outputs
            .iter()
            .map(RunnerOutput::from_workflow_output)
            .collect();
        let assertions = workflow
            .assertions
            .iter()
            .map(|assertion| RunnerAssertion {
                name: assertion.name.clone(),
                expected: assertion.expected.clone(),
                capability_hint: assertion.capability_hint.clone(),
            })
            .collect();

        Ok(Self {
            runner_id: runner_id.into(),
            version: version.into(),
            summary: summary.into(),
            intent: intent.into(),
            risk,
            target_technologies,
            inputs,
            secrets,
            workflow,
            outputs,
            assertions,
            evidence_policy: RunnerEvidencePolicy::default(),
            approval_policy: RunnerApprovalPolicy::for_risk(risk),
            compiled_steps: compiled.steps,
            compiled_outputs: compiled.outputs,
        })
    }

    pub fn refresh_compiled_steps(&mut self) -> Result<(), WorkflowCompileError> {
        let compiled = compile_workflow(&self.workflow)?;
        self.compiled_steps = compiled.steps;
        self.compiled_outputs = compiled.outputs;
        Ok(())
    }

    pub fn input_schema(&self) -> Vec<RunnerSchemaField> {
        self.inputs
            .iter()
            .map(|input| RunnerSchemaField {
                name: input.name.clone(),
                value_type: input.value_type.clone(),
                required: input.required,
                secret: false,
                default_value: input.default_value.clone(),
                enum_values: enum_values(&input.value_type),
                validation: input.validation.clone(),
            })
            .chain(self.secrets.iter().map(|secret| RunnerSchemaField {
                name: secret.name.clone(),
                value_type: secret.value_type.clone(),
                required: secret.required,
                secret: true,
                default_value: None,
                enum_values: enum_values(&secret.value_type),
                validation: secret.validation.clone(),
            }))
            .collect()
    }

    pub fn output_schema(&self) -> Vec<RunnerSchemaField> {
        self.outputs
            .iter()
            .map(|output| RunnerSchemaField {
                name: output.name.clone(),
                value_type: output.value_type.clone(),
                required: output.required,
                secret: false,
                default_value: None,
                enum_values: enum_values(&output.value_type),
                validation: None,
            })
            .collect()
    }

    pub fn into_package(self) -> RunnerPackage {
        RunnerPackage {
            id: self.runner_id,
            version: self.version,
            mode: RecordingMode::AssistedPrompt,
            inputs: self
                .inputs
                .into_iter()
                .map(|input| format!("inputs.{}", input.name))
                .collect(),
            secrets: self
                .secrets
                .into_iter()
                .map(|secret| format!("secrets.{}", secret.name))
                .collect(),
            steps: self.compiled_steps,
            assertions: self
                .assertions
                .into_iter()
                .map(|assertion| assertion.name)
                .collect(),
            outputs: self
                .outputs
                .into_iter()
                .map(|output| format!("outputs.{}", output.name))
                .collect(),
            open_questions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunnerRisk {
    Low,
    Medium,
    High,
    Critical,
}

impl RunnerRisk {
    pub fn from_workflow_risk(risk: WorkflowRisk) -> Self {
        match risk {
            WorkflowRisk::Low => Self::Low,
            WorkflowRisk::Medium => Self::Medium,
            WorkflowRisk::High => Self::High,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetTechnology {
    Web,
    NativeMacOs,
    NativeLinuxX11,
    NativeWindows,
    Java,
    Terminal,
    Vision,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerInput {
    pub name: String,
    pub value_type: WorkflowValueType,
    pub required: bool,
    pub default_value: Option<String>,
    pub redaction: RedactionPolicy,
    pub validation: Option<String>,
}

impl RunnerInput {
    fn from_workflow_input(input: &greentic_desktop_workflow::WorkflowInput) -> Self {
        Self {
            name: input.name.clone(),
            value_type: input.value_type.clone(),
            required: input.required,
            default_value: None,
            redaction: RedactionPolicy::None,
            validation: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerSecret {
    pub name: String,
    pub value_type: WorkflowValueType,
    pub required: bool,
    pub redaction: RedactionPolicy,
    pub validation: Option<String>,
}

impl RunnerSecret {
    fn from_workflow_input(input: &greentic_desktop_workflow::WorkflowInput) -> Self {
        Self {
            name: input.name.clone(),
            value_type: input.value_type.clone(),
            required: input.required,
            redaction: RedactionPolicy::Secret,
            validation: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedactionPolicy {
    None,
    Mask,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerOutput {
    pub name: String,
    pub value_type: WorkflowValueType,
    pub required: bool,
    pub extractor: WorkflowOutputExtractor,
    pub failure_behavior: OutputFailureBehavior,
}

impl RunnerOutput {
    fn from_workflow_output(output: &greentic_desktop_workflow::WorkflowOutput) -> Self {
        Self {
            name: output.name.clone(),
            value_type: output.value_type.clone(),
            required: output.required,
            extractor: output.extractor.clone(),
            failure_behavior: if output.required {
                OutputFailureBehavior::FailRunner
            } else {
                OutputFailureBehavior::OmitOutput
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFailureBehavior {
    FailRunner,
    OmitOutput,
    ReturnNull,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerAssertion {
    pub name: String,
    pub expected: String,
    pub capability_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerEvidencePolicy {
    pub capture_steps: bool,
    pub capture_screenshots: bool,
    pub retain_success_evidence: bool,
}

impl Default for RunnerEvidencePolicy {
    fn default() -> Self {
        Self {
            capture_steps: true,
            capture_screenshots: false,
            retain_success_evidence: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerApprovalPolicy {
    pub require_before_run: bool,
    pub require_before_submit: bool,
    pub reason: Option<String>,
}

impl RunnerApprovalPolicy {
    pub fn for_risk(risk: RunnerRisk) -> Self {
        match risk {
            RunnerRisk::Low => Self {
                require_before_run: false,
                require_before_submit: false,
                reason: None,
            },
            RunnerRisk::Medium => Self {
                require_before_run: false,
                require_before_submit: true,
                reason: Some("medium risk submit actions require approval".to_owned()),
            },
            RunnerRisk::High | RunnerRisk::Critical => Self {
                require_before_run: true,
                require_before_submit: true,
                reason: Some("high risk runners require explicit approval".to_owned()),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerSchemaField {
    pub name: String,
    pub value_type: WorkflowValueType,
    pub required: bool,
    pub secret: bool,
    pub default_value: Option<String>,
    pub enum_values: Vec<String>,
    pub validation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpInputSchema {
    pub fields: Vec<RunnerSchemaField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpOutputSchema {
    pub fields: Vec<RunnerSchemaField>,
}

impl McpInputSchema {
    pub fn from_runner(runner: &RunnerDefinition) -> Self {
        Self {
            fields: runner.input_schema(),
        }
    }

    pub fn to_json_schema(&self) -> String {
        fields_to_json_schema("MCP runner input", &self.fields)
    }
}

impl McpOutputSchema {
    pub fn from_runner(runner: &RunnerDefinition) -> Self {
        Self {
            fields: runner.output_schema(),
        }
    }

    pub fn to_json_schema(&self) -> String {
        fields_to_json_schema("MCP runner output", &self.fields)
    }
}

pub fn runner_draft_json_schema() -> String {
    serde_json::json!({
        "type": "object",
        "required": ["runner_id", "version", "summary", "risk_level", "required_capabilities", "inputs", "outputs"],
        "properties": {
            "runner_id": {"type": "string", "minLength": 1},
            "version": {"type": "string", "minLength": 1},
            "summary": {"type": "string"},
            "risk_level": {"type": "string", "enum": ["low", "medium", "high", "critical", "Low", "Medium", "High", "Critical"]},
            "required_capabilities": {"type": "array", "items": {"type": "string"}},
            "inputs": {"type": "object"},
            "outputs": {"type": "object"},
            "steps": {"type": "array", "items": runner_step_schema_value()},
            "assertions": {"type": "array", "items": {"type": "string"}},
            "open_questions": {"type": "array", "items": {"type": "string"}}
        }
    })
    .to_string()
}

pub fn runner_definition_json_schema() -> String {
    serde_json::json!({
        "type": "object",
        "required": ["runner_id", "version", "summary", "intent", "risk", "target_technologies", "workflow", "compiled_steps"],
        "properties": {
            "runner_id": {"type": "string"},
            "version": {"type": "string"},
            "summary": {"type": "string"},
            "intent": {"type": "string"},
            "risk": {"type": "string"},
            "target_technologies": {"type": "array", "items": {"type": "string"}},
            "inputs": {"type": "array"},
            "secrets": {"type": "array"},
            "workflow": {"type": "object"},
            "outputs": {"type": "array"},
            "assertions": {"type": "array"},
            "evidence_policy": {"type": "object"},
            "approval_policy": {"type": "object"},
            "compiled_steps": {"type": "array", "items": runner_step_schema_value()},
            "compiled_outputs": {"type": "array"}
        }
    })
    .to_string()
}

fn fields_to_json_schema(title: &str, fields: &[RunnerSchemaField]) -> String {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for field in fields {
        if field.required {
            required.push(serde_json::Value::String(field.name.clone()));
        }
        properties.insert(field.name.clone(), field_schema_value(field));
    }
    serde_json::json!({
        "title": title,
        "type": "object",
        "properties": properties,
        "required": required
    })
    .to_string()
}

fn field_schema_value(field: &RunnerSchemaField) -> serde_json::Value {
    let mut schema = match &field.value_type {
        WorkflowValueType::String | WorkflowValueType::Date | WorkflowValueType::File => {
            serde_json::json!({"type": "string"})
        }
        WorkflowValueType::Number => serde_json::json!({"type": "number"}),
        WorkflowValueType::Boolean => serde_json::json!({"type": "boolean"}),
        WorkflowValueType::Enum(values) => serde_json::json!({"type": "string", "enum": values}),
        WorkflowValueType::Json => serde_json::json!({}),
    };
    if field.secret {
        schema["writeOnly"] = serde_json::Value::Bool(true);
    }
    if let Some(default_value) = &field.default_value {
        schema["default"] = serde_json::Value::String(default_value.clone());
    }
    schema
}

fn runner_step_schema_value() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["id", "required_capability"],
        "properties": {
            "id": {"type": "string"},
            "action": {"type": "string"},
            "required_capability": {"type": "string"},
            "target": {"type": "object"},
            "value": {"type": ["string", "null"]}
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticRunnerStep {
    Open,
    Attach,
    Observe,
    Find,
    Input,
    Click,
    Key,
    Wait,
    Extract,
    Assert,
    Screenshot,
    Download,
    Close,
}

impl From<&WorkflowActionKind> for SemanticRunnerStep {
    fn from(value: &WorkflowActionKind) -> Self {
        match value {
            WorkflowActionKind::Open => Self::Open,
            WorkflowActionKind::Attach => Self::Attach,
            WorkflowActionKind::Observe => Self::Observe,
            WorkflowActionKind::Find => Self::Find,
            WorkflowActionKind::Input => Self::Input,
            WorkflowActionKind::Click => Self::Click,
            WorkflowActionKind::Key => Self::Key,
            WorkflowActionKind::Wait => Self::Wait,
            WorkflowActionKind::Extract => Self::Extract,
            WorkflowActionKind::Assert => Self::Assert,
            WorkflowActionKind::Screenshot => Self::Screenshot,
            WorkflowActionKind::Download => Self::Download,
            WorkflowActionKind::Close => Self::Close,
            WorkflowActionKind::AdapterCapability(_) => Self::Observe,
        }
    }
}

fn enum_values(value_type: &WorkflowValueType) -> Vec<String> {
    match value_type {
        WorkflowValueType::Enum(values) => values.clone(),
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            open_questions: self.open_questions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiagnostic {
    pub code: String,
    pub message: String,
}

pub fn parse_runner_draft_json(raw: &str) -> Result<RunnerDraftDocument, SchemaDiagnostic> {
    let parsed: JsonRunnerDraftDocument = serde_json::from_str(raw.trim()).map_err(|err| {
        if err.is_data() {
            diagnostic(
                "planner.schema_mismatch",
                &format!("runner JSON schema mismatch: {err}"),
            )
        } else {
            diagnostic(
                "planner.invalid_json",
                &format!("LLM output is not valid runner JSON: {err}"),
            )
        }
    })?;
    let risk_level = parse_risk(&parsed.risk_level)?;
    let required_capabilities = parsed.required_capabilities;
    let steps = if parsed.steps.is_empty() {
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
    } else {
        parsed.steps.into_iter().map(Into::into).collect()
    };

    let document = RunnerDraftDocument {
        runner_id: parsed.runner_id,
        version: parsed.version,
        summary: parsed.summary,
        risk_level,
        required_capabilities,
        inputs: parsed.inputs.into_keys().collect(),
        outputs: parsed.outputs.into_keys().collect(),
        steps,
        assertions: parsed.assertions,
        open_questions: parsed.open_questions,
    };
    validate_runner_draft(&document)?;
    Ok(document)
}

#[derive(Debug, Deserialize)]
struct JsonRunnerDraftDocument {
    runner_id: String,
    version: String,
    summary: String,
    risk_level: String,
    required_capabilities: Vec<String>,
    #[serde(default)]
    inputs: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    outputs: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    steps: Vec<JsonRunnerStep>,
    #[serde(default)]
    assertions: Vec<String>,
    #[serde(default)]
    open_questions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct JsonRunnerStep {
    id: String,
    #[serde(default = "default_execute_action")]
    action: String,
    required_capability: String,
    #[serde(default)]
    target: LocatorTarget,
    value: Option<String>,
}

impl From<JsonRunnerStep> for RunnerStep {
    fn from(value: JsonRunnerStep) -> Self {
        let target = if value.target == LocatorTarget::default() {
            LocatorTarget {
                preferred: Some(LocatorStrategy {
                    name: Some(value.id.clone()),
                    ..LocatorStrategy::default()
                }),
                ..LocatorTarget::default()
            }
        } else {
            value.target
        };
        Self {
            id: value.id,
            action: value.action,
            target,
            value: value.value,
            required_capability: value.required_capability,
        }
    }
}

fn default_execute_action() -> String {
    "execute".to_owned()
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

fn diagnostic(code: &str, message: &str) -> SchemaDiagnostic {
    SchemaDiagnostic {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorTarget;
    use greentic_desktop_workflow::{
        DesktopWorkflow, NativePlatform, WorkflowAction, WorkflowActionKind,
        WorkflowEvidencePolicy, WorkflowInput, WorkflowOutput, WorkflowOutputExtractor,
        WorkflowRisk, WorkflowTarget, WorkflowValueType,
    };

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

    #[test]
    fn serde_parser_accepts_typed_step_target() {
        let draft = parse_runner_draft_json(
            r#"{
                "runner_id": "web.search",
                "version": "0.1.3",
                "summary": "Search",
                "risk_level": "low",
                "required_capabilities": ["web.fill"],
                "inputs": {"query": {"type": "string"}},
                "outputs": {"result": {"type": "string"}},
                "steps": [{
                    "id": "fill_query",
                    "action": "fill",
                    "required_capability": "web.fill",
                    "target": {"preferred": {"name": "Search"}}
                }],
                "assertions": [],
                "open_questions": []
            }"#,
        )
        .expect("serde draft should parse");

        assert_eq!(
            draft.steps[0]
                .target
                .preferred
                .as_ref()
                .and_then(|target| target.name.clone()),
            Some("Search".to_owned())
        );
    }

    #[test]
    fn exports_runner_draft_json_schema() {
        let schema = runner_draft_json_schema();
        let value: serde_json::Value = serde_json::from_str(&schema).expect("schema json");

        assert_eq!(value["type"], "object");
        assert!(value["required"]
            .as_array()
            .expect("required")
            .contains(&serde_json::Value::String("runner_id".to_owned())));
    }

    #[test]
    fn runner_definition_compiles_workflow_and_derives_schemas() {
        let workflow = DesktopWorkflow {
            id: "sample-native".to_owned(),
            summary: "Fill a native form".to_owned(),
            target: WorkflowTarget::native_app(
                NativePlatform::Windows,
                Some("Sample.exe".to_owned()),
                "Sample".to_owned(),
            ),
            inputs: vec![
                WorkflowInput {
                    name: "record_id".to_owned(),
                    value_type: WorkflowValueType::String,
                    required: true,
                    secret: false,
                    target: LocatorTarget::default(),
                    value_template: "{{inputs.record_id}}".to_owned(),
                },
                WorkflowInput {
                    name: "api_token".to_owned(),
                    value_type: WorkflowValueType::String,
                    required: true,
                    secret: true,
                    target: LocatorTarget::default(),
                    value_template: "{{secrets.api_token}}".to_owned(),
                },
            ],
            actions: vec![WorkflowAction {
                name: "submit".to_owned(),
                kind: WorkflowActionKind::Click,
                target: LocatorTarget::default(),
                value_template: None,
                risk: WorkflowRisk::Medium,
            }],
            outputs: vec![WorkflowOutput {
                name: "confirmation".to_owned(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::TargetText(Box::default()),
                required: true,
                expected: None,
            }],
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };

        let runner = RunnerDefinition::from_workflow(
            "sample.native",
            "0.1.3",
            "Fill sample native form",
            "Submit a record and return confirmation",
            RunnerRisk::Medium,
            vec![TargetTechnology::NativeWindows],
            workflow,
        )
        .expect("runner definition should compile");

        assert_eq!(runner.inputs[0].name, "record_id");
        assert_eq!(runner.secrets[0].name, "api_token");
        assert!(runner.approval_policy.require_before_submit);
        assert_eq!(runner.output_schema()[0].name, "confirmation");
        assert!(runner
            .compiled_steps
            .iter()
            .any(|step| step.required_capability == "windows.open_app"));

        let input_schema = runner.input_schema();
        assert_eq!(input_schema.len(), 2);
        assert!(input_schema.iter().any(|field| field.secret));

        let json = serde_json::to_string(&runner).expect("runner serializes");
        let decoded: RunnerDefinition = serde_json::from_str(&json).expect("runner deserializes");
        assert_eq!(decoded.runner_id, "sample.native");

        let input_json_schema = McpInputSchema::from_runner(&decoded).to_json_schema();
        assert!(input_json_schema.contains("\"writeOnly\":true"));
        assert!(McpOutputSchema::from_runner(&decoded)
            .to_json_schema()
            .contains("confirmation"));
    }

    #[test]
    fn runner_definition_preserves_compiled_steps_in_runner_package() {
        let workflow = DesktopWorkflow {
            id: "web-search".to_owned(),
            summary: "Search the web".to_owned(),
            target: WorkflowTarget::web("https://example.test"),
            inputs: vec![WorkflowInput {
                name: "query".to_owned(),
                value_type: WorkflowValueType::String,
                required: true,
                secret: false,
                target: LocatorTarget::default(),
                value_template: "{{inputs.query}}".to_owned(),
            }],
            actions: vec![WorkflowAction {
                name: "submit".to_owned(),
                kind: WorkflowActionKind::Click,
                target: LocatorTarget::default(),
                value_template: None,
                risk: WorkflowRisk::Low,
            }],
            outputs: vec![WorkflowOutput {
                name: "result".to_owned(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::TargetText(Box::default()),
                required: true,
                expected: None,
            }],
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };

        let runner = RunnerDefinition::from_workflow(
            "web.search",
            "0.1.3",
            "Search",
            "Search and return result text",
            RunnerRisk::Low,
            vec![TargetTechnology::Web],
            workflow,
        )
        .expect("runner definition should compile");
        let package = runner.into_package();

        assert_eq!(package.inputs, vec!["inputs.query"]);
        assert_eq!(package.outputs, vec!["outputs.result"]);
        assert!(package
            .steps
            .iter()
            .any(|step| step.required_capability == "web.goto"));
    }
}
