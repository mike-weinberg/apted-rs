//! Parser for trees in bracket notation, e.g. `{a{b}{c}}`.
//!
//! Ports `parser.BracketStringInputParser` and the parts of
//! `util.FormatUtilities` it relies on. Positions are byte offsets; the
//! bracket characters are ASCII, so slicing at them is always on a UTF-8
//! character boundary.

use crate::node::{Node, StringNodeData};

#[derive(Debug, Clone, Copy, Default)]
pub struct BracketStringInputParser;

impl BracketStringInputParser {
    pub fn new() -> Self {
        Self
    }

    /// Parses a tree in bracket notation. Panics on malformed input, as the
    /// Java implementation throws.
    pub fn from_string(&self, s: &str) -> Node<StringNodeData> {
        let start = s.find('{').expect("bracket notation must contain '{'");
        let end = s.rfind('}').expect("bracket notation must contain '}'");
        let s = &s[start..=end];
        let mut node = Node::new(StringNodeData::new(
            get_root(s).expect("malformed bracket notation"),
        ));
        for c in get_children(s).expect("malformed bracket notation") {
            node.add_child(self.from_string(c));
        }
        node
    }
}

/// Position of the bracket matching the one at `pos`, or `None`.
fn matching_bracket(s: &[u8], pos: usize) -> Option<usize> {
    if pos >= s.len() {
        return None;
    }
    let open = s[pos];
    let close = match open {
        b'{' => b'}',
        b'(' => b')',
        b'[' => b']',
        b'<' => b'>',
        _ => return None,
    };
    let mut pos = pos + 1;
    let mut count = 1;
    while count != 0 && pos < s.len() {
        if s[pos] == open {
            count += 1;
        } else if s[pos] == close {
            count -= 1;
        }
        pos += 1;
    }
    if count != 0 {
        None
    } else {
        Some(pos - 1)
    }
}

fn is_bracketed(s: &str) -> bool {
    !s.is_empty() && s.starts_with('{') && s.ends_with('}')
}

/// Label of the root node of `s`.
fn get_root(s: &str) -> Option<&str> {
    if !is_bracketed(s) {
        return None;
    }
    let end = s[1..]
        .find('{')
        .or_else(|| s[1..].find('}'))
        .map(|i| i + 1)?;
    Some(&s[1..end])
}

/// Bracket strings of the root's children.
fn get_children(s: &str) -> Option<Vec<&str>> {
    if !is_bracketed(s) {
        return None;
    }
    let mut children = Vec::new();
    let end = match s[1..].find('{') {
        Some(i) => i + 1,
        None => return Some(children),
    };
    let mut rest = &s[end..s.len() - 1];
    while !rest.is_empty() {
        let m = match matching_bracket(rest.as_bytes(), 0) {
            Some(m) => m,
            None => break,
        };
        children.push(&rest[..=m]);
        rest = if m + 1 < rest.len() {
            &rest[m + 1..]
        } else {
            ""
        };
    }
    Some(children)
}
