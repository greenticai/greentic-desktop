//! Pure decisions made against a captured snapshot: which node to type into,
//! which option a value selects, what a `Label:` element's value is, and how a
//! read is labelled for output extraction.

use super::locator::{normalize_label, role_matches, values_match};
use super::model::TreeSnapshot;
use greentic_desktop_adapter::RunnerStep;

/// Roles that represent a pick-one-of-many control.
pub fn is_choice_role(role: &str) -> bool {
    role_matches("combobox", role)
}

/// Roles that represent one entry inside a choice control.
pub fn is_option_role(role: &str) -> bool {
    role_matches("option", role)
        || role_matches("checkbox", role)
        || role_matches("radiobutton", role)
}

fn is_editable_node<H>(snapshot: &TreeSnapshot<H>, index: usize) -> bool {
    let info = &snapshot.node(index).info;
    info.interfaces.editable_text || info.states.editable || is_choice_role(&info.role)
}

/// The node a `type_text` step should write into.
///
/// The located node itself when it is editable or a choice control; otherwise
/// its first editable descendant; otherwise, for a caption (`label`/`static`),
/// the editable node in the sibling immediately after it — which is how a
/// `<label>` located by text leads to the input beside it. Only the adjacent
/// sibling is considered, so a button never resolves to some later input.
pub fn editable_target<H>(snapshot: &TreeSnapshot<H>, index: usize) -> Option<usize> {
    if is_editable_node(snapshot, index) {
        return Some(index);
    }
    if let Some(found) = snapshot
        .descendants(index)
        .into_iter()
        .find(|candidate| is_editable_node(snapshot, *candidate))
    {
        return Some(found);
    }
    if !role_matches("label", &snapshot.node(index).info.role) {
        return None;
    }
    let sibling = snapshot.following_siblings(index).into_iter().next()?;
    std::iter::once(sibling)
        .chain(snapshot.descendants(sibling))
        .find(|candidate| is_editable_node(snapshot, *candidate))
}

/// Options offered by a choice control, in document order.
pub fn choice_options<H>(snapshot: &TreeSnapshot<H>, choice: usize) -> Vec<usize> {
    snapshot
        .descendants(choice)
        .into_iter()
        .filter(|candidate| is_option_role(&snapshot.node(*candidate).info.role))
        .collect()
}

/// The option whose visible text equals `value`, or — for a purely numeric
/// value — whose digits equal it. Exact text wins over the digit rule.
pub fn choose_option<H>(snapshot: &TreeSnapshot<H>, choice: usize, value: &str) -> Option<usize> {
    let options = choice_options(snapshot, choice);
    let wanted = normalize_label(value);
    options
        .iter()
        .copied()
        .find(|option| normalize_label(&snapshot.node(*option).info.readable_text()) == wanted)
        .or_else(|| {
            options
                .iter()
                .copied()
                .find(|option| values_match(&snapshot.node(*option).info.readable_text(), value))
        })
}

/// The value a choice control currently shows: its selected option, then its
/// Text contents. The accessible name is deliberately not used — for a
/// labelled `<select>` it is the label (`Business Activity:`), not the value.
pub fn choice_value<H>(snapshot: &TreeSnapshot<H>, choice: usize) -> String {
    let selected = choice_options(snapshot, choice)
        .into_iter()
        .find(|option| snapshot.node(*option).info.states.selected)
        .map(|option| snapshot.node(option).info.readable_text())
        .filter(|text| !text.is_empty());
    if let Some(selected) = selected {
        return selected;
    }
    snapshot
        .node(choice)
        .info
        .text
        .as_deref()
        .map(|text| text.replace('\u{fffc}', "").trim().to_owned())
        .unwrap_or_default()
}

/// First non-empty readable text in `index` or its subtree.
pub fn first_text<H>(snapshot: &TreeSnapshot<H>, index: usize) -> Option<String> {
    std::iter::once(index)
        .chain(snapshot.descendants(index))
        .map(|candidate| snapshot.node(candidate).info.readable_text())
        .find(|text| !text.is_empty())
}

/// The value a read of `index` yields. A node whose own text ends with `:`
/// is a caption, so the value is the first text in a following sibling
/// (`<span>Annual Premium:</span><strong>£3,438.05</strong>`).
pub fn read_value<H>(snapshot: &TreeSnapshot<H>, index: usize) -> String {
    let own = snapshot.node(index).info.readable_text();
    let is_caption = own.trim_end_matches('*').trim_end().ends_with(':');
    if is_caption {
        if let Some(adjacent) = snapshot
            .following_siblings(index)
            .into_iter()
            .find_map(|sibling| first_text(snapshot, sibling))
        {
            return adjacent;
        }
    }
    own
}

/// The label a `read_text` step publishes its value under, derived from an
/// `outputs.<name>` step value (`outputs.quote_reference` → `quote reference`).
/// Matches `macos_labeled_output`, which replay's `extract_labeled_output`
/// consumes.
pub fn labeled_output(step: &RunnerStep) -> Option<String> {
    step.value
        .as_deref()
        .and_then(|value| value.strip_prefix("outputs."))
        .map(|rest| rest.split_once('=').map(|(label, _)| label).unwrap_or(rest))
        .map(|label| label.replace('_', " ").trim().to_owned())
        .filter(|label| !label.is_empty())
}

/// Window-like roles an application exposes at its top level.
pub fn is_window_role(role: &str) -> bool {
    role_matches("window", role)
}

/// True when a window's accessible name satisfies a requested title.
pub fn window_title_matches(name: &str, title: &str) -> bool {
    let name = normalize_label(name);
    let title = normalize_label(title);
    !title.is_empty() && (name == title || name.contains(&title))
}
