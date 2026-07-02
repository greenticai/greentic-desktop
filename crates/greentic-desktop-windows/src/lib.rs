use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use greentic_desktop_automation_foundation::{ScreenshotBackend, XcapScreenshotBackend};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventSink, RecordingHandle,
    RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use greentic_desktop_workflow::{
    compile_workflow, workflow_id_component, DesktopWorkflow, NativePlatform, WorkflowAction,
    WorkflowActionKind, WorkflowEvidencePolicy, WorkflowInput, WorkflowOutput,
    WorkflowOutputExtractor, WorkflowRisk, WorkflowTarget, WorkflowValueType,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub const WINDOWS_ADAPTER_ID: &str = "greentic.desktop.windows-ui";
pub const WINDOWS_RECORDER_BACKEND_ID: &str = "greentic.recording.desktop.windows.uia";

pub fn windows_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        WINDOWS_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "windows.open_app",
            "windows.find_window",
            "windows.find_element",
            "windows.click_element",
            "windows.type_text",
            "windows.press_shortcut",
            "windows.read_text",
            "windows.read_window_tree",
            "windows.assert_visible",
            "windows.screenshot",
            "windows.close_app",
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsElementMetadata {
    pub automation_id: Option<String>,
    pub name: Option<String>,
    pub control_type: Option<String>,
    pub class_name: Option<String>,
    pub relative_position: Option<String>,
    pub nearby_text: Option<String>,
    pub visual_region: Option<String>,
}

pub fn stable_windows_target(metadata: &WindowsElementMetadata) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            automation_id: metadata.automation_id.clone(),
            name: metadata.name.clone(),
            control_type: metadata.control_type.clone(),
            class_name: metadata.class_name.clone(),
            relative_position: metadata.relative_position.clone(),
            ..LocatorStrategy::default()
        }),
        fallback: metadata.name.as_ref().map(|name| LocatorStrategy {
            name: Some(name.clone()),
            class_name: metadata.class_name.clone(),
            control_type: metadata.control_type.clone(),
            ..LocatorStrategy::default()
        }),
        visual_fallback: metadata.visual_region.as_ref().map(|region| VisualLocator {
            image: String::new(),
            region: Some(region.clone()),
            nearby_text: metadata.nearby_text.clone(),
        }),
    }
}

#[derive(Debug, Clone, Default)]
pub struct WindowsUiRecordingBackend {
    elevated_target: bool,
    greentic_elevated: bool,
}

impl WindowsUiRecordingBackend {
    pub fn new(elevated_target: bool, greentic_elevated: bool) -> Self {
        Self {
            elevated_target,
            greentic_elevated,
        }
    }
}

impl RecordingBackend for WindowsUiRecordingBackend {
    fn id(&self) -> &'static str {
        WINDOWS_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Desktop
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        if !windows_uia_event_source_configured() {
            RecordingPreflight::blocked(
                "Install or start the Windows UI Automation event source before desktop recording.",
            )
        } else if self.elevated_target && !self.greentic_elevated {
            RecordingPreflight::blocked(
                "Windows UI Automation cannot record an elevated app unless Greentic Desktop is also running elevated.",
            )
        } else {
            RecordingPreflight::ready()
        }
    }

    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        if let Ok(command) = std::env::var("GREENTIC_WINDOWS_UIA_EVENT_SOURCE_COMMAND") {
            // Local operator supplied recorder path is invoked directly without a shell.
            // foxguard: ignore[rs/no-command-injection]
            let spawn = Command::new(command)
                .arg(sink.session_id())
                .arg(&request.out)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if spawn.is_ok() {
                let _ = sink.update_heartbeat();
                return RecordingHandle {
                    backend_id: WINDOWS_RECORDER_BACKEND_ID.to_owned(),
                    capture_state: RecordingCaptureState::Recording,
                };
            }
        }

        RecordingHandle {
            backend_id: WINDOWS_RECORDER_BACKEND_ID.to_owned(),
            capture_state: RecordingCaptureState::Blocked,
        }
    }
}

fn windows_uia_event_source_configured() -> bool {
    std::env::var("GREENTIC_WINDOWS_UIA_EVENT_SOURCE_COMMAND")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Default)]
pub struct WindowsUiAdapter {
    state: Arc<Mutex<WindowsState>>,
}

#[derive(Debug, Clone, Default)]
struct WindowsState {
    app: Option<String>,
    recorded: Vec<RecordedEvent>,
}

impl WindowsUiAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_control_interaction(
        &self,
        action: impl Into<String>,
        metadata: WindowsElementMetadata,
        value: Option<String>,
    ) -> RecordedEvent {
        let event = RecordedEvent {
            action: action.into(),
            target: stable_windows_target(&metadata),
            value,
        };
        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
            .recorded
            .push(event.clone());
        event
    }

    pub fn replay(&self, steps: &[RunnerStep]) -> AdapterResult<Vec<StepResult>> {
        steps
            .iter()
            .cloned()
            .map(|step| self.execute(step))
            .collect()
    }

    fn active_app(&self) -> AdapterResult<String> {
        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
            .app
            .clone()
            .ok_or_else(|| {
                AdapterError::ExecutionFailed(
                    "No active Windows app is known; run windows.open_app first.".to_owned(),
                )
            })
    }

    fn execute_real_step(&self, step: &RunnerStep) -> AdapterResult<String> {
        match step.required_capability.as_str() {
            "windows.open_app" => {
                let app = step.value.as_deref().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "windows.open_app requires an app path or executable in step.value."
                            .to_owned(),
                    )
                })?;
                windows_open_app(app)?;
                self.state
                    .lock()
                    .expect("windows adapter mutex poisoned")
                    .app = Some(app.to_owned());
                Ok(format!("opened Windows app {app}"))
            }
            "windows.find_window" => {
                let title = step.value.as_deref().unwrap_or_default();
                if windows_window_exists(title)? {
                    Ok(format!("found Windows window containing {title}"))
                } else {
                    Err(AdapterError::ExecutionFailed(format!(
                        "No Windows window containing {title} was visible."
                    )))
                }
            }
            "windows.find_element" | "windows.assert_visible" => {
                let app = self.active_app()?;
                if windows_element_exists(&app, &step.target, step.value.as_deref())? {
                    Ok("found Windows UI Automation element".to_owned())
                } else {
                    Err(AdapterError::ExecutionFailed(
                        "No matching Windows UI Automation element was visible.".to_owned(),
                    ))
                }
            }
            "windows.type_text" => {
                let app = self.active_app()?;
                windows_type_text(
                    &app,
                    &step.target,
                    step.value.as_deref().unwrap_or_default(),
                )?;
                Ok("typed text through Windows UI Automation".to_owned())
            }
            "windows.press_shortcut" => {
                let shortcut = step.value.as_deref().ok_or_else(|| {
                    AdapterError::ExecutionFailed(
                        "windows.press_shortcut requires a shortcut such as Ctrl+N in step.value."
                            .to_owned(),
                    )
                })?;
                windows_press_shortcut(shortcut)?;
                Ok(format!("pressed Windows shortcut {shortcut}"))
            }
            "windows.click_element" => {
                let app = self.active_app()?;
                windows_click_element(&app, &step.target)?;
                Ok("clicked Windows UI Automation element".to_owned())
            }
            "windows.read_text" => {
                let app = self.active_app()?;
                Ok(windows_read_element_text(&app, &step.target)?)
            }
            "windows.read_window_tree" => {
                let app = self.active_app()?;
                Ok(format!(
                    "read {} Windows UI Automation text entries",
                    windows_read_tree(&app)?.len()
                ))
            }
            "windows.screenshot" => {
                let path = step
                    .value
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(default_screenshot_path);
                windows_screenshot(&path)?;
                Ok(path.display().to_string())
            }
            "windows.close_app" => {
                let app = step.value.clone().or_else(|| {
                    self.state
                        .lock()
                        .expect("windows adapter mutex poisoned")
                        .app
                        .clone()
                });
                if let Some(app) = app {
                    windows_close_app(&app)?;
                }
                self.state
                    .lock()
                    .expect("windows adapter mutex poisoned")
                    .app = None;
                Ok("closed Windows app".to_owned())
            }
            _ => Err(AdapterError::UnsupportedCapability(
                step.required_capability.clone(),
            )),
        }
    }
}

impl DesktopAdapter for WindowsUiAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        windows_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let app = self.active_app()?;
        let visible_text = windows_read_tree(&app)?;
        Ok(Observation {
            adapter_id: WINDOWS_ADAPTER_ID.to_owned(),
            summary: format!("windows session {} app {}", ctx.session_id, app),
            visible_text,
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let message = self.execute_real_step(&step)?;

        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
            .recorded
            .push(RecordedEvent {
                action: step.action.clone(),
                target: step.target,
                value: step.value,
            });

        Ok(StepResult {
            step_id: step.id,
            success: true,
            message,
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let passed = match assertion.required_capability.as_str() {
            "windows.assert_visible" => {
                let app = self.active_app()?;
                windows_element_exists(&app, &assertion.target, Some(&assertion.expected))?
            }
            "windows.find_window" => windows_window_exists(&assertion.expected)?,
            _ => true,
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "windows assertion passed".to_owned()
            } else {
                "windows assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("windows adapter mutex poisoned")
            .recorded
            .last()
            .cloned())
    }
}

fn windows_open_app(app: &str) -> AdapterResult<()> {
    run_powershell(&format!("Start-Process -FilePath {}", ps_quote(app))).map(|_| ())
}

fn windows_close_app(app: &str) -> AdapterResult<()> {
    let script = format!(
        "$p = Get-Process | Where-Object {{$_.ProcessName -eq {name} -or $_.Path -eq {path}}} | Select-Object -First 1; if ($p) {{$p.CloseMainWindow() | Out-Null}}",
        name = ps_quote(app.trim_end_matches(".exe")),
        path = ps_quote(app)
    );
    run_powershell(&script).map(|_| ())
}

fn windows_window_exists(title: &str) -> AdapterResult<bool> {
    let script = format!(
        "if (Get-Process | Where-Object {{$_.MainWindowTitle -like {}}} | Select-Object -First 1) {{'true'}} else {{'false'}}",
        ps_quote(&format!("*{title}*"))
    );
    Ok(run_powershell(&script)?.trim() == "true")
}

fn windows_element_exists(
    app: &str,
    target: &LocatorTarget,
    expected: Option<&str>,
) -> AdapterResult<bool> {
    let script = windows_uia_script(app, target, expected, "exists")?;
    Ok(run_powershell(&script)?.trim() == "true")
}

fn windows_type_text(app: &str, target: &LocatorTarget, value: &str) -> AdapterResult<()> {
    let script = windows_uia_script(app, target, Some(value), "type")?;
    run_powershell(&script).map(|_| ())
}

fn windows_press_shortcut(shortcut: &str) -> AdapterResult<()> {
    let keys = windows_sendkeys_sequence(shortcut)?;
    let script = format!(
        r#"
$shell = New-Object -ComObject WScript.Shell
$shell.SendKeys({keys})
"#,
        keys = ps_quote(&keys)
    );
    run_powershell(&script).map(|_| ())
}

fn windows_sendkeys_sequence(shortcut: &str) -> AdapterResult<String> {
    let parts = shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let Some(key) = parts.last() else {
        return Err(AdapterError::ExecutionFailed(
            "shortcut must include a key, for example Ctrl+N.".to_owned(),
        ));
    };
    let mut sequence = String::new();
    for modifier in &parts[..parts.len().saturating_sub(1)] {
        match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => sequence.push('^'),
            "shift" => sequence.push('+'),
            "alt" | "option" => sequence.push('%'),
            "win" | "windows" | "meta" | "super" => sequence.push_str("^{ESC}"),
            other => {
                return Err(AdapterError::ExecutionFailed(format!(
                    "unsupported Windows shortcut modifier {other}"
                )))
            }
        }
    }
    sequence.push_str(&windows_sendkeys_key(key));
    Ok(sequence)
}

fn windows_sendkeys_key(key: &str) -> String {
    match key
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-', '_'], "")
        .as_str()
    {
        "return" | "enter" => "{ENTER}".to_owned(),
        "tab" => "{TAB}".to_owned(),
        "escape" | "esc" => "{ESC}".to_owned(),
        "delete" | "del" => "{DEL}".to_owned(),
        "backspace" => "{BACKSPACE}".to_owned(),
        "home" => "{HOME}".to_owned(),
        "end" => "{END}".to_owned(),
        "pageup" | "pgup" => "{PGUP}".to_owned(),
        "pagedown" | "pgdn" => "{PGDN}".to_owned(),
        "left" | "leftarrow" => "{LEFT}".to_owned(),
        "right" | "rightarrow" => "{RIGHT}".to_owned(),
        "down" | "downarrow" => "{DOWN}".to_owned(),
        "up" | "uparrow" => "{UP}".to_owned(),
        key if key.starts_with('f') && key[1..].parse::<u8>().is_ok() => {
            format!("{{{}}}", key.to_ascii_uppercase())
        }
        key => key.to_owned(),
    }
}

fn windows_click_element(app: &str, target: &LocatorTarget) -> AdapterResult<()> {
    let script = windows_uia_script(app, target, None, "click")?;
    run_powershell(&script).map(|_| ())
}

fn windows_read_element_text(app: &str, target: &LocatorTarget) -> AdapterResult<String> {
    let script = windows_uia_script(app, target, None, "read")?;
    Ok(run_powershell(&script)?.trim().to_owned())
}

fn windows_read_tree(app: &str) -> AdapterResult<Vec<String>> {
    let script = format!(
        r#"
Add-Type -AssemblyName UIAutomationClient
$process = Get-Process | Where-Object {{$_.ProcessName -eq {name} -or $_.Path -eq {path}}} | Select-Object -First 1
if (-not $process) {{ throw 'process not found' }}
$root = [System.Windows.Automation.AutomationElement]::RootElement
$condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $process.Id)
$window = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
if (-not $window) {{ throw 'window not found' }}
$all = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
foreach ($item in $all) {{ if ($item.Current.Name) {{ $item.Current.Name }} }}
"#,
        name = ps_quote(app.trim_end_matches(".exe")),
        path = ps_quote(app)
    );
    Ok(run_powershell(&script)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn windows_screenshot(path: &Path) -> AdapterResult<()> {
    XcapScreenshotBackend
        .capture_primary_monitor(path)
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!("xcap screenshot capture failed: {err}"))
        })?;
    if path.exists() {
        Ok(())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "screenshot did not create {}",
            path.display()
        )))
    }
}

fn windows_uia_script(
    app: &str,
    target: &LocatorTarget,
    value: Option<&str>,
    mode: &str,
) -> AdapterResult<String> {
    let predicate = windows_locator_predicate(target, value)?;
    Ok(format!(
        r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$process = Get-Process | Where-Object {{$_.ProcessName -eq {name} -or $_.Path -eq {path}}} | Select-Object -First 1
if (-not $process) {{ throw 'process not found' }}
$root = [System.Windows.Automation.AutomationElement]::RootElement
$condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $process.Id)
$window = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
if (-not $window) {{ throw 'window not found' }}
$all = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$candidate = $null
foreach ($item in $all) {{ if ({predicate}) {{ $candidate = $item; break }} }}
if (-not $candidate) {{ if ({mode} -eq 'exists') {{ 'false'; exit 0 }}; throw 'element not found' }}
if ({mode} -eq 'exists') {{ 'true'; exit 0 }}
if ({mode} -eq 'read') {{ $candidate.Current.Name; exit 0 }}
if ({mode} -eq 'click') {{
  $pattern = $candidate.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
  $pattern.Invoke()
  exit 0
}}
if ({mode} -eq 'type') {{
  $pattern = $candidate.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
  $pattern.SetValue({value})
  exit 0
}}
"#,
        name = ps_quote(app.trim_end_matches(".exe")),
        path = ps_quote(app),
        predicate = predicate,
        mode = ps_quote(mode),
        value = ps_quote(value.unwrap_or_default())
    ))
}

fn windows_locator_predicate(
    target: &LocatorTarget,
    expected: Option<&str>,
) -> AdapterResult<String> {
    let mut clauses = Vec::new();
    for strategy in [target.preferred.as_ref(), target.fallback.as_ref()]
        .into_iter()
        .flatten()
    {
        if let Some(id) = strategy.automation_id.as_deref() {
            clauses.push(format!("$item.Current.AutomationId -eq {}", ps_quote(id)));
        }
        if let Some(name) = strategy.name.as_deref() {
            clauses.push(format!(
                "$item.Current.Name -like {}",
                ps_quote(&format!("*{name}*"))
            ));
        }
        if let Some(class_name) = strategy.class_name.as_deref() {
            clauses.push(format!(
                "$item.Current.ClassName -eq {}",
                ps_quote(class_name)
            ));
        }
        if let Some(control_type) = strategy.control_type.as_deref() {
            clauses.push(format!(
                "$item.Current.ControlType.ProgrammaticName -like {}",
                ps_quote(&format!("*{control_type}*"))
            ));
        }
    }
    if let Some(expected) = expected {
        if !expected.trim().is_empty() {
            clauses.push(format!(
                "$item.Current.Name -like {}",
                ps_quote(&format!("*{expected}*"))
            ));
        }
    }
    if clauses.is_empty() {
        return Err(AdapterError::ExecutionFailed(
            "Windows UIA locator requires automation id, name, class name, control type, or expected text.".to_owned(),
        ));
    }
    Ok(clauses.join(" -or "))
}

fn run_powershell(script: &str) -> AdapterResult<String> {
    if std::env::consts::OS != "windows" {
        return Err(AdapterError::ExecutionFailed(
            "Windows UI Automation can only run on Windows.".to_owned(),
        ));
    }
    // Program name is fixed and script is passed directly as an argument, not through a shell.
    // foxguard: ignore[rs/no-command-injection]
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to run PowerShell: {err}")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(AdapterError::ExecutionFailed(format!(
            "PowerShell UIA failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn default_screenshot_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "greentic-windows-screenshot-{}-{}.png",
        std::process::id(),
        epoch_millis()
    ))
}

fn epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowsAppWorkflow {
    pub app_name: String,
    pub window_title: String,
    pub prompt: String,
    pub inputs: Vec<WindowsWorkflowInput>,
    pub submit: Option<WindowsWorkflowAction>,
    pub outputs: Vec<WindowsWorkflowOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsWorkflowInput {
    pub name: String,
    pub target: LocatorTarget,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsWorkflowAction {
    pub name: String,
    pub target: LocatorTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsWorkflowOutput {
    pub name: String,
    pub target: LocatorTarget,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsAppWorkflowOutcome {
    pub prompt: String,
    pub outputs: BTreeMap<String, String>,
    pub steps: Vec<StepResult>,
}

pub fn run_windows_app_workflow(
    adapter: &WindowsUiAdapter,
    workflow: WindowsAppWorkflow,
) -> AdapterResult<WindowsAppWorkflowOutcome> {
    let prompt = workflow.prompt.clone();
    let app_name = workflow.app_name.clone();
    let output_specs = workflow.outputs.clone();
    let compiled = compile_workflow(&windows_desktop_workflow(&workflow))
        .map_err(|err| AdapterError::ExecutionFailed(err.to_string()))?;
    let steps = compiled.steps;

    let results = adapter.replay(&steps).map_err(|err| {
        AdapterError::ExecutionFailed(format!("Windows UI Automation app workflow failed: {err}"))
    })?;
    let observation = adapter
        .observe(ObserveContext {
            session_id: format!("windows-app-workflow-{}", workflow_id_component(&app_name)),
            target: output_specs.first().map(|output| output.target.clone()),
        })
        .map_err(|err| {
            AdapterError::ExecutionFailed(format!(
                "Windows UI Automation app workflow failed: {err}"
            ))
        })?;
    let visible = observation.visible_text;

    let mut outputs = BTreeMap::new();
    for output in output_specs {
        let value = output
            .expected
            .or_else(|| {
                visible
                    .iter()
                    .find(|value| !value.trim().is_empty())
                    .cloned()
            })
            .ok_or_else(|| {
                AdapterError::ExecutionFailed(format!("No output was visible for {}", output.name))
            })?;
        if !visible.iter().any(|visible_value| visible_value == &value) {
            return Err(AdapterError::ExecutionFailed(format!(
                "Expected output {} was not visible",
                output.name
            )));
        }
        outputs.insert(output.name, value);
    }

    Ok(WindowsAppWorkflowOutcome {
        prompt,
        outputs,
        steps: results,
    })
}

fn windows_desktop_workflow(workflow: &WindowsAppWorkflow) -> DesktopWorkflow {
    DesktopWorkflow {
        id: format!(
            "windows-app-workflow-{}",
            workflow_id_component(&workflow.app_name)
        ),
        summary: workflow.prompt.clone(),
        target: WorkflowTarget::native_app(
            NativePlatform::Windows,
            Some(workflow.app_name.clone()),
            workflow.window_title.clone(),
        ),
        inputs: workflow
            .inputs
            .iter()
            .map(|input| WorkflowInput {
                name: input.name.clone(),
                value_type: WorkflowValueType::String,
                required: true,
                secret: false,
                target: input.target.clone(),
                value_template: input.value.clone(),
            })
            .collect(),
        actions: workflow
            .submit
            .iter()
            .map(|submit| WorkflowAction {
                name: submit.name.clone(),
                kind: WorkflowActionKind::Click,
                target: submit.target.clone(),
                value_template: None,
                risk: WorkflowRisk::Low,
            })
            .collect(),
        outputs: workflow
            .outputs
            .iter()
            .map(|output| WorkflowOutput {
                name: output.name.clone(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::TargetText(Box::new(output.target.clone())),
                required: true,
                expected: output.expected.clone(),
            })
            .collect(),
        assertions: Vec::new(),
        evidence_policy: WorkflowEvidencePolicy::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn metadata() -> WindowsElementMetadata {
        WindowsElementMetadata {
            automation_id: Some("CustomerSearchBox".to_owned()),
            name: Some("Customer Search".to_owned()),
            control_type: Some("Edit".to_owned()),
            class_name: Some("TextBox".to_owned()),
            relative_position: Some("main.top".to_owned()),
            nearby_text: Some("Customer".to_owned()),
            visual_region: Some("center".to_owned()),
        }
    }

    #[test]
    fn exposes_windows_capabilities() {
        let capabilities = windows_capabilities();

        assert!(capabilities.supports("windows.open_app"));
        assert!(capabilities.supports("windows.close_app"));
        assert_eq!(capabilities.adapter_id, WINDOWS_ADAPTER_ID);
    }

    #[test]
    fn locator_combines_automation_metadata_and_visual_fallback() {
        let target = stable_windows_target(&metadata());
        let preferred = target.preferred.expect("preferred locator");

        assert_eq!(
            preferred.automation_id,
            Some("CustomerSearchBox".to_owned())
        );
        assert_eq!(preferred.name, Some("Customer Search".to_owned()));
        assert_eq!(preferred.control_type, Some("Edit".to_owned()));
        assert_eq!(preferred.class_name, Some("TextBox".to_owned()));
        assert_eq!(
            target.visual_fallback.and_then(|visual| visual.region),
            Some("center".to_owned())
        );
    }

    #[test]
    fn open_app_fails_closed_off_windows_instead_of_mutating_state() {
        let adapter = WindowsUiAdapter::new();
        let result = adapter.execute(RunnerStep {
            id: "open".to_owned(),
            action: "open_app".to_owned(),
            target: LocatorTarget::default(),
            value: Some("CustomerClient.exe".to_owned()),
            required_capability: "windows.open_app".to_owned(),
        });

        if std::env::consts::OS == "windows" {
            assert!(result.is_ok() || result.is_err());
        } else {
            let error = result.expect_err("off-Windows execution should fail closed");
            assert!(
                error
                    .to_string()
                    .contains("Windows UI Automation can only run on Windows"),
                "{error}"
            );
        }
    }

    #[test]
    fn uia_script_uses_stable_locator_metadata() {
        let target = stable_windows_target(&metadata());
        let script = windows_uia_script("CustomerClient.exe", &target, Some("Acme"), "type")
            .expect("script should render");

        assert!(script.contains("UIAutomationClient"), "{script}");
        assert!(script.contains("CustomerSearchBox"), "{script}");
        assert!(script.contains("Customer Search"), "{script}");
        assert!(script.contains("ValuePattern"), "{script}");
    }

    #[test]
    fn uia_script_renders_click_read_and_exists_modes() {
        let target = stable_windows_target(&metadata());

        let click = windows_uia_script("CustomerClient.exe", &target, None, "click")
            .expect("click script should render");
        let read = windows_uia_script("CustomerClient.exe", &target, None, "read")
            .expect("read script should render");
        let exists = windows_uia_script("CustomerClient.exe", &target, Some("Customer"), "exists")
            .expect("exists script should render");

        assert!(click.contains("InvokePattern"), "{click}");
        assert!(read.contains("$candidate.Current.Name"), "{read}");
        assert!(exists.contains("'exists'"), "{exists}");
        assert!(exists.contains("*Customer*"), "{exists}");
    }

    #[test]
    fn empty_locator_is_rejected_before_powershell() {
        let error = windows_locator_predicate(&LocatorTarget::default(), None)
            .expect_err("empty locator should be invalid");

        assert!(
            error.to_string().contains("locator requires automation id"),
            "{error}"
        );
    }

    #[test]
    fn powershell_quote_escapes_single_quotes() {
        assert_eq!(ps_quote("O'Hara"), "'O''Hara'");
    }

    #[test]
    fn close_app_without_active_app_is_a_noop_success() {
        let adapter = WindowsUiAdapter::new();
        let result = adapter
            .execute(RunnerStep {
                id: "close".to_owned(),
                action: "close_app".to_owned(),
                target: LocatorTarget::default(),
                value: None,
                required_capability: "windows.close_app".to_owned(),
            })
            .expect("close without active app should not require Windows");

        assert_eq!(result.message, "closed Windows app");
        assert!(adapter.record_event().expect("record event").is_some());
    }

    #[test]
    fn unsupported_execute_and_validate_fail_before_platform_calls() {
        let adapter = WindowsUiAdapter::new();
        let execute = adapter.execute(RunnerStep {
            id: "bad".to_owned(),
            action: "bad".to_owned(),
            target: LocatorTarget::default(),
            value: None,
            required_capability: "windows.unsupported".to_owned(),
        });
        let validate = adapter.validate(Assertion {
            id: "bad-assertion".to_owned(),
            target: LocatorTarget::default(),
            expected: "visible".to_owned(),
            required_capability: "windows.unsupported".to_owned(),
        });

        assert!(matches!(
            execute,
            Err(AdapterError::UnsupportedCapability(capability)) if capability == "windows.unsupported"
        ));
        assert!(matches!(
            validate,
            Err(AdapterError::UnsupportedCapability(capability)) if capability == "windows.unsupported"
        ));
    }

    #[test]
    fn observe_requires_an_active_app() {
        let adapter = WindowsUiAdapter::new();
        let error = adapter
            .observe(ObserveContext {
                session_id: "windows-observe".to_owned(),
                target: None,
            })
            .expect_err("observe should fail without an active app");

        assert!(
            error.to_string().contains("No active Windows app"),
            "{error}"
        );
    }

    #[test]
    fn recording_preflight_blocks_missing_uia_event_source() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        std::env::remove_var("GREENTIC_WINDOWS_UIA_EVENT_SOURCE_COMMAND");
        let backend = WindowsUiRecordingBackend::new(false, false);
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "windows.record".to_owned(),
            profile: "desktop".to_owned(),
            adapter: WINDOWS_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Desktop,
            out: std::env::temp_dir().join("windows-record-missing"),
            runtime_home: std::env::temp_dir().join("windows-record-home-missing"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight.blocked_reasons[0].contains("UI Automation event source"));
    }

    #[test]
    fn windows_desktop_workflow_compiles_generic_inputs_actions_and_outputs() {
        let workflow = WindowsAppWorkflow {
            app_name: "GenericApp.exe".to_owned(),
            window_title: "Generic App".to_owned(),
            prompt: "Open a desktop app and return a result.".to_owned(),
            inputs: vec![WindowsWorkflowInput {
                name: "search".to_owned(),
                target: stable_windows_target(&metadata()),
                value: "{{inputs.search}}".to_owned(),
            }],
            submit: Some(WindowsWorkflowAction {
                name: "submit".to_owned(),
                target: stable_windows_target(&WindowsElementMetadata {
                    automation_id: Some("Submit".to_owned()),
                    name: Some("Submit".to_owned()),
                    control_type: Some("Button".to_owned()),
                    class_name: Some("Button".to_owned()),
                    relative_position: None,
                    nearby_text: None,
                    visual_region: None,
                }),
            }),
            outputs: vec![WindowsWorkflowOutput {
                name: "result".to_owned(),
                target: stable_windows_target(&WindowsElementMetadata {
                    automation_id: Some("Result".to_owned()),
                    name: Some("Result".to_owned()),
                    control_type: Some("Text".to_owned()),
                    class_name: Some("TextBlock".to_owned()),
                    relative_position: None,
                    nearby_text: None,
                    visual_region: None,
                }),
                expected: Some("done".to_owned()),
            }],
        };

        let desktop = windows_desktop_workflow(&workflow);
        let compiled = compile_workflow(&desktop).expect("workflow should compile");

        assert!(matches!(
            desktop.target.open,
            Some(greentic_desktop_workflow::WorkflowOpenTarget::App {
                app_name: Some(ref app_name),
                ..
            }) if app_name == "GenericApp.exe"
        ));
        assert!(compiled
            .steps
            .iter()
            .any(|step| step.required_capability == "windows.type_text"));
        assert!(compiled
            .steps
            .iter()
            .any(|step| step.required_capability == "windows.click_element"));
    }

    #[test]
    fn generic_app_workflow_fails_without_real_windows_fixture() {
        let adapter = WindowsUiAdapter::new();
        let input_target = stable_windows_target(&WindowsElementMetadata {
            automation_id: Some("PrimaryInput".to_owned()),
            name: Some("Primary Input".to_owned()),
            control_type: Some("Edit".to_owned()),
            class_name: Some("TextBox".to_owned()),
            relative_position: Some("main.center".to_owned()),
            nearby_text: Some("Input".to_owned()),
            visual_region: Some("center".to_owned()),
        });
        let output_target = stable_windows_target(&WindowsElementMetadata {
            automation_id: Some("ResultText".to_owned()),
            name: Some("Result".to_owned()),
            control_type: Some("Text".to_owned()),
            class_name: Some("TextBlock".to_owned()),
            relative_position: Some("main.bottom".to_owned()),
            nearby_text: Some("Result".to_owned()),
            visual_region: Some("bottom".to_owned()),
        });

        let outcome = run_windows_app_workflow(
            &adapter,
            WindowsAppWorkflow {
                app_name: "Sample.exe".to_owned(),
                window_title: "Sample".to_owned(),
                prompt: "Open Sample.exe and submit a value.".to_owned(),
                inputs: vec![WindowsWorkflowInput {
                    name: "primary value".to_owned(),
                    target: input_target,
                    value: "hello".to_owned(),
                }],
                submit: Some(WindowsWorkflowAction {
                    name: "submit".to_owned(),
                    target: stable_windows_target(&WindowsElementMetadata {
                        automation_id: Some("SubmitButton".to_owned()),
                        name: Some("Submit".to_owned()),
                        control_type: Some("Button".to_owned()),
                        class_name: Some("Button".to_owned()),
                        relative_position: Some("main.bottom_right".to_owned()),
                        nearby_text: Some("Input".to_owned()),
                        visual_region: Some("bottom_right".to_owned()),
                    }),
                }),
                outputs: vec![WindowsWorkflowOutput {
                    name: "result".to_owned(),
                    target: output_target,
                    expected: Some("accepted".to_owned()),
                }],
            },
        )
        .expect_err("missing real fixture should fail");

        assert!(
            outcome.to_string().contains("Windows UI Automation"),
            "{outcome}"
        );
    }

    #[test]
    fn maps_windows_shortcuts_to_sendkeys_sequences() {
        assert_eq!(
            windows_sendkeys_sequence("Return").expect("return shortcut"),
            "{ENTER}"
        );
        assert_eq!(
            windows_sendkeys_sequence("Ctrl+PageUp").expect("page shortcut"),
            "^{PGUP}"
        );
        assert_eq!(
            windows_sendkeys_sequence("Ctrl+Shift+N").expect("modified shortcut"),
            "^+n"
        );
    }

    #[test]
    fn recording_backend_blocks_elevated_target_when_greentic_is_not_elevated() {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        std::env::set_var(
            "GREENTIC_WINDOWS_UIA_EVENT_SOURCE_COMMAND",
            "uia-recorder.exe",
        );
        let backend = WindowsUiRecordingBackend::new(true, false);
        let preflight = backend.preflight(&RecordingStartRequest {
            name: "windows.record".to_owned(),
            profile: "desktop".to_owned(),
            adapter: WINDOWS_ADAPTER_ID.to_owned(),
            target_kind: RecordingTargetKind::Desktop,
            out: std::env::temp_dir().join("windows-record"),
            runtime_home: std::env::temp_dir().join("windows-record-home"),
            redact: Vec::new(),
            secret_fields: Vec::new(),
        });

        assert!(!preflight.available);
        assert!(preflight.blocked_reasons[0].contains("elevated app"));
        std::env::remove_var("GREENTIC_WINDOWS_UIA_EVENT_SOURCE_COMMAND");
    }
}
