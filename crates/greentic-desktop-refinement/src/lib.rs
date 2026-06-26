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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerPatchOperation {
    AddInput { name: String },
    AddOutput { name: String },
    UpdateLocatorText { step_id: String, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerUpdatePlan {
    pub summary: String,
    pub operations: Vec<RunnerPatchOperation>,
    pub questions: Vec<String>,
    pub requires_test: bool,
}

pub fn plan_runner_update(package: &RunnerPackage, user_prompt: &str) -> RunnerUpdatePlan {
    let lower = user_prompt.to_ascii_lowercase();
    let mut operations = Vec::new();
    if lower.contains("input") {
        if let Some(name) = extract_after_words(&lower, &["add", "input", "field"]) {
            let name = normalize_name(&name);
            if !package.inputs.iter().any(|input| input.ends_with(&name)) {
                operations.push(RunnerPatchOperation::AddInput { name });
            }
        }
    }
    if lower.contains("output") || lower.contains("return") {
        if let Some(name) = extract_after_words(&lower, &["return", "output", "extract"]) {
            let name = normalize_name(&name);
            if !package.outputs.iter().any(|output| output.ends_with(&name)) {
                operations.push(RunnerPatchOperation::AddOutput { name });
            }
        }
    }
    if lower.contains("called") || lower.contains("named") {
        if let Some(first_step) = package.steps.first() {
            let text = extract_quoted(user_prompt)
                .or_else(|| extract_after_words(user_prompt, &["called", "named"]))
                .map(|value| value.trim_matches('.').trim().to_owned());
            if let Some(text) = text.filter(|value| !value.is_empty()) {
                operations.push(RunnerPatchOperation::UpdateLocatorText {
                    step_id: first_step.id.clone(),
                    text,
                });
            }
        }
    }
    let questions = if operations.is_empty() {
        vec!["Which input, output, step, or locator should be changed?".to_owned()]
    } else {
        Vec::new()
    };
    RunnerUpdatePlan {
        summary: user_prompt.to_owned(),
        operations,
        questions,
        requires_test: true,
    }
}

pub fn apply_update_plan(package: &mut RunnerPackage, plan: &RunnerUpdatePlan) -> Vec<RunnerDiff> {
    let mut diffs = Vec::new();
    for operation in &plan.operations {
        match operation {
            RunnerPatchOperation::AddInput { name } => {
                let value = format!("inputs.{name}");
                if !package.inputs.contains(&value) {
                    package.inputs.push(value.clone());
                    diffs.push(RunnerDiff {
                        step_id: "inputs".to_owned(),
                        before: "inputs unchanged".to_owned(),
                        after: format!("added {value}"),
                    });
                }
            }
            RunnerPatchOperation::AddOutput { name } => {
                let value = format!("outputs.{name}");
                if !package.outputs.contains(&value) {
                    package.outputs.push(value.clone());
                    diffs.push(RunnerDiff {
                        step_id: "outputs".to_owned(),
                        before: "outputs unchanged".to_owned(),
                        after: format!("added {value}"),
                    });
                }
            }
            RunnerPatchOperation::UpdateLocatorText { step_id, text } => {
                if let Some(step) = package.steps.iter_mut().find(|step| &step.id == step_id) {
                    let before = render_step(step);
                    let strategy = step
                        .target
                        .preferred
                        .get_or_insert_with(LocatorStrategy::default);
                    strategy.text = Some(text.clone());
                    diffs.push(RunnerDiff {
                        step_id: step_id.clone(),
                        before,
                        after: render_step(step),
                    });
                }
            }
        }
    }
    diffs
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

fn extract_after_words(input: &str, words: &[&str]) -> Option<String> {
    for word in words {
        if let Some((_, rest)) = input.split_once(word) {
            let value = rest
                .split(['.', ',', ';'])
                .next()
                .unwrap_or(rest)
                .trim()
                .trim_start_matches("as ")
                .trim_start_matches("the ")
                .to_owned();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn normalize_name(value: &str) -> String {
    let value = value
        .trim()
        .trim_end_matches(" as an input")
        .trim_end_matches(" as input")
        .trim_end_matches(" as an output")
        .trim_end_matches(" as output")
        .trim();
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
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
            open_questions: Vec::new(),
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

    #[test]
    fn prompt_update_adds_input_to_actual_package() {
        let mut package = package();
        let plan = plan_runner_update(&package, "Add phone number as an input");

        let diffs = apply_update_plan(&mut package, &plan);

        assert!(package.inputs.contains(&"inputs.phone_number".to_owned()));
        assert_eq!(diffs[0].step_id, "inputs");
    }

    #[test]
    fn prompt_update_adds_output_to_actual_package() {
        let mut package = package();
        let plan = plan_runner_update(&package, "Return confirmation number as an output");

        let _ = apply_update_plan(&mut package, &plan);

        assert!(package
            .outputs
            .contains(&"outputs.confirmation_number".to_owned()));
    }

    #[test]
    fn prompt_update_changes_locator_text() {
        let mut package = package();
        let plan = plan_runner_update(&package, "The button is now called \"Submit\"");

        let diffs = apply_update_plan(&mut package, &plan);

        assert!(!diffs.is_empty());
        assert_eq!(
            package.steps[0]
                .target
                .preferred
                .as_ref()
                .and_then(|strategy| strategy.text.clone()),
            Some("Submit".to_owned())
        );
    }
}
