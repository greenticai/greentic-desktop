use greentic_desktop_adapter::{
    validate_required_capabilities, AdapterCapabilities, AdapterError, Assertion, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RunnerStep, StepResult,
};
use greentic_desktop_evidence::{
    EvidenceArtifact, EvidenceArtifactKind, EvidenceBundle, EvidenceRef, EvidenceStatus,
    ToolTraceEntry,
};
use greentic_desktop_recorder::RunnerPackage;
use greentic_desktop_session::{plan_bootstrap, BootstrapPlan, SessionProfile};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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
    pub step_timeout: Option<Duration>,
}

impl ReplayExecutionContext {
    pub fn new(registry: AdapterRegistry) -> Self {
        Self {
            registry,
            on_failure: OnFailure::Stop,
            step_timeout: None,
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
    fail_without_real_registry(request)
}

fn fail_without_real_registry(request: ReplayRequest) -> ReplayOutcome {
    let bootstrap = BootstrapPlan {
        profile_id: request.session_profile.id.clone(),
        started_process_refs: Vec::new(),
        opened_targets: Vec::new(),
    };
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
    ReplayOutcome {
        passed: false,
        bootstrap,
        traces: Vec::new(),
        outputs: BTreeMap::new(),
        evidence,
        evidence_ref,
        failure_reason: Some(
            "real adapter registry is required; capability-only replay is disabled".to_owned(),
        ),
    }
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
        let executable_step = executable_step_for_replay(step);
        let execution_capability = executable_step.required_capability.clone();
        let skip_step = should_skip_step_for_replay(&executable_step);
        let adapter = (!skip_step)
            .then(|| selector.select(&execution_capability))
            .flatten();
        let result = if unresolved {
            Err(AdapterError::ExecutionFailed(
                "unresolved input or secret".to_owned(),
            ))
        } else if skip_step {
            Ok(StepResult {
                step_id: executable_step.id,
                success: true,
                message: "active document focus is handled by the following text step".to_owned(),
            })
        } else if let Some(adapter) = adapter {
            let mut executable = executable_step;
            executable.value = resolved;
            execute_step_with_timeout(
                adapter,
                executable,
                format!("replay-{}", request.package.id),
                Some(step.target.clone()),
                context.step_timeout,
            )
            .map(|(result, observation)| {
                observations.push(observation);
                result
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
            capability: execution_capability,
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
            let failure_reason = format_step_failure(step, reason.as_deref());
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
                failure_reason: Some(failure_reason),
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

    let outputs = match extract_outputs(&request.package, &observations, &step_results) {
        Ok(outputs) => outputs,
        Err(reason) => {
            tool_trace.push(ToolTraceEntry {
                step_id: "output-extraction".to_owned(),
                capability: "outputs.extract".to_owned(),
                status: EvidenceStatus::Failed,
                message: Some(reason.clone()),
            });
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
    };

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

fn execute_step_with_timeout(
    adapter: Arc<dyn DesktopAdapter>,
    executable: RunnerStep,
    session_id: String,
    target: Option<LocatorTarget>,
    timeout: Option<Duration>,
) -> Result<(StepResult, Observation), AdapterError> {
    let execute = move || {
        adapter.execute(executable).and_then(|result| {
            let observation = adapter.observe(ObserveContext { session_id, target })?;
            Ok((result, observation))
        })
    };
    if let Some(timeout) = timeout {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(execute());
        });
        rx.recv_timeout(timeout).map_err(|_| {
            AdapterError::ExecutionFailed(format!(
                "replay step timed out after {} ms",
                timeout.as_millis()
            ))
        })?
    } else {
        execute()
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

fn executable_step_for_replay(step: &RunnerStep) -> RunnerStep {
    if step.action == "press_shortcut"
        && step.required_capability.ends_with(".click_element")
        && step
            .value
            .as_deref()
            .is_some_and(|value| looks_like_shortcut(value))
    {
        let mut executable = step.clone();
        if let Some((prefix, _)) = step.required_capability.split_once('.') {
            executable.required_capability = format!("{prefix}.press_shortcut");
        }
        return executable;
    }
    step.clone()
}

fn should_skip_step_for_replay(step: &RunnerStep) -> bool {
    step.action == "focus_document"
        && step.required_capability == "macos.focus_document"
        && is_active_document_target(&step.target)
}

fn is_active_document_target(target: &LocatorTarget) -> bool {
    [target.preferred.as_ref(), target.fallback.as_ref()]
        .into_iter()
        .flatten()
        .any(|strategy| {
            strategy
                .name
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("active document"))
                || strategy
                    .label
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("active document"))
                || strategy
                    .role
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("document"))
        })
}

fn looks_like_shortcut(value: &str) -> bool {
    let value = value.trim();
    value.contains('+')
        && value.split('+').any(|part| {
            matches!(
                part.trim().to_ascii_lowercase().as_str(),
                "cmd"
                    | "command"
                    | "ctrl"
                    | "control"
                    | "alt"
                    | "option"
                    | "shift"
                    | "meta"
                    | "win"
            )
        })
}

fn format_step_failure(step: &RunnerStep, reason: Option<&str>) -> String {
    let detail = reason
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("adapter did not provide a failure reason");
    let value = step
        .value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!(" value={value:?}"))
        .unwrap_or_default();
    format!(
        "Step '{}' failed: action={} capability={}{}: {}",
        step.id, step.action, step.required_capability, value, detail
    )
}

fn run_assertions(
    request: &ReplayRequest,
    selector: &ReplayAdapterSelector<'_>,
    tool_trace: &mut Vec<ToolTraceEntry>,
) -> Option<String> {
    for (index, assertion) in request.package.assertions.iter().enumerate() {
        let adapter = selector
            .registry
            .adapters
            .values()
            .find(|adapter| {
                adapter
                    .capabilities()
                    .capabilities
                    .iter()
                    .any(|capability| capability.contains("assert") || capability.contains("wait"))
            })
            .cloned()
            .or_else(|| selector.registry.adapters.values().next().cloned())?;
        let capability = adapter
            .capabilities()
            .capabilities
            .into_iter()
            .find(|capability| capability.contains("assert") || capability.contains("wait"))
            .or_else(|| {
                request
                    .package
                    .steps
                    .first()
                    .map(|step| step.required_capability.clone())
            })
            .unwrap_or_else(|| "assertion.validate".to_owned());
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

fn extract_outputs(
    package: &RunnerPackage,
    observations: &[Observation],
    step_results: &[StepResult],
) -> Result<BTreeMap<String, String>, String> {
    let mut outputs = BTreeMap::new();
    for output in &package.outputs {
        let value = extract_output_value(output, observations, step_results)
            .ok_or_else(|| format!("required output {output} was not extracted"))?;
        if let Some(path) = local_path_output(&value) {
            if !Path::new(path).exists() {
                return Err(format!(
                    "output evidence missing: {output} points to {path} but that path does not exist"
                ));
            }
        }
        outputs.insert(output.clone(), value);
    }
    Ok(outputs)
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
    let observation_lines = observations
        .iter()
        .rev()
        .flat_map(|observation| observation.visible_text.iter());
    if let Some(value) = extract_labeled_output(&name, observation_lines) {
        return Some(value);
    }
    let step_lines = step_results
        .iter()
        .rev()
        .filter(|result| result.success)
        .filter(|result| !result.message.trim().is_empty())
        .map(|result| &result.message);
    if let Some(value) = extract_labeled_output(&name, step_lines) {
        return Some(value);
    }
    None
}

fn extract_labeled_output<'a>(
    name: &str,
    lines: impl Iterator<Item = &'a String>,
) -> Option<String> {
    for line in lines {
        let lower = line.to_ascii_lowercase();
        for separator in [":", "="] {
            let prefix = format!("{name}{separator}");
            let spaced_prefix = format!("{name} {separator}");
            let value_start = if lower.starts_with(&prefix) {
                Some(prefix.len())
            } else if lower.starts_with(&spaced_prefix) {
                Some(spaced_prefix.len())
            } else {
                None
            };
            if let Some(index) = value_start {
                return Some(line[index..].trim().to_owned());
            }
        }
    }
    None
}

fn local_path_output(value: &str) -> Option<&str> {
    let value = value.trim();
    if let Some(path) = value.strip_prefix("file://") {
        return Some(path);
    }
    if value.starts_with("evidence://")
        || value.starts_with("http://")
        || value.starts_with("https://")
    {
        return None;
    }
    if value.starts_with('/') || looks_like_windows_absolute_path(value) {
        Some(value)
    } else {
        None
    }
}

fn looks_like_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() > 2
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
        && bytes[0].is_ascii_alphabetic()
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
    use greentic_desktop_adapter::{AdapterResult, AssertionResult, RecordedEvent};
    use greentic_desktop_recorder::RecordingMode;
    use greentic_desktop_session::{BootstrapAction, BrowserKind};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug)]
    struct TestAdapter {
        capabilities: AdapterCapabilities,
        visible_text: Vec<String>,
        execute_count: AtomicUsize,
        assertions_pass: bool,
    }

    impl TestAdapter {
        fn new(capabilities: &[&str], visible_text: Vec<&str>, assertions_pass: bool) -> Self {
            Self {
                capabilities: AdapterCapabilities::new(
                    "greentic.desktop.test",
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

    struct FailingAdapter {
        capabilities: AdapterCapabilities,
        message: String,
    }

    impl DesktopAdapter for FailingAdapter {
        fn capabilities(&self) -> AdapterCapabilities {
            self.capabilities.clone()
        }

        fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
            Ok(Observation {
                adapter_id: self.capabilities.adapter_id.clone(),
                summary: ctx.session_id,
                visible_text: Vec::new(),
            })
        }

        fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
            Ok(StepResult {
                step_id: step.id,
                success: false,
                message: self.message.clone(),
            })
        }

        fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
            Ok(AssertionResult {
                assertion_id: assertion.id,
                passed: true,
                message: "assertion passed".to_owned(),
            })
        }

        fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
            Ok(None)
        }
    }

    struct SlowAdapter {
        capabilities: AdapterCapabilities,
        delay: Duration,
    }

    impl DesktopAdapter for SlowAdapter {
        fn capabilities(&self) -> AdapterCapabilities {
            self.capabilities.clone()
        }

        fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
            Ok(Observation {
                adapter_id: self.capabilities.adapter_id.clone(),
                summary: ctx.session_id,
                visible_text: Vec::new(),
            })
        }

        fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
            std::thread::sleep(self.delay);
            Ok(StepResult {
                step_id: step.id,
                success: true,
                message: "slow step eventually completed".to_owned(),
            })
        }

        fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
            Ok(AssertionResult {
                assertion_id: assertion.id,
                passed: true,
                message: "assertion passed".to_owned(),
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
    fn legacy_replay_without_adapter_registry_fails_closed() {
        let outcome = replay(request());

        assert!(!outcome.passed);
        assert!(outcome
            .failure_reason
            .expect("failure reason")
            .contains("real adapter registry is required"));
    }

    #[test]
    fn replays_runner_with_inputs_and_returns_outputs_json() {
        let adapter = Arc::new(TestAdapter::new(
            &["web.fill"],
            vec!["customer id: C-100"],
            true,
        ));
        let mut registry = AdapterRegistry::new();
        registry.insert(adapter);
        let context = ReplayExecutionContext {
            registry,
            on_failure: OnFailure::Stop,
            step_timeout: None,
        };

        let outcome = replay_with_context(request(), &context);

        assert!(outcome.passed);
        assert_eq!(outcome.traces.len(), 1);
        assert_eq!(
            outcome.outputs_json(),
            "{\"outputs.customer_id\":\"C-100\"}"
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
            step_timeout: None,
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
    fn replay_failure_reason_identifies_failed_step_and_adapter_message() {
        let mut registry = AdapterRegistry::new();
        registry.insert(Arc::new(FailingAdapter {
            capabilities: AdapterCapabilities::new(
                "greentic.desktop.test",
                "1.0.0",
                ["macos.activate_app"],
            ),
            message: "application 'Microsoft Word' is not installed or could not be launched"
                .to_owned(),
        }));
        let context = ReplayExecutionContext {
            registry,
            on_failure: OnFailure::Stop,
            step_timeout: None,
        };
        let mut request = request();
        request.package.steps = vec![RunnerStep {
            id: "primitive-1-open-app".to_owned(),
            action: "activate_app".to_owned(),
            target: LocatorTarget::default(),
            value: Some("Microsoft Word".to_owned()),
            required_capability: "macos.activate_app".to_owned(),
        }];
        request.package.assertions = Vec::new();
        request.package.outputs = Vec::new();

        let outcome = replay_with_context(request, &context);

        assert!(!outcome.passed);
        let reason = outcome.failure_reason.expect("failure reason");
        assert!(reason.contains("primitive-1-open-app"), "{reason}");
        assert!(reason.contains("activate_app"), "{reason}");
        assert!(reason.contains("macos.activate_app"), "{reason}");
        assert!(reason.contains("Microsoft Word"), "{reason}");
        assert!(reason.contains("not installed"), "{reason}");
        assert_eq!(
            outcome.traces[0].reason.as_deref(),
            Some("application 'Microsoft Word' is not installed or could not be launched")
        );
    }

    #[test]
    fn replay_repairs_shortcut_steps_with_click_capability() {
        let step = RunnerStep {
            id: "primitive-2-invoke-command".to_owned(),
            action: "press_shortcut".to_owned(),
            target: LocatorTarget::default(),
            value: Some("Cmd+N".to_owned()),
            required_capability: "macos.click_element".to_owned(),
        };

        let executable = executable_step_for_replay(&step);

        assert_eq!(executable.required_capability, "macos.press_shortcut");
        assert_eq!(executable.action, "press_shortcut");
        assert_eq!(executable.value.as_deref(), Some("Cmd+N"));
    }

    #[test]
    fn replay_skips_generic_active_document_focus_step() {
        let step = RunnerStep {
            id: "primitive-3-focus-target".to_owned(),
            action: "focus_document".to_owned(),
            target: LocatorTarget {
                preferred: Some(LocatorStrategy {
                    role: Some("document".to_owned()),
                    name: Some("active document".to_owned()),
                    label: Some("active document".to_owned()),
                    ..LocatorStrategy::default()
                }),
                ..LocatorTarget::default()
            },
            value: None,
            required_capability: "macos.focus_document".to_owned(),
        };

        assert!(should_skip_step_for_replay(&step));
    }

    #[test]
    fn replay_step_timeout_returns_failed_step_diagnostics() {
        let mut registry = AdapterRegistry::new();
        registry.insert(Arc::new(SlowAdapter {
            capabilities: AdapterCapabilities::new("greentic.desktop.test", "1.0.0", ["web.fill"]),
            delay: Duration::from_millis(100),
        }));
        let context = ReplayExecutionContext {
            registry,
            on_failure: OnFailure::Stop,
            step_timeout: Some(Duration::from_millis(5)),
        };

        let outcome = replay_with_context(request(), &context);

        assert!(!outcome.passed);
        let reason = outcome.failure_reason.expect("failure reason");
        assert!(reason.contains("fill_email"), "{reason}");
        assert!(reason.contains("timed out"), "{reason}");
        assert_eq!(outcome.traces.len(), 1);
        assert!(outcome.traces[0]
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("timed out"));
        assert_eq!(outcome.evidence.status, EvidenceStatus::Failed);
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
            step_timeout: None,
        };
        let mut request = request();
        request.package.assertions = vec!["customer id".to_owned()];
        request.adapters = Vec::new();

        let outcome = replay_with_context(request, &context);

        assert!(!outcome.passed);
        assert_eq!(outcome.failure_reason, Some("assertion failed".to_owned()));
    }

    #[test]
    fn replay_fails_when_path_output_does_not_exist() {
        let missing =
            std::env::temp_dir().join(format!("greentic-missing-output-{}", unique_suffix()));
        let adapter = Arc::new(TestAdapter::new(
            &["web.fill"],
            vec![&format!("saved status: {}", missing.to_string_lossy())],
            true,
        ));
        let mut registry = AdapterRegistry::new();
        registry.insert(adapter);
        let context = ReplayExecutionContext::new(registry);
        let mut request = request();
        request.package.outputs = vec!["outputs.saved_status".to_owned()];
        request.package.assertions.clear();

        let outcome = replay_with_context(request, &context);

        assert!(!outcome.passed);
        assert!(outcome
            .failure_reason
            .as_deref()
            .unwrap_or_default()
            .contains("output evidence missing"));
    }

    #[test]
    fn replay_accepts_existing_file_output_and_records_proof() {
        let root =
            std::env::temp_dir().join(format!("greentic-existing-output-{}", unique_suffix()));
        fs::create_dir_all(&root).expect("temp dir");
        let file = root.join("result.txt");
        fs::write(&file, "saved").expect("output file");
        let adapter = Arc::new(TestAdapter::new(
            &["web.fill"],
            vec![&format!("saved status: {}", file.to_string_lossy())],
            true,
        ));
        let mut registry = AdapterRegistry::new();
        registry.insert(adapter);
        let context = ReplayExecutionContext::new(registry);
        let mut request = request();
        request.package.outputs = vec!["outputs.saved_status".to_owned()];
        request.package.assertions.clear();

        let outcome = replay_with_context(request, &context);

        assert!(outcome.passed, "{:?}", outcome.failure_reason);
        assert_eq!(
            outcome.outputs.get("outputs.saved_status"),
            Some(&file.to_string_lossy().to_string())
        );
        assert!(outcome
            .evidence
            .to_json()
            .contains(&file.to_string_lossy().to_string()));
        let _ = fs::remove_dir_all(root);
    }

    fn unique_suffix() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        format!("{}-{nanos}", std::process::id())
    }
}
