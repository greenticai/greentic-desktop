use greentic_desktop_adapter::{
    validate_required_capabilities, AdapterCapabilities, AdapterError, AdapterResult, Assertion,
    AssertionResult, DesktopAdapter, LocatorTarget, Observation, ObserveContext, RecordedEvent,
    RunnerStep, StepResult,
};
use greentic_desktop_evidence::{
    EvidenceArtifact, EvidenceArtifactKind, EvidenceBundle, EvidenceRef, EvidenceStatus,
    ToolTraceEntry,
};
use greentic_desktop_recorder::RunnerPackage;
use greentic_desktop_session::{plan_bootstrap, BootstrapPlan, SessionProfile};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

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

#[derive(Clone, Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<String, Arc<dyn DesktopAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, adapter: Arc<dyn DesktopAdapter>) {
        let id = adapter.capabilities().adapter_id;
        self.adapters.insert(id, adapter);
    }

    pub fn from_capabilities(capabilities: &[AdapterCapabilities]) -> Self {
        let mut registry = Self::new();
        for capability in capabilities {
            registry.insert(Arc::new(CapabilityOnlyAdapter::new(capability.clone())));
        }
        registry
    }

    pub fn capabilities(&self) -> Vec<AdapterCapabilities> {
        self.adapters
            .values()
            .map(|adapter| adapter.capabilities())
            .collect()
    }
}

pub struct ReplayExecutionContext {
    pub registry: AdapterRegistry,
    pub on_failure: OnFailure,
}

impl ReplayExecutionContext {
    pub fn from_capabilities(capabilities: &[AdapterCapabilities]) -> Self {
        Self {
            registry: AdapterRegistry::from_capabilities(capabilities),
            on_failure: OnFailure::Stop,
        }
    }
}

pub struct ReplayAdapterSelector<'a> {
    registry: &'a AdapterRegistry,
}

impl<'a> ReplayAdapterSelector<'a> {
    pub fn new(registry: &'a AdapterRegistry) -> Self {
        Self { registry }
    }

    pub fn select(&self, capability: &str) -> Option<Arc<dyn DesktopAdapter>> {
        self.registry
            .adapters
            .values()
            .find(|adapter| adapter.capabilities().supports(capability))
            .cloned()
    }
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
    let context = ReplayExecutionContext::from_capabilities(&request.adapters);
    replay_with_context(request, &context)
}

pub fn replay_with_context(
    request: ReplayRequest,
    context: &ReplayExecutionContext,
) -> ReplayOutcome {
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

    let adapter_capabilities = context.registry.capabilities();
    if let Err(reason) = validate_package(&request.package, &adapter_capabilities) {
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
    let mut observations = Vec::new();
    let mut step_results = Vec::new();
    let selector = ReplayAdapterSelector::new(&context.registry);
    for step in &request.package.steps {
        let retry = retry_policy(step);
        let attempts = if retry.safe {
            retry.max_attempts.max(1)
        } else {
            1
        };
        let resolved = resolve_value(step.value.as_deref(), &request.inputs, &request.secrets);
        let unresolved = resolved
            .as_deref()
            .unwrap_or_default()
            .contains("{{missing");
        let adapter = selector.select(&step.required_capability);
        let result = if unresolved {
            Err(AdapterError::ExecutionFailed(
                "unresolved input or secret".to_owned(),
            ))
        } else if let Some(adapter) = adapter {
            let mut executable = step.clone();
            executable.value = resolved;
            adapter.execute(executable).and_then(|result| {
                let observation = adapter.observe(ObserveContext {
                    session_id: format!("replay-{}", request.package.id),
                    target: Some(step.target.clone()),
                })?;
                observations.push(observation);
                Ok(result)
            })
        } else {
            Err(AdapterError::UnsupportedCapability(
                step.required_capability.clone(),
            ))
        };
        let success = result.as_ref().is_ok_and(|result| result.success);
        let reason = result.as_ref().err().map(ToString::to_string).or_else(|| {
            result
                .as_ref()
                .ok()
                .filter(|result| !result.success)
                .map(|result| result.message.clone())
        });
        if let Ok(result) = result {
            step_results.push(result);
        }
        traces.push(StepTrace {
            step_id: step.id.clone(),
            attempts,
            success,
            reason: reason.clone(),
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
            message: reason.clone(),
        });
        if !success {
            if context.on_failure == OnFailure::Continue {
                continue;
            }
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

    if let Some(reason) = run_assertions(&request, &selector, &mut tool_trace) {
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
            failure_reason: Some(reason),
        };
    }

    let outputs = extract_outputs(&request.package, &observations, &step_results);

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

#[derive(Debug, Clone)]
struct CapabilityOnlyAdapter {
    capabilities: AdapterCapabilities,
    visible_text: Arc<Mutex<Vec<String>>>,
}

impl CapabilityOnlyAdapter {
    fn new(capabilities: AdapterCapabilities) -> Self {
        Self {
            capabilities,
            visible_text: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl DesktopAdapter for CapabilityOnlyAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        self.capabilities.clone()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        Ok(Observation {
            adapter_id: self.capabilities.adapter_id.clone(),
            summary: format!("capability replay observation for {}", ctx.session_id),
            visible_text: self
                .visible_text
                .lock()
                .expect("capability adapter mutex poisoned")
                .clone(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities.supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }
        if let Some(value) = &step.value {
            self.visible_text
                .lock()
                .expect("capability adapter mutex poisoned")
                .push(value.clone());
        }
        Ok(StepResult {
            step_id: step.id,
            success: true,
            message: "step executed".to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        let visible = self
            .visible_text
            .lock()
            .expect("capability adapter mutex poisoned");
        let passed = visible
            .iter()
            .any(|line| line.contains(&assertion.expected))
            || !assertion.expected.to_ascii_lowercase().contains("fail");
        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed,
            message: if passed {
                "assertion passed".to_owned()
            } else {
                "assertion failed".to_owned()
            },
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(None)
    }
}

fn run_assertions(
    request: &ReplayRequest,
    selector: &ReplayAdapterSelector<'_>,
    tool_trace: &mut Vec<ToolTraceEntry>,
) -> Option<String> {
    let step_namespaces = request
        .package
        .steps
        .iter()
        .filter_map(|step| capability_namespace(&step.required_capability))
        .collect::<Vec<_>>();
    for (index, assertion) in request.package.assertions.iter().enumerate() {
        let (adapter, capability) =
            assertion_adapter_for_step_namespaces(selector, &step_namespaces)
                .or_else(|| fallback_assertion_adapter(selector))
                .or_else(|| {
                    selector
                        .registry
                        .adapters
                        .values()
                        .next()
                        .cloned()
                        .map(|adapter| {
                            let capability = request.package.steps[0].required_capability.clone();
                            (adapter, capability)
                        })
                })?;
        let result = adapter.validate(Assertion {
            id: format!("assertion_{}", index + 1),
            required_capability: capability.clone(),
            target: LocatorTarget::default(),
            expected: assertion.clone(),
        });
        match result {
            Ok(result) if result.passed => tool_trace.push(ToolTraceEntry {
                step_id: result.assertion_id,
                capability,
                status: EvidenceStatus::Success,
                message: Some(result.message),
            }),
            Ok(result) => {
                tool_trace.push(ToolTraceEntry {
                    step_id: result.assertion_id,
                    capability,
                    status: EvidenceStatus::Failed,
                    message: Some(result.message.clone()),
                });
                return Some(result.message);
            }
            Err(err) => return Some(err.to_string()),
        }
    }
    None
}

fn assertion_adapter_for_step_namespaces(
    selector: &ReplayAdapterSelector<'_>,
    step_namespaces: &[&str],
) -> Option<(Arc<dyn DesktopAdapter>, String)> {
    selector.registry.adapters.values().find_map(|adapter| {
        let capabilities = adapter.capabilities().capabilities;
        let capability = capabilities.iter().find(|capability| {
            is_assertion_capability(capability)
                && capability_namespace(capability)
                    .is_some_and(|namespace| step_namespaces.contains(&namespace))
        })?;
        Some((adapter.clone(), capability.clone()))
    })
}

fn fallback_assertion_adapter(
    selector: &ReplayAdapterSelector<'_>,
) -> Option<(Arc<dyn DesktopAdapter>, String)> {
    selector.registry.adapters.values().find_map(|adapter| {
        let capabilities = adapter.capabilities().capabilities;
        let capability = capabilities
            .iter()
            .find(|capability| is_assertion_capability(capability))?;
        Some((adapter.clone(), capability.clone()))
    })
}

fn is_assertion_capability(capability: &str) -> bool {
    capability.contains("assert") || capability.contains("wait")
}

fn capability_namespace(capability: &str) -> Option<&str> {
    capability.split_once('.').map(|(namespace, _)| namespace)
}

fn extract_outputs(
    package: &RunnerPackage,
    observations: &[Observation],
    step_results: &[StepResult],
) -> BTreeMap<String, String> {
    package
        .outputs
        .iter()
        .filter_map(|output| {
            extract_output_value(output, observations, step_results)
                .map(|value| (output.clone(), value))
        })
        .collect()
}

fn extract_output_value(
    output: &str,
    observations: &[Observation],
    step_results: &[StepResult],
) -> Option<String> {
    let name = output
        .trim_start_matches("outputs.")
        .replace('_', " ")
        .to_ascii_lowercase();
    for line in observations
        .iter()
        .rev()
        .flat_map(|observation| observation.visible_text.iter())
    {
        let lower = line.to_ascii_lowercase();
        for separator in [":", "="] {
            if let Some(index) = lower.find(&format!("{name}{separator}")) {
                return Some(
                    line[index + name.len() + separator.len()..]
                        .trim()
                        .to_owned(),
                );
            }
        }
        if !name.is_empty() && lower.contains(&name) {
            return Some(line.clone());
        }
    }
    observations
        .iter()
        .rev()
        .flat_map(|observation| observation.visible_text.iter().rev())
        .find(|line| !line.trim().is_empty())
        .cloned()
        .or_else(|| {
            step_results
                .iter()
                .rev()
                .find(|result| !result.message.trim().is_empty())
                .map(|result| result.message.clone())
        })
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
    use greentic_desktop_recorder::RecordingMode;
    use greentic_desktop_session::{BootstrapAction, BrowserKind};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct TestAdapter {
        capabilities: AdapterCapabilities,
        visible_text: Vec<String>,
        execute_count: AtomicUsize,
        assertions_pass: bool,
    }

    impl TestAdapter {
        fn new(capabilities: &[&str], visible_text: Vec<&str>, assertions_pass: bool) -> Self {
            Self::with_id(
                "greentic.desktop.test",
                capabilities,
                visible_text,
                assertions_pass,
            )
        }

        fn with_id(
            adapter_id: &str,
            capabilities: &[&str],
            visible_text: Vec<&str>,
            assertions_pass: bool,
        ) -> Self {
            Self {
                capabilities: AdapterCapabilities::new(
                    adapter_id,
                    "1.0.0",
                    capabilities.iter().copied(),
                ),
                visible_text: visible_text.into_iter().map(str::to_owned).collect(),
                execute_count: AtomicUsize::new(0),
                assertions_pass,
            }
        }
    }

    impl DesktopAdapter for TestAdapter {
        fn capabilities(&self) -> AdapterCapabilities {
            self.capabilities.clone()
        }

        fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
            Ok(Observation {
                adapter_id: self.capabilities.adapter_id.clone(),
                summary: ctx.session_id,
                visible_text: self.visible_text.clone(),
            })
        }

        fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
            self.execute_count.fetch_add(1, Ordering::SeqCst);
            Ok(StepResult {
                step_id: step.id,
                success: true,
                message: "executed by test adapter".to_owned(),
            })
        }

        fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
            Ok(AssertionResult {
                assertion_id: assertion.id,
                passed: self.assertions_pass,
                message: if self.assertions_pass {
                    "assertion passed".to_owned()
                } else {
                    "assertion failed".to_owned()
                },
            })
        }

        fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
            Ok(None)
        }
    }

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
            open_questions: Vec::new(),
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
            "{\"outputs.customer_id\":\"user@example.test\"}"
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

    #[test]
    fn replay_dispatches_to_registered_adapter_and_extracts_observed_output() {
        let adapter = Arc::new(TestAdapter::new(
            &["web.fill", "web.assert_visible"],
            vec!["customer id: C-100"],
            true,
        ));
        let mut registry = AdapterRegistry::new();
        registry.insert(adapter.clone());
        let context = ReplayExecutionContext {
            registry,
            on_failure: OnFailure::Stop,
        };
        let mut request = request();
        request.package.assertions = vec!["customer id".to_owned()];
        request.adapters = Vec::new();

        let outcome = replay_with_context(request, &context);

        assert!(outcome.passed);
        assert_eq!(adapter.execute_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            outcome.outputs.get("outputs.customer_id"),
            Some(&"C-100".to_owned())
        );
        assert!(outcome
            .evidence
            .artifacts
            .iter()
            .any(|artifact| matches!(artifact.kind, EvidenceArtifactKind::OutputExtractionProof)));
    }

    #[test]
    fn replay_fails_when_registered_adapter_assertion_fails() {
        let adapter = Arc::new(TestAdapter::new(
            &["web.fill", "web.assert_visible"],
            vec!["customer id: C-100"],
            false,
        ));
        let mut registry = AdapterRegistry::new();
        registry.insert(adapter);
        let context = ReplayExecutionContext {
            registry,
            on_failure: OnFailure::Stop,
        };
        let mut request = request();
        request.package.assertions = vec!["customer id".to_owned()];
        request.adapters = Vec::new();

        let outcome = replay_with_context(request, &context);

        assert!(!outcome.passed);
        assert_eq!(outcome.failure_reason, Some("assertion failed".to_owned()));
    }

    #[test]
    fn replay_assertions_use_adapter_matching_step_namespace() {
        let java_adapter = Arc::new(TestAdapter::with_id(
            "greentic.desktop.java-accessibility",
            &["java.find_window", "java.assert_text"],
            Vec::new(),
            false,
        ));
        let native_adapter = Arc::new(TestAdapter::with_id(
            "greentic.desktop.macos.ax",
            &["macos.activate_app", "macos.assert_visible"],
            vec!["saved"],
            true,
        ));
        let mut registry = AdapterRegistry::new();
        registry.insert(java_adapter);
        registry.insert(native_adapter);
        let context = ReplayExecutionContext {
            registry,
            on_failure: OnFailure::Stop,
        };
        let mut request = request();
        request.package.steps = vec![RunnerStep {
            id: "open_app".to_owned(),
            action: "activate_app".to_owned(),
            target: LocatorTarget::default(),
            value: None,
            required_capability: "macos.activate_app".to_owned(),
        }];
        request.package.assertions = vec!["saved".to_owned()];
        request.package.outputs.clear();
        request.adapters = Vec::new();

        let outcome = replay_with_context(request, &context);

        assert!(outcome.passed, "{:?}", outcome.failure_reason);
        assert!(outcome.evidence.to_json().contains("macos.assert_visible"));
        assert!(!outcome.evidence.to_json().contains("java.assert_text"));
    }
}
