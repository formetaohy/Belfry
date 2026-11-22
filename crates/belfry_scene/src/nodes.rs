use crate::node::Node;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

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

    pub(crate) fn at_mut(&mut self, at: usize) -> &mut Node {
        &mut self.entries[at]
    }

    pub(crate) fn reparent(&mut self, at: usize, parent: Option<String>) {
        self.entries[at].parent = parent;
        self.reindex();
    }

    pub(crate) fn push(&mut self, node: Node) {
        let at = self.entries.len();
        self.attach(&node, at);
        self.entries.push(node);
    }

    pub(crate) fn extend(&mut self, nodes: impl IntoIterator<Item = Node>) {
        for node in nodes {
            self.push(node);
        }
    }

    pub(crate) fn insert(&mut self, at: usize, node: Node) {
        assert!(
            at <= self.entries.len(),
            "an inserted node lands inside the scene"
        );
        self.entries.insert(at, node);
        self.reindex();
    }

    pub(crate) fn splice(&mut self, at: usize, nodes: Vec<Node>) {
        assert!(
            at <= self.entries.len(),
            "a spliced node lands inside the scene"
        );
        assert!(!nodes.is_empty(), "a splice inserts at least one node");
        self.entries.splice(at..at, nodes);
        self.reindex();
    }

    pub(crate) fn remove_many(&mut self, at: &[usize]) {
        if at.is_empty() {
            return;
        }
        assert!(
            at.windows(2).all(|pair| pair[0] < pair[1]) && at[at.len() - 1] < self.entries.len(),
            "a batch removal names its nodes once, in order, inside the scene",
        );
        let mut taken = at.iter().peekable();
        let mut kept = Vec::with_capacity(self.entries.len() - at.len());
        for (index, node) in std::mem::take(&mut self.entries).into_iter().enumerate() {
            if taken.peek() == Some(&&index) {
                taken.next();
            } else {
                kept.push(node);
            }
        }
        self.entries = kept;
        self.reindex();
    }

    pub(crate) fn take(&mut self) -> Vec<Node> {
        self.index.clear();
        self.children.clear();
        std::mem::take(&mut self.entries)
    }

    pub(crate) fn mutable(&mut self) -> Mutable<'_> {
        Mutable { nodes: self }
    }

    fn attach(&mut self, node: &Node, at: usize) {
        self.index.insert(node.id.clone(), at);
        if let Some(parent) = &node.parent {
            self.children.entry(parent.clone()).or_default().push(at);
        }
    }

    fn reindex(&mut self) {
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

pub(crate) struct Mutable<'a> {
    nodes: &'a mut Nodes,
}

impl Deref for Mutable<'_> {
    type Target = Vec<Node>;

    fn deref(&self) -> &Self::Target {
        &self.nodes.entries
    }
}

impl DerefMut for Mutable<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes.entries
    }
}

impl Drop for Mutable<'_> {
    fn drop(&mut self) {
        self.nodes.reindex();
    }
}
