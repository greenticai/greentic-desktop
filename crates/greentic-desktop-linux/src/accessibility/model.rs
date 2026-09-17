//! Toolkit-neutral accessible-tree model.
//!
//! Every AT-SPI round trip goes through [`AccessibleBackend`], and every
//! decision about *which* node a locator means is made against a
//! [`TreeSnapshot`] captured from it. That split is what lets locator matching,
//! label→value reads and option selection be unit-tested against an in-memory
//! tree without a live accessibility bus.

use greentic_desktop_adapter::AdapterResult;
use std::collections::BTreeMap;

/// The subset of AT-SPI states the adapter reasons about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeStates {
    pub showing: bool,
    pub visible: bool,
    pub enabled: bool,
    pub focused: bool,
    pub editable: bool,
    pub selected: bool,
    pub checked: bool,
}

/// Which AT-SPI interfaces a node implements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeInterfaces {
    pub action: bool,
    pub text: bool,
    pub editable_text: bool,
    pub selection: bool,
    pub component: bool,
}

/// One node's readable facts, fetched once per snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessibleNodeInfo {
    /// AT-SPI role name as reported by `GetRoleName` (for example `push button`).
    pub role: String,
    pub name: String,
    pub description: String,
    /// Contents of the Text interface, when the node implements it.
    pub text: Option<String>,
    /// Object attributes. WebKitGTK and Chromium expose the DOM `id` as `id`.
    pub attributes: BTreeMap<String, String>,
    pub states: NodeStates,
    pub interfaces: NodeInterfaces,
}

impl AccessibleNodeInfo {
    /// The DOM/toolkit identifier, when the toolkit exposes one.
    pub fn identifier(&self) -> Option<&str> {
        ["id", "html-id", "automation-id", "toolkit-id"]
            .iter()
            .find_map(|key| self.attributes.get(*key))
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    /// The text a person would read for this node: Text contents first, then
    /// the accessible name. Object-replacement characters (U+FFFC, which
    /// AT-SPI uses to mark embedded children inside a Text run) are removed.
    pub fn readable_text(&self) -> String {
        let text = self
            .text
            .as_deref()
            .map(|value| value.replace('\u{fffc}', ""))
            .map(|value| value.trim().to_owned())
            .unwrap_or_default();
        if !text.is_empty() {
            return text;
        }
        self.name.trim().to_owned()
    }
}

/// I/O seam over an accessibility tree. The live implementation talks AT-SPI
/// over D-Bus; tests use an in-memory fixture.
pub trait AccessibleBackend {
    type Handle: Clone + std::fmt::Debug;

    /// Top-level accessible applications registered on the bus.
    fn applications(&self) -> AdapterResult<Vec<Self::Handle>>;
    fn children(&self, node: &Self::Handle) -> AdapterResult<Vec<Self::Handle>>;
    fn describe(&self, node: &Self::Handle) -> AdapterResult<AccessibleNodeInfo>;
    /// Unix process id of the application owning `node`, when resolvable.
    fn process_id(&self, node: &Self::Handle) -> Option<u32>;
    /// Invoke the first available action whose name is in `preferred`
    /// (case-insensitive), falling back to action 0. Returns the action name.
    fn perform_action(&self, node: &Self::Handle, preferred: &[&str]) -> AdapterResult<String>;
    fn set_text_contents(&self, node: &Self::Handle, value: &str) -> AdapterResult<()>;
    fn grab_focus(&self, node: &Self::Handle) -> AdapterResult<()>;
    /// Focus `node`, select its current text and type `value` through
    /// keyboard synthesis, replacing the selection.
    fn replace_text_by_keyboard(&self, node: &Self::Handle, value: &str) -> AdapterResult<()>;
    /// Select the child at `index` of a node implementing Selection.
    fn select_child(&self, parent: &Self::Handle, index: usize) -> AdapterResult<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
}

impl Default for WalkLimits {
    fn default() -> Self {
        Self {
            max_depth: 64,
            max_nodes: 6000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotNode<H> {
    pub handle: H,
    pub info: AccessibleNodeInfo,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub depth: usize,
}

/// A breadth-first capture of one subtree, in document order per level.
#[derive(Debug, Clone)]
pub struct TreeSnapshot<H> {
    pub nodes: Vec<SnapshotNode<H>>,
    /// Nodes whose children or description could not be read (a web page
    /// mutating mid-walk makes this normal) plus nodes cut by [`WalkLimits`].
    pub skipped: usize,
}

impl<H: Clone + std::fmt::Debug> TreeSnapshot<H> {
    /// Capture `root` and its descendants. Only a failure to describe the root
    /// itself is an error; a vanished descendant is counted in `skipped`.
    pub fn capture<B>(backend: &B, root: &H, limits: WalkLimits) -> AdapterResult<Self>
    where
        B: AccessibleBackend<Handle = H>,
    {
        let root_info = backend.describe(root)?;
        let mut snapshot = Self {
            nodes: vec![SnapshotNode {
                handle: root.clone(),
                info: root_info,
                parent: None,
                children: Vec::new(),
                depth: 0,
            }],
            skipped: 0,
        };
        let mut cursor = 0;
        while cursor < snapshot.nodes.len() {
            let depth = snapshot.nodes[cursor].depth;
            if depth >= limits.max_depth {
                cursor += 1;
                continue;
            }
            let handle = snapshot.nodes[cursor].handle.clone();
            let children = match backend.children(&handle) {
                Ok(children) => children,
                Err(_) => {
                    snapshot.skipped += 1;
                    cursor += 1;
                    continue;
                }
            };
            for child in children {
                if snapshot.nodes.len() >= limits.max_nodes {
                    snapshot.skipped += 1;
                    continue;
                }
                let Ok(info) = backend.describe(&child) else {
                    snapshot.skipped += 1;
                    continue;
                };
                let index = snapshot.nodes.len();
                snapshot.nodes.push(SnapshotNode {
                    handle: child,
                    info,
                    parent: Some(cursor),
                    children: Vec::new(),
                    depth: depth + 1,
                });
                snapshot.nodes[cursor].children.push(index);
            }
            cursor += 1;
        }
        Ok(snapshot)
    }
}

impl<H> TreeSnapshot<H> {
    pub fn node(&self, index: usize) -> &SnapshotNode<H> {
        &self.nodes[index]
    }

    /// Descendants of `index` in breadth-first order, excluding `index`.
    pub fn descendants(&self, index: usize) -> Vec<usize> {
        let mut queue = self.nodes[index].children.clone();
        let mut cursor = 0;
        while cursor < queue.len() {
            let children = self.nodes[queue[cursor]].children.clone();
            queue.extend(children);
            cursor += 1;
        }
        queue
    }

    /// Siblings that come after `index` under the same parent.
    pub fn following_siblings(&self, index: usize) -> Vec<usize> {
        let Some(parent) = self.nodes[index].parent else {
            return Vec::new();
        };
        let siblings = &self.nodes[parent].children;
        siblings
            .iter()
            .position(|candidate| *candidate == index)
            .map(|position| siblings[position + 1..].to_vec())
            .unwrap_or_default()
    }

    /// Position of `index` among its parent's children.
    pub fn index_in_parent(&self, index: usize) -> Option<usize> {
        let parent = self.nodes[index].parent?;
        self.nodes[parent]
            .children
            .iter()
            .position(|candidate| *candidate == index)
    }

    /// Every non-empty readable text in the snapshot, in capture order,
    /// de-duplicated. Used for observations.
    pub fn visible_texts(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        self.nodes
            .iter()
            .map(|node| node.info.readable_text())
            .filter(|text| !text.is_empty())
            .filter(|text| seen.insert(text.clone()))
            .collect()
    }
}
