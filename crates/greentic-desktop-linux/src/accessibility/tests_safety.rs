//! Regression tests for the guards that keep AT-SPI actions inside the
//! intended application and stop a step from reporting success it did not earn.

use super::executor::{AccessibilityExecutor, AccessibilityTiming};
use super::fixture::{node, FixtureTree};
use super::locator::{resolve_target, resolve_target_definite, text_equals};
use super::model::{TreeSnapshot, WalkLimits};
use greentic_desktop_adapter::{LocatorStrategy, LocatorTarget, RunnerStep};
use std::time::Duration;

const TITLE: &str = "Meridian Commercial Insurance – Broker Workstation";

fn named(role: &str, name: &str) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(LocatorStrategy {
            role: Some(role.to_owned()),
            name: Some(name.to_owned()),
            ..LocatorStrategy::default()
        }),
        ..LocatorTarget::default()
    }
}

fn step(capability: &str, target: LocatorTarget, value: Option<&str>) -> RunnerStep {
    RunnerStep {
        id: "step".to_owned(),
        action: capability.to_owned(),
        target,
        value: value.map(str::to_owned),
        required_capability: capability.to_owned(),
    }
}

/// Meridian (pid 4242) plus a browser (pid 7) whose tab title contains it.
fn desktop() -> (FixtureTree, usize, usize) {
    let mut tree = FixtureTree::new();
    let browser = tree.add(None, node("application", "firefox"));
    tree.set_process_id(browser, 7);
    let tab = tree.add(
        Some(browser),
        node("frame", &format!("{TITLE} — Mozilla Firefox")),
    );
    tree.add(Some(tab), node("entry", "Company Name:*").editable(""));

    let app = tree.add(None, node("application", "meridian"));
    tree.set_process_id(app, 4242);
    let frame = tree.add(Some(app), node("frame", TITLE));
    let entry = tree.add(
        Some(frame),
        node("entry", "Company Name:*").id("company-name").text(""),
    );
    tree.add(
        Some(frame),
        node("push button", "Close Quote").clickable("press"),
    );
    (tree, frame, entry)
}

#[test]
fn an_exact_title_wins_over_a_browser_tab_containing_it() {
    let (tree, _, _) = desktop();
    let executor = AccessibilityExecutor::new(&tree, None, AccessibilityTiming::immediate());
    let scope = executor
        .wait_for_scope(TITLE, Duration::ZERO)
        .expect("scope");
    assert_eq!(scope.process_id, Some(4242));

    let ambiguous = executor
        .wait_for_scope("Commercial Insurance", Duration::ZERO)
        .expect_err("two processes match only by substring");
    assert!(
        ambiguous.to_string().contains("more than one process"),
        "{ambiguous}"
    );
}

#[test]
fn a_process_scope_hides_other_applications_with_the_same_title() {
    let (tree, _, entry) = desktop();
    let executor = AccessibilityExecutor::new(
        &tree,
        Some("Commercial Insurance".to_owned()),
        AccessibilityTiming::immediate(),
    )
    .with_scope_process(Some(4242))
    .with_keyboard_input(true);
    executor
        .type_text(&step(
            "linux.type_text",
            named("textbox", "Company Name"),
            Some("ACME"),
        ))
        .expect("typed into Meridian");
    assert_eq!(
        tree.with_node(entry, |node| node.info.text.clone())
            .as_deref(),
        Some("ACME")
    );
}

#[test]
fn keystrokes_are_not_sent_when_the_scoped_window_is_inactive() {
    let (mut tree, frame, _) = desktop();
    tree.set_active(frame, false);
    let executor = AccessibilityExecutor::new(
        &tree,
        Some(TITLE.to_owned()),
        AccessibilityTiming::immediate(),
    )
    .with_scope_process(Some(4242))
    .with_keyboard_input(true);
    let error = executor
        .type_text(&step(
            "linux.type_text",
            named("textbox", "Company Name"),
            Some("ACME"),
        ))
        .expect_err("inactive window");
    assert!(
        error.to_string().contains("not the active window"),
        "{error}"
    );
    assert!(!tree
        .log
        .lock()
        .expect("log")
        .iter()
        .any(|entry| entry.starts_with("keyboard")));
}

#[test]
fn actions_prefer_definite_matches_and_typing_is_verified_case_sensitively() {
    let (tree, _, _) = desktop();
    let snapshot = TreeSnapshot::capture(&tree, &3, WalkLimits::default()).expect("capture");
    let close = named("button", "Close");
    assert!(resolve_target_definite(&snapshot, &close).is_none());
    assert!(resolve_target(&snapshot, &close).is_some());
    assert!(text_equals("ACME Trading Ltd\u{fffc} ", "ACME Trading Ltd"));
    assert!(!text_equals("acme tRADING lTD", "ACME Trading Ltd"));
}

#[test]
fn an_action_name_outside_the_preferred_list_is_not_invoked() {
    let mut tree = FixtureTree::new();
    let app = tree.add(None, node("application", "meridian"));
    tree.set_process_id(app, 4242);
    let frame = tree.add(Some(app), node("frame", TITLE));
    tree.add(
        Some(frame),
        node("push button", "Expand").clickable("expand or contract"),
    );
    let executor = AccessibilityExecutor::new(
        &tree,
        Some(TITLE.to_owned()),
        AccessibilityTiming::immediate(),
    );
    assert!(executor
        .click(&step(
            "linux.click_element",
            named("button", "Expand"),
            None
        ))
        .is_err());
    assert!(tree.log.lock().expect("log").is_empty());
}

#[test]
fn popup_selection_refuses_when_the_window_process_is_unknown() {
    let mut tree = FixtureTree::new();
    let app = tree.add(None, node("application", "meridian"));
    let frame = tree.add(Some(app), node("frame", TITLE));
    let combo = tree.add(
        Some(frame),
        node("combo box", "Business Activity:*").clickable("select"),
    );
    let popup = tree.add(Some(app), node("window", ""));
    tree.add(
        Some(popup),
        node("table cell", "Retail").clickable("activate"),
    );
    tree.link_popup(combo, popup);
    let executor = AccessibilityExecutor::new(
        &tree,
        Some(TITLE.to_owned()),
        AccessibilityTiming::immediate(),
    );
    let error = executor
        .type_text(&step(
            "linux.type_text",
            named("combobox", "Business Activity"),
            Some("Retail"),
        ))
        .expect_err("unknown process");
    assert!(
        error.to_string().contains("Could not resolve the process"),
        "{error}"
    );
}
