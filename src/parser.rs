//! Parser for trees in bracket notation, e.g. `{a{b}{c}}`.
//!
//! Ports `parser.BracketStringInputParser`, with two deliberate differences:
//! the parser is a single iterative pass (linear time, no recursion, so any
//! nesting depth is safe), and it is strict. The Java implementation (and
//! the first version of this port) silently dropped or merged nodes on
//! malformed input such as `{a{b}x{c}}` or `{a}{b}`; here that is an error.
//!
//! Grammar: `tree := '{' label tree* '}'`, optionally surrounded by
//! whitespace. A label is any text up to the next unescaped brace; `\{`,
//! `\}` and `\\` stand for a literal brace or backslash, and a backslash
//! before any other character is kept as is.

use std::fmt;

use crate::node::{Node, StringNodeData};

/// Why a bracket-notation string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset in the input where the problem was detected.
    pub position: usize,
    /// What was wrong.
    pub message: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid bracket notation at byte {}: {}",
            self.position, self.message
        )
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy, Default)]
pub struct BracketStringInputParser;

impl BracketStringInputParser {
    pub fn new() -> Self {
        Self
    }

    /// Parses a tree in bracket notation. Panics on malformed input, as the
    /// Java implementation throws; use [`Self::try_from_string`] for
    /// untrusted input.
    pub fn from_string(&self, s: &str) -> Node<StringNodeData> {
        self.try_from_string(s).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Parses a tree in bracket notation, returning an error instead of
    /// panicking on malformed input. Runs in linear time and constant stack
    /// space for any input.
    pub fn try_from_string(&self, s: &str) -> Result<Node<StringNodeData>, ParseError> {
        let bytes = s.as_bytes();
        let err = |position, message| Err(ParseError { position, message });
        let mut pos = skip_whitespace(bytes, 0);
        if bytes.get(pos) != Some(&b'{') {
            return err(pos, "expected '{' at the start of the tree");
        }
        // Nodes whose closing brace has not been seen yet, outermost first.
        let mut open: Vec<Node<StringNodeData>> = Vec::new();
        loop {
            // At an opening brace: read the label up to the next brace.
            pos += 1;
            let (label, end) = read_label(s, pos);
            open.push(Node::new(StringNodeData::new(label)));
            pos = end;
            // Children follow directly; anything else must be a closing
            // brace, possibly several in a row.
            loop {
                match bytes.get(pos) {
                    Some(b'{') => break,
                    Some(b'}') => {
                        pos += 1;
                        let done = open.pop().expect("a node is open at every '}'");
                        match open.last_mut() {
                            Some(parent) => parent.add_child(done),
                            None => {
                                let rest = skip_whitespace(bytes, pos);
                                if rest != bytes.len() {
                                    return err(rest, "unexpected text after the tree");
                                }
                                return Ok(done);
                            }
                        }
                    }
                    Some(_) => return err(pos, "unexpected text between sibling nodes"),
                    None => return err(pos, "unexpected end of input: missing '}'"),
                }
            }
        }
    }
}

fn skip_whitespace(bytes: &[u8], mut pos: usize) -> usize {
    while bytes.get(pos).is_some_and(u8::is_ascii_whitespace) {
        pos += 1;
    }
    pos
}

/// Reads a label starting at byte `start`, unescaping `\{`, `\}` and `\\`.
/// Returns the label and the byte offset of the brace that ends it (or the
/// input length).
fn read_label(s: &str, start: usize) -> (String, usize) {
    let bytes = s.as_bytes();
    let mut label = String::new();
    let mut chunk = start; // start of the text not yet copied into `label`
    let mut pos = start;
    while let Some(&b) = bytes.get(pos) {
        match b {
            b'{' | b'}' => break,
            b'\\' if matches!(bytes.get(pos + 1), Some(b'{' | b'}' | b'\\')) => {
                // Braces and backslashes are ASCII, so these slice
                // boundaries always fall on UTF-8 character boundaries.
                label.push_str(&s[chunk..pos]);
                chunk = pos + 1; // keep the escaped character, drop the '\'
                pos += 2;
            }
            _ => pos += 1,
        }
    }
    label.push_str(&s[chunk..pos]);
    (label, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Node<StringNodeData>, ParseError> {
        BracketStringInputParser::new().try_from_string(s)
    }

    #[test]
    fn parses_well_formed_trees() {
        let t = p("{a{b}{c{d}}}").unwrap();
        assert_eq!(t.to_string(), "{a{b}{c{d}}}");
        assert_eq!(t.node_count(), 4);
        assert_eq!(p("  {x}\n").unwrap().to_string(), "{x}");
        assert_eq!(p("{}").unwrap().node_data().label(), "");
        assert_eq!(p("{a b{é}{日本}}").unwrap().to_string(), "{a b{é}{日本}}");
    }

    #[test]
    fn rejects_malformed_input() {
        for bad in [
            "", "   ", "}", "{", "a{b}", "{a}{b}", "{a}x", "{a{b}x{c}}", "{a{b}", "{a{b} {c}}",
            "{a<{b}>}",
        ] {
            assert!(p(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn escapes_round_trip() {
        let mut root = Node::new(StringNodeData::new("r"));
        root.add_child(Node::new(StringNodeData::new("x}{injected\\")));
        let text = root.to_string();
        assert_eq!(text, r"{r{x\}\{injected\\}}");
        let back = p(&text).unwrap();
        assert_eq!(back.node_count(), 2);
        assert_eq!(back.children()[0].node_data().label(), "x}{injected\\");
        // A backslash before any other character is literal.
        assert_eq!(p(r"{a\b}").unwrap().node_data().label(), r"a\b");
    }

    #[test]
    fn deep_nesting_is_linear_and_stack_safe() {
        let depth = 200_000;
        let s = "{a".repeat(depth) + &"}".repeat(depth);
        let t = p(&s).unwrap();
        assert_eq!(t.node_count(), depth);
        assert_eq!(t.to_string(), s);
        assert!(t.clone() == t);
    }
}
