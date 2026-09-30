//! Helpers shared by the fuzz targets: decoding a byte string into trees and
//! cost models with only std.

use apted::{Node, PerEditOperationStringNodeDataCostModel, StringNodeData};

/// Sequential reader over the fuzz input; reads past the end yield 0.
pub struct Bytes<'a>(pub &'a [u8]);

impl Bytes<'_> {
    pub fn byte(&mut self) -> u8 {
        match self.0.split_first() {
            Some((&b, rest)) => {
                self.0 = rest;
                b
            }
            None => 0,
        }
    }
}

/// Decodes a tree of 1..=`max_nodes` nodes. The first byte is the size, the
/// second is the parent window (1 gives a path, larger values give bushier
/// trees), then each non-root node takes a parent byte and a label byte.
/// Labels come from `alphabet` letters so equal labels are common.
pub fn decode_tree(b: &mut Bytes, max_nodes: usize, alphabet: u8) -> Node<StringNodeData> {
    let size = 1 + b.byte() as usize % max_nodes;
    let window = 1 + b.byte() as usize % 8;
    let mut parent = vec![0usize; size];
    for (i, p) in parent.iter_mut().enumerate().skip(1) {
        *p = i - 1 - b.byte() as usize % i.min(window);
    }
    let mut nodes: Vec<Option<Node<StringNodeData>>> = (0..size)
        .map(|_| {
            let c = (b'a' + b.byte() % alphabet.max(1)) as char;
            Some(Node::new(StringNodeData::new(c.to_string())))
        })
        .collect();
    // Children have higher indices than parents: attach from the back.
    for i in (1..size).rev() {
        let child = nodes[i].take().unwrap();
        nodes[parent[i]].as_mut().unwrap().add_child(child);
    }
    nodes[0].take().unwrap()
}

/// A symmetric cost model satisfying the triangle inequality:
/// del = ins = c, rename cost r with 0 <= r <= 2c, all multiples of 0.25.
pub fn decode_cost_model(b: &mut Bytes) -> (PerEditOperationStringNodeDataCostModel, f32, f32) {
    let c = 1 + b.byte() as u32 % 8; // quarters
    let r = b.byte() as u32 % (2 * c + 1);
    let (c, r) = (c as f32 / 4.0, r as f32 / 4.0);
    (PerEditOperationStringNodeDataCostModel::new(c, c, r), c, r)
}

/// Equal up to f32 rounding of sums of multiples of 0.25.
pub fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-3 * (1.0 + a.abs().max(b.abs()))
}
