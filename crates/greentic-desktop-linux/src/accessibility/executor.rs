//! Runner-step execution against any [`AccessibleBackend`].
//!
//! Both Linux adapters (X11 and Wayland) route their AT-SPI capabilities
//! through [`AccessibilityExecutor`]: element traversal and actions are D-Bus
//! calls, so they work the same on either display server. Only global input
//! (`xdotool`) and window-manager control (`wmctrl`) stay X11-specific.

use super::locator::{normalize_label, resolve_target, target_is_resolvable, values_match};
use super::model::{AccessibleBackend, TreeSnapshot, WalkLimits};
use super::operations::{
    editable_target, is_choice_role, is_window_role, labeled_output, read_value,
    window_title_matches,
};
use greentic_desktop_adapter::{
    AdapterError, AdapterResult, LocatorStrategy, LocatorTarget, RunnerStep,
};
use std::time::{Duration, Instant};

/// How long locators wait for the UI to settle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibilityTiming {
    /// `find_element`, `assert_visible`, `read_text` and window lookups.
    pub find_timeout: Duration,
    /// Locating the element a click or type acts on, and verifying the result.
    pub action_timeout: Duration,
    pub poll_interval: Duration,
}

impl Default for AccessibilityTiming {
    fn default() -> Self {
        Self {
            find_timeout: Duration::from_secs(15),
            action_timeout: Duration::from_secs(5),
            poll_interval: Duration::from_millis(200),
        }
    }
}

impl AccessibilityTiming {
    /// Defaults, overridable with `GREENTIC_LINUX_ATSPI_FIND_TIMEOUT_MS` and
    /// `GREENTIC_LINUX_ATSPI_ACTION_TIMEOUT_MS`.
    pub fn from_env() -> Self {
        let millis = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(Duration::from_millis)
        };
        let defaults = Self::default();
        Self {
            find_timeout: millis("GREENTIC_LINUX_ATSPI_FIND_TIMEOUT_MS")
                .unwrap_or(defaults.find_timeout),
            action_timeout: millis("GREENTIC_LINUX_ATSPI_ACTION_TIMEOUT_MS")
                .unwrap_or(defaults.action_timeout),
            poll_interval: defaults.poll_interval,
        }
    }

    /// No waiting at all; used by deterministic tests.
    pub fn immediate() -> Self {
        Self {
            find_timeout: Duration::ZERO,
            action_timeout: Duration::ZERO,
            poll_interval: Duration::ZERO,
        }
    }
}

/// Executes AT-SPI runner steps scoped to one window.
pub struct AccessibilityExecutor<'a, B: AccessibleBackend> {
    pub(super) backend: &'a B,
    pub(super) window_title: Option<String>,
    pub(super) timing: AccessibilityTiming,
    pub(super) limits: WalkLimits,
    pub(super) keyboard_input: bool,
}

impl<'a, B: AccessibleBackend> AccessibilityExecutor<'a, B> {
    pub fn new(backend: &'a B, window_title: Option<String>, timing: AccessibilityTiming) -> Self {
        Self {
            backend,
            window_title,
            timing,
            limits: WalkLimits::default(),
            keyboard_input: false,
        }
    }

    /// Allow typing through AT-SPI keyboard synthesis when a control has no
    /// EditableText interface. Synthesis goes through XTest, so only X11
    /// sessions should enable it.
    pub fn with_keyboard_input(mut self, enabled: bool) -> Self {
        self.keyboard_input = enabled;
        self
    }

    /// Top-level windows whose name satisfies `title`, across all applications.
    pub fn windows_matching(&self, title: &str) -> AdapterResult<Vec<(B::Handle, String)>> {
        let mut found = Vec::new();
        for application in self.backend.applications()? {
            let Ok(children) = self.backend.children(&application) else {
                continue;
            };
            for child in children {
                let Ok(info) = self.backend.describe(&child) else {
                    continue;
                };
                if is_window_role(&info.role) && window_title_matches(&info.name, title) {
                    found.push((child, info.name));
                }
            }
        }
        Ok(found)
    }

    /// Wait until a window matching `title` exists. When `process_id` is set a
    /// window owned by that process is preferred.
    pub fn wait_for_window(
        &self,
        title: &str,
        process_id: Option<u32>,
        timeout: Duration,
    ) -> AdapterResult<String> {
        self.poll(timeout, || {
            let windows = self.windows_matching(title)?;
            let preferred = windows.iter().find(|(handle, _)| {
                process_id.is_some() && self.backend.process_id(handle) == process_id
            });
            Ok(preferred.or(windows.first()).map(|(_, name)| name.clone()))
        })?
        .ok_or_else(|| {
            AdapterError::ExecutionFailed(format!(
                "No accessible window titled {title:?} appeared within {}ms. Check that the application is running and exposes AT-SPI (toolkit accessibility enabled, NO_AT_BRIDGE unset).",
                timeout.as_millis()
            ))
        })
    }

    /// Capture every window in scope. With no scope, every application.
    pub fn capture_scope(&self) -> AdapterResult<Vec<TreeSnapshot<B::Handle>>> {
        let roots = match &self.window_title {
            Some(title) => {
                let windows = self.windows_matching(title)?;
                if windows.is_empty() {
                    return Err(AdapterError::ExecutionFailed(format!(
                        "No accessible window titled {title:?} is open."
                    )));
                }
                windows.into_iter().map(|(handle, _)| handle).collect()
            }
            None => self.backend.applications()?,
        };
        roots
            .iter()
            .map(|root| TreeSnapshot::capture(self.backend, root, self.limits))
            .collect()
    }

    pub(super) fn poll<T>(
        &self,
        timeout: Duration,
        mut attempt: impl FnMut() -> AdapterResult<Option<T>>,
    ) -> AdapterResult<Option<T>> {
        let deadline = Instant::now() + timeout;
        loop {
            let last_error = match attempt() {
                Ok(Some(value)) => return Ok(Some(value)),
                Ok(None) => None,
                Err(error) => Some(error),
            };
            if Instant::now() >= deadline {
                return match last_error {
                    Some(error) => Err(error),
                    None => Ok(None),
                };
            }
            std::thread::sleep(self.timing.poll_interval.max(Duration::from_millis(1)));
        }
    }

    /// Locate `target` (or, when it carries no locator fields, an element
    /// whose text is `fallback_text`), waiting up to `timeout`.
    fn locate(
        &self,
        target: &LocatorTarget,
        fallback_text: Option<&str>,
        timeout: Duration,
    ) -> AdapterResult<(TreeSnapshot<B::Handle>, usize)> {
        let effective = effective_target(target, fallback_text).ok_or_else(|| {
            AdapterError::ExecutionFailed(
                "AT-SPI locator requires a role, name, label, text or automation_id (or a step value to search for).".to_owned(),
            )
        })?;
        self.poll(timeout, || {
            for snapshot in self.capture_scope()? {
                if let Some(index) = resolve_target(&snapshot, &effective) {
                    return Ok(Some((snapshot, index)));
                }
            }
            Ok(None)
        })?
        .ok_or_else(|| {
            AdapterError::ExecutionFailed(format!(
                "No AT-SPI element matched {} within {}ms{}.",
                describe_target(&effective),
                timeout.as_millis(),
                self.window_title
                    .as_deref()
                    .map(|title| format!(" in window {title:?}"))
                    .unwrap_or_default()
            ))
        })
    }

    pub fn find_element(&self, step: &RunnerStep) -> AdapterResult<String> {
        let (snapshot, index) = self.locate(
            &step.target,
            step.value.as_deref(),
            self.timing.find_timeout,
        )?;
        let node = &snapshot.node(index).info;
        Ok(format!(
            "found AT-SPI {} {:?}",
            node.role,
            node.readable_text()
        ))
    }

    pub fn click(&self, step: &RunnerStep) -> AdapterResult<String> {
        let (snapshot, index) = self.locate(&step.target, None, self.timing.action_timeout)?;
        let node = snapshot.node(index);
        let action = self
            .backend
            .perform_action(&node.handle, &["click", "press", "activate", "jump"])?;
        Ok(format!(
            "invoked AT-SPI action {action:?} on {} {:?}",
            node.info.role,
            node.info.readable_text()
        ))
    }

    pub fn type_text(&self, step: &RunnerStep) -> AdapterResult<String> {
        let value = step.value.as_deref().unwrap_or_default();
        let (snapshot, located) = self.locate(&step.target, None, self.timing.action_timeout)?;
        let index = editable_target(&snapshot, located).ok_or_else(|| {
            AdapterError::ExecutionFailed(format!(
                "AT-SPI element {:?} is not editable and has no editable neighbour.",
                snapshot.node(located).info.readable_text()
            ))
        })?;
        if is_choice_role(&snapshot.node(index).info.role) {
            return self.select_option(snapshot, index, value);
        }
        let node = snapshot.node(index);
        let method = if node.info.interfaces.editable_text {
            let _ = self.backend.grab_focus(&node.handle);
            self.backend.set_text_contents(&node.handle, value)?;
            "EditableText"
        } else if node.info.interfaces.text && self.keyboard_input {
            // WebKitGTK entries implement Text but not EditableText: focus the
            // field, select its contents and synthesize the string.
            self.backend.replace_text_by_keyboard(&node.handle, value)?;
            "keyboard synthesis"
        } else {
            return Err(AdapterError::ExecutionFailed(format!(
                "AT-SPI {} {:?} does not implement EditableText{}, so its contents cannot be set safely.",
                node.info.role,
                node.info.readable_text(),
                if node.info.interfaces.text {
                    " and keyboard synthesis is unavailable in this session (Wayland blocks global input)"
                } else {
                    ""
                }
            )));
        };
        let handle = node.handle.clone();
        let observed = self.poll(self.timing.action_timeout, || {
            let fresh = self.backend.describe(&handle)?;
            let text = fresh.text.unwrap_or_default();
            Ok(values_match(&text, value).then_some(text))
        })?;
        match observed {
            Some(_) => Ok(format!(
                "set AT-SPI text of {} to {value:?} via {method}",
                node.info.role
            )),
            None => Err(AdapterError::ExecutionFailed(format!(
                "AT-SPI text verification failed after {method}: expected {value:?}, observed {:?}.",
                self.backend
                    .describe(&handle)
                    .ok()
                    .and_then(|info| info.text)
                    .unwrap_or_default()
            ))),
        }
    }

    pub fn read_text(&self, step: &RunnerStep) -> AdapterResult<String> {
        let timeout = self.timing.find_timeout;
        let text = self.poll(timeout, || {
            let (snapshot, index) = self.locate(&step.target, None, Duration::ZERO)?;
            let value = read_value(&snapshot, index);
            Ok((!value.trim().is_empty()).then_some(value))
        })?;
        let text = text.ok_or_else(|| {
            AdapterError::ExecutionFailed(format!(
                "AT-SPI element {} had no readable text within {}ms.",
                describe_target(&step.target),
                timeout.as_millis()
            ))
        })?;
        Ok(labeled_output(step)
            .map(|label| format!("{label}: {text}"))
            .unwrap_or(text))
    }

    /// Visible texts in scope, for observations and tree reads.
    pub fn visible_texts(&self) -> AdapterResult<Vec<String>> {
        Ok(self
            .capture_scope()?
            .iter()
            .flat_map(TreeSnapshot::visible_texts)
            .collect())
    }

    /// True when `expected` is visible as an element (or the target resolves).
    pub fn is_visible(&self, target: &LocatorTarget, expected: &str) -> AdapterResult<bool> {
        let fallback = (!expected.trim().is_empty()).then_some(expected);
        match self.locate(target, fallback, self.timing.find_timeout) {
            Ok(_) => Ok(true),
            Err(AdapterError::ExecutionFailed(message))
                if message.starts_with("No AT-SPI element") =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Renders every node in scope as an indented `role "name"` line.
    pub fn dump_tree(&self) -> AdapterResult<String> {
        let mut lines = Vec::new();
        for snapshot in self.capture_scope()? {
            for node in &snapshot.nodes {
                let id = node
                    .info
                    .identifier()
                    .map(|id| format!(" #{id}"))
                    .unwrap_or_default();
                let text = node
                    .info
                    .text
                    .as_deref()
                    .filter(|text| normalize_label(text) != normalize_label(&node.info.name))
                    .map(|text| format!(" text={:?}", text.replace('\u{fffc}', "")))
                    .unwrap_or_default();
                lines.push(format!(
                    "{}{} {:?}{id}{text}{}",
                    "  ".repeat(node.depth),
                    node.info.role,
                    node.info.name,
                    if node.info.states.showing {
                        ""
                    } else {
                        " (not showing)"
                    }
                ));
            }
            if snapshot.skipped > 0 {
                lines.push(format!("… {} nodes skipped", snapshot.skipped));
            }
        }
        Ok(lines.join("\n"))
    }
}

/// When a target has no locator fields but the step carries a value, search
/// for an element whose text is that value.
fn effective_target(target: &LocatorTarget, fallback_text: Option<&str>) -> Option<LocatorTarget> {
    if target_is_resolvable(target) {
        return Some(target.clone());
    }
    let text = fallback_text
        .map(str::trim)
        .filter(|text| !text.is_empty())?;
    Some(LocatorTarget {
        preferred: Some(LocatorStrategy {
            name: Some(text.to_owned()),
            ..LocatorStrategy::default()
        }),
        ..LocatorTarget::default()
    })
}

fn describe_target(target: &LocatorTarget) -> String {
    let describe = |strategy: &LocatorStrategy| {
        [
            ("role", &strategy.role),
            ("name", &strategy.name),
            ("label", &strategy.label),
            ("text", &strategy.text),
            ("automation_id", &strategy.automation_id),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.as_deref().map(|value| format!("{key}={value:?}")))
        .collect::<Vec<_>>()
        .join(" ")
    };
    [target.preferred.as_ref(), target.fallback.as_ref()]
        .into_iter()
        .flatten()
        .map(describe)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" | fallback ")
}
