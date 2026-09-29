//! Tree nodes and node indexing.

mod node_indexer;

use std::fmt;

pub use node_indexer::NodeIndexer;

/// A tree node holding data of type `D` and an ordered list of children.
///
/// Every operation that walks the tree (`node_count`, `Clone`, `PartialEq`,
/// `Debug`, `Display`, `Drop`) is iterative, so arbitrarily deep trees
/// cannot overflow the stack.
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
        let mut count = 0;
        let mut stack = vec![self];
        while let Some(n) = stack.pop() {
            count += 1;
            stack.extend(n.children.iter());
        }
        count
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

/// Drops descendants from an explicit stack instead of recursively.
impl<D> Drop for Node<D> {
    fn drop(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let mut stack = std::mem::take(&mut self.children);
        while let Some(mut n) = stack.pop() {
            stack.append(&mut n.children);
        }
    }
}

impl<D: Clone> Clone for Node<D> {
    fn clone(&self) -> Self {
        // `open` holds the source nodes whose children are being copied and
        // the next child to copy; `built` the copies under construction.
        let mut open: Vec<(&Node<D>, usize)> = vec![(self, 0)];
        let mut built: Vec<Node<D>> = vec![self.shallow_clone()];
        loop {
            let (src, next) = open.last_mut().expect("root stays open until returned");
            if let Some(child) = src.children.get(*next) {
                *next += 1;
                open.push((child, 0));
                built.push(child.shallow_clone());
            } else {
                open.pop();
                let done = built.pop().expect("one copy per open node");
                match built.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => return done,
                }
            }
        }
    }
}

impl<D: Clone> Node<D> {
    fn shallow_clone(&self) -> Self {
        Node {
            node_data: self.node_data.clone(),
            children: Vec::with_capacity(self.children.len()),
        }
    }
}

impl<D: PartialEq> PartialEq for Node<D> {
    fn eq(&self, other: &Self) -> bool {
        let mut stack = vec![(self, other)];
        while let Some((a, b)) = stack.pop() {
            if a.node_data != b.node_data || a.children.len() != b.children.len() {
                return false;
            }
            stack.extend(a.children.iter().zip(b.children.iter()));
        }
        true
    }
}

impl<D: Eq> Eq for Node<D> {}

/// Writes the tree in preorder as `{data{child}...}`, calling `label` for
/// each node's data.
fn write_bracketed<D>(
    root: &Node<D>,
    f: &mut fmt::Formatter<'_>,
    mut label: impl FnMut(&D, &mut fmt::Formatter<'_>) -> fmt::Result,
) -> fmt::Result {
    enum Step<'a, D> {
        Open(&'a Node<D>),
        Close,
    }
    let mut stack = vec![Step::Open(root)];
    while let Some(step) = stack.pop() {
        match step {
            Step::Open(n) => {
                f.write_str("{")?;
                label(&n.node_data, f)?;
                stack.push(Step::Close);
                stack.extend(n.children.iter().rev().map(Step::Open));
            }
            Step::Close => f.write_str("}")?,
        }
    }
    Ok(())
}

/// Shows the tree in bracket notation with each node's data in `Debug`
/// form, e.g. `Node({"a"{"b"}})`.
impl<D: fmt::Debug> fmt::Debug for Node<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Node(")?;
        write_bracketed(self, f, |d, f| write!(f, "{d:?}"))?;
        f.write_str(")")
    }
}

/// Renders the tree in bracket notation, e.g. `{a{b}{c}}`. Braces and
/// backslashes in labels are escaped with a backslash, so the output parses
/// back to the same tree with [`crate::BracketStringInputParser`].
impl fmt::Display for Node<StringNodeData> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_bracketed(self, f, |d, f| {
            for c in d.label().chars() {
                if matches!(c, '{' | '}' | '\\') {
                    f.write_str("\\")?;
                }
                write!(f, "{c}")?;
            }
            Ok(())
        })
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
