use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    LocatorStrategy, LocatorTarget, Observation, ObserveContext, RecordedEvent, RunnerStep,
    StepResult, VisualLocator,
};
use greentic_desktop_recorder::{
    RecordingBackend, RecordingCaptureState, RecordingEventEnvelope, RecordingEventSink,
    RecordingHandle, RecordingPreflight, RecordingStartRequest, RecordingTargetKind,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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
    if (await input.count()) await input.fill('41');
    const button = page.locator('button,input[type="button"],input[type="submit"]').first();
    if (await button.count()) await button.click();
    await page.waitForTimeout(250);
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
            "web.click" | "web.press" => {}
            "web.screenshot" => state.visible_text.push("screenshot captured".to_owned()),
            "web.download_file" => state.visible_text.push("download completed".to_owned()),
            "web.wait_for" | "web.wait_for_text" | "web.extract_text" | "web.extract_regex"
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
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn can_open_fill_submit_and_extract_seeded_output() {
        let adapter = PlaywrightWebAdapter::new();
        adapter.insert_visible_text("Record saved");
        adapter.insert_identifier("confirmation_id", "REC-1001");
        let steps = vec![
            RunnerStep {
                id: "open".to_owned(),
                action: "goto".to_owned(),
                target: LocatorTarget::default(),
                value: Some("https://example.test/records/new".to_owned()),
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
                expected: "Record saved".to_owned(),
            })
            .expect("visible assertion should run");
        assert!(visible.passed);

        let id = adapter
            .validate(Assertion {
                id: "confirmation_id".to_owned(),
                required_capability: "web.extract_text".to_owned(),
                target: target("Confirmation ID"),
                expected: "confirmation_id".to_owned(),
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
        std::env::set_var("GREENTIC_WEB_RECORDER_AUTO_CLOSE_MS", "100");

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
        for _ in 0..30 {
            raw = fs::read_to_string(&raw_path).unwrap_or_default();
            if raw.contains(r#""kind":"input""#) && raw.contains(r#""kind":"click""#) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
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
