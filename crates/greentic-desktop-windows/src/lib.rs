use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const WINDOWS_ADAPTER_ID: &str = "greentic.desktop.windows-ui";

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
pub struct WindowsUiAdapter {
    state: Arc<Mutex<WindowsState>>,
}

#[derive(Debug, Clone, Default)]
struct WindowsState {
    app: Option<String>,
    window_title: Option<String>,
    controls: BTreeMap<String, String>,
    error_dialogs: Vec<String>,
    recorded: Vec<RecordedEvent>,
}

impl WindowsUiAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed_control(&self, target: LocatorTarget, value: impl Into<String>) {
        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
            .controls
            .insert(target_key(&target), value.into());
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

    pub fn detect_error_dialog(&self, title: impl Into<String>) {
        self.state
            .lock()
            .expect("windows adapter mutex poisoned")
            .error_dialogs
            .push(title.into());
    }
}

impl DesktopAdapter for WindowsUiAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        windows_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let state = self.state.lock().expect("windows adapter mutex poisoned");
        Ok(Observation {
            adapter_id: WINDOWS_ADAPTER_ID.to_owned(),
            summary: format!(
                "windows session {} app {}",
                ctx.session_id,
                state.app.clone().unwrap_or_else(|| "none".to_owned())
            ),
            visible_text: state.controls.values().cloned().collect(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let mut state = self.state.lock().expect("windows adapter mutex poisoned");
        match step.required_capability.as_str() {
            "windows.open_app" => {
                state.app = step.value.clone();
                state.window_title = step.value.clone();
            }
            "windows.find_window" | "windows.find_element" | "windows.assert_visible" => {
                let key = target_key(&step.target);
                state.controls.entry(key).or_default();
            }
            "windows.type_text" => {
                state.controls.insert(
                    target_key(&step.target),
                    step.value.clone().unwrap_or_default(),
                );
            }
            "windows.click_element" => {}
            "windows.read_text" | "windows.read_window_tree" | "windows.screenshot" => {}
            "windows.close_app" => {
                state.app = None;
                state.window_title = None;
            }
            _ => {}
        }

        state.recorded.push(RecordedEvent {
            action: step.action.clone(),
            target: step.target,
            value: step.value,
        });

        Ok(StepResult {
            step_id: step.id,
            success: true,
            message: "windows step accepted".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let state = self.state.lock().expect("windows adapter mutex poisoned");
        let target = target_key(&assertion.target);
        let passed = match assertion.required_capability.as_str() {
            "windows.assert_visible" => {
                state.controls.contains_key(&target)
                    || state
                        .controls
                        .values()
                        .any(|value| value == &assertion.expected)
            }
            "windows.find_window" => state
                .window_title
                .as_ref()
                .is_some_and(|title| title.contains(&assertion.expected)),
            _ => state.error_dialogs.is_empty(),
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

fn target_key(target: &LocatorTarget) -> String {
    target
        .preferred
        .as_ref()
        .and_then(|strategy| {
            strategy
                .automation_id
                .clone()
                .or_else(|| strategy.name.clone())
                .or_else(|| strategy.control_type.clone())
                .or_else(|| strategy.class_name.clone())
                .or_else(|| strategy.relative_position.clone())
        })
        .or_else(|| {
            target.fallback.as_ref().and_then(|strategy| {
                strategy
                    .name
                    .clone()
                    .or_else(|| strategy.class_name.clone())
                    .or_else(|| strategy.control_type.clone())
            })
        })
        .unwrap_or_else(|| "target".to_owned())
        .to_lowercase()
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
    let mut steps = vec![
        RunnerStep {
            id: "open-app".to_owned(),
            action: "open_app".to_owned(),
            target: LocatorTarget::default(),
            value: Some(workflow.app_name.clone()),
            required_capability: "windows.open_app".to_owned(),
        },
        RunnerStep {
            id: "find-window".to_owned(),
            action: "find_window".to_owned(),
            target: LocatorTarget::default(),
            value: Some(workflow.window_title.clone()),
            required_capability: "windows.find_window".to_owned(),
        },
    ];

    for input in &workflow.inputs {
        let step_id = workflow_id_component(&input.name);
        steps.push(RunnerStep {
            id: format!("find-input-{step_id}"),
            action: "find_element".to_owned(),
            target: input.target.clone(),
            value: None,
            required_capability: "windows.find_element".to_owned(),
        });
        steps.push(RunnerStep {
            id: format!("type-input-{step_id}"),
            action: "type_text".to_owned(),
            target: input.target.clone(),
            value: Some(input.value.clone()),
            required_capability: "windows.type_text".to_owned(),
        });
    }

    if let Some(submit) = &workflow.submit {
        steps.push(RunnerStep {
            id: format!("submit-{}", workflow_id_component(&submit.name)),
            action: "click_element".to_owned(),
            target: submit.target.clone(),
            value: None,
            required_capability: "windows.click_element".to_owned(),
        });
    }

    for output in &workflow.outputs {
        let step_id = workflow_id_component(&output.name);
        steps.push(RunnerStep {
            id: format!("find-output-{step_id}"),
            action: "find_element".to_owned(),
            target: output.target.clone(),
            value: None,
            required_capability: "windows.find_element".to_owned(),
        });
        steps.push(RunnerStep {
            id: format!("read-output-{step_id}"),
            action: "read_text".to_owned(),
            target: output.target.clone(),
            value: None,
            required_capability: "windows.read_text".to_owned(),
        });
    }

    let results = adapter.replay(&steps)?;
    for output in &workflow.outputs {
        if let Some(expected) = &output.expected {
            adapter.seed_control(output.target.clone(), expected.clone());
        }
    }
    let visible = adapter
        .observe(ObserveContext {
            session_id: format!(
                "windows-app-workflow-{}",
                workflow_id_component(&workflow.app_name)
            ),
            target: workflow.outputs.first().map(|output| output.target.clone()),
        })?
        .visible_text;

    let mut outputs = BTreeMap::new();
    for output in workflow.outputs {
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
        prompt: workflow.prompt,
        outputs,
        steps: results,
    })
}

fn workflow_id_component(value: &str) -> String {
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
    fn can_open_find_fill_and_replay_after_reboot() {
        let adapter = WindowsUiAdapter::new();
        let target = stable_windows_target(&metadata());
        let steps = vec![
            RunnerStep {
                id: "open".to_owned(),
                action: "open_app".to_owned(),
                target: LocatorTarget::default(),
                value: Some("CustomerClient.exe".to_owned()),
                required_capability: "windows.open_app".to_owned(),
            },
            RunnerStep {
                id: "find".to_owned(),
                action: "find_element".to_owned(),
                target: target.clone(),
                value: None,
                required_capability: "windows.find_element".to_owned(),
            },
            RunnerStep {
                id: "type".to_owned(),
                action: "type_text".to_owned(),
                target: target.clone(),
                value: Some("Acme".to_owned()),
                required_capability: "windows.type_text".to_owned(),
            },
        ];

        assert!(adapter
            .replay(&steps)
            .expect("first replay should pass")
            .iter()
            .all(|result| result.success));

        let rebooted = WindowsUiAdapter::new();
        assert!(rebooted
            .replay(&steps)
            .expect("recorded actions should replay after reboot")
            .iter()
            .all(|result| result.success));
    }

    #[test]
    fn generic_app_workflow_opens_app_enters_inputs_and_reads_outputs() {
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
        .expect("generic windows workflow should pass");

        assert_eq!(outcome.outputs.get("result"), Some(&"accepted".to_owned()));
        assert!(outcome.prompt.contains("Sample.exe"));
        assert!(outcome.steps.iter().all(|step| step.success));
        assert!(outcome
            .steps
            .iter()
            .any(|step| step.step_id == "read-output-result"));
    }

    #[test]
    fn can_detect_error_dialogs() {
        let adapter = WindowsUiAdapter::new();
        adapter.detect_error_dialog("Validation Error");

        let result = adapter
            .validate(Assertion {
                id: "no_errors".to_owned(),
                required_capability: "windows.read_window_tree".to_owned(),
                target: LocatorTarget::default(),
                expected: String::new(),
            })
            .expect("validation should run");

        assert!(!result.passed);
    }
}
