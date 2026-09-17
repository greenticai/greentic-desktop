//! Runner-step execution against any [`AccessibleBackend`].
//!
//! Both Linux adapters (X11 and Wayland) route their AT-SPI capabilities
//! through [`AccessibilityExecutor`]: element traversal and actions are D-Bus
//! calls, so they work the same on either display server. Only global input
//! (`xdotool`) and window-manager control (`wmctrl`) stay X11-specific.

use super::locator::{
    normalize_label, resolve_target, resolve_target_definite, target_is_resolvable, text_equals,
};
use super::model::{AccessibleBackend, TreeSnapshot, WalkLimits};
use super::operations::{
    editable_target, is_choice_role, is_window_role, labeled_output, read_value,
    window_title_equals, window_title_matches,
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
    /// Process owning the scoped window. When set, only that process's
    /// windows are in scope, whatever other windows share the title.
    pub(super) scope_process: Option<u32>,
}

/// A window the session is scoped to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowScope {
    pub title: String,
    pub process_id: Option<u32>,
}

impl<'a, B: AccessibleBackend> AccessibilityExecutor<'a, B> {
    pub fn new(backend: &'a B, window_title: Option<String>, timing: AccessibilityTiming) -> Self {
        Self {
            backend,
            window_title,
            timing,
            limits: WalkLimits::default(),
            keyboard_input: false,
            scope_process: None,
        }
    }

    /// Process owning `node`, when the backend can resolve it.
    pub fn process_of(&self, node: &B::Handle) -> Option<u32> {
        self.backend.process_id(node)
    }

    /// Restrict the scope to windows owned by `process_id`.
    pub fn with_scope_process(mut self, process_id: Option<u32>) -> Self {
        self.scope_process = process_id;
        self
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
                if !is_window_role(&info.role) || !window_title_matches(&info.name, title) {
                    continue;
                }
                if self.scope_process.is_some()
                    && self.backend.process_id(&child) != self.scope_process
                {
                    continue;
                }
                found.push((child, info.name));
            }
        }
        Ok(found)
    }

    /// Wait for a window matching `title` and return it as a scope.
    ///
    /// Exact title matches win over substring matches. If the winning matches
    /// belong to more than one process the lookup fails rather than guess —
    /// a browser tab titled like the application must not become the scope.
    pub fn wait_for_scope(&self, title: &str, timeout: Duration) -> AdapterResult<WindowScope> {
        let outcome = self.poll(timeout, || {
            let windows = self.windows_matching(title)?;
            if windows.is_empty() {
                return Ok(None);
            }
            let exact = windows
                .iter()
                .filter(|(_, name)| window_title_equals(name, title))
                .collect::<Vec<_>>();
            let chosen = if exact.is_empty() {
                windows.iter().collect::<Vec<_>>()
            } else {
                exact
            };
            let mut processes = chosen
                .iter()
                .map(|(handle, _)| self.backend.process_id(handle))
                .collect::<Vec<_>>();
            processes.sort_unstable();
            processes.dedup();
            if processes.len() > 1 {
                return Ok(Some(Err(chosen
                    .iter()
                    .map(|(_, name)| name.clone())
                    .collect::<Vec<_>>())));
            }
            let (_, name) = chosen[0];
            Ok(Some(Ok(WindowScope {
                title: name.clone(),
                process_id: processes.first().copied().flatten(),
            })))
        })?;
        match outcome {
            Some(Ok(scope)) => Ok(scope),
            Some(Err(names)) => Err(AdapterError::ExecutionFailed(format!(
                "Window title {title:?} matches windows of more than one process ({names:?}); use a title that identifies one application."
            ))),
            None => Err(AdapterError::ExecutionFailed(format!(
                "No accessible window titled {title:?} appeared within {}ms. Check that the application is running and exposes AT-SPI (toolkit accessibility enabled, NO_AT_BRIDGE unset).",
                timeout.as_millis()
            ))),
        }
    }

    /// Wait for the window of a process this adapter just spawned.
    ///
    /// A window owned by `process_id` wins (and must satisfy `title` when one
    /// is given). A window from another process is accepted only when its name
    /// equals `title` exactly — launchers such as `flatpak run` hand off to a
    /// child with a different pid — never on a substring, so an unrelated
    /// "Meridian docs – Firefox" cannot stand in for the application.
    /// `exited` is polled so a process that dies at start-up is reported as
    /// such instead of as a missing accessibility tree.
    pub fn wait_for_launched_window(
        &self,
        title: Option<&str>,
        process_id: u32,
        timeout: Duration,
        mut exited: impl FnMut() -> Option<String>,
    ) -> AdapterResult<WindowScope> {
        let outcome = self.poll(timeout, || {
            if let Some(status) = exited() {
                return Ok(Some(Err(status)));
            }
            let mut exact_elsewhere = None;
            for application in self.backend.applications()? {
                let owned = self.backend.process_id(&application) == Some(process_id);
                for child in self.backend.children(&application).unwrap_or_default() {
                    let Ok(info) = self.backend.describe(&child) else {
                        continue;
                    };
                    if !is_window_role(&info.role) || info.name.trim().is_empty() {
                        continue;
                    }
                    if owned && title.is_none_or(|title| window_title_matches(&info.name, title)) {
                        return Ok(Some(Ok(WindowScope {
                            title: info.name,
                            process_id: Some(process_id),
                        })));
                    }
                    if !owned
                        && exact_elsewhere.is_none()
                        && title.is_some_and(|title| window_title_equals(&info.name, title))
                    {
                        exact_elsewhere = Some(WindowScope {
                            process_id: self.backend.process_id(&child),
                            title: info.name,
                        });
                    }
                }
            }
            Ok(exact_elsewhere.map(Ok))
        })?;
        match outcome {
            Some(Ok(scope)) => Ok(scope),
            Some(Err(status)) => Err(AdapterError::ExecutionFailed(format!(
                "Launched process {process_id} exited before exposing a window ({status})."
            ))),
            None => Err(AdapterError::ExecutionFailed(format!(
                "Process {process_id} exposed no accessible window{} within {}ms. Check that it registers with AT-SPI (NO_AT_BRIDGE unset, toolkit accessibility enabled).",
                title.map(|title| format!(" titled {title:?}")).unwrap_or_default(),
                timeout.as_millis()
            ))),
        }
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

    /// One locate attempt: `Ok(None)` when nothing matches yet.
    fn try_locate(
        &self,
        target: &LocatorTarget,
        definite_only: bool,
    ) -> AdapterResult<Option<(TreeSnapshot<B::Handle>, usize)>> {
        for snapshot in self.capture_scope()? {
            let found = if definite_only {
                resolve_target_definite(&snapshot, target)
            } else {
                resolve_target(&snapshot, target)
            };
            if let Some(index) = found {
                return Ok(Some((snapshot, index)));
            }
        }
        Ok(None)
    }

    fn effective_or_error(
        target: &LocatorTarget,
        fallback_text: Option<&str>,
    ) -> AdapterResult<LocatorTarget> {
        effective_target(target, fallback_text).ok_or_else(|| {
            AdapterError::ExecutionFailed(
                "AT-SPI locator requires a role, name, label, text or automation_id (or a step value to search for).".to_owned(),
            )
        })
    }

    fn not_found(&self, target: &LocatorTarget, timeout: Duration) -> AdapterError {
        AdapterError::ExecutionFailed(format!(
            "No AT-SPI element matched {} within {}ms{}.",
            describe_target(target),
            timeout.as_millis(),
            self.window_title
                .as_deref()
                .map(|title| format!(" in window {title:?}"))
                .unwrap_or_default()
        ))
    }

    /// Locate `target` (or, when it carries no locator fields, an element
    /// whose text is `fallback_text`), waiting up to `timeout`.
    fn locate(
        &self,
        target: &LocatorTarget,
        fallback_text: Option<&str>,
        timeout: Duration,
    ) -> AdapterResult<(TreeSnapshot<B::Handle>, usize)> {
        let effective = Self::effective_or_error(target, fallback_text)?;
        self.poll(timeout, || self.try_locate(&effective, false))?
            .ok_or_else(|| self.not_found(&effective, timeout))
    }

    /// Locate the element an action will act on. Until `timeout` only
    /// definite matches count; a substring-only match is accepted once, at
    /// the end, so a page that is still rendering cannot divert the action.
    fn locate_for_action(
        &self,
        target: &LocatorTarget,
    ) -> AdapterResult<(TreeSnapshot<B::Handle>, usize)> {
        let effective = Self::effective_or_error(target, None)?;
        let timeout = self.timing.action_timeout;
        if let Some(found) = self.poll(timeout, || self.try_locate(&effective, true))? {
            return Ok(found);
        }
        self.try_locate(&effective, false)?
            .ok_or_else(|| self.not_found(&effective, timeout))
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
        let (snapshot, index) = self.locate_for_action(&step.target)?;
        let node = snapshot.node(index);
        let action = self.backend.perform_action(
            &node.handle,
            &[
                "click", "press", "activate", "jump", "check", "toggle", "select",
            ],
        )?;
        Ok(format!(
            "invoked AT-SPI action {action:?} on {} {:?}",
            node.info.role,
            node.info.readable_text()
        ))
    }

    pub fn type_text(&self, step: &RunnerStep) -> AdapterResult<String> {
        let value = step.value.as_deref().unwrap_or_default();
        let (snapshot, located) = self.locate_for_action(&step.target)?;
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
            self.ensure_keyboard_focus(&node.handle)?;
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
            Ok(text_equals(&text, value).then_some(text))
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

    /// Synthesized keystrokes go to whichever window holds X input focus, so
    /// refuse to type unless the target control is focused and the scoped
    /// window is the active one.
    fn ensure_keyboard_focus(&self, node: &B::Handle) -> AdapterResult<()> {
        self.backend.grab_focus(node)?;
        let focused = self.poll(self.timing.action_timeout, || {
            Ok(self.backend.describe(node)?.states.focused.then_some(()))
        })?;
        if focused.is_none() {
            return Err(AdapterError::ExecutionFailed(
                "AT-SPI could not focus the target control, so keystrokes were not sent."
                    .to_owned(),
            ));
        }
        if let Some(title) = &self.window_title {
            let active = self.windows_matching(title)?.iter().any(|(window, _)| {
                self.backend
                    .describe(window)
                    .is_ok_and(|info| info.states.active)
            });
            if !active {
                return Err(AdapterError::ExecutionFailed(format!(
                    "Window {title:?} is not the active window, so synthesized keystrokes would reach another application; they were not sent."
                )));
            }
        }
        Ok(())
    }

    pub fn read_text(&self, step: &RunnerStep) -> AdapterResult<String> {
        let timeout = self.timing.find_timeout;
        let effective = Self::effective_or_error(&step.target, None)?;
        let mut located = false;
        let text = self.poll(timeout, || {
            let Some((snapshot, index)) = self.try_locate(&effective, false)? else {
                return Ok(None);
            };
            located = true;
            let value = read_value(&snapshot, index);
            Ok((!value.trim().is_empty()).then_some(value))
        })?;
        let text = text.ok_or_else(|| {
            if located {
                AdapterError::ExecutionFailed(format!(
                    "AT-SPI element {} had no readable text within {}ms.",
                    describe_target(&effective),
                    timeout.as_millis()
                ))
            } else {
                self.not_found(&effective, timeout)
            }
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
                lines.push(format!(
                    "… {} nodes skipped: {:?}",
                    snapshot.skipped, snapshot.skip_reasons
                ));
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
