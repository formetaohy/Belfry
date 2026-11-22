use crate::node::Node;
use std::collections::HashMap;
use std::ops::Deref;

pub(crate) struct Nodes {
    entries: Vec<Node>,
    index: HashMap<String, usize>,
    children: HashMap<String, Vec<usize>>,
}

impl Nodes {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
            children: HashMap::new(),
        }
    }

    pub(crate) fn index_of(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    pub(crate) fn children_of(&self, index: usize) -> &[usize] {
        self.entries
            .get(index)
            .and_then(|node| self.children.get(&node.id))
            .map_or(&[], Vec::as_slice)
    }

    pub(crate) fn splice(&mut self, at: usize, removed: usize, inserted: Vec<Node>) -> Vec<Node> {
        assert!(
            at <= self.entries.len() && at + removed <= self.entries.len(),
            "a spliced node range lands inside the scene",
        );
        if removed == 0 && inserted.is_empty() {
            return Vec::new();
        }
        let taken = self
            .entries
            .splice(at..at + removed, inserted)
            .collect::<Vec<_>>();
        self.reindex();
        taken
    }

    pub(crate) fn replace(&mut self, at: usize, node: Node) -> Node {
        let held = std::mem::replace(&mut self.entries[at], node);
        let held_parent = held.parent.as_deref();
        let held_id = held.id.as_str();
        if held_id != self.entries[at].id || held_parent != self.entries[at].parent.as_deref() {
            self.reindex();
        }
        held
    }

    pub(crate) fn reindex(&mut self) {
        self.index = HashMap::with_capacity(self.entries.len());
        self.children = HashMap::new();
        for (at, node) in self.entries.iter().enumerate() {
            self.index.insert(node.id.clone(), at);
            if let Some(parent) = &node.parent {
                self.children.entry(parent.clone()).or_default().push(at);
            }
        }
    }
}

impl Clone for Nodes {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            index: self.index.clone(),
            children: self.children.clone(),
        }
    }
}

impl Default for Nodes {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Nodes {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl std::fmt::Debug for Nodes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.entries.fmt(formatter)
    }
}

impl Deref for Nodes {
    type Target = [Node];

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl From<Vec<Node>> for Nodes {
    fn from(entries: Vec<Node>) -> Self {
        let mut nodes = Self {
            entries,
            index: HashMap::new(),
            children: HashMap::new(),
        };
        nodes.reindex();
        nodes
    }
}

impl<'a> IntoIterator for &'a Nodes {
    type Item = &'a Node;
    type IntoIter = std::slice::Iter<'a, Node>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}
