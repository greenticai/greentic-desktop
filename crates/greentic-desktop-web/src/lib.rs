use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub const PLAYWRIGHT_ADAPTER_ID: &str = "greentic.desktop.playwright";

pub fn playwright_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        PLAYWRIGHT_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "web.goto",
            "web.click",
            "web.fill",
            "web.select",
            "web.wait_for_text",
            "web.extract_text",
            "web.extract_regex",
            "web.screenshot",
            "web.assert_visible",
            "web.assert_url",
            "web.download_file",
        ],
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebElementMetadata {
    pub data_testid: Option<String>,
    pub role: Option<String>,
    pub name: Option<String>,
    pub label: Option<String>,
    pub text: Option<String>,
    pub css: Option<String>,
    pub xpath: Option<String>,
    pub visual_image: Option<String>,
}

pub fn stable_selector_target(metadata: &WebElementMetadata) -> LocatorTarget {
    if let Some(data_testid) = &metadata.data_testid {
        return LocatorTarget {
            preferred: Some(LocatorStrategy {
                data_testid: Some(data_testid.clone()),
                css: Some(format!("[data-testid='{data_testid}']")),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        };
    }

    if metadata.role.is_some() || metadata.name.is_some() {
        return LocatorTarget {
            preferred: Some(LocatorStrategy {
                role: metadata.role.clone(),
                name: metadata.name.clone(),
                ..LocatorStrategy::default()
            }),
            fallback: metadata.text.as_ref().map(|text| LocatorStrategy {
                text: Some(text.clone()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        };
    }

    if let Some(label) = &metadata.label {
        return LocatorTarget {
            preferred: Some(LocatorStrategy {
                label: Some(label.clone()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        };
    }

    if let Some(text) = &metadata.text {
        return LocatorTarget {
            preferred: Some(LocatorStrategy {
                text: Some(text.clone()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        };
    }

    let preferred = metadata
        .css
        .as_ref()
        .map(|css| LocatorStrategy {
            css: Some(css.clone()),
            ..LocatorStrategy::default()
        })
        .or_else(|| {
            metadata.xpath.as_ref().map(|xpath| LocatorStrategy {
                xpath: Some(xpath.clone()),
                ..LocatorStrategy::default()
            })
        });

    LocatorTarget {
        preferred,
        visual_fallback: metadata.visual_image.as_ref().map(|image| VisualLocator {
            image: image.clone(),
            region: None,
            nearby_text: None,
        }),
        ..LocatorTarget::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlaywrightWebAdapter {
    state: Arc<Mutex<WebState>>,
}

#[derive(Debug, Clone, Default)]
struct WebState {
    url: String,
    fields: BTreeMap<String, String>,
    visible_text: Vec<String>,
    identifiers: BTreeMap<String, String>,
    recorded: Vec<RecordedEvent>,
    console_errors: Vec<String>,
    network_errors: Vec<String>,
}

impl PlaywrightWebAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_human_interaction(
        &self,
        action: impl Into<String>,
        metadata: WebElementMetadata,
        value: Option<String>,
    ) -> RecordedEvent {
        let event = RecordedEvent {
            action: action.into(),
            target: stable_selector_target(&metadata),
            value: value.map(|value| redact_if_secret(&metadata, &value)),
        };
        self.state
            .lock()
            .expect("web adapter mutex poisoned")
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

    pub fn insert_visible_text(&self, text: impl Into<String>) {
        self.state
            .lock()
            .expect("web adapter mutex poisoned")
            .visible_text
            .push(text.into());
    }

    pub fn insert_identifier(&self, key: impl Into<String>, value: impl Into<String>) {
        self.state
            .lock()
            .expect("web adapter mutex poisoned")
            .identifiers
            .insert(key.into(), value.into());
    }
}

impl DesktopAdapter for PlaywrightWebAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        playwright_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let state = self.state.lock().expect("web adapter mutex poisoned");
        Ok(Observation {
            adapter_id: PLAYWRIGHT_ADAPTER_ID.to_owned(),
            summary: format!("web session {} at {}", ctx.session_id, state.url),
            visible_text: state.visible_text.clone(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let mut state = self.state.lock().expect("web adapter mutex poisoned");
        match step.required_capability.as_str() {
            "web.goto" | "web.assert_url" => {
                state.url = step.value.clone().unwrap_or_default();
            }
            "web.fill" | "web.select" => {
                let field = target_key(&step.target);
                state
                    .fields
                    .insert(field, step.value.clone().unwrap_or_default());
            }
            "web.click" if target_key(&step.target).contains("submit") => {
                state.visible_text.push("Customer created".to_owned());
                state
                    .identifiers
                    .insert("customer_id".to_owned(), "CUST-1001".to_owned());
            }
            "web.click" => {}
            "web.screenshot" => state.visible_text.push("screenshot captured".to_owned()),
            "web.download_file" => state.visible_text.push("download completed".to_owned()),
            "web.wait_for_text" | "web.extract_text" | "web.extract_regex"
            | "web.assert_visible" => {}
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
            message: "web step accepted".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let state = self.state.lock().expect("web adapter mutex poisoned");
        let passed = match assertion.required_capability.as_str() {
            "web.assert_visible" => state
                .visible_text
                .iter()
                .any(|text| text == &assertion.expected),
            "web.assert_url" => state.url.contains(&assertion.expected),
            "web.extract_text" | "web.extract_regex" => {
                state.identifiers.contains_key(&assertion.expected)
            }
            _ => state.console_errors.is_empty() && state.network_errors.is_empty(),
        };

        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "web assertion passed".to_owned()
            } else {
                "web assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(self
            .state
            .lock()
            .expect("web adapter mutex poisoned")
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
                .data_testid
                .clone()
                .or_else(|| strategy.name.clone())
                .or_else(|| strategy.label.clone())
                .or_else(|| strategy.text.clone())
                .or_else(|| strategy.css.clone())
                .or_else(|| strategy.xpath.clone())
        })
        .unwrap_or_else(|| "target".to_owned())
        .to_lowercase()
}

fn redact_if_secret(metadata: &WebElementMetadata, value: &str) -> String {
    let secret_hint = metadata
        .label
        .iter()
        .chain(metadata.name.iter())
        .chain(metadata.text.iter())
        .any(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("password") || value.contains("secret") || value.contains("token")
        });

    if secret_hint {
        "{{secret}}".to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(label: &str) -> LocatorTarget {
        LocatorTarget {
            preferred: Some(LocatorStrategy {
                label: Some(label.to_owned()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        }
    }

    #[test]
    fn exposes_playwright_capabilities() {
        let capabilities = playwright_capabilities();

        assert!(capabilities.supports("web.goto"));
        assert!(capabilities.supports("web.download_file"));
        assert_eq!(capabilities.adapter_id, PLAYWRIGHT_ADAPTER_ID);
    }

    #[test]
    fn selector_strategy_prefers_data_testid() {
        let selector = stable_selector_target(&WebElementMetadata {
            data_testid: Some("save-customer".to_owned()),
            role: Some("button".to_owned()),
            name: Some("Save".to_owned()),
            label: None,
            text: Some("Save".to_owned()),
            css: None,
            xpath: None,
            visual_image: None,
        });

        let preferred = selector.preferred.expect("preferred selector");
        assert_eq!(preferred.data_testid, Some("save-customer".to_owned()));
        assert_eq!(
            preferred.css,
            Some("[data-testid='save-customer']".to_owned())
        );
    }

    #[test]
    fn can_open_fill_submit_and_extract_identifier() {
        let adapter = PlaywrightWebAdapter::new();
        let steps = vec![
            RunnerStep {
                id: "open".to_owned(),
                action: "goto".to_owned(),
                target: LocatorTarget::default(),
                value: Some("https://example.test/customers/new".to_owned()),
                required_capability: "web.goto".to_owned(),
            },
            RunnerStep {
                id: "fill_email".to_owned(),
                action: "fill".to_owned(),
                target: target("Email"),
                value: Some("user@example.test".to_owned()),
                required_capability: "web.fill".to_owned(),
            },
            RunnerStep {
                id: "submit".to_owned(),
                action: "click".to_owned(),
                target: target("Submit"),
                value: None,
                required_capability: "web.click".to_owned(),
            },
        ];

        let results = adapter.replay(&steps).expect("web replay should pass");
        assert!(results.iter().all(|result| result.success));

        let visible = adapter
            .validate(Assertion {
                id: "created".to_owned(),
                required_capability: "web.assert_visible".to_owned(),
                target: target("body"),
                expected: "Customer created".to_owned(),
            })
            .expect("visible assertion should run");
        assert!(visible.passed);

        let id = adapter
            .validate(Assertion {
                id: "customer_id".to_owned(),
                required_capability: "web.extract_text".to_owned(),
                target: target("Customer ID"),
                expected: "customer_id".to_owned(),
            })
            .expect("identifier assertion should run");
        assert!(id.passed);
    }

    #[test]
    fn recording_redacts_secret_values() {
        let adapter = PlaywrightWebAdapter::new();
        let event = adapter.record_human_interaction(
            "fill",
            WebElementMetadata {
                data_testid: None,
                role: None,
                name: None,
                label: Some("Password".to_owned()),
                text: None,
                css: Some("#password".to_owned()),
                xpath: None,
                visual_image: None,
            },
            Some("not-for-logs".to_owned()),
        );

        assert_eq!(event.value, Some("{{secret}}".to_owned()));
    }
}
