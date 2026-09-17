//! Per-adapter AT-SPI state: the lazily opened bus connection and the window
//! the runner is currently scoped to.

use crate::accessibility::executor::WindowScope;
use crate::accessibility::locator::target_is_resolvable;
use crate::accessibility::operations::window_title_equals;
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
    scope: Mutex<Option<WindowScope>>,
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
            scope: Mutex::new(None),
            keyboard_input,
            timing,
            launch_timeout,
        }
    }

    pub fn window_title(&self) -> Option<String> {
        self.scope().map(|scope| scope.title)
    }

    fn scope(&self) -> Option<WindowScope> {
        self.scope
            .lock()
            .map(|scope| scope.clone())
            .unwrap_or_default()
    }

    fn set_scope(&self, scope: WindowScope) {
        if let Ok(mut current) = self.scope.lock() {
            *current = Some(scope);
        }
    }

    /// Record a window found without AT-SPI (the X11 `wmctrl` fallback), so a
    /// later step reports the real problem — no accessibility bus — rather
    /// than asking for a window scope that was just established.
    pub fn set_window_title_without_process(&self, title: String) {
        self.set_scope(WindowScope {
            title,
            process_id: None,
        });
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
        let scope = self.scope();
        let executor = AccessibilityExecutor::new(
            backend.as_ref(),
            scope.as_ref().map(|scope| scope.title.clone()),
            self.timing,
        )
        .with_keyboard_input(self.keyboard_input)
        .with_scope_process(scope.and_then(|scope| scope.process_id));
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
            "linux.wayland.accessibility_tree" | "linux.wayland.assert_visible" => true,
            capability => ATSPI_CAPABILITIES.contains(&capability),
        }
    }

    pub fn execute(&self, step: &RunnerStep) -> AdapterResult<String> {
        let started = std::time::Instant::now();
        let result = self.execute_step(step);
        if trace_enabled() {
            eprintln!(
                "[greentic-linux-atspi] {} {} {}ms -> {:?}",
                step.id,
                step.required_capability,
                started.elapsed().as_millis(),
                result
            );
        }
        result
    }

    fn execute_step(&self, step: &RunnerStep) -> AdapterResult<String> {
        match step.required_capability.as_str() {
            "linux.open_app" => self.open_app(step),
            "linux.find_window" => {
                let title = window_title_for(step)?;
                // A new lookup replaces the scope, including its process.
                if let Ok(mut current) = self.scope.lock() {
                    *current = None;
                }
                let found = self.with_executor(|executor| {
                    executor.wait_for_scope(&title, executor_find_timeout(self))
                })?;
                let message = format!("found accessible window {:?}", found.title);
                self.set_scope(found);
                Ok(message)
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
            "linux.read_text" if target_is_resolvable(&step.target) => {
                self.with_executor(|executor| executor.read_text(step))
            }
            "linux.read_text" => self
                .with_executor(|executor| executor.visible_texts())
                .map(|texts| texts.join("\n")),
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
        let title = target_title(&step.target);
        if let Some(title) = &title {
            if let Ok(mut current) = self.scope.lock() {
                *current = None;
            }
            // Reuse only a window whose title is exactly the requested one,
            // owned by a single process; a substring such as a browser tab
            // titled after the app must not stand in for it.
            let running = self.with_executor(|executor| {
                let windows = executor.windows_matching(title)?;
                let exact = windows
                    .into_iter()
                    .filter(|(_, name)| window_title_equals(name, title))
                    .map(|(window, name)| (executor_process(executor, &window), name))
                    .collect::<Vec<_>>();
                Ok(exact)
            })?;
            let mut processes = running
                .iter()
                .map(|(process, _)| *process)
                .collect::<Vec<_>>();
            processes.sort_unstable();
            processes.dedup();
            if let ([process], Some((_, name))) = (processes.as_slice(), running.first()) {
                let name = name.clone();
                self.set_scope(WindowScope {
                    title: name.clone(),
                    process_id: *process,
                });
                return Ok(format!("application window {name:?} is already open"));
            }
        }
        let value = step.value.as_deref().unwrap_or_default();
        let command = resolve_launch_command(value)?;
        let mut child = spawn_detached(&command)?;
        let process_id = child.id();
        let found = self.with_executor(|executor| {
            executor.wait_for_launched_window(
                title.as_deref(),
                process_id,
                self.launch_timeout,
                || match child.try_wait() {
                    Ok(Some(status)) => Some(status.to_string()),
                    Ok(None) => None,
                    Err(error) => Some(format!("could not query the process: {error}")),
                },
            )
        })?;
        let message = format!(
            "launched {} (pid {process_id}); accessible window {:?} is open",
            command.program, found.title
        );
        self.set_scope(found);
        Ok(message)
    }
}

fn executor_process<B: AccessibleBackend>(
    executor: &AccessibilityExecutor<'_, B>,
    window: &B::Handle,
) -> Option<u32> {
    executor.process_of(window)
}

/// `GREENTIC_LINUX_ATSPI_TRACE=1` prints every AT-SPI step and its result to
/// stderr, which is the fastest way to see where a live replay diverged.
fn trace_enabled() -> bool {
    std::env::var("GREENTIC_LINUX_ATSPI_TRACE").is_ok_and(|value| value.trim() == "1")
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
