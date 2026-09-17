//! Per-adapter AT-SPI state: the lazily opened bus connection and the window
//! the runner is currently scoped to.

use crate::accessibility::locator::target_is_resolvable;
use crate::accessibility::{AccessibilityExecutor, AccessibilityTiming, AccessibleBackend};
use crate::launch::{resolve_launch_command, spawn_detached};
use greentic_desktop_adapter::{AdapterError, AdapterResult, LocatorTarget, RunnerStep};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Capabilities the AT-SPI session can execute on either display server.
pub const ATSPI_CAPABILITIES: &[&str] = &[
    "linux.open_app",
    "linux.find_window",
    "linux.read_window_tree",
    "linux.find_element",
    "linux.click_element",
    "linux.type_text",
    "linux.read_text",
    "linux.assert_visible",
];

type Connector<B> = dyn Fn() -> AdapterResult<B> + Send + Sync;

pub struct AccessibilitySession<B: AccessibleBackend> {
    connector: Box<Connector<B>>,
    backend: Mutex<Option<Arc<B>>>,
    window_title: Mutex<Option<String>>,
    keyboard_input: bool,
    timing: AccessibilityTiming,
    launch_timeout: Duration,
}

impl<B: AccessibleBackend> std::fmt::Debug for AccessibilitySession<B> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccessibilitySession")
            .field("window_title", &self.window_title())
            .field("keyboard_input", &self.keyboard_input)
            .finish()
    }
}

impl<B: AccessibleBackend> AccessibilitySession<B> {
    pub fn new(
        connector: impl Fn() -> AdapterResult<B> + Send + Sync + 'static,
        keyboard_input: bool,
        timing: AccessibilityTiming,
    ) -> Self {
        let launch_timeout = std::env::var("GREENTIC_LINUX_ATSPI_LAUNCH_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(30));
        Self {
            connector: Box::new(connector),
            backend: Mutex::new(None),
            window_title: Mutex::new(None),
            keyboard_input,
            timing,
            launch_timeout,
        }
    }

    pub fn window_title(&self) -> Option<String> {
        self.window_title
            .lock()
            .map(|title| title.clone())
            .unwrap_or_default()
    }

    fn set_window_title(&self, title: String) {
        if let Ok(mut current) = self.window_title.lock() {
            *current = Some(title);
        }
    }

    fn backend(&self) -> AdapterResult<Arc<B>> {
        let mut slot = self.backend.lock().map_err(|_| {
            AdapterError::ExecutionFailed("AT-SPI session lock poisoned".to_owned())
        })?;
        if let Some(backend) = slot.as_ref() {
            return Ok(Arc::clone(backend));
        }
        let backend = Arc::new((self.connector)()?);
        *slot = Some(Arc::clone(&backend));
        Ok(backend)
    }

    fn with_executor<T>(
        &self,
        run: impl FnOnce(&AccessibilityExecutor<'_, B>) -> AdapterResult<T>,
    ) -> AdapterResult<T> {
        let backend = self.backend()?;
        let executor =
            AccessibilityExecutor::new(backend.as_ref(), self.window_title(), self.timing)
                .with_keyboard_input(self.keyboard_input);
        let result = run(&executor);
        if result.is_err() {
            // A dropped bus connection must not poison every later step.
            if let Ok(mut slot) = self.backend.lock() {
                *slot = None;
            }
        }
        result
    }

    /// True when this session should execute `step` rather than a
    /// display-server specific path (region clicks, focus typing).
    pub fn handles(step: &RunnerStep) -> bool {
        match step.required_capability.as_str() {
            "linux.click_element" | "linux.type_text" => target_is_resolvable(&step.target),
            "linux.read_text" => target_is_resolvable(&step.target),
            "linux.wayland.accessibility_tree" | "linux.wayland.assert_visible" => true,
            capability => ATSPI_CAPABILITIES.contains(&capability),
        }
    }

    pub fn execute(&self, step: &RunnerStep) -> AdapterResult<String> {
        match step.required_capability.as_str() {
            "linux.open_app" => self.open_app(step),
            "linux.find_window" => {
                let title = window_title_for(step)?;
                let found = self.with_executor(|executor| {
                    executor.wait_for_window(&title, None, executor_find_timeout(self))
                })?;
                self.set_window_title(found.clone());
                Ok(format!("found accessible window {found:?}"))
            }
            "linux.read_window_tree" | "linux.wayland.accessibility_tree" => {
                self.with_executor(|executor| executor.dump_tree())
            }
            "linux.find_element" | "linux.assert_visible" | "linux.wayland.assert_visible" => {
                self.with_executor(|executor| executor.find_element(step))
            }
            "linux.click_element" => {
                self.require_window_scope(step)?;
                self.with_executor(|executor| executor.click(step))
            }
            "linux.type_text" => {
                self.require_window_scope(step)?;
                self.with_executor(|executor| executor.type_text(step))
            }
            "linux.read_text" => self.with_executor(|executor| executor.read_text(step)),
            other => Err(AdapterError::UnsupportedCapability(other.to_owned())),
        }
    }

    /// Clicks and typing act on whatever matches, so they only run once
    /// `linux.open_app` or `linux.find_window` has scoped the session to one
    /// window. Without that, a locator such as `name: Close` could match a
    /// control in any application on the user's desktop.
    fn require_window_scope(&self, step: &RunnerStep) -> AdapterResult<()> {
        if self.window_title().is_some() {
            return Ok(());
        }
        Err(AdapterError::ExecutionFailed(format!(
            "{} requires a window scope: run linux.open_app or linux.find_window first so AT-SPI actions cannot reach other applications.",
            step.required_capability
        )))
    }

    pub fn visible_texts(&self) -> AdapterResult<Vec<String>> {
        self.with_executor(|executor| executor.visible_texts())
    }

    pub fn is_visible(&self, target: &LocatorTarget, expected: &str) -> AdapterResult<bool> {
        self.with_executor(|executor| executor.is_visible(target, expected))
    }

    fn open_app(&self, step: &RunnerStep) -> AdapterResult<String> {
        let title = crate::accessibility::locator::target_is_resolvable(&step.target)
            .then(|| target_title(&step.target))
            .flatten();
        if let Some(title) = &title {
            let running = self.with_executor(|executor| executor.windows_matching(title))?;
            if let Some((_, name)) = running.into_iter().next() {
                self.set_window_title(name.clone());
                return Ok(format!("application window {name:?} is already open"));
            }
        }
        let value = step.value.as_deref().unwrap_or_default();
        let command = resolve_launch_command(value)?;
        let process_id = spawn_detached(&command)?;
        let found = self.with_executor(|executor| match &title {
            Some(title) => executor.wait_for_window(title, Some(process_id), self.launch_timeout),
            None => executor.wait_for_process_window(process_id, self.launch_timeout),
        })?;
        self.set_window_title(found.clone());
        Ok(format!(
            "launched {} (pid {process_id}); accessible window {found:?} is open",
            command.program
        ))
    }
}

fn executor_find_timeout<B: AccessibleBackend>(session: &AccessibilitySession<B>) -> Duration {
    session.timing.find_timeout
}

fn target_title(target: &LocatorTarget) -> Option<String> {
    [target.preferred.as_ref(), target.fallback.as_ref()]
        .into_iter()
        .flatten()
        .find_map(|strategy| strategy.name.clone().or_else(|| strategy.text.clone()))
        .filter(|title| !title.trim().is_empty())
}

fn window_title_for(step: &RunnerStep) -> AdapterResult<String> {
    step.value
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| target_title(&step.target))
        .ok_or_else(|| {
            AdapterError::ExecutionFailed(
                "linux.find_window requires a window title in step.value or target.name."
                    .to_owned(),
            )
        })
}
