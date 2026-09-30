//! `try_from_string` never panics on arbitrary bytes, and whatever it
//! accepts prints and parses back to an equal tree. Odd first byte: also
//! round-trip a tree whose labels are arbitrary text, through `Display`.
#![no_main]

use apted::{BracketStringInputParser, Node, StringNodeData};
use libfuzzer_sys::fuzz_target;

fn roundtrip(p: &BracketStringInputParser, t: &Node<StringNodeData>) {
    let printed = t.to_string();
    let again = p
        .try_from_string(&printed)
        .unwrap_or_else(|e| panic!("printed tree does not parse: {e}\n{printed:?}"));
    assert!(again == *t, "round trip changed the tree: {printed:?}");
    assert_eq!(again.to_string(), printed);
    assert_eq!(again.node_count(), t.node_count());
}

fuzz_target!(|data: &[u8]| {
    let p = BracketStringInputParser::new();
    let text = String::from_utf8_lossy(data);
    if let Ok(t) = p.try_from_string(&text) {
        assert!(t.node_count() >= 1);
        roundtrip(&p, &t);
    }
    // Labels made of arbitrary text, including braces and backslashes.
    if let Some((&first, rest)) = data.split_first() {
        if first & 1 == 1 {
            let mut root = Node::new(StringNodeData::new("r"));
            for chunk in rest.chunks(3) {
                let label = String::from_utf8_lossy(chunk).into_owned();
                let mut child = Node::new(StringNodeData::new(label));
                if chunk.first().is_some_and(|b| b & 1 == 1) {
                    child.add_child(Node::new(StringNodeData::new("\\{}\\")));
                }
                root.add_child(child);
            }
            roundtrip(&p, &root);
        }
    }
});
