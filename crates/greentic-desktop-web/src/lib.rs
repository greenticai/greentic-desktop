use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventEnvelope, RecordingEventSink,
    RecordingHandle, RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

pub const PLAYWRIGHT_ADAPTER_ID: &str = "greentic.desktop.playwright";
pub const PLAYWRIGHT_RECORDER_BACKEND_ID: &str = "greentic.recording.web.playwright";

pub fn playwright_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        PLAYWRIGHT_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "web.goto",
            "web.click",
            "web.fill",
            "web.select",
            "web.press",
            "web.wait_for",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaywrightRecorderOptions {
    pub initial_url: String,
    pub sidecar_command: String,
    pub browser_context: String,
    pub require_playwright: bool,
}

impl Default for PlaywrightRecorderOptions {
    fn default() -> Self {
        Self {
            initial_url: "about:blank".to_owned(),
            sidecar_command: "playwright".to_owned(),
            browser_context: "greentic-owned".to_owned(),
            require_playwright: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlaywrightWebRecordingBackend {
    options: PlaywrightRecorderOptions,
}

impl PlaywrightWebRecordingBackend {
    pub fn new(options: PlaywrightRecorderOptions) -> Self {
        Self { options }
    }
}

impl RecordingBackend for PlaywrightWebRecordingBackend {
    fn id(&self) -> &'static str {
        PLAYWRIGHT_RECORDER_BACKEND_ID
    }

    fn target_kind(&self) -> RecordingTargetKind {
        RecordingTargetKind::Web
    }

    fn preflight(&self, _request: &RecordingStartRequest) -> RecordingPreflight {
        if self.options.browser_context != "greentic-owned" {
            return RecordingPreflight::blocked(
                "Browser recording only supports Greentic-owned browser contexts.",
            );
        }

        if self.options.require_playwright {
            if find_node_command().is_none() {
                return RecordingPreflight::blocked(
                    "Node.js is required to start the Playwright web recorder.",
                );
            }
            if find_playwright_module_dir().is_none() {
                return RecordingPreflight::blocked(
                    "Playwright is required to start the web recorder. Run npm ci in frontend/automate-hub or install the browser automation extension.",
                );
            }
        }

        RecordingPreflight::ready()
    }

    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle {
        let mut state = RecordingCaptureState::Recording;
        let _ = append_initial_navigation_event(&self.options, &sink);
        match start_playwright_recorder_process(&request, &self.options, &sink) {
            Ok(()) => {
                let _ = sink.update_heartbeat();
            }
            Err(err) => {
                let _ = sink.append_backend_warning(&err);
                if self.options.require_playwright {
                    state = RecordingCaptureState::Failed;
                } else {
                    let _ = sink.update_heartbeat();
                }
            }
        }
        RecordingHandle {
            backend_id: PLAYWRIGHT_RECORDER_BACKEND_ID.to_owned(),
            capture_state: state,
        }
    }
}

fn append_initial_navigation_event(
    options: &PlaywrightRecorderOptions,
    sink: &RecordingEventSink,
) -> Result<(), greentic_desktop_recorder::RecordingLifecycleError> {
    let mut event = RecordingEventEnvelope::new(
        sink.session_id(),
        PLAYWRIGHT_RECORDER_BACKEND_ID,
        RecordingTargetKind::Web,
        1,
        "navigate",
    );
    event.value = Some(options.initial_url.clone());
    event.target_json = format!(
        r#"{{"url":"{}","ownership":"greentic-owned","sidecar":"{}"}}"#,
        escape_json(&options.initial_url),
        escape_json(&options.sidecar_command)
    );
    sink.append_event(event)
}

fn start_playwright_recorder_process(
    request: &RecordingStartRequest,
    options: &PlaywrightRecorderOptions,
    sink: &RecordingEventSink,
) -> Result<(), String> {
    let node = find_node_command().ok_or_else(|| "Node.js was not found on PATH.".to_owned())?;
    let playwright_module = find_playwright_module_dir()
        .ok_or_else(|| "Playwright module was not found for the web recorder.".to_owned())?;
    let script = request.out.join("web-recorder.js");
    let log = request.out.join("logs").join("web-recorder.log");
    if let Some(parent) = script.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(&script, web_recorder_script()).map_err(|err| err.to_string())?;
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|err| err.to_string())?;
    let err_file = log_file.try_clone().map_err(|err| err.to_string())?;
    // GREENTIC_NODE is a local operator override or fixed PATH lookup and is invoked directly without a shell.
    // foxguard: ignore[rs/no-command-injection]
    let mut command = Command::new(node);
    command
        .arg(&script)
        .arg(&request.out)
        .arg(sink.session_id())
        .arg(&options.initial_url)
        .arg(playwright_module)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(err_file));
    command
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("failed to start Playwright web recorder: {err}"))
}

fn find_node_command() -> Option<String> {
    std::env::var("GREENTIC_NODE").ok().or_else(|| {
        ["node", "nodejs"].iter().find_map(|candidate| {
            // Candidate comes from the fixed node/nodejs allow-list and is invoked directly without a shell.
            // foxguard: ignore[rs/no-command-injection]
            Command::new(candidate)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .ok()
                .filter(|status| status.success())
                .map(|_| (*candidate).to_owned())
        })
    })
}

fn find_playwright_module_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("GREENTIC_PLAYWRIGHT_MODULE") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.clone());
        for ancestor in cwd.ancestors() {
            roots.push(ancestor.to_path_buf());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.to_path_buf());
        }
    }

    roots.into_iter().find_map(|root| {
        [
            root.join("frontend/automate-hub/node_modules/playwright"),
            root.join("node_modules/playwright"),
        ]
        .into_iter()
        .find(|candidate| candidate.exists())
    })
}

fn web_recorder_script() -> &'static str {
    r#"
const fs = require('fs');
const path = require('path');

const [root, sessionId, initialUrl, playwrightModule] = process.argv.slice(2);
const rawEvents = path.join(root, 'raw', 'events.jsonl');
const snapshots = path.join(root, 'evidence', 'dom');
fs.mkdirSync(path.dirname(rawEvents), { recursive: true });
fs.mkdirSync(snapshots, { recursive: true });

const { chromium } = require(playwrightModule);
let sequence = 1;

function escapeText(value) {
  return String(value ?? '').replace(/\s+/g, ' ').trim().slice(0, 240);
}

function snake(value, fallback) {
  const out = escapeText(value)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '');
  return out || fallback;
}

function quote(value) {
  return JSON.stringify(value ?? null);
}

function redactTarget(target, value) {
  const text = [
    target?.label,
    target?.name,
    target?.placeholder,
    target?.test_id,
    target?.type,
  ].join(' ').toLowerCase();
  if (/password|secret|token|api[_ -]?key|credential/.test(text)) return '{{secret}}';
  return value;
}

function inputTemplate(target, value) {
  if (value == null || value === '') return value;
  const redacted = redactTarget(target, value);
  if (redacted === '{{secret}}') return redacted;
  return `{{inputs.${snake(target?.label || target?.name || target?.placeholder || target?.test_id || 'value', 'value')}}}`;
}

function append(kind, target, value, evidence = {}) {
  const renderedValue = kind === 'input' || kind === 'change' ? inputTemplate(target, value) : value;
  const redaction = renderedValue === '{{secret}}'
    ? 'redacted'
    : renderedValue && /^\{\{inputs\./.test(renderedValue)
      ? 'input_candidate'
      : 'none';
  const event = {
    schema_version: 'recording.event.v1',
    session_id: sessionId,
    backend: 'greentic.recording.web.playwright',
    target_kind: 'web',
    timestamp: String(Math.floor(Date.now() / 1000)),
    sequence: sequence++,
    event: { kind, target: target || {}, value: renderedValue ?? null, redaction },
    evidence: {
      screenshot_ref: evidence.screenshot_ref ?? null,
      dom_snapshot_ref: evidence.dom_snapshot_ref ?? null,
      ui_tree_ref: null,
      terminal_buffer_ref: null,
    },
  };
  fs.appendFileSync(rawEvents, `${JSON.stringify(event)}\n`);
}

function initRecorder() {
  window.__greenticSelector = function cssPath(element) {
    if (!element || !element.tagName) return null;
    if (element.id) return `#${CSS.escape(element.id)}`;
    const parts = [];
    let current = element;
    while (current && current.nodeType === Node.ELEMENT_NODE && parts.length < 5) {
      let part = current.tagName.toLowerCase();
      const testId = current.getAttribute('data-testid') || current.getAttribute('data-test');
      if (testId) {
        part += `[data-testid="${CSS.escape(testId)}"]`;
        parts.unshift(part);
        break;
      }
      const parent = current.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter((child) => child.tagName === current.tagName);
        if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(current) + 1})`;
      }
      parts.unshift(part);
      current = parent;
    }
    return parts.join(' > ');
  };

  window.__greenticTarget = function targetFor(element) {
    const labelledBy = element.getAttribute('aria-labelledby');
    const labelledText = labelledBy
      ? labelledBy.split(/\s+/).map((id) => document.getElementById(id)?.innerText || '').join(' ').trim()
      : '';
    const label = element.labels && element.labels.length
      ? Array.from(element.labels).map((item) => item.innerText).join(' ').trim()
      : '';
    return {
      role: element.getAttribute('role') || null,
      name: element.getAttribute('aria-label') || labelledText || null,
      label: label || null,
      placeholder: element.getAttribute('placeholder') || null,
      test_id: element.getAttribute('data-testid') || element.getAttribute('data-test') || null,
      accessible_name: element.getAttribute('aria-label') || labelledText || label || element.innerText || element.value || null,
      type: element.getAttribute('type') || null,
      text: element.innerText ? element.innerText.trim().slice(0, 160) : null,
      css: window.__greenticSelector(element),
      xpath: null,
    };
  };

  document.addEventListener('click', (event) => {
    window.__greenticRecord({ kind: 'click', target: window.__greenticTarget(event.target), value: null });
  }, true);
  document.addEventListener('input', (event) => {
    window.__greenticRecord({ kind: 'input', target: window.__greenticTarget(event.target), value: event.target.value ?? null });
  }, true);
  document.addEventListener('change', (event) => {
    window.__greenticRecord({ kind: 'change', target: window.__greenticTarget(event.target), value: event.target.value ?? null });
  }, true);
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' || event.key === 'Tab' || event.key === 'Escape') {
      window.__greenticRecord({ kind: 'key', target: window.__greenticTarget(event.target), value: event.key });
    }
  }, true);
}

(async () => {
  const headless = process.env.GREENTIC_WEB_RECORDER_HEADLESS === '1';
  const browser = await chromium.launch({ headless });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  await context.exposeBinding('__greenticRecord', async ({ page }, payload) => {
    let snapshot = null;
    try {
      const ref = `dom-${Date.now()}-${sequence}.html`;
      fs.writeFileSync(path.join(snapshots, ref), await page.content());
      snapshot = `evidence/dom/${ref}`;
    } catch (_) {}
    append(payload.kind, payload.target, payload.value, { dom_snapshot_ref: snapshot });
  });
  await context.addInitScript(`(${initRecorder.toString()})();`);
  const page = await context.newPage();
  page.on('framenavigated', (frame) => {
    if (frame === page.mainFrame()) append('navigate', { url: frame.url(), ownership: 'greentic-owned' }, frame.url());
  });
  await page.goto(initialUrl || 'about:blank');
  if (process.env.GREENTIC_WEB_RECORDER_SMOKE === '1') {
    const input = page.locator('input,textarea,[contenteditable="true"]').first();
    if (await input.count()) {
      await input.click();
      await input.pressSequentially('41');
    }
    const button = page.locator('button,input[type="button"],input[type="submit"]').first();
    if (await button.count()) await button.click();
    await page.waitForTimeout(1000);
  }
  console.log(`Greentic web recorder started for ${sessionId}`);
  if (process.env.GREENTIC_WEB_RECORDER_HEADLESS === '1' && process.env.GREENTIC_WEB_RECORDER_AUTO_CLOSE_MS) {
    setTimeout(async () => {
      await browser.close();
      process.exit(0);
    }, Number(process.env.GREENTIC_WEB_RECORDER_AUTO_CLOSE_MS));
  }
})().catch((error) => {
  append('backend_warning', {}, error && error.stack ? error.stack : String(error));
  console.error(error);
  process.exit(1);
});
"#
}

pub fn web_recording_event(
    session_id: &str,
    sequence: u64,
    kind: &str,
    metadata: &WebElementMetadata,
    value: Option<String>,
) -> RecordingEventEnvelope {
    let redacted_value = value.map(|value| redact_if_secret(metadata, &value));
    let mut event = RecordingEventEnvelope::new(
        session_id,
        PLAYWRIGHT_RECORDER_BACKEND_ID,
        RecordingTargetKind::Web,
        sequence,
        kind,
    );
    event.target_json = locator_candidates_json(metadata);
    event.redaction = if redacted_value.as_deref() == Some("{{secret}}") {
        "redacted".to_owned()
    } else if redacted_value.is_some() {
        "input_candidate".to_owned()
    } else {
        "none".to_owned()
    };
    event.value = redacted_value;
    event
}

fn locator_candidates_json(metadata: &WebElementMetadata) -> String {
    format!(
        r#"{{"role":{},"name":{},"label":{},"placeholder":null,"test_id":{},"accessible_name":{},"css":{},"xpath":{}}}"#,
        json_option(metadata.role.as_deref()),
        json_option(metadata.name.as_deref()),
        json_option(metadata.label.as_deref()),
        json_option(metadata.data_testid.as_deref()),
        json_option(metadata.name.as_deref().or(metadata.label.as_deref())),
        json_option(metadata.css.as_deref()),
        json_option(metadata.xpath.as_deref()),
    )
}

#[derive(Debug, Clone, Default)]
pub struct PlaywrightWebAdapter {
    state: Arc<Mutex<WebAdapterState>>,
}

#[derive(Debug, Default)]
struct WebAdapterState {
    sidecar: Option<PlaywrightReplaySidecar>,
    recorded: Vec<RecordedEvent>,
}

#[derive(Debug)]
struct PlaywrightReplaySidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

#[derive(Debug, Serialize)]
struct PlaywrightSidecarRequest {
    id: String,
    #[serde(rename = "type")]
    kind: PlaywrightSidecarRequestKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<RunnerStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assertion: Option<Assertion>,
}

impl PlaywrightSidecarRequest {
    fn observe(id: String, session_id: String) -> Self {
        Self {
            id,
            kind: PlaywrightSidecarRequestKind::Observe,
            session_id: Some(session_id),
            step: None,
            assertion: None,
        }
    }

    fn step(id: String, step: RunnerStep) -> Self {
        Self {
            id,
            kind: PlaywrightSidecarRequestKind::Step,
            session_id: None,
            step: Some(step),
            assertion: None,
        }
    }

    fn assertion(id: String, assertion: Assertion) -> Self {
        Self {
            id,
            kind: PlaywrightSidecarRequestKind::Assert,
            session_id: None,
            step: None,
            assertion: Some(assertion),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum PlaywrightSidecarRequestKind {
    Observe,
    Step,
    Assert,
}

#[derive(Debug, Deserialize)]
struct PlaywrightSidecarResponse {
    id: Option<String>,
    ok: bool,
    #[serde(default)]
    result: serde_json::Value,
    error: Option<String>,
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
}

impl DesktopAdapter for PlaywrightWebAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        playwright_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        let response = self.with_sidecar(|sidecar| sidecar.observe(ctx.session_id))?;
        let summary = response
            .get("summary")
            .and_then(|value| value.as_str())
            .unwrap_or("web page observed")
            .to_owned();
        let visible_text = response
            .get("visible_text")
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Observation {
            adapter_id: PLAYWRIGHT_ADAPTER_ID.to_owned(),
            summary,
            visible_text,
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }

        let original_step = step.clone();
        let response = self.with_sidecar(|sidecar| sidecar.step(step))?;
        self.state
            .lock()
            .expect("web adapter mutex poisoned")
            .recorded
            .push(RecordedEvent {
                action: original_step.action,
                target: original_step.target,
                value: original_step.value,
            });

        Ok(StepResult {
            step_id: original_step.id,
            success: response
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(true),
            message: response
                .get("message")
                .and_then(|value| value.as_str())
                .unwrap_or("web step executed")
                .to_owned(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }

        let assertion_id = assertion.id.clone();
        let response = self.with_sidecar(|sidecar| sidecar.assertion(assertion))?;
        let passed = response
            .get("passed")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        Ok(AssertionResult {
            assertion_id,
            passed,
            message: if passed {
                "web assertion passed".to_owned()
            } else {
                response
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("web assertion failed")
                    .to_owned()
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

impl PlaywrightWebAdapter {
    fn with_sidecar<T>(
        &self,
        operation: impl FnOnce(&mut PlaywrightReplaySidecar) -> AdapterResult<T>,
    ) -> AdapterResult<T> {
        let mut state = self.state.lock().expect("web adapter mutex poisoned");
        if state.sidecar.is_none() {
            state.sidecar = Some(PlaywrightReplaySidecar::start()?);
        }
        operation(state.sidecar.as_mut().expect("sidecar initialized"))
    }
}

impl PlaywrightReplaySidecar {
    fn start() -> AdapterResult<Self> {
        let node = find_node_command().ok_or_else(|| {
            AdapterError::ExecutionFailed(
                "Node.js is required for real Playwright web replay.".to_owned(),
            )
        })?;
        let playwright_module = find_playwright_module_dir().ok_or_else(|| {
            AdapterError::ExecutionFailed(
                "Playwright is required for real web replay. Run npm ci in frontend/automate-hub or install the browser automation extension.".to_owned(),
            )
        })?;
        let root = std::env::temp_dir().join(format!(
            "greentic-web-replay-{}-{}",
            std::process::id(),
            epoch_millis()
        ));
        fs::create_dir_all(root.join("evidence")).map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to create web replay directory: {err}"))
        })?;
        let script = root.join("web-replay-sidecar.js");
        fs::write(&script, web_replay_sidecar_script()).map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to write web replay sidecar: {err}"))
        })?;
        // GREENTIC_NODE is a local operator override or fixed PATH lookup and is invoked directly without a shell.
        // foxguard: ignore[rs/no-command-injection]
        let mut child = Command::new(node)
            .arg(&script)
            .arg(&root)
            .arg(playwright_module)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                AdapterError::ExecutionFailed(format!(
                    "failed to start Playwright web replay sidecar: {err}"
                ))
            })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            AdapterError::ExecutionFailed("Playwright sidecar stdin is unavailable.".to_owned())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AdapterError::ExecutionFailed("Playwright sidecar stdout is unavailable.".to_owned())
        })?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
        })
    }

    fn observe(&mut self, session_id: String) -> AdapterResult<serde_json::Value> {
        let id = self.next_request_id();
        self.request(PlaywrightSidecarRequest::observe(id, session_id))
    }

    fn step(&mut self, step: RunnerStep) -> AdapterResult<serde_json::Value> {
        let id = self.next_request_id();
        self.request(PlaywrightSidecarRequest::step(id, step))
    }

    fn assertion(&mut self, assertion: Assertion) -> AdapterResult<serde_json::Value> {
        let id = self.next_request_id();
        self.request(PlaywrightSidecarRequest::assertion(id, assertion))
    }

    fn next_request_id(&mut self) -> String {
        self.next_id = self.next_id.saturating_add(1);
        format!("web-sidecar-{}-{}", epoch_millis(), self.next_id)
    }

    fn request(&mut self, request: PlaywrightSidecarRequest) -> AdapterResult<serde_json::Value> {
        if let Some(status) = self.child.try_wait().map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to inspect Playwright sidecar: {err}"))
        })? {
            return Err(AdapterError::ExecutionFailed(format!(
                "Playwright sidecar exited before request {} with status {status}",
                request.id
            )));
        }
        let request_id = request.id.clone();
        let line = serde_json::to_string(&request).map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to encode Playwright request: {err}"))
        })?;
        writeln!(self.stdin, "{line}").map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to write Playwright request: {err}"))
        })?;
        self.stdin.flush().map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to flush Playwright request: {err}"))
        })?;
        let mut response = String::new();
        self.stdout.read_line(&mut response).map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to read Playwright response: {err}"))
        })?;
        if response.trim().is_empty() {
            return Err(AdapterError::ExecutionFailed(format!(
                "Playwright web replay sidecar exited without response to {request_id}."
            )));
        }
        let response: PlaywrightSidecarResponse =
            serde_json::from_str(&response).map_err(|err| {
                AdapterError::ExecutionFailed(format!("invalid Playwright response JSON: {err}"))
            })?;
        if response.id.as_deref() != Some(request_id.as_str()) {
            return Err(AdapterError::ExecutionFailed(format!(
                "Playwright sidecar response id {:?} did not match request id {request_id}",
                response.id
            )));
        }
        if response.ok {
            Ok(response.result)
        } else {
            Err(AdapterError::ExecutionFailed(
                response
                    .error
                    .unwrap_or_else(|| "Playwright web replay failed".to_owned()),
            ))
        }
    }
}

fn epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn web_replay_sidecar_script() -> &'static str {
    r#"
const fs = require('fs');
const path = require('path');
const readline = require('readline');

const [root, playwrightModule] = process.argv.slice(2);
const evidenceRoot = path.join(root, 'evidence');
fs.mkdirSync(evidenceRoot, { recursive: true });

const { chromium } = require(playwrightModule);
let browser;
let context;
let page;
let sequence = 1;
const consoleErrors = [];
const networkFailures = [];

function targetStrategies(target) {
  return [target?.preferred, target?.fallback].filter(Boolean);
}

function roleOptions(strategy) {
  const options = {};
  if (strategy.name) options.name = strategy.name;
  return options;
}

function locatorFor(strategy) {
  if (!strategy) return null;
  if (strategy.data_testid) return page.getByTestId(strategy.data_testid);
  if (strategy.role) return page.getByRole(strategy.role, roleOptions(strategy));
  if (strategy.label) return page.getByLabel(strategy.label);
  if (strategy.text) return page.getByText(strategy.text);
  if (strategy.name) return page.getByRole('button', { name: strategy.name }).or(page.getByLabel(strategy.name)).or(page.getByText(strategy.name));
  if (strategy.css) return page.locator(strategy.css);
  if (strategy.xpath) return page.locator(`xpath=${strategy.xpath}`);
  return null;
}

async function resolveLocator(target) {
  for (const strategy of targetStrategies(target)) {
    const locator = locatorFor(strategy);
    if (!locator) continue;
    if (await locator.count().catch(() => 0)) return locator.first();
  }
  return null;
}

function valueFromTargetUrl(step) {
  return step.value
    || step.target?.preferred?.text
    || step.target?.preferred?.css
    || step.target?.fallback?.text
    || 'about:blank';
}

async function ensurePage() {
  if (page) return;
  browser = await chromium.launch({ headless: process.env.GREENTIC_WEB_REPLAY_HEADLESS !== '0' });
  context = await browser.newContext({ acceptDownloads: true, viewport: { width: 1280, height: 900 } });
  page = await context.newPage();
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('requestfailed', (request) => {
    networkFailures.push(`${request.method()} ${request.url()} ${request.failure()?.errorText || 'failed'}`);
  });
}

async function captureEvidence(label) {
  await ensurePage();
  const name = `${Date.now()}-${sequence++}-${String(label || 'failure').replace(/[^a-z0-9_.-]+/gi, '_')}`;
  const screenshot = path.join(evidenceRoot, `${name}.png`);
  const dom = path.join(evidenceRoot, `${name}.html`);
  try { await page.screenshot({ path: screenshot, fullPage: true }); } catch (_) {}
  try { fs.writeFileSync(dom, await page.content()); } catch (_) {}
  return { screenshot, dom, console_errors: consoleErrors.slice(), network_failures: networkFailures.slice() };
}

async function runStep(step) {
  await ensurePage();
  const capability = step.required_capability;
  if (capability === 'web.goto' || capability === 'web.assert_url') {
    await page.goto(valueFromTargetUrl(step), { waitUntil: 'domcontentloaded' });
    return { success: true, message: `navigated to ${page.url()}` };
  }
  if (capability === 'web.wait_for') {
    const locator = await resolveLocator(step.target);
    if (locator) await locator.waitFor({ state: 'visible', timeout: 10000 });
    else await page.waitForTimeout(Number(step.value || 250));
    return { success: true, message: 'wait completed' };
  }
  if (capability === 'web.wait_for_text') {
    await page.getByText(step.value || step.target?.preferred?.text || '').waitFor({ timeout: 10000 });
    return { success: true, message: 'text appeared' };
  }
  const locator = await resolveLocator(step.target);
  if (!locator && !['web.press', 'web.screenshot'].includes(capability)) {
    throw new Error(`No element matched locator for ${capability}`);
  }
  if (capability === 'web.fill') {
    await locator.fill(step.value || '');
    return { success: true, message: 'field filled' };
  }
  if (capability === 'web.select') {
    await locator.selectOption(step.value || '');
    return { success: true, message: 'option selected' };
  }
  if (capability === 'web.click' || capability === 'web.assert_visible') {
    await locator.click();
    return { success: true, message: 'element clicked' };
  }
  if (capability === 'web.press') {
    if (locator) await locator.press(step.value || 'Enter');
    else await page.keyboard.press(step.value || 'Enter');
    return { success: true, message: 'key pressed' };
  }
  if (capability === 'web.extract_text') {
    const text = locator ? await locator.innerText() : await page.locator('body').innerText();
    return { success: true, message: text };
  }
  if (capability === 'web.extract_regex') {
    const text = locator ? await locator.innerText() : await page.locator('body').innerText();
    const pattern = step.value || step.target?.preferred?.text || '(.+)';
    const match = text.match(new RegExp(pattern));
    return { success: true, message: match ? match[0] : '' };
  }
  if (capability === 'web.screenshot') {
    const file = step.value || path.join(evidenceRoot, `screenshot-${Date.now()}.png`);
    await page.screenshot({ path: file, fullPage: true });
    return { success: fs.existsSync(file), message: file };
  }
  if (capability === 'web.download_file') {
    const file = step.value;
    if (!file) throw new Error('web.download_file requires an output file path in step.value');
    const downloadPromise = page.waitForEvent('download', { timeout: 15000 });
    await locator.click();
    const download = await downloadPromise;
    await download.saveAs(file);
    if (!fs.existsSync(file)) throw new Error(`download did not create ${file}`);
    return { success: true, message: file };
  }
  throw new Error(`Unsupported web capability ${capability}`);
}

async function runAssert(assertion) {
  await ensurePage();
  if (assertion.required_capability === 'web.assert_url') {
    const passed = page.url().includes(assertion.expected || '');
    return { passed, message: passed ? 'url matched' : `url ${page.url()} did not include ${assertion.expected}` };
  }
  const locator = await resolveLocator(assertion.target);
  if (assertion.required_capability === 'web.assert_visible') {
    if (locator) {
      const passed = await locator.isVisible().catch(() => false);
      return { passed, message: passed ? 'element visible' : 'element not visible' };
    }
    const passed = await page.getByText(assertion.expected || '').isVisible().catch(() => false);
    return { passed, message: passed ? 'text visible' : 'text not visible' };
  }
  const text = locator ? await locator.innerText().catch(() => '') : await page.locator('body').innerText().catch(() => '');
  const passed = assertion.required_capability === 'web.extract_regex'
    ? new RegExp(assertion.expected || '.*').test(text)
    : text.includes(assertion.expected || '');
  return { passed, message: passed ? 'assertion matched' : 'assertion did not match page text' };
}

async function observe() {
  await ensurePage();
  const body = await page.locator('body').innerText().catch(() => '');
  return {
    summary: `web page at ${page.url()}`,
    visible_text: body ? [body] : [],
    console_errors: consoleErrors.slice(),
    network_failures: networkFailures.slice(),
  };
}

async function dispatch(request) {
  if (request.type === 'step') return runStep(request.step);
  if (request.type === 'assert') return runAssert(request.assertion);
  if (request.type === 'observe') return observe();
  throw new Error(`Unknown request type ${request.type}`);
}

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
rl.on('line', async (line) => {
  let request;
  try {
    request = JSON.parse(line);
    const result = await dispatch(request);
    process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, result })}\n`);
  } catch (error) {
    const evidence = await captureEvidence(request?.type || 'error').catch(() => ({}));
    process.stdout.write(`${JSON.stringify({ id: request?.id || null, ok: false, error: `${error && error.stack ? error.stack : String(error)} evidence=${JSON.stringify(evidence)}` })}\n`);
  }
});
process.on('SIGTERM', async () => {
  try { if (browser) await browser.close(); } catch (_) {}
  process.exit(0);
});
"#
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

fn json_option(value: Option<&str>) -> String {
    value
        .map(|value| format!(r#""{}""#, escape_json(value)))
        .unwrap_or_else(|| "null".to_owned())
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_desktop_recorder::{
        start_recording_session_with_registry, RecordingBackendRegistry, RecordingStartRequest,
    };
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn target(label: &str) -> LocatorTarget {
        LocatorTarget {
            preferred: Some(LocatorStrategy {
                label: Some(label.to_owned()),
                ..LocatorStrategy::default()
            }),
            fallback: Some(LocatorStrategy {
                text: Some(label.to_owned()),
                ..LocatorStrategy::default()
            }),
            ..LocatorTarget::default()
        }
    }

    fn serve_web_form_fixture() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fixture server should bind");
        let addr = listener.local_addr().expect("fixture addr");
        std::thread::spawn(move || {
            for _ in 0..8 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buffer = [0_u8; 2048];
                let _ = stream.read(&mut buffer);
                let html = r#"<!doctype html>
<html>
  <body>
    <form onsubmit="event.preventDefault(); document.querySelector('[data-testid=result]').textContent = 'Record saved REC-1001 for ' + document.querySelector('#email').value;">
      <label for="email">Email</label>
      <input id="email" data-testid="email" />
      <button type="submit">Submit</button>
    </form>
    <output data-testid="result"></output>
  </body>
</html>"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    html.len(),
                    html
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}/")
    }

    fn serve_download_fixture() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("download fixture should bind");
        let addr = listener.local_addr().expect("download fixture addr");
        std::thread::spawn(move || {
            for _ in 0..8 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buffer = [0_u8; 2048];
                let read = stream.read(&mut buffer).unwrap_or_default();
                let request = String::from_utf8_lossy(&buffer[..read]);
                if request.starts_with("GET /file") {
                    let body = "downloaded by greentic\n";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-disposition: attachment; filename=\"report.txt\"\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                } else {
                    let html = r#"<!doctype html><html><body><a href="/file" download>Download report</a></body></html>"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        html.len(),
                        html
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        format!("http://{addr}/")
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
            data_testid: Some("save-record".to_owned()),
            role: Some("button".to_owned()),
            name: Some("Save".to_owned()),
            label: None,
            text: Some("Save".to_owned()),
            css: None,
            xpath: None,
            visual_image: None,
        });

        let preferred = selector.preferred.expect("preferred selector");
        assert_eq!(preferred.data_testid, Some("save-record".to_owned()));
        assert_eq!(
            preferred.css,
            Some("[data-testid='save-record']".to_owned())
        );
    }

    #[test]
    fn can_open_fill_submit_and_observe_real_page_output() {
        if find_node_command().is_none() || find_playwright_module_dir().is_none() {
            eprintln!(
                "skipping real web replay smoke test because Node.js or Playwright is missing"
            );
            return;
        }

        let url = serve_web_form_fixture();
        let adapter = PlaywrightWebAdapter::new();
        let steps = vec![
            RunnerStep {
                id: "open".to_owned(),
                action: "goto".to_owned(),
                target: LocatorTarget::default(),
                value: Some(url),
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
                target: LocatorTarget::default(),
                expected: "Record saved REC-1001".to_owned(),
            })
            .expect("visible assertion should run");
        assert!(visible.passed);

        let observed = adapter
            .observe(ObserveContext {
                session_id: "test".to_owned(),
                target: None,
            })
            .expect("web page should be observable");
        assert!(observed
            .visible_text
            .iter()
            .any(|text| text.contains("Record saved REC-1001")));
    }

    #[test]
    fn download_file_passes_only_when_file_exists() {
        if find_node_command().is_none() || find_playwright_module_dir().is_none() {
            eprintln!(
                "skipping real web download smoke test because Node.js or Playwright is missing"
            );
            return;
        }

        let url = serve_download_fixture();
        let out = temp_dir("greentic-web-download").join("report.txt");
        let adapter = PlaywrightWebAdapter::new();

        let results = adapter
            .replay(&[
                RunnerStep {
                    id: "open".to_owned(),
                    action: "goto".to_owned(),
                    target: LocatorTarget::default(),
                    value: Some(url),
                    required_capability: "web.goto".to_owned(),
                },
                RunnerStep {
                    id: "download".to_owned(),
                    action: "download".to_owned(),
                    target: target("Download report"),
                    value: Some(out.display().to_string()),
                    required_capability: "web.download_file".to_owned(),
                },
            ])
            .expect("download replay should pass");

        assert!(results.iter().all(|result| result.success));
        assert!(out.exists(), "download should create {}", out.display());
        assert_eq!(
            fs::read_to_string(&out).expect("download contents"),
            "downloaded by greentic\n"
        );
    }

    #[test]
    fn replay_failures_include_screenshot_and_dom_evidence() {
        if find_node_command().is_none() || find_playwright_module_dir().is_none() {
            eprintln!(
                "skipping real web failure evidence smoke test because Node.js or Playwright is missing"
            );
            return;
        }

        let url = serve_web_form_fixture();
        let adapter = PlaywrightWebAdapter::new();
        adapter
            .execute(RunnerStep {
                id: "open".to_owned(),
                action: "goto".to_owned(),
                target: LocatorTarget::default(),
                value: Some(url),
                required_capability: "web.goto".to_owned(),
            })
            .expect("page should open");

        let err = adapter
            .execute(RunnerStep {
                id: "missing".to_owned(),
                action: "click".to_owned(),
                target: target("Missing Button"),
                value: None,
                required_capability: "web.click".to_owned(),
            })
            .expect_err("missing element should fail");
        let message = err.to_string();
        assert!(message.contains("evidence="), "{message}");
        assert!(message.contains(".png"), "{message}");
        assert!(message.contains(".html"), "{message}");
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

    #[test]
    fn web_recording_backend_emits_v1_events() {
        let runtime_home = temp_dir("greentic-web-recorder-home");
        let out = temp_dir("greentic-web-recorder");
        let mut registry = RecordingBackendRegistry::new();
        registry.register(PlaywrightWebRecordingBackend::new(
            PlaywrightRecorderOptions {
                initial_url: "http://127.0.0.1:3000/fixture/calculator".to_owned(),
                sidecar_command: "playwright".to_owned(),
                browser_context: "greentic-owned".to_owned(),
                require_playwright: false,
            },
        ));

        let manifest = start_recording_session_with_registry(
            RecordingStartRequest {
                name: "web.calculator".to_owned(),
                profile: "web".to_owned(),
                adapter: PLAYWRIGHT_ADAPTER_ID.to_owned(),
                target_kind: RecordingTargetKind::Web,
                out: out.clone(),
                runtime_home,
                redact: vec!["password".to_owned()],
                secret_fields: vec!["password".to_owned()],
            },
            &registry,
        )
        .expect("web recording should start");

        assert_eq!(manifest.capture_state.as_str(), "recording");
        assert_eq!(
            manifest.capture_backend.as_deref(),
            Some(PLAYWRIGHT_RECORDER_BACKEND_ID)
        );
        let raw = fs::read_to_string(out.join("raw/events.jsonl")).expect("events");
        assert!(raw.contains("\"schema_version\":\"recording.event.v1\""));
        assert!(raw.contains("\"target_kind\":\"web\""));
        assert!(raw.contains("greentic-owned"));
    }

    #[test]
    fn web_recording_backend_records_real_browser_events_when_playwright_is_available() {
        if find_node_command().is_none() || find_playwright_module_dir().is_none() {
            eprintln!(
                "skipping real web recorder smoke test because Node.js or Playwright is missing"
            );
            return;
        }

        let runtime_home = temp_dir("greentic-web-recorder-real-home");
        let out = temp_dir("greentic-web-recorder-real");
        let fixture = out.join("fixture.html");
        fs::create_dir_all(&out).expect("fixture dir");
        fs::write(
            &fixture,
            r#"<!doctype html>
<html>
  <body>
    <label for="number-one">Number 1</label>
    <input id="number-one" data-testid="number-one" />
    <button data-testid="calculate">Calculate</button>
  </body>
</html>"#,
        )
        .expect("fixture");

        std::env::set_var("GREENTIC_WEB_RECORDER_HEADLESS", "1");
        std::env::set_var("GREENTIC_WEB_RECORDER_SMOKE", "1");
        std::env::set_var("GREENTIC_WEB_RECORDER_AUTO_CLOSE_MS", "1500");

        let mut registry = RecordingBackendRegistry::new();
        registry.register(PlaywrightWebRecordingBackend::new(
            PlaywrightRecorderOptions {
                initial_url: format!("file://{}", fixture.display()),
                sidecar_command: "playwright".to_owned(),
                browser_context: "greentic-owned".to_owned(),
                require_playwright: true,
            },
        ));

        let manifest = start_recording_session_with_registry(
            RecordingStartRequest {
                name: "web.real".to_owned(),
                profile: "web".to_owned(),
                adapter: PLAYWRIGHT_ADAPTER_ID.to_owned(),
                target_kind: RecordingTargetKind::Web,
                out: out.clone(),
                runtime_home,
                redact: vec!["password".to_owned()],
                secret_fields: vec!["password".to_owned()],
            },
            &registry,
        )
        .expect("web recording should start");

        assert_eq!(manifest.capture_state.as_str(), "recording");
        let raw_path = out.join("raw/events.jsonl");
        let mut raw = String::new();
        for _ in 0..180 {
            raw = fs::read_to_string(&raw_path).unwrap_or_default();
            if raw.contains(r#""kind":"input""#) && raw.contains(r#""kind":"click""#) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        assert!(raw.contains(r#""kind":"input""#), "{raw}");
        assert!(raw.contains(r#""kind":"click""#), "{raw}");
        assert!(raw.contains("{{inputs.number_1}}"), "{raw}");
        assert!(out.join("evidence/dom").exists());
    }

    #[test]
    fn web_recording_event_prefers_semantic_locator_and_redacts_password() {
        let event = web_recording_event(
            "rec_test",
            7,
            "input",
            &WebElementMetadata {
                data_testid: Some("password".to_owned()),
                role: Some("textbox".to_owned()),
                name: Some("Password".to_owned()),
                label: Some("Password".to_owned()),
                text: None,
                css: Some("#password".to_owned()),
                xpath: Some("/html/body/input[1]".to_owned()),
                visual_image: None,
            },
            Some("super-secret".to_owned()),
        );

        let json = event.render_json();
        assert!(json.contains("\"role\":\"textbox\""));
        assert!(json.contains("\"test_id\":\"password\""));
        assert!(json.contains("\"redaction\":\"redacted\""));
        assert!(!json.contains("super-secret"));
    }

    #[test]
    fn playwright_sidecar_requests_are_typed_and_correlated() {
        let step = RunnerStep {
            id: "step-1".to_owned(),
            action: "fill".to_owned(),
            target: LocatorTarget::default(),
            value: Some("Ada".to_owned()),
            required_capability: "web.fill".to_owned(),
        };
        let request = PlaywrightSidecarRequest::step("req-42".to_owned(), step);

        let json = serde_json::to_value(&request).expect("request should serialize");

        assert_eq!(json["id"], "req-42");
        assert_eq!(json["type"], "step");
        assert_eq!(json["step"]["required_capability"], "web.fill");
        assert!(json.get("assertion").is_none());

        let response: PlaywrightSidecarResponse =
            serde_json::from_str(r#"{"id":"req-42","ok":true,"result":{"success":true}}"#)
                .expect("response should deserialize");
        assert_eq!(response.id.as_deref(), Some("req-42"));
        assert!(response.ok);
        assert_eq!(response.result["success"], true);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!("{name}-{}-{}", std::process::id(), nanos));
        if root.exists() {
            fs::remove_dir_all(&root).expect("old temp dir should remove");
        }
        root
    }
}
