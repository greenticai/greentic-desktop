//! Pure locator matching over a [`TreeSnapshot`].
//!
//! Rules, in the order they are applied:
//!
//! * A strategy's `role` is a hard filter, mapped from runner vocabulary
//!   (`button`, `textbox`, `combobox`, …) onto AT-SPI role names.
//! * `name`, `label` and `text` compare against the node's name, Text contents
//!   and description after [`normalize_label`], which ignores case, leading
//!   emoji/symbols and trailing `:`/`*` — so `New Quote` matches `📄 New Quote`
//!   and `Company Name` matches `Company Name:*`. Exact beats contains; a query
//!   of two characters or fewer must match exactly.
//! * `automation_id` compares against the toolkit id attribute. When the
//!   strategy also carries a semantic field the id only ranks candidates —
//!   name/label wins over identifier, mirroring the macOS adapter — otherwise
//!   the id is required. If nothing satisfies the name, the id alone is tried.
//! * Showing nodes rank above non-showing ones; remaining ties go to capture
//!   order.

use super::model::{SnapshotNode, TreeSnapshot};
use greentic_desktop_adapter::{LocatorStrategy, LocatorTarget};

/// Lowercase, collapse whitespace, drop leading non-alphanumeric characters
/// (emoji, check marks, bullets) and trailing `:` / `*` / whitespace.
pub fn normalize_label(value: &str) -> String {
    let collapsed = value
        .replace('\u{fffc}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let trimmed_start = collapsed.trim_start_matches(|character: char| {
        !character.is_alphanumeric() && character != '£' && character != '$' && character != '€'
    });
    trimmed_start
        .trim_end_matches(|character: char| {
            character == ':' || character == '*' || character.is_whitespace()
        })
        .trim()
        .to_owned()
}

/// True when an observed value satisfies an expected one: equal after
/// [`normalize_label`], or — when the expected value is purely numeric — the
/// digits of the observed value equal it (`2000000` matches `£2,000,000`).
pub fn values_match(observed: &str, expected: &str) -> bool {
    if normalize_label(observed) == normalize_label(expected) {
        return true;
    }
    let expected = expected.trim();
    if expected.is_empty() || !expected.chars().all(|character| character.is_ascii_digit()) {
        return false;
    }
    let digits = observed
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    digits == expected
}

/// AT-SPI role names accepted for a runner role keyword. Unknown keywords
/// match a role with the same letters ignoring spaces, dashes and underscores.
pub fn role_matches(query: &str, role: &str) -> bool {
    let key = compact(query);
    if key.is_empty() {
        return true;
    }
    let role_key = compact(role);
    let accepted: &[&str] = match key.as_str() {
        "button" | "pushbutton" => &["pushbutton", "button", "togglebutton"],
        "textbox" | "textfield" | "entry" | "edit" => &["entry", "text", "passwordtext", "editbar"],
        "combobox" | "select" | "popupbutton" => &["combobox", "listbox", "menubutton"],
        "spinbutton" | "number" => &["spinbutton", "entry"],
        "heading" => &["heading", "label", "static"],
        "statictext" | "static" | "label" => {
            &["static", "label", "text", "paragraph", "section", "heading"]
        }
        "checkbox" => &["checkbox", "checkmenuitem"],
        "radio" | "radiobutton" => &["radiobutton", "radiomenuitem"],
        "dialog" => &["dialog", "alert", "window", "frame"],
        "window" | "frame" => &["frame", "window", "dialog"],
        "option" | "menuitem" | "listitem" => &["menuitem", "listitem", "option", "tablecell"],
        _ => return role_key == key,
    };
    accepted.contains(&role_key.as_str())
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn non_empty(value: Option<&String>) -> Option<&str> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TextMatch {
    Contains,
    Exact,
}

fn text_match<H>(node: &SnapshotNode<H>, query: &str) -> Option<TextMatch> {
    let query = normalize_label(query);
    if query.is_empty() {
        return None;
    }
    let candidates = [
        node.info.name.as_str(),
        node.info.text.as_deref().unwrap_or_default(),
        node.info.description.as_str(),
    ];
    let mut best = None;
    for candidate in candidates {
        let candidate = normalize_label(candidate);
        if candidate.is_empty() {
            continue;
        }
        if candidate == query {
            return Some(TextMatch::Exact);
        }
        if query.chars().count() > 2 && candidate.contains(&query) {
            best = Some(TextMatch::Contains);
        }
    }
    best
}

/// Score a node against one strategy. `None` means it does not match.
fn score<H>(node: &SnapshotNode<H>, strategy: &LocatorStrategy) -> Option<u32> {
    let role = non_empty(strategy.role.as_ref());
    let semantic = [
        non_empty(strategy.name.as_ref()),
        non_empty(strategy.label.as_ref()),
        non_empty(strategy.text.as_ref()),
    ];
    let automation_id = non_empty(strategy.automation_id.as_ref())
        .or_else(|| non_empty(strategy.data_testid.as_ref()));
    if role.is_none() && automation_id.is_none() && semantic.iter().all(Option::is_none) {
        return None;
    }
    if let Some(role) = role {
        if !role_matches(role, &node.info.role) {
            return None;
        }
    }
    let mut total = 0;
    for query in semantic.into_iter().flatten() {
        total += match text_match(node, query)? {
            TextMatch::Exact => 20,
            TextMatch::Contains => 10,
        };
    }
    let has_semantic = total > 0;
    if let Some(automation_id) = automation_id {
        let id_matches = node.info.identifier() == Some(automation_id);
        if id_matches {
            total += 40;
        } else if !has_semantic {
            return None;
        }
    }
    if node.info.states.showing {
        total += 2;
    }
    Some(total)
}

/// Best node for one strategy, or `None`.
///
/// When a strategy pairs an identifier with a name/label/text and no node
/// satisfies the name, a second pass accepts the identifier alone. Web
/// toolkits often expose a value element (`<strong id="annual-premium">`)
/// while flattening its caption away, so the name cannot match there even
/// though the identifier is exact.
pub fn find_by_strategy<H>(
    snapshot: &TreeSnapshot<H>,
    strategy: &LocatorStrategy,
) -> Option<usize> {
    best_match(snapshot, strategy).or_else(|| {
        let has_semantic = [&strategy.name, &strategy.label, &strategy.text]
            .into_iter()
            .any(|field| non_empty(field.as_ref()).is_some());
        let has_identifier = non_empty(strategy.automation_id.as_ref()).is_some()
            || non_empty(strategy.data_testid.as_ref()).is_some();
        (has_semantic && has_identifier)
            .then(|| LocatorStrategy {
                name: None,
                label: None,
                text: None,
                ..strategy.clone()
            })
            .and_then(|identifier_only| best_match(snapshot, &identifier_only))
    })
}

fn best_match<H>(snapshot: &TreeSnapshot<H>, strategy: &LocatorStrategy) -> Option<usize> {
    let mut best: Option<(u32, usize)> = None;
    for (index, node) in snapshot.nodes.iter().enumerate() {
        if let Some(points) = score(node, strategy) {
            if best.is_none_or(|(best_points, _)| points > best_points) {
                best = Some((points, index));
            }
        }
    }
    best.map(|(_, index)| index)
}

/// Resolve a runner target: preferred strategy first, then fallback.
pub fn resolve_target<H>(snapshot: &TreeSnapshot<H>, target: &LocatorTarget) -> Option<usize> {
    target
        .preferred
        .as_ref()
        .and_then(|strategy| find_by_strategy(snapshot, strategy))
        .or_else(|| {
            target
                .fallback
                .as_ref()
                .and_then(|strategy| find_by_strategy(snapshot, strategy))
        })
}

/// True when the target carries at least one field this matcher reads.
pub fn target_is_resolvable(target: &LocatorTarget) -> bool {
    [target.preferred.as_ref(), target.fallback.as_ref()]
        .into_iter()
        .flatten()
        .any(|strategy| {
            [
                &strategy.role,
                &strategy.name,
                &strategy.label,
                &strategy.text,
                &strategy.automation_id,
                &strategy.data_testid,
            ]
            .into_iter()
            .any(|field| non_empty(field.as_ref()).is_some())
        })
}
