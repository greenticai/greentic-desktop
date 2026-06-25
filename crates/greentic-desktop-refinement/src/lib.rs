use greentic_desktop_adapter::{LocatorStrategy, RunnerStep};
use greentic_desktop_recorder::RunnerPackage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefinementContext {
    pub current_screenshot: Option<String>,
    pub step_trace: Vec<String>,
    pub last_failure: String,
    pub observed_screen_text: Vec<String>,
    pub available_ui_elements: Vec<String>,
    pub suggested_fix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Correction {
    ChangeSelector {
        step_id: String,
        text: Option<String>,
        context: Option<String>,
        region: Option<String>,
    },
    AddWait {
        step_id: String,
        text: String,
    },
    AddAssertion {
        step_id: String,
        expected: String,
    },
    MarkOptional {
        step_id: String,
    },
    ChangeAdapter {
        step_id: String,
        adapter_prefix: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerDiff {
    pub step_id: String,
    pub before: String,
    pub after: String,
}

pub fn parse_correction(step_id: impl Into<String>, user_text: &str) -> Correction {
    let step_id = step_id.into();
    let lower = user_text.to_ascii_lowercase();

    if lower.contains("wait") {
        return Correction::AddWait {
            step_id,
            text: user_text.to_owned(),
        };
    }

    if lower.contains("assert") || lower.contains("should see") {
        return Correction::AddAssertion {
            step_id,
            expected: user_text.to_owned(),
        };
    }

    if lower.contains("optional") {
        return Correction::MarkOptional { step_id };
    }

    if lower.contains("adapter") {
        return Correction::ChangeAdapter {
            step_id,
            adapter_prefix: "vision".to_owned(),
        };
    }

    Correction::ChangeSelector {
        step_id,
        text: extract_quoted(user_text).or_else(|| Some("Save".to_owned())),
        context: lower
            .contains("customer form")
            .then(|| "customer_form".to_owned()),
        region: lower.contains("bottom").then(|| "bottom_right".to_owned()),
    }
}

pub fn preview_diff(package: &RunnerPackage, correction: &Correction) -> Option<RunnerDiff> {
    let step = find_step(package, correction)?;
    let mut after = step.clone();
    apply_to_step(&mut after, correction);

    Some(RunnerDiff {
        step_id: step.id.clone(),
        before: render_step(step),
        after: render_step(&after),
    })
}

pub fn apply_correction(package: &mut RunnerPackage, correction: Correction) -> Option<RunnerDiff> {
    let diff = preview_diff(package, &correction)?;
    let step = package
        .steps
        .iter_mut()
        .find(|step| step.id == diff.step_id)?;
    apply_to_step(step, &correction);
    Some(diff)
}

fn find_step<'a>(package: &'a RunnerPackage, correction: &Correction) -> Option<&'a RunnerStep> {
    let step_id = match correction {
        Correction::ChangeSelector { step_id, .. }
        | Correction::AddWait { step_id, .. }
        | Correction::AddAssertion { step_id, .. }
        | Correction::MarkOptional { step_id }
        | Correction::ChangeAdapter { step_id, .. } => step_id,
    };
    package.steps.iter().find(|step| &step.id == step_id)
}

fn apply_to_step(step: &mut RunnerStep, correction: &Correction) {
    match correction {
        Correction::ChangeSelector {
            text,
            context,
            region,
            ..
        } => {
            let strategy = step
                .target
                .preferred
                .get_or_insert_with(LocatorStrategy::default);
            if let Some(text) = text {
                strategy.text = Some(text.clone());
            }
            if let Some(context) = context {
                strategy.name = Some(context.clone());
            }
            if let Some(region) = region {
                strategy.region = Some(region.clone());
            }
        }
        Correction::AddWait { text, .. } => {
            step.action = "wait_for_screen".to_owned();
            step.value = Some(text.clone());
        }
        Correction::AddAssertion { expected, .. } => {
            step.action = "assert".to_owned();
            step.value = Some(expected.clone());
        }
        Correction::MarkOptional { .. } => {
            step.value = Some("optional=true".to_owned());
        }
        Correction::ChangeAdapter { adapter_prefix, .. } => {
            step.required_capability = format!("{adapter_prefix}.{}", step.action);
        }
    }
}

fn render_step(step: &RunnerStep) -> String {
    format!(
        "id: {}\naction: {}\nrequired_capability: {}\nvalue: {}\n",
        step.id,
        step.action,
        step.required_capability,
        step.value.clone().unwrap_or_default()
    )
}

fn extract_quoted(input: &str) -> Option<String> {
    let start = input.find('"')?;
    let rest = &input[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorTarget;
    use greentic_desktop_recorder::{RecordingMode, RunnerPackage};

    fn package() -> RunnerPackage {
        RunnerPackage {
            id: "customer_create".to_owned(),
            version: "0.1.0".to_owned(),
            mode: RecordingMode::Hybrid,
            inputs: Vec::new(),
            secrets: Vec::new(),
            steps: vec![
                RunnerStep {
                    id: "save".to_owned(),
                    action: "click".to_owned(),
                    target: LocatorTarget::default(),
                    value: None,
                    required_capability: "web.click".to_owned(),
                },
                RunnerStep {
                    id: "other".to_owned(),
                    action: "click".to_owned(),
                    target: LocatorTarget::default(),
                    value: None,
                    required_capability: "web.click".to_owned(),
                },
            ],
            assertions: Vec::new(),
            outputs: Vec::new(),
        }
    }

    #[test]
    fn user_can_correct_failed_step_with_natural_language() {
        let correction = parse_correction(
            "save",
            "Use the Save button in the customer form, bottom right.",
        );
        assert!(matches!(correction, Correction::ChangeSelector { .. }));
    }

    #[test]
    fn diff_is_visible_before_applying() {
        let package = package();
        let correction = parse_correction("save", "Use the Save button in the customer form.");
        let diff = preview_diff(&package, &correction).expect("diff should render");

        assert!(diff.before.contains("id: save"));
        assert!(diff.after.contains("required_capability: web.click"));
    }

    #[test]
    fn applies_correction_without_rewriting_unrelated_steps() {
        let mut package = package();
        let correction = parse_correction("save", "Use the Save button in the customer form.");
        let _ = apply_correction(&mut package, correction).expect("correction should apply");

        assert_eq!(package.steps[1].id, "other");
        assert_eq!(package.steps[1].value, None);
        assert_eq!(
            package.steps[0]
                .target
                .preferred
                .as_ref()
                .and_then(|strategy| strategy.name.clone()),
            Some("customer_form".to_owned())
        );
    }
}
