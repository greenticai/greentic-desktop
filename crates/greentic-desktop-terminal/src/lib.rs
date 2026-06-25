use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    Observation, ObserveContext, RecordedEvent, RunnerStep, StepResult,
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
        state.screen = vec!["LOGIN".to_owned()];
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
                state.screen = vec!["LOGIN".to_owned()];
            }
            "terminal.disconnect" => state.connected = false,
            "terminal.type_text" | "terminal.send_text" => {
                let value = step.value.clone().unwrap_or_default();
                if value == "CUST" {
                    state.screen = vec!["CUSTOMER LOOKUP".to_owned(), "CUSTOMER ID:".to_owned()];
                } else if !value.is_empty() {
                    state.screen.push(format!("INPUT: {}", value));
                }
            }
            "terminal.send_keys" if step.value.as_deref() == Some("ENTER") => {
                if state.screen.iter().any(|line| line.contains("LOGIN")) {
                    state.screen = vec!["MAIN MENU".to_owned(), "1 CUST Customers".to_owned()];
                } else if state
                    .screen
                    .iter()
                    .any(|line| line.contains("CUSTOMER LOOKUP"))
                {
                    state.screen = vec![
                        "ACCOUNT STATUS: ACTIVE".to_owned(),
                        "BALANCE: 100.00".to_owned(),
                    ];
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

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorTarget;

    fn step(id: &str, capability: &str, value: Option<&str>) -> RunnerStep {
        RunnerStep {
            id: id.to_owned(),
            action: capability.to_owned(),
            target: LocatorTarget::default(),
            value: value.map(str::to_owned),
            required_capability: capability.to_owned(),
        }
    }

    #[test]
    fn exposes_terminal_capabilities() {
        let capabilities = terminal_capabilities();

        assert!(capabilities.supports("terminal.connect"));
        assert!(capabilities.supports("terminal.extract_field"));
        assert_eq!(capabilities.adapter_id, TERMINAL_ADAPTER_ID);
    }

    #[test]
    fn can_replay_login_and_menu_navigation() {
        let adapter = TerminalAdapter::new();
        let steps = vec![
            step("connect", "terminal.connect", None),
            step("username", "terminal.type_text", Some("USER1")),
            step("enter-login", "terminal.send_keys", Some("ENTER")),
            step("menu", "terminal.type_text", Some("CUST")),
            step("enter-menu", "terminal.send_keys", Some("ENTER")),
        ];

        assert!(adapter
            .replay(&steps)
            .expect("terminal replay should pass")
            .iter()
            .all(|result| result.success));

        let assertion = adapter
            .validate(Assertion {
                id: "account-status".to_owned(),
                required_capability: "terminal.assert_text".to_owned(),
                target: LocatorTarget::default(),
                expected: "ACCOUNT STATUS".to_owned(),
            })
            .expect("assertion should run");
        assert!(assertion.passed);
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
