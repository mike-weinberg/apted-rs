//! Tree nodes and node indexing.

mod node_indexer;

use std::fmt;

pub use node_indexer::NodeIndexer;

/// A tree node holding data of type `D` and an ordered list of children.
#[derive(Debug, Clone, PartialEq)]
pub struct Node<D> {
    node_data: D,
    children: Vec<Node<D>>,
}

impl<D> Node<D> {
    pub fn new(node_data: D) -> Self {
        Self {
            node_data,
            children: Vec::new(),
        }
    }

    /// Number of nodes in the subtree rooted at this node.
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(Node::node_count).sum::<usize>()
    }

    pub fn add_child(&mut self, c: Node<D>) {
        self.children.push(c);
    }

    pub fn node_data(&self) -> &D {
        &self.node_data
    }

    pub fn set_node_data(&mut self, node_data: D) {
        self.node_data = node_data;
    }

    pub fn children(&self) -> &[Node<D>] {
        &self.children
    }
}

/// Renders the tree in bracket notation, e.g. `{a{b}{c}}`.
impl fmt::Display for Node<StringNodeData> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}", self.node_data.label())?;
        for child in &self.children {
            write!(f, "{child}")?;
        }
        write!(f, "}}")
    }
}

/// Node data consisting of a single string label.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StringNodeData {
    label: String,
}

impl StringNodeData {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}
