//! In-memory [`AccessibleBackend`] for unit tests. It models just enough
//! behaviour to verify the executor's decisions: EditableText writes, Selection
//! and action-driven option changes, and failing subtrees.

use super::model::{AccessibleBackend, AccessibleNodeInfo, NodeInterfaces, NodeStates};
use greentic_desktop_adapter::{AdapterError, AdapterResult};
use std::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub struct FixtureNode {
    pub info: AccessibleNodeInfo,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub process_id: Option<u32>,
    pub actions: Vec<String>,
    pub fail_children: bool,
    /// When set, EditableText writes are silently ignored (a control that
    /// reports success but does not change) so verification can be tested.
    pub ignores_text_writes: bool,
    /// Invoking any action on this node shows the popup window at this index.
    pub opens_popup: Option<usize>,
    /// This node is a popup window: activating an option inside hides it.
    pub is_popup: bool,
}

#[derive(Debug, Default)]
pub struct FixtureTree {
    nodes: Mutex<Vec<FixtureNode>>,
    applications: Vec<usize>,
    pub log: Mutex<Vec<String>>,
}

pub fn node(role: &str, name: &str) -> FixtureNode {
    FixtureNode {
        info: AccessibleNodeInfo {
            role: role.to_owned(),
            name: name.to_owned(),
            states: NodeStates {
                showing: true,
                visible: true,
                enabled: true,
                ..NodeStates::default()
            },
            ..AccessibleNodeInfo::default()
        },
        ..FixtureNode::default()
    }
}

impl FixtureNode {
    pub fn id(mut self, id: &str) -> Self {
        self.info.attributes.insert("id".to_owned(), id.to_owned());
        self
    }

    pub fn text(mut self, text: &str) -> Self {
        self.info.text = Some(text.to_owned());
        self.info.interfaces.text = true;
        self
    }

    pub fn editable(mut self, text: &str) -> Self {
        self = self.text(text);
        self.info.interfaces.editable_text = true;
        self.info.states.editable = true;
        self
    }

    pub fn clickable(mut self, action: &str) -> Self {
        self.actions.push(action.to_owned());
        self.info.interfaces.action = true;
        self
    }

    pub fn selection(mut self) -> Self {
        self.info.interfaces.selection = true;
        self
    }

    pub fn selected(mut self) -> Self {
        self.info.states.selected = true;
        self
    }

    #[allow(dead_code)]
    pub fn hidden(mut self) -> Self {
        self.info.states.showing = false;
        self.info.states.visible = false;
        self
    }

    #[allow(dead_code)]
    pub fn interfaces(mut self, interfaces: NodeInterfaces) -> Self {
        self.info.interfaces = interfaces;
        self
    }
}

impl FixtureTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, parent: Option<usize>, node: FixtureNode) -> usize {
        let nodes = self.nodes.get_mut().expect("fixture mutex");
        let index = nodes.len();
        nodes.push(FixtureNode { parent, ..node });
        match parent {
            Some(parent) => nodes[parent].children.push(index),
            None => self.applications.push(index),
        }
        index
    }

    pub fn with_node<T>(&self, index: usize, read: impl FnOnce(&FixtureNode) -> T) -> T {
        read(&self.nodes.lock().expect("fixture mutex")[index])
    }

    pub fn set_process_id(&mut self, index: usize, process_id: u32) {
        self.nodes.get_mut().expect("fixture mutex")[index].process_id = Some(process_id);
    }

    pub fn fail_children(&mut self, index: usize) {
        self.nodes.get_mut().expect("fixture mutex")[index].fail_children = true;
    }

    pub fn link_popup(&mut self, opener: usize, popup: usize) {
        let nodes = self.nodes.get_mut().expect("fixture mutex");
        nodes[opener].opens_popup = Some(popup);
        nodes[popup].is_popup = true;
        Self::set_showing(nodes, popup, false);
    }

    fn set_showing(nodes: &mut [FixtureNode], index: usize, showing: bool) {
        let mut stack = vec![index];
        while let Some(current) = stack.pop() {
            nodes[current].info.states.showing = showing;
            stack.extend(nodes[current].children.iter().copied());
        }
    }

    fn enclosing_popup(nodes: &[FixtureNode], index: usize) -> Option<usize> {
        let mut cursor = nodes[index].parent;
        while let Some(current) = cursor {
            if nodes[current].is_popup {
                return Some(current);
            }
            cursor = nodes[current].parent;
        }
        None
    }

    pub fn ignore_text_writes(&mut self, index: usize) {
        self.nodes.get_mut().expect("fixture mutex")[index].ignores_text_writes = true;
    }

    fn record(&self, entry: String) {
        self.log.lock().expect("fixture log").push(entry);
    }

    fn select_among_siblings(nodes: &mut [FixtureNode], index: usize) {
        if let Some(parent) = nodes[index].parent {
            for sibling in nodes[parent].children.clone() {
                nodes[sibling].info.states.selected = sibling == index;
            }
        }
    }
}

impl AccessibleBackend for FixtureTree {
    type Handle = usize;

    fn applications(&self) -> AdapterResult<Vec<usize>> {
        Ok(self.applications.clone())
    }

    fn children(&self, node: &usize) -> AdapterResult<Vec<usize>> {
        let nodes = self.nodes.lock().expect("fixture mutex");
        let node = &nodes[*node];
        if node.fail_children {
            return Err(AdapterError::ExecutionFailed("object vanished".to_owned()));
        }
        Ok(node.children.clone())
    }

    fn describe(&self, node: &usize) -> AdapterResult<AccessibleNodeInfo> {
        Ok(self.nodes.lock().expect("fixture mutex")[*node]
            .info
            .clone())
    }

    fn process_id(&self, node: &usize) -> Option<u32> {
        let nodes = self.nodes.lock().expect("fixture mutex");
        let mut cursor = Some(*node);
        while let Some(index) = cursor {
            if let Some(process_id) = nodes[index].process_id {
                return Some(process_id);
            }
            cursor = nodes[index].parent;
        }
        None
    }

    fn perform_action(&self, node: &usize, preferred: &[&str]) -> AdapterResult<String> {
        let mut nodes = self.nodes.lock().expect("fixture mutex");
        let actions = nodes[*node].actions.clone();
        let action = preferred
            .iter()
            .find_map(|wanted| {
                actions
                    .iter()
                    .find(|action| action.eq_ignore_ascii_case(wanted))
            })
            .or(actions.first())
            .cloned()
            .ok_or_else(|| AdapterError::ExecutionFailed("node has no actions".to_owned()))?;
        if let Some(popup) = nodes[*node].opens_popup {
            Self::set_showing(&mut nodes, popup, true);
        }
        if matches!(
            nodes[*node].info.role.as_str(),
            "menu item" | "list item" | "table cell"
        ) {
            Self::select_among_siblings(&mut nodes, *node);
            if let Some(popup) = Self::enclosing_popup(&nodes, *node) {
                Self::set_showing(&mut nodes, popup, false);
            }
        }
        drop(nodes);
        self.record(format!("action {action} on {node}"));
        Ok(action)
    }

    fn set_text_contents(&self, node: &usize, value: &str) -> AdapterResult<()> {
        let mut nodes = self.nodes.lock().expect("fixture mutex");
        if !nodes[*node].info.interfaces.editable_text {
            return Err(AdapterError::ExecutionFailed("not editable".to_owned()));
        }
        if !nodes[*node].ignores_text_writes {
            nodes[*node].info.text = Some(value.to_owned());
        }
        drop(nodes);
        self.record(format!("set_text {node} {value}"));
        Ok(())
    }

    fn replace_text_by_keyboard(&self, node: &usize, value: &str) -> AdapterResult<()> {
        let mut nodes = self.nodes.lock().expect("fixture mutex");
        if !nodes[*node].info.interfaces.text {
            return Err(AdapterError::ExecutionFailed(
                "no text interface".to_owned(),
            ));
        }
        if !nodes[*node].ignores_text_writes {
            nodes[*node].info.text = Some(value.to_owned());
        }
        drop(nodes);
        self.record(format!("keyboard {node} {value}"));
        Ok(())
    }

    fn grab_focus(&self, node: &usize) -> AdapterResult<()> {
        self.record(format!("focus {node}"));
        Ok(())
    }

    fn select_child(&self, parent: &usize, index: usize) -> AdapterResult<()> {
        let mut nodes = self.nodes.lock().expect("fixture mutex");
        let child = *nodes[*parent]
            .children
            .get(index)
            .ok_or_else(|| AdapterError::ExecutionFailed("no such child".to_owned()))?;
        Self::select_among_siblings(&mut nodes, child);
        drop(nodes);
        self.record(format!("select_child {parent} {index}"));
        Ok(())
    }
}
