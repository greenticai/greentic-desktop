use greentic_desktop_adapter::{LocatorTarget, RunnerStep};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopWorkflow {
    pub id: String,
    pub summary: String,
    pub target: WorkflowTarget,
    pub inputs: Vec<WorkflowInput>,
    pub actions: Vec<WorkflowAction>,
    pub outputs: Vec<WorkflowOutput>,
    pub assertions: Vec<WorkflowAssertion>,
    pub evidence_policy: WorkflowEvidencePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTarget {
    pub kind: WorkflowTargetKind,
    pub open: Option<WorkflowOpenTarget>,
}

impl WorkflowTarget {
    pub fn web(url: impl Into<String>) -> Self {
        Self {
            kind: WorkflowTargetKind::Web,
            open: Some(WorkflowOpenTarget::Url(url.into())),
        }
    }

    pub fn native_app(
        platform: NativePlatform,
        app_name: Option<String>,
        window_title: String,
    ) -> Self {
        Self {
            kind: WorkflowTargetKind::NativeApp(platform),
            open: Some(WorkflowOpenTarget::App {
                app_name,
                window_title: Some(window_title),
            }),
        }
    }

    pub fn java_app(window_title: impl Into<String>) -> Self {
        Self {
            kind: WorkflowTargetKind::JavaApp,
            open: Some(WorkflowOpenTarget::App {
                app_name: None,
                window_title: Some(window_title.into()),
            }),
        }
    }

    pub fn terminal(profile_name: impl Into<String>) -> Self {
        Self {
            kind: WorkflowTargetKind::Terminal,
            open: Some(WorkflowOpenTarget::Connection {
                profile: profile_name.into(),
            }),
        }
    }

    pub fn vision() -> Self {
        Self {
            kind: WorkflowTargetKind::Vision,
            open: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowTargetKind {
    Web,
    NativeApp(NativePlatform),
    JavaApp,
    Terminal,
    Vision,
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativePlatform {
    MacOs,
    LinuxX11,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowOpenTarget {
    Url(String),
    App {
        app_name: Option<String>,
        window_title: Option<String>,
    },
    Connection {
        profile: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowInput {
    pub name: String,
    pub value_type: WorkflowValueType,
    pub required: bool,
    pub secret: bool,
    pub target: LocatorTarget,
    pub value_template: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowValueType {
    String,
    Number,
    Boolean,
    Date,
    Enum(Vec<String>),
    File,
    Json,
}

impl Default for WorkflowValueType {
    fn default() -> Self {
        Self::String
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowAction {
    pub name: String,
    pub kind: WorkflowActionKind,
    pub target: LocatorTarget,
    pub value_template: Option<String>,
    pub risk: WorkflowRisk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowActionKind {
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
    AdapterCapability(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowRisk {
    Low,
    Medium,
    High,
}

impl Default for WorkflowRisk {
    fn default() -> Self {
        Self::Low
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowOutput {
    pub name: String,
    pub value_type: WorkflowValueType,
    pub extractor: WorkflowOutputExtractor,
    pub required: bool,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowOutputExtractor {
    TargetText(LocatorTarget),
    VisibleText(String),
    Regex(String),
    TerminalField(TerminalField),
    JsonPath(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalField {
    pub row: usize,
    pub col: usize,
    pub len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowAssertion {
    pub name: String,
    pub target: LocatorTarget,
    pub expected: String,
    pub capability_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEvidencePolicy {
    pub capture_steps: bool,
    pub capture_screenshots: bool,
}

impl Default for WorkflowEvidencePolicy {
    fn default() -> Self {
        Self {
            capture_steps: true,
            capture_screenshots: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowCompileContext {
    pub adapter_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowCompileResult {
    pub workflow_id: String,
    pub steps: Vec<RunnerStep>,
    pub outputs: Vec<CompiledWorkflowOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledWorkflowOutput {
    pub name: String,
    pub extractor: WorkflowOutputExtractor,
    pub required: bool,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowCompileError {
    MissingOpenTarget(&'static str),
    MissingLocator(String),
    MissingInput(String),
    MissingOutputExtractor(String),
    UnsupportedTargetKind(String),
    UnsupportedAction { target: String, action: String },
}

impl fmt::Display for WorkflowCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingOpenTarget(field) => write!(f, "workflow is missing open target: {field}"),
            Self::MissingLocator(name) => write!(f, "workflow item {name} is missing a locator"),
            Self::MissingInput(name) => write!(f, "workflow input {name} is missing a value"),
            Self::MissingOutputExtractor(name) => {
                write!(f, "workflow output {name} is missing an extractor")
            }
            Self::UnsupportedTargetKind(kind) => {
                write!(f, "unsupported workflow target kind: {kind}")
            }
            Self::UnsupportedAction { target, action } => {
                write!(
                    f,
                    "unsupported action {action} for workflow target {target}"
                )
            }
        }
    }
}

impl std::error::Error for WorkflowCompileError {}

pub type WorkflowCompileOutcome<T> = Result<T, WorkflowCompileError>;

pub fn compile_workflow(
    workflow: &DesktopWorkflow,
) -> WorkflowCompileOutcome<WorkflowCompileResult> {
    compile_workflow_with_context(workflow, &WorkflowCompileContext::default())
}

pub fn compile_workflow_with_context(
    workflow: &DesktopWorkflow,
    _context: &WorkflowCompileContext,
) -> WorkflowCompileOutcome<WorkflowCompileResult> {
    let mut compiler = StepCompiler::default();
    match &workflow.target.kind {
        WorkflowTargetKind::Web => compile_web(workflow, &mut compiler)?,
        WorkflowTargetKind::NativeApp(platform) => {
            compile_native(workflow, *platform, &mut compiler)?
        }
        WorkflowTargetKind::JavaApp => compile_java(workflow, &mut compiler)?,
        WorkflowTargetKind::Terminal => compile_terminal(workflow, &mut compiler)?,
        WorkflowTargetKind::Vision => compile_vision(workflow, &mut compiler)?,
        WorkflowTargetKind::Workspace => {
            return Err(WorkflowCompileError::UnsupportedTargetKind(
                "workspace".to_owned(),
            ))
        }
    }

    Ok(WorkflowCompileResult {
        workflow_id: workflow.id.clone(),
        steps: compiler.steps,
        outputs: workflow
            .outputs
            .iter()
            .map(|output| CompiledWorkflowOutput {
                name: output.name.clone(),
                extractor: output.extractor.clone(),
                required: output.required,
                expected: output.expected.clone(),
            })
            .collect(),
    })
}

#[derive(Default)]
struct StepCompiler {
    steps: Vec<RunnerStep>,
}

impl StepCompiler {
    fn push(
        &mut self,
        id: impl Into<String>,
        action: impl Into<String>,
        target: LocatorTarget,
        value: Option<String>,
        required_capability: impl Into<String>,
    ) {
        self.steps.push(RunnerStep {
            id: id.into(),
            action: action.into(),
            target,
            value,
            required_capability: required_capability.into(),
        });
    }
}

fn compile_web(
    workflow: &DesktopWorkflow,
    compiler: &mut StepCompiler,
) -> WorkflowCompileOutcome<()> {
    if let Some(WorkflowOpenTarget::Url(url)) = &workflow.target.open {
        compiler.push(
            "open-url",
            "goto",
            LocatorTarget::default(),
            Some(url.clone()),
            "web.goto",
        );
    }

    for input in &workflow.inputs {
        require_value(input)?;
        compiler.push(
            format!("fill-input-{}", workflow_id_component(&input.name)),
            "fill",
            input.target.clone(),
            Some(input.value_template.clone()),
            "web.fill",
        );
    }
    compile_common_actions(workflow, "web", compiler)?;
    compile_outputs(
        workflow,
        "web",
        "extract_text",
        "web.extract_text",
        compiler,
    )
}

fn compile_native(
    workflow: &DesktopWorkflow,
    platform: NativePlatform,
    compiler: &mut StepCompiler,
) -> WorkflowCompileOutcome<()> {
    let prefix = match platform {
        NativePlatform::MacOs => "macos",
        NativePlatform::LinuxX11 => "linux",
        NativePlatform::Windows => "windows",
    };
    let open = app_open_target(workflow)?;
    match platform {
        NativePlatform::MacOs => {
            let app_name = open
                .app_name
                .ok_or(WorkflowCompileError::MissingOpenTarget("app_name"))?;
            compiler.push(
                "activate-app",
                "activate_app",
                LocatorTarget::default(),
                Some(app_name.to_owned()),
                "macos.activate_app",
            );
            let window_title = open
                .window_title
                .ok_or(WorkflowCompileError::MissingOpenTarget("window_title"))?;
            compiler.push(
                "find-window",
                "find_window",
                LocatorTarget::default(),
                Some(window_title.to_owned()),
                "macos.find_window",
            );
        }
        NativePlatform::LinuxX11 => {
            let window_title = open
                .window_title
                .ok_or(WorkflowCompileError::MissingOpenTarget("window_title"))?;
            compiler.push(
                "find-window",
                "find_window",
                LocatorTarget::default(),
                Some(window_title.to_owned()),
                "linux.find_window",
            );
            compiler.push(
                "activate-window",
                "activate_window",
                LocatorTarget::default(),
                Some(window_title.to_owned()),
                "linux.activate_window",
            );
        }
        NativePlatform::Windows => {
            let app_name = open
                .app_name
                .ok_or(WorkflowCompileError::MissingOpenTarget("app_name"))?;
            compiler.push(
                "open-app",
                "open_app",
                LocatorTarget::default(),
                Some(app_name.to_owned()),
                "windows.open_app",
            );
            let window_title = open
                .window_title
                .ok_or(WorkflowCompileError::MissingOpenTarget("window_title"))?;
            compiler.push(
                "find-window",
                "find_window",
                LocatorTarget::default(),
                Some(window_title.to_owned()),
                "windows.find_window",
            );
        }
    }

    for input in &workflow.inputs {
        require_value(input)?;
        let step_id = workflow_id_component(&input.name);
        compiler.push(
            format!("find-input-{step_id}"),
            "find_element",
            input.target.clone(),
            None,
            format!("{prefix}.find_element"),
        );
        compiler.push(
            format!("type-input-{step_id}"),
            "type_text",
            input.target.clone(),
            Some(input.value_template.clone()),
            format!("{prefix}.type_text"),
        );
    }
    compile_common_actions(workflow, prefix, compiler)?;
    compile_outputs(
        workflow,
        prefix,
        "read_text",
        format!("{prefix}.read_text"),
        compiler,
    )
}

fn compile_java(
    workflow: &DesktopWorkflow,
    compiler: &mut StepCompiler,
) -> WorkflowCompileOutcome<()> {
    let open = app_open_target(workflow)?;
    let window_title = open
        .window_title
        .ok_or(WorkflowCompileError::MissingOpenTarget("window_title"))?;
    compiler.push(
        "find-window",
        "find_window",
        LocatorTarget::default(),
        Some(window_title.to_owned()),
        "java.find_window",
    );

    for input in &workflow.inputs {
        require_value(input)?;
        let step_id = workflow_id_component(&input.name);
        compiler.push(
            format!("find-input-{step_id}"),
            "find_component",
            input.target.clone(),
            None,
            "java.find_component",
        );
        compiler.push(
            format!("type-input-{step_id}"),
            "type_text",
            input.target.clone(),
            Some(input.value_template.clone()),
            "java.type_text",
        );
    }
    compile_common_actions(workflow, "java", compiler)?;
    compile_outputs(workflow, "java", "read_text", "java.read_text", compiler)
}

fn compile_terminal(
    workflow: &DesktopWorkflow,
    compiler: &mut StepCompiler,
) -> WorkflowCompileOutcome<()> {
    if !matches!(
        workflow.target.open,
        Some(WorkflowOpenTarget::Connection { .. })
    ) {
        return Err(WorkflowCompileError::MissingOpenTarget("profile"));
    }
    compiler.push(
        "connect",
        "connect",
        LocatorTarget::default(),
        None,
        "terminal.connect",
    );
    for action in &workflow.actions {
        let capability = match &action.kind {
            WorkflowActionKind::Input => "terminal.type_text".to_owned(),
            WorkflowActionKind::Key => "terminal.send_keys".to_owned(),
            WorkflowActionKind::Wait => "terminal.wait_for_screen".to_owned(),
            WorkflowActionKind::AdapterCapability(capability) => capability.clone(),
            other => {
                return Err(WorkflowCompileError::UnsupportedAction {
                    target: "terminal".to_owned(),
                    action: format!("{other:?}"),
                })
            }
        };
        compiler.push(
            workflow_id_component(&action.name),
            terminal_action_name(&capability),
            LocatorTarget::default(),
            action.value_template.clone(),
            capability,
        );
    }
    for output in &workflow.outputs {
        match &output.extractor {
            WorkflowOutputExtractor::TerminalField(_) => compiler.push(
                format!("extract-output-{}", workflow_id_component(&output.name)),
                "extract_field",
                LocatorTarget::default(),
                None,
                "terminal.extract_field",
            ),
            WorkflowOutputExtractor::VisibleText(text) => compiler.push(
                format!("wait-output-{}", workflow_id_component(&output.name)),
                "wait_for_screen",
                LocatorTarget::default(),
                Some(text.clone()),
                "terminal.wait_for_screen",
            ),
            _ => {
                return Err(WorkflowCompileError::MissingOutputExtractor(
                    output.name.clone(),
                ))
            }
        }
    }
    Ok(())
}

fn compile_vision(
    workflow: &DesktopWorkflow,
    compiler: &mut StepCompiler,
) -> WorkflowCompileOutcome<()> {
    compiler.push(
        "screenshot",
        "screenshot",
        LocatorTarget::default(),
        None,
        "vision.screenshot",
    );
    compile_common_actions(workflow, "vision", compiler)?;
    for output in &workflow.outputs {
        compiler.push(
            format!("extract-output-{}", workflow_id_component(&output.name)),
            "extract_text",
            output_target(output)?,
            None,
            "vision.extract_text",
        );
    }
    Ok(())
}

fn compile_common_actions(
    workflow: &DesktopWorkflow,
    prefix: &str,
    compiler: &mut StepCompiler,
) -> WorkflowCompileOutcome<()> {
    for action in &workflow.actions {
        let step_id = workflow_id_component(&action.name);
        match &action.kind {
            WorkflowActionKind::Click => compiler.push(
                format!("submit-{step_id}"),
                if prefix == "java" {
                    "click_component"
                } else if prefix == "web" {
                    "click"
                } else if prefix == "vision" {
                    "click_region"
                } else {
                    "click_element"
                },
                action.target.clone(),
                action.value_template.clone(),
                if prefix == "java" {
                    "java.click_component".to_owned()
                } else if prefix == "web" {
                    "web.click".to_owned()
                } else if prefix == "vision" {
                    "vision.click_region".to_owned()
                } else {
                    format!("{prefix}.click_element")
                },
            ),
            WorkflowActionKind::Wait => compiler.push(
                format!("wait-{step_id}"),
                if prefix == "web" {
                    "wait_for_text"
                } else {
                    "assert_visible"
                },
                action.target.clone(),
                action.value_template.clone(),
                if prefix == "web" {
                    "web.wait_for_text".to_owned()
                } else {
                    format!("{prefix}.assert_visible")
                },
            ),
            WorkflowActionKind::Screenshot => compiler.push(
                format!("screenshot-{step_id}"),
                "screenshot",
                action.target.clone(),
                None,
                format!("{prefix}.screenshot"),
            ),
            WorkflowActionKind::AdapterCapability(capability) => compiler.push(
                step_id,
                capability
                    .strip_prefix(&format!("{prefix}."))
                    .unwrap_or(capability)
                    .to_owned(),
                action.target.clone(),
                action.value_template.clone(),
                capability.clone(),
            ),
            WorkflowActionKind::Open
            | WorkflowActionKind::Attach
            | WorkflowActionKind::Observe
            | WorkflowActionKind::Find
            | WorkflowActionKind::Input
            | WorkflowActionKind::Key
            | WorkflowActionKind::Extract
            | WorkflowActionKind::Assert
            | WorkflowActionKind::Download
            | WorkflowActionKind::Close => {
                return Err(WorkflowCompileError::UnsupportedAction {
                    target: prefix.to_owned(),
                    action: format!("{:?}", action.kind),
                });
            }
        }
    }
    Ok(())
}

fn compile_outputs(
    workflow: &DesktopWorkflow,
    prefix: &str,
    read_action: impl Into<String> + Clone,
    read_capability: impl Into<String> + Clone,
    compiler: &mut StepCompiler,
) -> WorkflowCompileOutcome<()> {
    for output in &workflow.outputs {
        let step_id = workflow_id_component(&output.name);
        let target = output_target(output)?;
        if prefix == "java" {
            compiler.push(
                format!("find-output-{step_id}"),
                "find_component",
                target.clone(),
                None,
                "java.find_component",
            );
        } else if prefix != "web" {
            compiler.push(
                format!("find-output-{step_id}"),
                "find_element",
                target.clone(),
                None,
                format!("{prefix}.find_element"),
            );
        }
        compiler.push(
            format!("read-output-{step_id}"),
            read_action.clone().into(),
            target,
            None,
            read_capability.clone().into(),
        );
    }
    Ok(())
}

fn output_target(output: &WorkflowOutput) -> WorkflowCompileOutcome<LocatorTarget> {
    match &output.extractor {
        WorkflowOutputExtractor::TargetText(target) => Ok(target.clone()),
        _ => Err(WorkflowCompileError::MissingOutputExtractor(
            output.name.clone(),
        )),
    }
}

fn app_open_target(workflow: &DesktopWorkflow) -> WorkflowCompileOutcome<AppOpenTarget<'_>> {
    match &workflow.target.open {
        Some(WorkflowOpenTarget::App {
            app_name,
            window_title,
        }) => Ok(AppOpenTarget {
            app_name: app_name.as_ref(),
            window_title: window_title.as_ref(),
        }),
        _ => Err(WorkflowCompileError::MissingOpenTarget("app")),
    }
}

struct AppOpenTarget<'a> {
    app_name: Option<&'a String>,
    window_title: Option<&'a String>,
}

fn require_value(input: &WorkflowInput) -> WorkflowCompileOutcome<()> {
    if input.required && input.value_template.trim().is_empty() {
        Err(WorkflowCompileError::MissingInput(input.name.clone()))
    } else {
        Ok(())
    }
}

fn terminal_action_name(capability: &str) -> String {
    capability
        .strip_prefix("terminal.")
        .unwrap_or(capability)
        .to_owned()
}

pub fn workflow_id_component(value: &str) -> String {
    let rendered = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if rendered.is_empty() {
        "item".to_owned()
    } else {
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorStrategy;

    fn target(name: &str) -> LocatorTarget {
        LocatorTarget {
            preferred: Some(LocatorStrategy {
                name: Some(name.to_owned()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        }
    }

    #[test]
    fn compiles_native_macos_workflow() {
        let workflow = DesktopWorkflow {
            id: "calculator".to_owned(),
            summary: "Add two values".to_owned(),
            target: WorkflowTarget::native_app(
                NativePlatform::MacOs,
                Some("Calculator".to_owned()),
                "Calculator".to_owned(),
            ),
            inputs: vec![WorkflowInput {
                name: "number one".to_owned(),
                value_type: WorkflowValueType::Number,
                required: true,
                secret: false,
                target: target("Number one"),
                value_template: "1".to_owned(),
            }],
            actions: vec![WorkflowAction {
                name: "equals".to_owned(),
                kind: WorkflowActionKind::Click,
                target: target("Equals"),
                value_template: None,
                risk: WorkflowRisk::Low,
            }],
            outputs: vec![WorkflowOutput {
                name: "result".to_owned(),
                value_type: WorkflowValueType::Number,
                extractor: WorkflowOutputExtractor::TargetText(target("Result")),
                required: true,
                expected: Some("2".to_owned()),
            }],
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };

        let compiled = compile_workflow(&workflow).expect("workflow should compile");
        let capabilities: Vec<_> = compiled
            .steps
            .iter()
            .map(|step| step.required_capability.as_str())
            .collect();

        assert_eq!(
            capabilities,
            vec![
                "macos.activate_app",
                "macos.find_window",
                "macos.find_element",
                "macos.type_text",
                "macos.click_element",
                "macos.find_element",
                "macos.read_text",
            ]
        );
        assert_eq!(compiled.outputs[0].name, "result");
    }

    #[test]
    fn compiles_terminal_field_output() {
        let workflow = DesktopWorkflow {
            id: "terminal-lookup".to_owned(),
            summary: "Lookup a record".to_owned(),
            target: WorkflowTarget::terminal("mainframe"),
            inputs: Vec::new(),
            actions: vec![WorkflowAction {
                name: "enter account".to_owned(),
                kind: WorkflowActionKind::AdapterCapability("terminal.send_text".to_owned()),
                target: LocatorTarget::default(),
                value_template: Some("123".to_owned()),
                risk: WorkflowRisk::Low,
            }],
            outputs: vec![WorkflowOutput {
                name: "status".to_owned(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::TerminalField(TerminalField {
                    row: 1,
                    col: 10,
                    len: 6,
                }),
                required: true,
                expected: Some("ACTIVE".to_owned()),
            }],
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };

        let compiled = compile_workflow(&workflow).expect("workflow should compile");
        assert_eq!(compiled.steps[0].required_capability, "terminal.connect");
        assert_eq!(
            compiled.steps.last().unwrap().required_capability,
            "terminal.extract_field"
        );
    }

    #[test]
    fn reports_missing_native_app_name() {
        let workflow = DesktopWorkflow {
            id: "bad".to_owned(),
            summary: String::new(),
            target: WorkflowTarget::native_app(NativePlatform::Windows, None, "App".to_owned()),
            inputs: Vec::new(),
            actions: Vec::new(),
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };

        assert!(matches!(
            compile_workflow(&workflow),
            Err(WorkflowCompileError::MissingOpenTarget("app_name"))
        ));
    }
}
