use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep, StepResult,
};
use greentic_desktop_workflow::{
    compile_workflow, workflow_id_component, DesktopWorkflow, TerminalField, WorkflowAction,
    WorkflowActionKind, WorkflowEvidencePolicy, WorkflowOutput, WorkflowOutputExtractor,
    WorkflowRisk, WorkflowTarget, WorkflowValueType,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const TERMINAL_ADAPTER_ID: &str = "greentic.desktop.terminal-tn3270";

pub fn terminal_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        TERMINAL_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "terminal.connect",
            "terminal.disconnect",
            "terminal.read_screen",
            "terminal.send_keys",
            "terminal.send_text",
            "terminal.type_text",
            "terminal.wait_for_screen",
            "terminal.assert_text",
            "terminal.extract_field",
            "terminal.capture_screen",
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalProtocol {
    Vt100,
    Vt220,
    Tn3270,
    Tn5250,
    Ssh,
    Serial,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalProfile {
    pub name: String,
    pub protocol: TerminalProtocol,
    pub host: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenField {
    pub row: usize,
    pub col: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalAdapter {
    state: Arc<Mutex<TerminalState>>,
}

#[derive(Debug, Clone, Default)]
struct TerminalState {
    connected: bool,
    profile: Option<TerminalProfile>,
    screen: Vec<String>,
    fields: BTreeMap<String, ScreenField>,
    recorded: Vec<RecordedEvent>,
}

impl TerminalAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connect_profile(&self, profile: TerminalProfile) {
        let mut state = self.state.lock().expect("terminal adapter mutex poisoned");
        state.connected = true;
        state.profile = Some(profile);
    }

    pub fn record_screen_buffer(&self, lines: impl IntoIterator<Item = impl Into<String>>) {
        self.state
            .lock()
            .expect("terminal adapter mutex poisoned")
            .screen = lines.into_iter().map(Into::into).collect();
    }

    pub fn define_field(&self, name: impl Into<String>, field: ScreenField) {
        self.state
            .lock()
            .expect("terminal adapter mutex poisoned")
            .fields
            .insert(name.into(), field);
    }

    pub fn extract_field(&self, field: ScreenField) -> Option<String> {
        let state = self.state.lock().expect("terminal adapter mutex poisoned");
        let line = state.screen.get(field.row)?;
        Some(
            line.chars()
                .skip(field.col)
                .take(field.len)
                .collect::<String>()
                .trim()
                .to_owned(),
        )
    }

    pub fn extract_after_anchor(&self, anchor: &str) -> Option<String> {
        let state = self.state.lock().expect("terminal adapter mutex poisoned");
        state.screen.iter().find_map(|line| {
            let index = line.find(anchor)?;
            Some(line[index + anchor.len()..].trim().to_owned())
        })
    }

    pub fn replay(&self, steps: &[RunnerStep]) -> AdapterResult<Vec<StepResult>> {
        steps
            .iter()
            .cloned()
            .map(|step| self.execute(step))
            .collect()
    }
}

impl DesktopAdapter for TerminalAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        terminal_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let state = self.state.lock().expect("terminal adapter mutex poisoned");
        Ok(Observation {
            adapter_id: TERMINAL_ADAPTER_ID.to_owned(),
            summary: format!(
                "terminal session {} connected={}",
                ctx.session_id, state.connected
            ),
            visible_text: state.screen.clone(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let mut state = self.state.lock().expect("terminal adapter mutex poisoned");
        match step.required_capability.as_str() {
            "terminal.connect" => {
                state.connected = true;
            }
            "terminal.disconnect" => state.connected = false,
            "terminal.type_text" | "terminal.send_text" => {
                let value = step.value.clone().unwrap_or_default();
                if !value.is_empty() {
                    state.screen.push(format!("INPUT: {}", value));
                }
            }
            "terminal.send_keys" => {}
            "terminal.read_screen"
            | "terminal.wait_for_screen"
            | "terminal.assert_text"
            | "terminal.extract_field"
            | "terminal.capture_screen" => {}
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
            message: "terminal step accepted".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let state = self.state.lock().expect("terminal adapter mutex poisoned");
        let passed = match assertion.required_capability.as_str() {
            "terminal.assert_text" | "terminal.wait_for_screen" => state
                .screen
                .iter()
                .any(|line| line.contains(&assertion.expected)),
            "terminal.extract_field" => state.fields.contains_key(&assertion.expected),
            _ => state.connected,
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "terminal assertion passed".to_owned()
            } else {
                "terminal assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("terminal adapter mutex poisoned")
            .recorded
            .last()
            .cloned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalWorkflow {
    pub profile: TerminalProfile,
    pub prompt: String,
    pub initial_screen: Vec<String>,
    pub actions: Vec<TerminalWorkflowAction>,
    pub final_screen: Vec<String>,
    pub text_outputs: Vec<TerminalTextOutput>,
    pub field_outputs: Vec<TerminalFieldOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalWorkflowAction {
    pub name: String,
    pub required_capability: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTextOutput {
    pub name: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFieldOutput {
    pub name: String,
    pub field: ScreenField,
    pub expected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalWorkflowOutcome {
    pub prompt: String,
    pub outputs: BTreeMap<String, String>,
    pub steps: Vec<StepResult>,
}

pub fn run_terminal_workflow(
    adapter: &TerminalAdapter,
    workflow: TerminalWorkflow,
) -> AdapterResult<TerminalWorkflowOutcome> {
    let prompt = workflow.prompt.clone();
    let text_outputs = workflow.text_outputs.clone();
    let field_outputs = workflow.field_outputs.clone();
    let compiled = compile_workflow(&terminal_desktop_workflow(&workflow))
        .map_err(|err| AdapterError::ExecutionFailed(err.to_string()))?;

    adapter.connect_profile(workflow.profile);
    if !workflow.initial_screen.is_empty() {
        adapter.record_screen_buffer(workflow.initial_screen);
    }

    let steps = compiled.steps;
    let results = adapter.replay(&steps)?;
    if !workflow.final_screen.is_empty() {
        adapter.record_screen_buffer(workflow.final_screen);
    }

    let visible = adapter
        .observe(ObserveContext {
            session_id: "terminal-workflow".to_owned(),
            target: None,
        })?
        .visible_text;

    let mut outputs = BTreeMap::new();
    for output in text_outputs {
        if !visible.iter().any(|line| line.contains(&output.expected)) {
            return Err(AdapterError::ExecutionFailed(format!(
                "Expected terminal text {} was not visible",
                output.name
            )));
        }
        outputs.insert(output.name, output.expected);
    }

    for output in field_outputs {
        let value = adapter.extract_field(output.field).ok_or_else(|| {
            AdapterError::ExecutionFailed(format!(
                "No terminal field was visible for {}",
                output.name
            ))
        })?;
        if let Some(expected) = &output.expected {
            if &value != expected {
                return Err(AdapterError::ExecutionFailed(format!(
                    "Expected terminal field {} to be {}",
                    output.name, expected
                )));
            }
        }
        outputs.insert(output.name, value);
    }

    Ok(TerminalWorkflowOutcome {
        prompt,
        outputs,
        steps: results,
    })
}

fn terminal_desktop_workflow(workflow: &TerminalWorkflow) -> DesktopWorkflow {
    DesktopWorkflow {
        id: format!(
            "terminal-workflow-{}",
            workflow_id_component(&workflow.profile.name)
        ),
        summary: workflow.prompt.clone(),
        target: WorkflowTarget::terminal(workflow.profile.name.clone()),
        inputs: Vec::new(),
        actions: workflow
            .actions
            .iter()
            .map(|action| WorkflowAction {
                name: action.name.clone(),
                kind: WorkflowActionKind::AdapterCapability(action.required_capability.clone()),
                target: LocatorTarget::default(),
                value_template: action.value.clone(),
                risk: WorkflowRisk::Low,
            })
            .collect(),
        outputs: workflow
            .text_outputs
            .iter()
            .map(|output| WorkflowOutput {
                name: output.name.clone(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::VisibleText(output.expected.clone()),
                required: true,
                expected: Some(output.expected.clone()),
            })
            .chain(workflow.field_outputs.iter().map(|output| WorkflowOutput {
                name: output.name.clone(),
                value_type: WorkflowValueType::String,
                extractor: WorkflowOutputExtractor::TerminalField(TerminalField {
                    row: output.field.row,
                    col: output.field.col,
                    len: output.field.len,
                }),
                required: true,
                expected: output.expected.clone(),
            }))
            .collect(),
        assertions: Vec::new(),
        evidence_policy: WorkflowEvidencePolicy::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_terminal_capabilities() {
        let capabilities = terminal_capabilities();

        assert!(capabilities.supports("terminal.connect"));
        assert!(capabilities.supports("terminal.extract_field"));
        assert_eq!(capabilities.adapter_id, TERMINAL_ADAPTER_ID);
    }

    #[test]
    fn generic_terminal_workflow_sends_actions_and_reads_outputs() {
        let adapter = TerminalAdapter::new();
        let outcome = run_terminal_workflow(
            &adapter,
            TerminalWorkflow {
                profile: TerminalProfile {
                    name: "test".to_owned(),
                    protocol: TerminalProtocol::Tn3270,
                    host: "terminal.test".to_owned(),
                },
                prompt: "Connect to a terminal and complete the supplied workflow.".to_owned(),
                initial_screen: vec!["LOGIN".to_owned()],
                actions: vec![
                    TerminalWorkflowAction {
                        name: "username".to_owned(),
                        required_capability: "terminal.type_text".to_owned(),
                        value: Some("USER1".to_owned()),
                    },
                    TerminalWorkflowAction {
                        name: "enter-login".to_owned(),
                        required_capability: "terminal.send_keys".to_owned(),
                        value: Some("ENTER".to_owned()),
                    },
                    TerminalWorkflowAction {
                        name: "menu".to_owned(),
                        required_capability: "terminal.type_text".to_owned(),
                        value: Some("CUST".to_owned()),
                    },
                ],
                final_screen: vec![
                    "ACCOUNT STATUS: ACTIVE".to_owned(),
                    "BALANCE: 100.00".to_owned(),
                ],
                text_outputs: vec![TerminalTextOutput {
                    name: "status-line".to_owned(),
                    expected: "ACCOUNT STATUS".to_owned(),
                }],
                field_outputs: vec![TerminalFieldOutput {
                    name: "status".to_owned(),
                    field: ScreenField {
                        row: 0,
                        col: 16,
                        len: 6,
                    },
                    expected: Some("ACTIVE".to_owned()),
                }],
            },
        )
        .expect("generic terminal workflow should pass");

        assert_eq!(outcome.outputs.get("status"), Some(&"ACTIVE".to_owned()));
        assert!(outcome.steps.iter().all(|step| step.success));
    }

    #[test]
    fn records_and_extracts_screen_fields() {
        let adapter = TerminalAdapter::new();
        adapter.record_screen_buffer(["ACCOUNT STATUS: ACTIVE", "BALANCE: 100.00"]);
        adapter.define_field(
            "status",
            ScreenField {
                row: 0,
                col: 16,
                len: 6,
            },
        );

        assert_eq!(
            adapter.extract_field(ScreenField {
                row: 0,
                col: 16,
                len: 6,
            }),
            Some("ACTIVE".to_owned())
        );
        assert_eq!(
            adapter.extract_after_anchor("BALANCE:"),
            Some("100.00".to_owned())
        );
    }
}
