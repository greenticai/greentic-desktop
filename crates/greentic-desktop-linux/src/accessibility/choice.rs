//! Selecting a value in a choice control (`<select>`, GtkComboBox, …).
//!
//! Two shapes exist in practice and both are handled:
//!
//! * **Inline options** — the combo box exposes its options as descendants
//!   (GTK3 GtkComboBox menus). Selection goes through the Selection interface
//!   of the options' parent, falling back to the option's own action.
//! * **Popup options** — the combo box exposes no children and its options
//!   only exist while a popup window is open. WebKitGTK renders `<select>`
//!   this way: invoking the combo's `select` action opens a GTK window whose
//!   tree table holds one `table cell` per option. The option is activated,
//!   then the popup is reopened to read which option is SELECTED — the combo
//!   box itself exposes no value — and closed again by activating that option.

use super::executor::AccessibilityExecutor;
use super::locator::{role_matches, values_match};
use super::model::{AccessibleBackend, TreeSnapshot};
use super::operations::{
    choice_options, choice_value, choose_among, choose_option, is_window_role, window_title_matches,
};
use greentic_desktop_adapter::{AdapterError, AdapterResult};

const OPEN_ACTIONS: &[&str] = &["select", "press", "open", "click", "activate"];
const OPTION_ACTIONS: &[&str] = &["activate", "click", "press", "select"];

impl<B: AccessibleBackend> AccessibilityExecutor<'_, B> {
    pub(super) fn select_option(
        &self,
        snapshot: TreeSnapshot<B::Handle>,
        choice: usize,
        value: &str,
    ) -> AdapterResult<String> {
        if choice_options(&snapshot, choice).is_empty() {
            let handle = snapshot.node(choice).handle.clone();
            return self.select_popup_option(&handle, value);
        }
        self.select_inline_option(snapshot, choice, value)
    }

    fn select_inline_option(
        &self,
        snapshot: TreeSnapshot<B::Handle>,
        choice: usize,
        value: &str,
    ) -> AdapterResult<String> {
        if values_match(&choice_value(&snapshot, choice), value) {
            return Ok(format!("AT-SPI choice already shows {value:?}"));
        }
        let option = choose_option(&snapshot, choice, value)
            .ok_or_else(|| no_option_error(&snapshot, &choice_options(&snapshot, choice), value))?;
        let option_node = snapshot.node(option);
        let selected_via_selection = option_node
            .parent
            .filter(|parent| snapshot.node(*parent).info.interfaces.selection)
            .and_then(|parent| {
                let index = snapshot.index_in_parent(option)?;
                self.backend
                    .select_child(&snapshot.node(parent).handle, index)
                    .ok()
            })
            .is_some();
        if !selected_via_selection {
            self.backend
                .perform_action(&option_node.handle, OPTION_ACTIONS)?;
        }
        let choice_handle = snapshot.node(choice).handle.clone();
        let observed = self.poll(self.timing.action_timeout, || {
            let fresh = TreeSnapshot::capture(self.backend, &choice_handle, self.limits)?;
            let current = choice_value(&fresh, 0);
            Ok(values_match(&current, value).then_some(current))
        })?;
        observed
            .map(|current| format!("selected AT-SPI option {current:?}"))
            .ok_or_else(|| {
                AdapterError::ExecutionFailed(format!(
                    "AT-SPI option selection did not take effect: expected {value:?}."
                ))
            })
    }

    fn select_popup_option(&self, choice: &B::Handle, value: &str) -> AdapterResult<String> {
        let (popup, options) = self.open_popup(choice)?;
        let Some(option) = choose_among(&popup, &options, value) else {
            let error = no_option_error(&popup, &options, value);
            self.close_popup(&popup, &options);
            return Err(error);
        };
        self.backend
            .perform_action(&popup.node(option).handle, OPTION_ACTIONS)?;
        self.wait_popup_closed()?;

        let (popup, options) = self.open_popup(choice).map_err(|error| {
            AdapterError::ExecutionFailed(format!(
                "AT-SPI option {value:?} was activated but the selection could not be verified: reopening the control failed ({error})."
            ))
        })?;
        let selected = options
            .iter()
            .copied()
            .find(|option| popup.node(*option).info.states.selected)
            .map(|option| popup.node(option).info.readable_text());
        self.close_popup(&popup, &options);
        self.wait_popup_closed()?;
        match selected {
            Some(current) if values_match(&current, value) => {
                Ok(format!("selected AT-SPI popup option {current:?}"))
            }
            other => Err(AdapterError::ExecutionFailed(format!(
                "AT-SPI popup selection did not take effect: expected {value:?}, the reopened popup shows {:?} selected.",
                other.unwrap_or_default()
            ))),
        }
    }

    /// Invoke the choice's open action and wait for a showing popup window
    /// with option nodes.
    fn open_popup(
        &self,
        choice: &B::Handle,
    ) -> AdapterResult<(TreeSnapshot<B::Handle>, Vec<usize>)> {
        self.backend.perform_action(choice, OPEN_ACTIONS)?;
        self.poll(self.timing.action_timeout, || {
            for popup in self.popup_snapshots()? {
                let options = popup
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, node)| {
                        node.info.states.showing && role_matches("option", &node.info.role)
                    })
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                if !options.is_empty() {
                    return Ok(Some((popup, options)));
                }
            }
            Ok(None)
        })?
        .ok_or_else(|| {
            AdapterError::ExecutionFailed(
                "AT-SPI choice control exposes no options and opening it showed no popup with options."
                    .to_owned(),
            )
        })
    }

    /// Fail unless every popup with options has closed: an open popup would
    /// swallow the next step's input.
    fn wait_popup_closed(&self) -> AdapterResult<()> {
        let closed = self.poll(self.timing.action_timeout, || {
            let still_open = self.popup_snapshots()?.iter().any(|popup| {
                popup
                    .nodes
                    .iter()
                    .any(|node| node.info.states.showing && role_matches("option", &node.info.role))
            });
            Ok((!still_open).then_some(()))
        })?;
        closed.ok_or_else(|| {
            AdapterError::ExecutionFailed(
                "AT-SPI choice popup did not close after activating an option.".to_owned(),
            )
        })
    }

    /// Close an open popup without changing the value: activate the option
    /// that is already selected. The caller verifies that it closed.
    fn close_popup(&self, popup: &TreeSnapshot<B::Handle>, options: &[usize]) {
        if let Some(selected) = options
            .iter()
            .find(|option| popup.node(**option).info.states.selected)
        {
            let _ = self
                .backend
                .perform_action(&popup.node(*selected).handle, OPTION_ACTIONS);
        }
    }

    /// Showing top-level windows other than the scoped window. When a window
    /// scope is set, only windows of the scoped window's own process count, so
    /// another application's dialog can never be mistaken for the popup.
    fn popup_snapshots(&self) -> AdapterResult<Vec<TreeSnapshot<B::Handle>>> {
        let scope_processes = match (self.scope_process, self.window_title.as_deref()) {
            (Some(process), _) => vec![process],
            (None, Some(title)) => {
                let processes = self
                    .windows_matching(title)?
                    .iter()
                    .filter_map(|(window, _)| self.backend.process_id(window))
                    .collect::<Vec<_>>();
                if processes.is_empty() {
                    // Widening the search to every application could activate
                    // an option in someone else's window.
                    return Err(AdapterError::ExecutionFailed(format!(
                        "Could not resolve the process owning window {title:?}, so its choice popup cannot be told apart from other applications' windows."
                    )));
                }
                processes
            }
            (None, None) => Vec::new(),
        };
        let mut popups = Vec::new();
        for application in self.backend.applications()? {
            if !scope_processes.is_empty()
                && !self
                    .backend
                    .process_id(&application)
                    .is_some_and(|process| scope_processes.contains(&process))
            {
                continue;
            }
            let Ok(children) = self.backend.children(&application) else {
                continue;
            };
            for child in children {
                let Ok(info) = self.backend.describe(&child) else {
                    continue;
                };
                let is_scope = self
                    .window_title
                    .as_deref()
                    .is_some_and(|title| window_title_matches(&info.name, title));
                let popup_role = is_window_role(&info.role)
                    || matches!(
                        info.role.as_str(),
                        "menu" | "popup menu" | "list" | "list box"
                    );
                if popup_role && !is_scope && info.states.showing {
                    if let Ok(snapshot) = TreeSnapshot::capture(self.backend, &child, self.limits) {
                        popups.push(snapshot);
                    }
                }
            }
        }
        Ok(popups)
    }
}

fn no_option_error<H>(snapshot: &TreeSnapshot<H>, options: &[usize], value: &str) -> AdapterError {
    let available = options
        .iter()
        .map(|option| snapshot.node(*option).info.readable_text())
        .collect::<Vec<_>>();
    AdapterError::ExecutionFailed(format!(
        "AT-SPI choice has no option matching {value:?}; available options: {available:?}."
    ))
}
