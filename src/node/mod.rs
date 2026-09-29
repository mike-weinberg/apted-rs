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
///
/// Stores a hash of the label so that [`StringNodeData::same_label`] rejects
/// different labels with one integer comparison. Rename costs compare labels
/// in the innermost loops of APTED.
#[derive(Debug, Clone)]
pub struct StringNodeData {
    label: String,
    label_hash: u64,
}

impl StringNodeData {
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        let label_hash = fnv1a(label.as_bytes());
        Self { label, label_hash }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// True if both labels are equal.
    #[inline]
    pub fn same_label(&self, other: &StringNodeData) -> bool {
        self.label_hash == other.label_hash && self.label == other.label
    }
}

impl PartialEq for StringNodeData {
    fn eq(&self, other: &Self) -> bool {
        self.same_label(other)
    }
}

impl Eq for StringNodeData {}

impl std::hash::Hash for StringNodeData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.label.hash(state);
    }
}

/// 64-bit FNV-1a hash.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
