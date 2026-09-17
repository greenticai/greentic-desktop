use super::executor::{AccessibilityExecutor, AccessibilityTiming};
use super::fixture::{node, FixtureTree};
use super::locator::{
    find_by_strategy, normalize_label, resolve_target, role_matches, values_match,
};
use super::model::{TreeSnapshot, WalkLimits};
use super::operations::{choose_option, editable_target, labeled_output, read_value};
use greentic_desktop_adapter::{LocatorStrategy, LocatorTarget, RunnerStep};

const TITLE: &str = "Meridian Commercial Insurance – Broker Workstation";

struct Meridian {
    tree: FixtureTree,
    turnover_entry: usize,
    activity_popup: usize,
    company_entry: usize,
    liability_menu: usize,
    other_close: usize,
    result_close: usize,
}

/// A tree shaped like WebKitGTK's rendering of the Meridian workstation.
fn meridian() -> Meridian {
    let mut tree = FixtureTree::new();
    let other_app = tree.add(None, node("application", "gnome-shell"));
    let other_frame = tree.add(Some(other_app), node("frame", "Some Other Window"));
    let other_close = tree.add(
        Some(other_frame),
        node("push button", "Close").clickable("press"),
    );
    // Another application's window that happens to list a "Retail" cell.
    let other_popup = tree.add(Some(other_app), node("window", ""));
    tree.add(
        Some(other_popup),
        node("table cell", "Retail").clickable("activate"),
    );

    let app = tree.add(None, node("application", "aws-demo-meridian-insurance"));
    // Indices below assume the application root is node 5.
    tree.set_process_id(app, 4242);
    let frame = tree.add(Some(app), node("frame", TITLE));
    let doc = tree.add(Some(frame), node("document web", TITLE));
    tree.add(
        Some(doc),
        node("push button", "📄 New Quote")
            .id("new-quote")
            .clickable("press"),
    );
    tree.add(
        Some(doc),
        node("push button", "👤 Load Demo Customer")
            .id("load-demo-customer")
            .clickable("press"),
    );
    let grid = tree.add(Some(doc), node("section", ""));
    tree.add(
        Some(grid),
        node("label", "Company Name:*").text("Company Name:*"),
    );
    let company_entry = tree.add(
        Some(grid),
        node("entry", "Company Name:*")
            .id("company-name")
            .editable(""),
    );
    tree.add(Some(grid), node("label", "Postcode:*").text("Postcode:*"));
    tree.add(
        Some(grid),
        node("entry", "Postcode:*").id("postcode").editable(""),
    );
    tree.add(
        Some(grid),
        node("check box", "Public Liability").clickable("click"),
    );
    let combo = tree.add(
        Some(grid),
        node("combo box", "Public Liability Limit:")
            .id("public-liability-limit")
            .clickable("press"),
    );
    let liability_menu = tree.add(Some(combo), node("menu", "").selection());
    for (index, label) in ["£1,000,000", "£2,000,000", "£5,000,000", "£10,000,000"]
        .iter()
        .enumerate()
    {
        let option = node("menu item", label).clickable("click");
        tree.add(
            Some(liability_menu),
            if index == 0 {
                option.selected()
            } else {
                option
            },
        );
    }

    // WebKitGTK shape: entries implement Text but not EditableText, and a
    // <select> exposes no options until its popup window is opened.
    let turnover_entry = tree.add(
        Some(grid),
        node("entry", "Annual Turnover (£):*")
            .id("annual-turnover")
            .text(""),
    );
    let activity = tree.add(
        Some(grid),
        node("combo box", "Business Activity:*")
            .id("business-activity")
            .clickable("select"),
    );
    let activity_popup = tree.add(Some(app), node("window", ""));
    let table = tree.add(Some(activity_popup), node("tree table", "").selection());
    for (index, label) in ["-- Select activity --", "Manufacturing", "Retail"]
        .iter()
        .enumerate()
    {
        let cell = node("table cell", label).clickable("activate");
        tree.add(Some(table), if index == 0 { cell.selected() } else { cell });
    }
    tree.link_popup(activity, activity_popup);

    let dialog = tree.add(Some(doc), node("dialog", "Quotation Result"));
    tree.add(Some(dialog), node("heading", "✓ QUOTE SUCCESSFUL"));
    let results = tree.add(Some(dialog), node("section", ""));
    for (caption, value, id) in [
        (
            "Quote Reference:",
            "BQ-123456",
            Some("result-quote-reference"),
        ),
        ("Insurer:", "Meridian Commercial", None),
        ("Annual Premium:", "£3,438.05", Some("annual-premium")),
        ("Excess:", "£500", None),
        (
            "Public Liability Limit:",
            "£2,000,000",
            Some("result-liability-limit"),
        ),
    ] {
        tree.add(Some(results), node("static", caption));
        let value_node = node("static", value);
        tree.add(
            Some(results),
            match id {
                Some(id) => value_node.id(id),
                None => value_node,
            },
        );
    }
    tree.add(
        Some(dialog),
        node("unknown", "").id("monthly-premium").text("£286.50"),
    );
    let result_close = tree.add(
        Some(dialog),
        node("push button", "Close")
            .id("close-result")
            .clickable("press"),
    );
    Meridian {
        tree,
        turnover_entry,
        activity_popup,
        company_entry,
        liability_menu,
        other_close,
        result_close,
    }
}

fn strategy(role: Option<&str>, name: Option<&str>, id: Option<&str>) -> LocatorStrategy {
    LocatorStrategy {
        role: role.map(str::to_owned),
        name: name.map(str::to_owned),
        automation_id: id.map(str::to_owned),
        ..LocatorStrategy::default()
    }
}

fn target(preferred: LocatorStrategy, fallback: Option<LocatorStrategy>) -> LocatorTarget {
    LocatorTarget {
        preferred: Some(preferred),
        fallback,
        visual_fallback: None,
    }
}

fn step(capability: &str, target: LocatorTarget, value: Option<&str>) -> RunnerStep {
    RunnerStep {
        id: "step".to_owned(),
        action: capability.trim_start_matches("linux.").to_owned(),
        target,
        value: value.map(str::to_owned),
        required_capability: capability.to_owned(),
    }
}

fn whole_tree(fixture: &Meridian) -> TreeSnapshot<usize> {
    // Application 1 is Meridian; capture from its root.
    TreeSnapshot::capture(&fixture.tree, &5, WalkLimits::default()).expect("capture")
}

fn executor(fixture: &Meridian) -> AccessibilityExecutor<'_, FixtureTree> {
    AccessibilityExecutor::new(
        &fixture.tree,
        Some("Meridian Commercial Insurance".to_owned()),
        AccessibilityTiming::immediate(),
    )
}

#[test]
fn normalize_label_ignores_emoji_case_and_required_markers() {
    assert_eq!(normalize_label("📄 New Quote"), "new quote");
    assert_eq!(normalize_label("Company Name:*"), "company name");
    assert_eq!(
        normalize_label("  ✓ QUOTE   SUCCESSFUL "),
        "quote successful"
    );
    assert_eq!(normalize_label("£500"), "£500");
    assert_eq!(normalize_label("Next >"), "next >");
}

#[test]
fn values_match_uses_digits_only_for_numeric_expectations() {
    assert!(values_match("£2,000,000", "2000000"));
    assert!(values_match("Retail", "retail"));
    assert!(!values_match("£20,000,000", "2000000"));
    assert!(!values_match("BQ-2000000", "2 000 000"));
    assert!(!values_match("Office 2", "Office"));
}

#[test]
fn runner_role_keywords_map_onto_atspi_role_names() {
    assert!(role_matches("button", "push button"));
    assert!(role_matches("textbox", "entry"));
    assert!(role_matches("combobox", "combo box"));
    assert!(role_matches("combo box", "combo box"));
    assert!(role_matches("spinbutton", "spin button"));
    assert!(role_matches("static text", "static"));
    assert!(role_matches("heading", "heading"));
    assert!(!role_matches("button", "entry"));
    assert!(role_matches("document web", "document web"));
}

#[test]
fn name_locator_tolerates_leading_emoji() {
    let fixture = meridian();
    let snapshot = whole_tree(&fixture);
    let index = find_by_strategy(
        &snapshot,
        &strategy(Some("button"), Some("New Quote"), None),
    )
    .expect("New Quote button");
    assert_eq!(snapshot.node(index).info.identifier(), Some("new-quote"));
}

#[test]
fn automation_id_alone_is_required_but_name_wins_over_a_wrong_id() {
    let fixture = meridian();
    let snapshot = whole_tree(&fixture);
    let by_id = find_by_strategy(&snapshot, &strategy(None, None, Some("postcode")))
        .expect("postcode by id");
    assert_eq!(snapshot.node(by_id).info.role, "entry");
    assert!(find_by_strategy(&snapshot, &strategy(None, None, Some("missing-id"))).is_none());

    let by_name = find_by_strategy(
        &snapshot,
        &strategy(
            Some("button"),
            Some("Load Demo Customer"),
            Some("renamed-id"),
        ),
    )
    .expect("name wins over identifier");
    assert_eq!(
        snapshot.node(by_name).info.identifier(),
        Some("load-demo-customer")
    );
}

#[test]
fn exact_name_beats_a_longer_name_that_contains_it() {
    let fixture = meridian();
    let snapshot = whole_tree(&fixture);
    let index = find_by_strategy(&snapshot, &strategy(None, Some("Public Liability"), None))
        .expect("public liability");
    assert_eq!(snapshot.node(index).info.role, "check box");
}

#[test]
fn identifier_ranks_between_equally_named_candidates() {
    let fixture = meridian();
    let snapshot = whole_tree(&fixture);
    // The caption and the entry are both named "Postcode:*"; without an id the
    // caption wins on document order, with the id the entry wins.
    let index = find_by_strategy(&snapshot, &strategy(None, Some("Postcode"), None))
        .expect("postcode caption");
    assert_eq!(snapshot.node(index).info.role, "label");
    let index = find_by_strategy(
        &snapshot,
        &strategy(None, Some("Postcode"), Some("postcode")),
    )
    .expect("postcode entry");
    assert_eq!(snapshot.node(index).info.role, "entry");
}

#[test]
fn fallback_strategy_is_used_when_preferred_does_not_match() {
    let fixture = meridian();
    let snapshot = whole_tree(&fixture);
    let located = resolve_target(
        &snapshot,
        &target(
            strategy(Some("textbox"), Some("Company Name Missing"), None),
            Some(LocatorStrategy {
                label: Some("Company Name".to_owned()),
                ..LocatorStrategy::default()
            }),
        ),
    )
    .expect("label fallback");
    // The caption comes first in document order; typing resolves to its input.
    assert_eq!(snapshot.node(located).info.role, "label");
    let editable = editable_target(&snapshot, located).expect("editable neighbour");
    assert_eq!(
        snapshot.node(editable).info.identifier(),
        Some("company-name")
    );
}

#[test]
fn a_caption_reads_the_value_of_its_next_sibling() {
    let fixture = meridian();
    let snapshot = whole_tree(&fixture);
    let caption = find_by_strategy(&snapshot, &strategy(None, Some("Annual Premium"), None))
        .expect("caption");
    assert_eq!(read_value(&snapshot, caption), "£3,438.05");
    let plain = find_by_strategy(
        &snapshot,
        &strategy(Some("static text"), Some("£500"), None),
    )
    .expect("excess value");
    assert_eq!(read_value(&snapshot, plain), "£500");
}

#[test]
fn numeric_value_selects_the_option_whose_digits_match() {
    let fixture = meridian();
    let snapshot = whole_tree(&fixture);
    let combo = find_by_strategy(
        &snapshot,
        &strategy(Some("combobox"), None, Some("public-liability-limit")),
    )
    .expect("combo");
    let option = choose_option(&snapshot, combo, "2000000").expect("option");
    assert_eq!(snapshot.node(option).info.name, "£2,000,000");
    assert!(choose_option(&snapshot, combo, "3000000").is_none());
}

#[test]
fn labeled_output_matches_the_macos_convention() {
    let read = step(
        "linux.read_text",
        LocatorTarget::default(),
        Some("outputs.quote_reference"),
    );
    assert_eq!(labeled_output(&read).as_deref(), Some("quote reference"));
    let assigned = step(
        "linux.read_text",
        LocatorTarget::default(),
        Some("outputs.annual_premium=x"),
    );
    assert_eq!(labeled_output(&assigned).as_deref(), Some("annual premium"));
    assert!(labeled_output(&step(
        "linux.read_text",
        LocatorTarget::default(),
        Some("plain")
    ))
    .is_none());
}

#[test]
fn read_text_emits_a_labeled_line_for_output_extraction() {
    let fixture = meridian();
    let read = step(
        "linux.read_text",
        target(
            strategy(None, Some("Annual Premium"), Some("annual-premium")),
            Some(LocatorStrategy {
                label: Some("Annual Premium".to_owned()),
                ..LocatorStrategy::default()
            }),
        ),
        Some("outputs.annual_premium"),
    );
    assert_eq!(
        executor(&fixture).read_text(&read).expect("read"),
        "annual premium: £3,438.05"
    );
}

#[test]
fn type_text_writes_through_editable_text_and_verifies() {
    let fixture = meridian();
    let typed = step(
        "linux.type_text",
        target(
            strategy(Some("textbox"), Some("Company Name"), Some("company-name")),
            None,
        ),
        Some("ACME Trading Ltd"),
    );
    executor(&fixture).type_text(&typed).expect("type");
    let text = fixture
        .tree
        .with_node(fixture.company_entry, |node| node.info.text.clone());
    assert_eq!(text.as_deref(), Some("ACME Trading Ltd"));
    let log = fixture.tree.log.lock().expect("log").clone();
    assert_eq!(
        log,
        vec![
            format!("focus {}", fixture.company_entry),
            format!("set_text {} ACME Trading Ltd", fixture.company_entry)
        ]
    );
}

#[test]
fn type_text_fails_when_the_value_does_not_stick() {
    let mut fixture = meridian();
    fixture.tree.ignore_text_writes(fixture.company_entry);
    let typed = step(
        "linux.type_text",
        target(strategy(Some("textbox"), Some("Company Name"), None), None),
        Some("ACME Trading Ltd"),
    );
    let error = executor(&fixture)
        .type_text(&typed)
        .expect_err("must verify");
    assert!(error.to_string().contains("verification failed"), "{error}");
}

#[test]
fn type_text_refuses_a_node_without_editable_text() {
    let fixture = meridian();
    let typed = step(
        "linux.type_text",
        target(strategy(Some("button"), Some("New Quote"), None), None),
        Some("x"),
    );
    let error = executor(&fixture)
        .type_text(&typed)
        .expect_err("not editable");
    assert!(error.to_string().contains("not editable"), "{error}");
}

#[test]
fn type_text_into_a_combo_box_selects_by_visible_digits() {
    let fixture = meridian();
    let typed = step(
        "linux.type_text",
        target(
            strategy(
                Some("combobox"),
                Some("Public Liability Limit"),
                Some("public-liability-limit"),
            ),
            None,
        ),
        Some("2000000"),
    );
    let message = executor(&fixture).type_text(&typed).expect("select");
    assert!(message.contains("£2,000,000"), "{message}");
    let selected = fixture
        .tree
        .with_node(fixture.liability_menu, |menu| menu.children[1]);
    assert!(fixture
        .tree
        .with_node(selected, |option| option.info.states.selected));
    let log = fixture.tree.log.lock().expect("log").clone();
    assert_eq!(
        log,
        vec![format!("select_child {} 1", fixture.liability_menu)]
    );
}

#[test]
fn click_is_scoped_to_the_requested_window() {
    let fixture = meridian();
    let click = step(
        "linux.click_element",
        target(strategy(Some("button"), Some("Close"), None), None),
        None,
    );
    executor(&fixture).click(&click).expect("click");
    let log = fixture.tree.log.lock().expect("log").clone();
    assert_eq!(
        log,
        vec![format!("action press on {}", fixture.result_close)]
    );
    assert!(!log
        .iter()
        .any(|entry| entry.ends_with(&format!(" {}", fixture.other_close))));
}

#[test]
fn find_element_searches_the_step_value_when_the_target_is_empty() {
    let fixture = meridian();
    let find = step(
        "linux.find_element",
        LocatorTarget::default(),
        Some("QUOTE SUCCESSFUL"),
    );
    let message = executor(&fixture).find_element(&find).expect("find");
    assert!(message.contains("heading"), "{message}");
}

#[test]
fn a_missing_element_fails_with_the_locator_and_window_named() {
    let fixture = meridian();
    let find = step(
        "linux.find_element",
        target(strategy(Some("button"), Some("Submit Claim"), None), None),
        None,
    );
    let error = executor(&fixture).find_element(&find).expect_err("absent");
    let message = error.to_string();
    assert!(message.contains("Submit Claim"), "{message}");
    assert!(
        message.contains("Meridian Commercial Insurance"),
        "{message}"
    );
    assert!(!executor(&fixture)
        .is_visible(&LocatorTarget::default(), "Submit Claim")
        .expect("visibility"));
}

#[test]
fn a_missing_window_is_reported_instead_of_searching_everything() {
    let fixture = meridian();
    let scoped = AccessibilityExecutor::new(
        &fixture.tree,
        Some("Not Running".to_owned()),
        AccessibilityTiming::immediate(),
    );
    let error = scoped.visible_texts().expect_err("no window");
    assert!(error.to_string().contains("Not Running"), "{error}");
}

#[test]
fn wait_for_window_prefers_the_spawned_process() {
    let fixture = meridian();
    let found = executor(&fixture)
        .wait_for_window("Broker Workstation", Some(4242), std::time::Duration::ZERO)
        .expect("window");
    assert_eq!(found, TITLE);
    assert!(executor(&fixture)
        .wait_for_window("Nope", None, std::time::Duration::ZERO)
        .is_err());
}

#[test]
fn snapshot_skips_subtrees_that_vanish_mid_walk() {
    let mut fixture = meridian();
    fixture.tree.fail_children(fixture.liability_menu);
    let snapshot = whole_tree(&fixture);
    assert_eq!(snapshot.skipped, 1);
    assert!(snapshot
        .nodes
        .iter()
        .all(|node| node.info.role != "menu item"));
    let limited = TreeSnapshot::capture(
        &fixture.tree,
        &5,
        WalkLimits {
            max_depth: 1,
            max_nodes: 100,
        },
    )
    .expect("capture");
    // Application root plus its two top-level windows (frame and popup).
    assert_eq!(limited.nodes.len(), 3);
}

#[test]
fn observations_list_visible_texts_once_and_dump_names_ids() {
    let fixture = meridian();
    let texts = executor(&fixture).visible_texts().expect("texts");
    assert!(texts.iter().any(|text| text == "£3,438.05"));
    assert_eq!(texts.iter().filter(|text| *text == TITLE).count(), 1);
    let dump = executor(&fixture).dump_tree().expect("dump");
    assert!(
        dump.contains("push button \"📄 New Quote\" #new-quote"),
        "{dump}"
    );
}

#[test]
fn identifier_alone_resolves_when_the_caption_is_flattened_away() {
    let fixture = meridian();
    let snapshot = whole_tree(&fixture);
    let index = find_by_strategy(
        &snapshot,
        &strategy(None, Some("Monthly Premium"), Some("monthly-premium")),
    )
    .expect("identifier fallback");
    assert_eq!(read_value(&snapshot, index), "£286.50");
    assert!(find_by_strategy(&snapshot, &strategy(None, Some("Monthly Premium"), None)).is_none());
}

#[test]
fn text_only_entries_are_typed_by_keyboard_synthesis_when_allowed() {
    let fixture = meridian();
    let typed = step(
        "linux.type_text",
        target(
            strategy(
                Some("spinbutton"),
                Some("Annual Turnover"),
                Some("annual-turnover"),
            ),
            None,
        ),
        Some("750000"),
    );
    let refused = executor(&fixture)
        .type_text(&typed)
        .expect_err("keyboard disabled");
    assert!(refused.to_string().contains("Wayland"), "{refused}");

    let message = executor(&fixture)
        .with_keyboard_input(true)
        .type_text(&typed)
        .expect("keyboard");
    assert!(message.contains("keyboard synthesis"), "{message}");
    let text = fixture
        .tree
        .with_node(fixture.turnover_entry, |node| node.info.text.clone());
    assert_eq!(text.as_deref(), Some("750000"));
}

#[test]
fn a_select_without_inline_options_is_chosen_through_its_popup_and_verified() {
    let fixture = meridian();
    let typed = step(
        "linux.type_text",
        target(
            strategy(
                Some("combobox"),
                Some("Business Activity"),
                Some("business-activity"),
            ),
            None,
        ),
        Some("Retail"),
    );
    let message = executor(&fixture).type_text(&typed).expect("popup select");
    assert!(message.contains("Retail"), "{message}");
    let log = fixture.tree.log.lock().expect("log").clone();
    assert_eq!(log.len(), 4, "{log:?}");
    assert!(log[0].starts_with("action select on"), "{log:?}");
    assert!(log[1].starts_with("action activate on"), "{log:?}");
    assert!(log[2].starts_with("action select on"), "{log:?}");
    assert_eq!(
        log[1], log[3],
        "popup closed by re-activating the selection"
    );
    assert!(!fixture
        .tree
        .with_node(fixture.activity_popup, |popup| popup.info.states.showing));
}

#[test]
fn an_unknown_popup_option_is_reported_with_the_available_choices() {
    let fixture = meridian();
    let typed = step(
        "linux.type_text",
        target(
            strategy(Some("combobox"), Some("Business Activity"), None),
            None,
        ),
        Some("Farming"),
    );
    let error = executor(&fixture).type_text(&typed).expect_err("no option");
    let message = error.to_string();
    assert!(message.contains("Manufacturing"), "{message}");
    assert!(!fixture
        .tree
        .with_node(fixture.activity_popup, |popup| popup.info.states.showing));
}
