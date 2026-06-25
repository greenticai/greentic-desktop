use greentic_desktop_adapter::{validate_required_capabilities, AdapterCapabilities, RunnerStep};
use greentic_desktop_evidence::{
    EvidenceArtifact, EvidenceArtifactKind, EvidenceBundle, EvidenceRef, EvidenceStatus,
    ToolTraceEntry,
};
use greentic_desktop_recorder::RunnerPackage;
use greentic_desktop_session::{plan_bootstrap, BootstrapPlan, SessionProfile};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayRequest {
    pub package: RunnerPackage,
    pub session_profile: SessionProfile,
    pub inputs: BTreeMap<String, String>,
    pub secrets: BTreeMap<String, String>,
    pub adapters: Vec<AdapterCapabilities>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnFailure {
    Stop,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepTrace {
    pub step_id: String,
    pub attempts: u8,
    pub success: bool,
    pub reason: Option<String>,
    pub evidence_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayOutcome {
    pub passed: bool,
    pub bootstrap: BootstrapPlan,
    pub traces: Vec<StepTrace>,
    pub outputs: BTreeMap<String, String>,
    pub evidence: EvidenceBundle,
    pub evidence_ref: EvidenceRef,
    pub failure_reason: Option<String>,
}

pub fn validate_package(
    package: &RunnerPackage,
    adapters: &[AdapterCapabilities],
) -> Result<(), String> {
    if package.id.trim().is_empty() {
        return Err("runner package id must not be empty".to_owned());
    }

    let required = package
        .steps
        .iter()
        .map(|step| step.required_capability.as_str());
    let validation = validate_required_capabilities(adapters, required);
    if !validation.is_valid() {
        return Err(format!(
            "missing capabilities: {}",
            validation.missing.join(",")
        ));
    }

    Ok(())
}

pub fn replay(request: ReplayRequest) -> ReplayOutcome {
    let bootstrap = match plan_bootstrap(&request.session_profile) {
        Ok(plan) => plan,
        Err(reason) => {
            let evidence = evidence_bundle(
                "run_invalid_session",
                &request.package,
                EvidenceStatus::Failed,
                &request.inputs,
                &request.secrets,
                BTreeMap::new(),
                Vec::new(),
            );
            let evidence_ref = evidence.reference();
            return ReplayOutcome {
                passed: false,
                bootstrap: BootstrapPlan {
                    profile_id: request.session_profile.id,
                    started_process_refs: Vec::new(),
                    opened_targets: Vec::new(),
                },
                traces: Vec::new(),
                outputs: BTreeMap::new(),
                evidence,
                evidence_ref,
                failure_reason: Some(reason),
            };
        }
    };

    if let Err(reason) = validate_package(&request.package, &request.adapters) {
        let evidence = evidence_bundle(
            &format!("run_{}", request.package.id),
            &request.package,
            EvidenceStatus::Failed,
            &request.inputs,
            &request.secrets,
            BTreeMap::new(),
            Vec::new(),
        );
        let evidence_ref = evidence.reference();
        return ReplayOutcome {
            passed: false,
            bootstrap,
            traces: Vec::new(),
            outputs: BTreeMap::new(),
            evidence,
            evidence_ref,
            failure_reason: Some(reason),
        };
    }

    let mut traces = Vec::new();
    let mut tool_trace = Vec::new();
    for step in &request.package.steps {
        let retry = retry_policy(step);
        let attempts = if retry.safe {
            retry.max_attempts.max(1)
        } else {
            1
        };
        let resolved = resolve_value(step.value.as_deref(), &request.inputs, &request.secrets);
        let success = !resolved
            .as_deref()
            .unwrap_or_default()
            .contains("{{missing");
        traces.push(StepTrace {
            step_id: step.id.clone(),
            attempts,
            success,
            reason: (!success).then(|| "unresolved input or secret".to_owned()),
            evidence_ref: Some(format!("evidence://{}.json", step.id)),
        });
        tool_trace.push(ToolTraceEntry {
            step_id: step.id.clone(),
            capability: step.required_capability.clone(),
            status: if success {
                EvidenceStatus::Success
            } else {
                EvidenceStatus::Failed
            },
            message: (!success).then(|| "unresolved input or secret".to_owned()),
        });
        if !success {
            let evidence = evidence_bundle(
                &format!("run_{}", request.package.id),
                &request.package,
                EvidenceStatus::Failed,
                &request.inputs,
                &request.secrets,
                BTreeMap::new(),
                tool_trace,
            );
            let evidence_ref = evidence.reference();
            return ReplayOutcome {
                passed: false,
                bootstrap,
                traces,
                outputs: BTreeMap::new(),
                evidence,
                evidence_ref,
                failure_reason: Some("step failed".to_owned()),
            };
        }
    }

    let outputs: BTreeMap<String, String> = request
        .package
        .outputs
        .iter()
        .map(|output| (output.clone(), "resolved".to_owned()))
        .collect();

    let evidence = evidence_bundle(
        &format!("run_{}", request.package.id),
        &request.package,
        EvidenceStatus::Success,
        &request.inputs,
        &request.secrets,
        outputs.clone(),
        tool_trace,
    );
    let evidence_ref = evidence.reference();

    ReplayOutcome {
        passed: true,
        bootstrap,
        traces,
        outputs,
        evidence,
        evidence_ref,
        failure_reason: None,
    }
}

impl ReplayOutcome {
    pub fn outputs_json(&self) -> String {
        let body = self
            .outputs
            .iter()
            .map(|(key, value)| format!("\"{key}\":\"{value}\""))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{{body}}}")
    }
}

fn retry_policy(step: &RunnerStep) -> RetryPolicy {
    let safe = step
        .value
        .as_deref()
        .is_some_and(|value| value.contains("retry_safe=true"));
    RetryPolicy {
        max_attempts: if safe { 2 } else { 1 },
        safe,
    }
}

fn resolve_value(
    value: Option<&str>,
    inputs: &BTreeMap<String, String>,
    secrets: &BTreeMap<String, String>,
) -> Option<String> {
    let mut value = value?.to_owned();
    for (key, replacement) in inputs {
        value = value.replace(&format!("{{{{{key}}}}}"), replacement);
    }
    for (key, replacement) in secrets {
        value = value.replace(&format!("{{{{{key}}}}}"), replacement);
    }
    Some(value)
}

fn evidence_bundle(
    run_id: &str,
    package: &RunnerPackage,
    status: EvidenceStatus,
    inputs: &BTreeMap<String, String>,
    secrets: &BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
    tool_trace: Vec<ToolTraceEntry>,
) -> EvidenceBundle {
    EvidenceBundle::new(
        run_id,
        &package.id,
        &package.version,
        status,
        inputs,
        &secrets.keys().cloned().collect::<Vec<_>>(),
        outputs,
        vec![EvidenceArtifact::new(
            EvidenceArtifactKind::OutputExtractionProof,
            "outputs.json",
            format!("evidence://{run_id}/outputs.json"),
        )],
        tool_trace,
        "replay-start",
        "replay-complete",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_adapter::LocatorTarget;
    use greentic_desktop_recorder::RecordingMode;
    use greentic_desktop_session::{BootstrapAction, BrowserKind};

    fn package() -> RunnerPackage {
        RunnerPackage {
            id: "customer_create".to_owned(),
            version: "0.1.0".to_owned(),
            mode: RecordingMode::Hybrid,
            inputs: vec!["inputs.email".to_owned()],
            secrets: vec!["secrets.password".to_owned()],
            steps: vec![RunnerStep {
                id: "fill_email".to_owned(),
                action: "fill".to_owned(),
                target: LocatorTarget::default(),
                value: Some("{{inputs.email}}".to_owned()),
                required_capability: "web.fill".to_owned(),
            }],
            assertions: vec!["text visible".to_owned()],
            outputs: vec!["outputs.customer_id".to_owned()],
        }
    }

    fn request() -> ReplayRequest {
        ReplayRequest {
            package: package(),
            session_profile: SessionProfile {
                id: "web".to_owned(),
                bootstrap: vec![BootstrapAction::OpenBrowser {
                    browser: BrowserKind::Default,
                    url: "http://localhost".to_owned(),
                }],
                teardown: Vec::new(),
            },
            inputs: BTreeMap::from([("inputs.email".to_owned(), "user@example.test".to_owned())]),
            secrets: BTreeMap::new(),
            adapters: vec![AdapterCapabilities::new(
                "greentic.desktop.playwright",
                "1.0.0",
                ["web.fill"],
            )],
        }
    }

    #[test]
    fn validates_missing_capabilities_deterministically() {
        let mut request = request();
        request.adapters.clear();
        let outcome = replay(request);

        assert!(!outcome.passed);
        assert!(outcome
            .failure_reason
            .expect("failure reason")
            .contains("missing capabilities"));
    }

    #[test]
    fn replays_runner_with_inputs_and_returns_outputs_json() {
        let outcome = replay(request());

        assert!(outcome.passed);
        assert_eq!(outcome.traces.len(), 1);
        assert_eq!(
            outcome.outputs_json(),
            "{\"outputs.customer_id\":\"resolved\"}"
        );
        assert_eq!(
            outcome.evidence_ref.uri,
            "evidence://run_customer_create/bundle.json"
        );
        assert!(outcome
            .evidence
            .to_json()
            .contains("\"outputs.customer_id\""));
    }
}
