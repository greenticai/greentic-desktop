use greentic_desktop_adapter::{AdapterCapabilities, LocatorStrategy, LocatorTarget, RunnerStep};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimitiveWorkflow {
    pub id: String,
    pub summary: String,
    pub target: WorkflowTarget,
    pub inputs: Vec<WorkflowInput>,
    pub primitives: Vec<DesktopPrimitive>,
    pub outputs: Vec<WorkflowOutput>,
    pub assertions: Vec<WorkflowAssertion>,
    pub evidence_policy: WorkflowEvidencePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DesktopPrimitive {
    OpenApp {
        app: AppReference,
    },
    OpenResource {
        resource: ResourceReference,
        create_if_missing: bool,
    },
    Focus {
        target: TargetQuery,
    },
    EnterText {
        target: TargetQuery,
        value_template: String,
    },
    InvokeCommand {
        command: CommandReference,
    },
    SaveResource {
        path_template: Option<String>,
        policy: SavePolicy,
    },
    ObserveOutput {
        name: String,
        extractor: OutputExtractorReference,
    },
    AssertState {
        condition: WorkflowCondition,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppReference {
    #[serde(default, alias = "app_name", alias = "title")]
    pub name: String,
    #[serde(default, alias = "bundle")]
    pub bundle_id: Option<String>,
    #[serde(default, alias = "command", alias = "path")]
    pub executable: Option<String>,
    #[serde(default, alias = "window")]
    pub window_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceReference {
    #[serde(default, alias = "path", alias = "name", alias = "url")]
    pub path_template: String,
    #[serde(default)]
    pub resource_type: ResourceType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ResourceType {
    Document,
    Spreadsheet,
    BrowserPage,
    TerminalSession,
    RemoteDesktop,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TargetQuery {
    #[serde(default, alias = "name")]
    pub label: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default, alias = "id")]
    pub automation_id: Option<String>,
    #[serde(default)]
    pub shortcut: Option<String>,
}

impl TargetQuery {
    pub fn by_label(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            role: None,
            text: None,
            automation_id: None,
            shortcut: None,
        }
    }

    pub fn active_document() -> Self {
        Self {
            label: Some("active document".to_owned()),
            role: Some("document".to_owned()),
            text: None,
            automation_id: None,
            shortcut: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReference {
    #[serde(default, alias = "action")]
    pub name: String,
    #[serde(default)]
    pub shortcut: Option<String>,
    #[serde(default)]
    pub menu_path: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SavePolicy {
    #[serde(alias = "Create", alias = "Update", alias = "Overwrite")]
    #[default]
    CreateOrUpdate,
    #[serde(alias = "CreateOnly")]
    MustCreate,
    #[serde(alias = "Existing", alias = "OpenExisting")]
    MustExist,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputExtractorReference {
    #[serde(default)]
    pub target: TargetQuery,
    #[serde(default)]
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowCondition {
    ResourceExists { path_template: String },
    OutputPresent { name: String },
    VisibleText { text_template: String },
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkflowValueType {
    #[default]
    String,
    Number,
    Boolean,
    Date,
    Enum(Vec<String>),
    File,
    Json,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WorkflowRisk {
    #[default]
    Low,
    Medium,
    High,
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
    TargetText(Box<LocatorTarget>),
    VisibleText(String),
    Regex(String),
    TerminalField(TerminalField),
    FileExists(String),
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
    pub available_adapters: Vec<AdapterCapabilities>,
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
    MissingSemanticCapability(String),
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
            Self::MissingSemanticCapability(capability) => {
                write!(f, "no adapter supports semantic capability {capability}")
            }
        }
    }
}

impl std::error::Error for WorkflowCompileError {}

pub type WorkflowCompileOutcome<T> = Result<T, WorkflowCompileError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCapability {
    OpenTarget,
    OpenResource,
    CreateResourceIfMissing,
    Attach,
    Focus,
    Find,
    Input,
    Command,
    Save,
    Extract,
    Assert,
}

impl SemanticCapability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenTarget => "open.target",
            Self::OpenResource => "open.resource",
            Self::CreateResourceIfMissing => "resource.create_if_missing",
            Self::Attach => "app.attach",
            Self::Focus => "ui.focus",
            Self::Find => "ui.find",
            Self::Input => "ui.input",
            Self::Command => "ui.command",
            Self::Save => "ui.save",
            Self::Extract => "ui.extract",
            Self::Assert => "ui.assert",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCapabilityRoute {
    pub semantic: SemanticCapability,
    pub adapter_id: String,
    pub concrete_capability: String,
}

pub fn route_semantic_capability(
    semantic: SemanticCapability,
    adapters: &[AdapterCapabilities],
) -> WorkflowCompileOutcome<SemanticCapabilityRoute> {
    for adapter in adapters {
        for candidate in semantic_capability_candidates(semantic) {
            if adapter.supports(candidate) {
                return Ok(SemanticCapabilityRoute {
                    semantic,
                    adapter_id: adapter.adapter_id.clone(),
                    concrete_capability: (*candidate).to_owned(),
                });
            }
        }
    }
    Err(WorkflowCompileError::MissingSemanticCapability(
        semantic.as_str().to_owned(),
    ))
}

pub fn route_semantic_capabilities(
    semantics: &[SemanticCapability],
    adapters: &[AdapterCapabilities],
) -> WorkflowCompileOutcome<Vec<SemanticCapabilityRoute>> {
    semantics
        .iter()
        .copied()
        .map(|semantic| route_semantic_capability(semantic, adapters))
        .collect()
}

fn semantic_capability_candidates(semantic: SemanticCapability) -> &'static [&'static str] {
    match semantic {
        SemanticCapability::OpenTarget => &[
            "web.goto",
            "windows.open_app",
            "macos.activate_app",
            "linux.find_window",
            "java.find_window",
            "terminal.connect",
        ],
        SemanticCapability::OpenResource => &[
            "web.goto",
            "macos.open_resource",
            "windows.open_app",
            "linux.find_window",
            "terminal.connect",
        ],
        SemanticCapability::CreateResourceIfMissing => &[
            "web.fill",
            "macos.open_resource",
            "windows.type_text",
            "linux.type_text",
            "terminal.send_text",
        ],
        SemanticCapability::Attach => &[
            "windows.find_window",
            "macos.find_window",
            "linux.find_window",
            "java.find_window",
        ],
        SemanticCapability::Focus => &[
            "windows.focus_window",
            "macos.focus_document",
            "linux.focus_window",
            "java.focus_component",
            "vision.find_text",
        ],
        SemanticCapability::Find => &[
            "web.wait_for",
            "windows.find_element",
            "macos.find_element",
            "linux.find_element",
            "java.find_component",
            "vision.find_text",
        ],
        SemanticCapability::Input => &[
            "web.fill",
            "web.press",
            "windows.type_text",
            "macos.type_text",
            "linux.type_text",
            "java.type_text",
            "terminal.send_text",
        ],
        SemanticCapability::Command => &[
            "web.press",
            "windows.press_shortcut",
            "macos.press_shortcut",
            "linux.press_shortcut",
            "java.invoke_action",
            "terminal.send_keys",
            "web.click",
            "windows.click_element",
            "macos.invoke_menu",
            "macos.click_element",
            "linux.click_element",
            "terminal.send_text",
        ],
        SemanticCapability::Save => &[
            "web.press",
            "windows.save_as",
            "macos.save_as",
            "linux.save_as",
            "web.click",
            "web.press",
            "windows.click_element",
            "macos.click_element",
            "linux.click_element",
            "terminal.send_keys",
            "terminal.send_text",
        ],
        SemanticCapability::Extract => &[
            "web.extract_text",
            "web.extract_regex",
            "windows.read_text",
            "macos.read_text",
            "linux.read_text",
            "java.read_text",
            "terminal.extract_field",
            "vision.extract_text",
        ],
        SemanticCapability::Assert => &[
            "web.assert_visible",
            "web.assert_url",
            "windows.assert_visible",
            "macos.assert_visible",
            "linux.assert_visible",
            "java.assert_visible",
            "terminal.wait_for_screen",
            "vision.assert_visible",
        ],
    }
}

pub fn compile_primitive_workflow(
    workflow: &PrimitiveWorkflow,
) -> WorkflowCompileOutcome<WorkflowCompileResult> {
    compile_primitive_workflow_with_context(workflow, &WorkflowCompileContext::default())
}

pub fn compile_primitive_workflow_with_context(
    workflow: &PrimitiveWorkflow,
    context: &WorkflowCompileContext,
) -> WorkflowCompileOutcome<WorkflowCompileResult> {
    let mut compiler = StepCompiler::default();
    let mut outputs = workflow
        .outputs
        .iter()
        .map(|output| CompiledWorkflowOutput {
            name: output.name.clone(),
            extractor: output.extractor.clone(),
            required: output.required,
            expected: output.expected.clone(),
        })
        .collect::<Vec<_>>();

    for (index, primitive) in workflow.primitives.iter().enumerate() {
        compile_primitive_step(
            workflow,
            context,
            primitive,
            index,
            &mut compiler,
            &mut outputs,
        )?;
    }

    Ok(WorkflowCompileResult {
        workflow_id: workflow.id.clone(),
        steps: compiler.steps,
        outputs,
    })
}

fn compile_primitive_step(
    workflow: &PrimitiveWorkflow,
    context: &WorkflowCompileContext,
    primitive: &DesktopPrimitive,
    index: usize,
    compiler: &mut StepCompiler,
    outputs: &mut Vec<CompiledWorkflowOutput>,
) -> WorkflowCompileOutcome<()> {
    let step_id = |suffix: &str| format!("primitive-{}-{suffix}", index + 1);
    match primitive {
        DesktopPrimitive::OpenApp { app } => {
            let capability =
                concrete_capability(workflow, context, SemanticCapability::OpenTarget)?;
            compiler.push(
                step_id("open-app"),
                action_for_capability(&capability, "open_app"),
                LocatorTarget::default(),
                Some(app.executable.clone().unwrap_or_else(|| app.name.clone())),
                capability,
            );
        }
        DesktopPrimitive::OpenResource {
            resource,
            create_if_missing,
        } => {
            let capability = concrete_capability(
                workflow,
                context,
                if *create_if_missing {
                    SemanticCapability::CreateResourceIfMissing
                } else {
                    SemanticCapability::OpenResource
                },
            )?;
            compiler.push(
                step_id("open-resource"),
                action_for_capability(&capability, "open_resource"),
                locator_for_resource(resource),
                Some(resource.path_template.clone()),
                capability,
            );
        }
        DesktopPrimitive::Focus { target } => {
            let capability = concrete_capability(workflow, context, SemanticCapability::Focus)?;
            compiler.push(
                step_id("focus"),
                action_for_capability(&capability, "focus"),
                locator_for_query(target),
                None,
                capability,
            );
        }
        DesktopPrimitive::EnterText {
            target,
            value_template,
        } => {
            let find_capability = if target_is_macos(workflow) && is_active_document_query(target) {
                "macos.focus_document".to_owned()
            } else {
                concrete_capability(workflow, context, SemanticCapability::Find)?
            };
            let find_step_suffix = if find_capability == "macos.focus_document" {
                "focus-target"
            } else {
                "find-target"
            };
            compiler.push(
                step_id(find_step_suffix),
                action_for_capability(&find_capability, "find"),
                locator_for_query(target),
                None,
                find_capability,
            );
            let input_capability =
                concrete_capability(workflow, context, SemanticCapability::Input)?;
            compiler.push(
                step_id("enter-text"),
                action_for_capability(&input_capability, "type_text"),
                locator_for_query(target),
                Some(value_template.clone()),
                input_capability,
            );
        }
        DesktopPrimitive::InvokeCommand { command } => {
            let capability = if target_is_macos(workflow) {
                if command.shortcut.is_some() {
                    "macos.press_shortcut".to_owned()
                } else if !command.menu_path.is_empty() {
                    "macos.invoke_menu".to_owned()
                } else {
                    concrete_capability(workflow, context, SemanticCapability::Command)?
                }
            } else {
                concrete_capability(workflow, context, SemanticCapability::Command)?
            };
            compiler.push(
                step_id("invoke-command"),
                action_for_capability(&capability, "invoke_command"),
                locator_for_command(command),
                command
                    .shortcut
                    .clone()
                    .or_else(|| {
                        (!command.menu_path.is_empty()).then(|| command.menu_path.join(" > "))
                    })
                    .or_else(|| Some(command.name.clone())),
                capability,
            );
        }
        DesktopPrimitive::SaveResource {
            path_template,
            policy: _,
        } => {
            let capability = if target_is_macos(workflow) {
                "macos.save_as".to_owned()
            } else {
                concrete_capability(workflow, context, SemanticCapability::Save)?
            };
            compiler.push(
                step_id("save-resource"),
                action_for_capability(&capability, "save"),
                LocatorTarget::default(),
                path_template.clone(),
                capability,
            );
        }
        DesktopPrimitive::ObserveOutput { name, extractor } => {
            let capability = concrete_capability(workflow, context, SemanticCapability::Extract)?;
            compiler.push(
                step_id("observe-output"),
                action_for_capability(&capability, "read_text"),
                locator_for_query(&extractor.target),
                extractor.pattern.clone(),
                capability,
            );
            outputs.push(CompiledWorkflowOutput {
                name: name.clone(),
                extractor: WorkflowOutputExtractor::TargetText(Box::new(locator_for_query(
                    &extractor.target,
                ))),
                required: true,
                expected: extractor.pattern.clone(),
            });
        }
        DesktopPrimitive::AssertState { condition } => match condition {
            WorkflowCondition::ResourceExists { path_template } => {
                outputs.push(CompiledWorkflowOutput {
                    name: "resource_exists".to_owned(),
                    extractor: WorkflowOutputExtractor::FileExists(path_template.clone()),
                    required: true,
                    expected: Some(path_template.clone()),
                });
            }
            WorkflowCondition::OutputPresent { name } => {
                let capability =
                    concrete_capability(workflow, context, SemanticCapability::Assert)?;
                compiler.push(
                    step_id("assert-output"),
                    action_for_capability(&capability, "assert_visible"),
                    LocatorTarget::default(),
                    Some(format!("{{{{outputs.{name}}}}}")),
                    capability,
                );
            }
            WorkflowCondition::VisibleText { text_template } => {
                let capability =
                    concrete_capability(workflow, context, SemanticCapability::Assert)?;
                compiler.push(
                    step_id("assert-visible-text"),
                    action_for_capability(&capability, "assert_visible"),
                    LocatorTarget {
                        preferred: Some(LocatorStrategy {
                            text: Some(text_template.clone()),
                            ..LocatorStrategy::default()
                        }),
                        ..LocatorTarget::default()
                    },
                    Some(text_template.clone()),
                    capability,
                );
            }
        },
    }
    Ok(())
}

fn target_is_macos(workflow: &PrimitiveWorkflow) -> bool {
    matches!(
        workflow.target.kind,
        WorkflowTargetKind::NativeApp(NativePlatform::MacOs)
    )
}

fn is_active_document_query(target: &TargetQuery) -> bool {
    target
        .label
        .as_deref()
        .map(|label| label.eq_ignore_ascii_case("active document"))
        .unwrap_or(false)
        || target
            .role
            .as_deref()
            .map(|role| role.eq_ignore_ascii_case("document"))
            .unwrap_or(false)
}

fn concrete_capability(
    workflow: &PrimitiveWorkflow,
    context: &WorkflowCompileContext,
    semantic: SemanticCapability,
) -> WorkflowCompileOutcome<String> {
    if !context.available_adapters.is_empty() {
        if let Some(capability) =
            target_kind_capability(&workflow.target.kind, semantic, &context.available_adapters)
        {
            return Ok(capability);
        }
        return route_semantic_capability(semantic, &context.available_adapters)
            .map(|route| route.concrete_capability);
    }
    Ok(default_capability_for_target(&workflow.target.kind, semantic)?.to_owned())
}

fn target_kind_capability(
    kind: &WorkflowTargetKind,
    semantic: SemanticCapability,
    adapters: &[AdapterCapabilities],
) -> Option<String> {
    let default = default_capability_for_target(kind, semantic).ok()?;
    adapters
        .iter()
        .find(|adapter| adapter.supports(default))
        .map(|_| default.to_owned())
}

fn default_capability_for_target(
    kind: &WorkflowTargetKind,
    semantic: SemanticCapability,
) -> WorkflowCompileOutcome<&'static str> {
    let prefix = match kind {
        WorkflowTargetKind::Web => "web",
        WorkflowTargetKind::NativeApp(NativePlatform::MacOs) => "macos",
        WorkflowTargetKind::NativeApp(NativePlatform::LinuxX11) => "linux",
        WorkflowTargetKind::NativeApp(NativePlatform::Windows) => "windows",
        WorkflowTargetKind::JavaApp => "java",
        WorkflowTargetKind::Terminal => "terminal",
        WorkflowTargetKind::Vision => "vision",
        WorkflowTargetKind::Workspace => {
            return Err(WorkflowCompileError::UnsupportedTargetKind(
                "workspace".to_owned(),
            ))
        }
    };
    semantic_capability_candidates(semantic)
        .iter()
        .copied()
        .find(|capability| capability.starts_with(prefix))
        .ok_or_else(|| {
            WorkflowCompileError::MissingSemanticCapability(semantic.as_str().to_owned())
        })
}

fn action_for_capability(capability: &str, fallback: &str) -> String {
    capability
        .rsplit('.')
        .next()
        .filter(|suffix| !suffix.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn locator_for_query(query: &TargetQuery) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            role: query.role.clone(),
            name: query.label.clone(),
            automation_id: query.automation_id.clone(),
            text: query.text.clone(),
            label: query.label.clone(),
            keyboard_shortcut: query.shortcut.clone(),
            ..LocatorStrategy::default()
        }),
        ..LocatorTarget::default()
    }
}

fn locator_for_resource(resource: &ResourceReference) -> LocatorTarget {
    if matches!(
        resource.resource_type,
        ResourceType::Document | ResourceType::Spreadsheet
    ) {
        return locator_for_query(&TargetQuery::active_document());
    }

    LocatorTarget {
        preferred: Some(LocatorStrategy {
            name: Some(resource.path_template.clone()),
            control_type: Some(format!("{:?}", resource.resource_type).to_ascii_lowercase()),
            ..LocatorStrategy::default()
        }),
        ..LocatorTarget::default()
    }
}

fn locator_for_command(command: &CommandReference) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            name: Some(command.name.clone()),
            keyboard_shortcut: command.shortcut.clone(),
            text: (!command.menu_path.is_empty()).then(|| command.menu_path.join(" > ")),
            ..LocatorStrategy::default()
        }),
        ..LocatorTarget::default()
    }
}

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
            WorkflowOutputExtractor::FileExists(_) => {}
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
            WorkflowActionKind::Key => {
                let (action_name, capability) = key_action_for_prefix(prefix)?;
                compiler.push(
                    format!("key-{step_id}"),
                    action_name,
                    action.target.clone(),
                    action.value_template.clone(),
                    capability,
                );
            }
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

fn key_action_for_prefix(prefix: &str) -> WorkflowCompileOutcome<(&'static str, String)> {
    match prefix {
        "web" => Ok(("press", "web.press".to_owned())),
        "macos" | "windows" | "linux" => Ok(("press_shortcut", format!("{prefix}.press_shortcut"))),
        "terminal" => Ok(("send_keys", "terminal.send_keys".to_owned())),
        "remote" => Ok(("press_key", "remote.press_key".to_owned())),
        other => Err(WorkflowCompileError::UnsupportedAction {
            target: other.to_owned(),
            action: "Key".to_owned(),
        }),
    }
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
        WorkflowOutputExtractor::TargetText(target) => Ok(target.as_ref().clone()),
        WorkflowOutputExtractor::FileExists(_) => Ok(LocatorTarget::default()),
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
                extractor: WorkflowOutputExtractor::TargetText(Box::new(target("Result"))),
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
    fn compiles_document_resource_text_step_with_active_document_target() {
        let workflow = PrimitiveWorkflow {
            id: "word-document".to_owned(),
            summary: "Create a document".to_owned(),
            target: WorkflowTarget::native_app(
                NativePlatform::MacOs,
                Some("Word".to_owned()),
                "Word".to_owned(),
            ),
            inputs: Vec::new(),
            primitives: vec![
                DesktopPrimitive::OpenApp {
                    app: AppReference {
                        name: "Word".to_owned(),
                        bundle_id: None,
                        executable: None,
                        window_title: Some("Word".to_owned()),
                    },
                },
                DesktopPrimitive::OpenResource {
                    resource: ResourceReference {
                        path_template: "{{inputs.document_path}}".to_owned(),
                        resource_type: ResourceType::Document,
                    },
                    create_if_missing: true,
                },
            ],
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };

        let compiled =
            compile_primitive_workflow(&workflow).expect("primitive workflow should compile");
        let open_resource = compiled
            .steps
            .iter()
            .find(|step| step.id == "primitive-2-open-resource")
            .expect("open resource step");

        assert_eq!(open_resource.action, "open_resource");
        assert_eq!(open_resource.required_capability, "macos.open_resource");
        assert_eq!(
            open_resource
                .target
                .preferred
                .as_ref()
                .and_then(|target| target.role.as_deref()),
            Some("document")
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

    #[test]
    fn routes_semantic_capabilities_to_web_adapter() {
        let adapters = vec![AdapterCapabilities::new(
            "greentic.desktop.playwright",
            "1.0.0",
            ["web.goto", "web.fill", "web.click", "web.extract_text"],
        )];

        let routes = route_semantic_capabilities(
            &[
                SemanticCapability::OpenTarget,
                SemanticCapability::Input,
                SemanticCapability::Save,
                SemanticCapability::Extract,
            ],
            &adapters,
        )
        .expect("web semantic capabilities should route");

        assert_eq!(routes[0].concrete_capability, "web.goto");
        assert_eq!(routes[1].concrete_capability, "web.fill");
        assert_eq!(routes[2].concrete_capability, "web.click");
        assert_eq!(routes[3].concrete_capability, "web.extract_text");
    }

    #[test]
    fn routes_semantic_capabilities_to_native_adapter() {
        let adapters = vec![AdapterCapabilities::new(
            "greentic.desktop.macos.ax",
            "1.0.0",
            [
                "macos.activate_app",
                "macos.find_element",
                "macos.open_resource",
                "macos.type_text",
                "macos.click_element",
                "macos.read_text",
            ],
        )];

        assert_eq!(
            route_semantic_capability(SemanticCapability::OpenTarget, &adapters)
                .expect("open should route")
                .concrete_capability,
            "macos.activate_app"
        );
        assert_eq!(
            route_semantic_capability(SemanticCapability::Input, &adapters)
                .expect("input should route")
                .concrete_capability,
            "macos.type_text"
        );
        assert_eq!(
            route_semantic_capability(SemanticCapability::CreateResourceIfMissing, &adapters)
                .expect("create resource should route")
                .concrete_capability,
            "macos.open_resource"
        );
        assert_eq!(
            route_semantic_capability(SemanticCapability::Extract, &adapters)
                .expect("extract should route")
                .concrete_capability,
            "macos.read_text"
        );
    }

    #[test]
    fn reports_missing_semantic_capability() {
        let err = route_semantic_capability(SemanticCapability::Save, &[])
            .expect_err("missing semantic capability should fail");

        assert!(matches!(
            err,
            WorkflowCompileError::MissingSemanticCapability(capability)
                if capability == "ui.save"
        ));
    }

    #[test]
    fn compiles_primitive_document_workflow_to_installed_native_adapter() {
        let workflow = PrimitiveWorkflow {
            id: "word.document.create".to_owned(),
            summary: "Create a document".to_owned(),
            target: WorkflowTarget::native_app(
                NativePlatform::MacOs,
                Some("Microsoft Word".to_owned()),
                "Microsoft Word".to_owned(),
            ),
            inputs: Vec::new(),
            primitives: vec![
                DesktopPrimitive::OpenApp {
                    app: AppReference {
                        name: "Microsoft Word".to_owned(),
                        bundle_id: None,
                        executable: None,
                        window_title: Some("Microsoft Word".to_owned()),
                    },
                },
                DesktopPrimitive::EnterText {
                    target: TargetQuery::active_document(),
                    value_template: "{{inputs.text_content}}".to_owned(),
                },
                DesktopPrimitive::SaveResource {
                    path_template: Some(
                        "{{inputs.save_location}}/{{inputs.document_name}}".to_owned(),
                    ),
                    policy: SavePolicy::CreateOrUpdate,
                },
                DesktopPrimitive::AssertState {
                    condition: WorkflowCondition::ResourceExists {
                        path_template: "{{inputs.save_location}}/{{inputs.document_name}}"
                            .to_owned(),
                    },
                },
            ],
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };
        let context = WorkflowCompileContext {
            adapter_id: Some("greentic.desktop.macos.ax".to_owned()),
            available_adapters: vec![AdapterCapabilities::new(
                "greentic.desktop.macos.ax",
                "1.0.0",
                [
                    "macos.activate_app",
                    "macos.open_resource",
                    "macos.focus_document",
                    "macos.find_element",
                    "macos.type_text",
                    "macos.click_element",
                    "macos.save_as",
                    "macos.read_text",
                ],
            )],
        };

        let compiled = compile_primitive_workflow_with_context(&workflow, &context)
            .expect("primitive workflow should compile");
        let capabilities = compiled
            .steps
            .iter()
            .map(|step| step.required_capability.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            capabilities,
            vec![
                "macos.activate_app",
                "macos.focus_document",
                "macos.type_text",
                "macos.save_as",
            ]
        );
        assert!(matches!(
            compiled.outputs[0].extractor,
            WorkflowOutputExtractor::FileExists(_)
        ));
    }

    #[test]
    fn primitive_macos_commands_compile_to_shortcut_and_menu_actions() {
        let workflow = PrimitiveWorkflow {
            id: "native.commands".to_owned(),
            summary: "Use generic macOS commands".to_owned(),
            target: WorkflowTarget::native_app(
                NativePlatform::MacOs,
                Some("TextEdit".to_owned()),
                "TextEdit".to_owned(),
            ),
            inputs: Vec::new(),
            primitives: vec![
                DesktopPrimitive::InvokeCommand {
                    command: CommandReference {
                        name: "New".to_owned(),
                        shortcut: Some("Cmd+N".to_owned()),
                        menu_path: Vec::new(),
                    },
                },
                DesktopPrimitive::InvokeCommand {
                    command: CommandReference {
                        name: "Export".to_owned(),
                        shortcut: None,
                        menu_path: vec!["File".to_owned(), "Export as PDF...".to_owned()],
                    },
                },
            ],
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };

        let compiled = compile_primitive_workflow(&workflow).expect("workflow should compile");
        assert_eq!(
            compiled.steps[0].required_capability,
            "macos.press_shortcut"
        );
        assert_eq!(compiled.steps[0].action, "press_shortcut");
        assert_eq!(compiled.steps[0].value.as_deref(), Some("Cmd+N"));
        assert_eq!(compiled.steps[1].required_capability, "macos.invoke_menu");
        assert_eq!(compiled.steps[1].action, "invoke_menu");
        assert_eq!(
            compiled.steps[1].value.as_deref(),
            Some("File > Export as PDF...")
        );
    }

    #[test]
    fn native_macos_key_actions_compile_to_shortcuts() {
        let workflow = DesktopWorkflow {
            id: "recorded-word".to_owned(),
            summary: "Recorded Word workflow".to_owned(),
            target: WorkflowTarget::native_app(
                NativePlatform::MacOs,
                Some("Microsoft Word".to_owned()),
                "Microsoft Word".to_owned(),
            ),
            inputs: Vec::new(),
            actions: vec![WorkflowAction {
                name: "new document".to_owned(),
                kind: WorkflowActionKind::Key,
                target: LocatorTarget::default(),
                value_template: Some("Cmd+N".to_owned()),
                risk: WorkflowRisk::Low,
            }],
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };

        let compiled = compile_workflow(&workflow).expect("workflow should compile");
        let shortcut = compiled
            .steps
            .iter()
            .find(|step| step.action == "press_shortcut")
            .expect("shortcut step");

        assert_eq!(shortcut.required_capability, "macos.press_shortcut");
        assert_eq!(shortcut.value.as_deref(), Some("Cmd+N"));
    }

    #[test]
    fn primitive_compiler_does_not_select_java_for_native_document_when_native_is_available() {
        let workflow = PrimitiveWorkflow {
            id: "native.document".to_owned(),
            summary: "Create a native document".to_owned(),
            target: WorkflowTarget::native_app(
                NativePlatform::MacOs,
                Some("Word".to_owned()),
                "Word".to_owned(),
            ),
            inputs: Vec::new(),
            primitives: vec![DesktopPrimitive::OpenApp {
                app: AppReference {
                    name: "Word".to_owned(),
                    bundle_id: None,
                    executable: None,
                    window_title: Some("Word".to_owned()),
                },
            }],
            outputs: Vec::new(),
            assertions: Vec::new(),
            evidence_policy: WorkflowEvidencePolicy::default(),
        };
        let context = WorkflowCompileContext {
            adapter_id: None,
            available_adapters: vec![
                AdapterCapabilities::new(
                    "greentic.desktop.java-accessibility",
                    "1.0.0",
                    ["java.find_window", "java.type_text", "java.read_text"],
                ),
                AdapterCapabilities::new(
                    "greentic.desktop.macos.ax",
                    "1.0.0",
                    ["macos.activate_app", "macos.type_text", "macos.read_text"],
                ),
            ],
        };

        let compiled = compile_primitive_workflow_with_context(&workflow, &context)
            .expect("primitive workflow should compile");

        assert_eq!(compiled.steps[0].required_capability, "macos.activate_app");
    }
}
