//! Independent Zhang-Shasha oracle. Shared by `randomized.rs` and, through a
//! `#[path]` include, by the fuzz targets in `fuzz/`; it uses only the
//! public API of `apted`. Also holds `check_mapping`, the edit-mapping
//! validity check shared by the same callers.
// Each includer uses a subset of these helpers.
#![allow(dead_code)]

use apted::{CostModel, Node, StringNodeData, APTED};

/// Textbook Zhang-Shasha tree edit distance, used as an independent oracle.
pub fn zhang_shasha<C: CostModel<StringNodeData>>(
    cm: &C,
    t1: &Node<StringNodeData>,
    t2: &Node<StringNodeData>,
) -> f32 {
    struct Post<'a> {
        nodes: Vec<&'a Node<StringNodeData>>,
        lld: Vec<usize>,
        keyroots: Vec<usize>,
    }
    fn walk<'a>(n: &'a Node<StringNodeData>, p: &mut Post<'a>) -> usize {
        let mut first = None;
        for c in n.children() {
            let c_id = walk(c, p);
            let c_lld = p.lld[c_id];
            first.get_or_insert(c_lld);
        }
        let id = p.nodes.len();
        p.nodes.push(n);
        p.lld.push(first.unwrap_or(id));
        id
    }
    fn index(t: &Node<StringNodeData>) -> Post<'_> {
        let mut p = Post {
            nodes: vec![],
            lld: vec![],
            keyroots: vec![],
        };
        walk(t, &mut p);
        for i in 0..p.nodes.len() {
            if !(i + 1..p.nodes.len()).any(|j| p.lld[j] == p.lld[i]) {
                p.keyroots.push(i);
            }
        }
        p
    }
    let a = index(t1);
    let b = index(t2);
    let (n, m) = (a.nodes.len(), b.nodes.len());
    let mut td = vec![vec![0.0f32; m]; n];
    let mut fd = vec![vec![0.0f32; m + 1]; n + 1];
    for &i in &a.keyroots {
        for &j in &b.keyroots {
            let (li, lj) = (a.lld[i], b.lld[j]);
            fd[li][lj] = 0.0;
            for x in li..=i {
                fd[x + 1][lj] = fd[x][lj] + cm.del(a.nodes[x]);
            }
            for y in lj..=j {
                fd[li][y + 1] = fd[li][y] + cm.ins(b.nodes[y]);
            }
            for x in li..=i {
                for y in lj..=j {
                    let del = fd[x][y + 1] + cm.del(a.nodes[x]);
                    let ins = fd[x + 1][y] + cm.ins(b.nodes[y]);
                    if a.lld[x] == li && b.lld[y] == lj {
                        let ren = fd[x][y] + cm.ren(a.nodes[x], b.nodes[y]);
                        fd[x + 1][y + 1] = del.min(ins).min(ren);
                        td[x][y] = fd[x + 1][y + 1];
                    } else {
                        let sub = fd[a.lld[x]][b.lld[y]] + td[x][y];
                        fd[x + 1][y + 1] = del.min(ins).min(sub);
                    }
                }
            }
        }
    }
    td[n - 1][m - 1]
}

/// A tree indexed by 1-based postorder id, the numbering edit mappings use.
struct Indexed<'a> {
    nodes: Vec<&'a Node<StringNodeData>>,
    /// Preorder number of each node, same indexing as `nodes`.
    pre: Vec<usize>,
}

fn index_tree(t: &Node<StringNodeData>) -> Indexed<'_> {
    fn walk<'a>(n: &'a Node<StringNodeData>, counter: &mut usize, ix: &mut Indexed<'a>) {
        let pre = *counter;
        *counter += 1;
        for c in n.children() {
            walk(c, counter, ix);
        }
        ix.nodes.push(n);
        ix.pre.push(pre);
    }
    let mut ix = Indexed {
        nodes: Vec::new(),
        pre: Vec::new(),
    };
    walk(t, &mut 0, &mut ix);
    ix
}

/// An edit mapping is valid when every node is used exactly once, mapped
/// pairs keep ancestor and sibling order, and its cost is the distance.
pub fn check_mapping<C: CostModel<StringNodeData> + Copy>(
    cm: C,
    t1: &Node<StringNodeData>,
    t2: &Node<StringNodeData>,
) -> Result<(), String> {
    let fail = |msg: String| Err(format!("{msg}\n  t1={t1}\n  t2={t2}"));
    let (x1, x2) = (index_tree(t1), index_tree(t2));
    let (n1, n2) = (x1.nodes.len(), x2.nodes.len());
    let mut apted = APTED::new(cm);
    let d = apted.compute_edit_distance(t1, t2);
    let mapping = apted.compute_edit_mapping();

    let (mut seen1, mut seen2) = (vec![0u32; n1 + 1], vec![0u32; n2 + 1]);
    let mut cost = 0.0f32;
    let mut pairs = Vec::new();
    for &[a, b] in &mapping {
        let (a, b) = (a as usize, b as usize);
        if a > n1 || b > n2 || (a == 0 && b == 0) {
            return fail(format!(
                "pair ({a}, {b}) out of range for {n1} and {n2} nodes"
            ));
        }
        seen1[a] += 1;
        seen2[b] += 1;
        cost += match (a, b) {
            (a, 0) => cm.del(x1.nodes[a - 1]),
            (0, b) => cm.ins(x2.nodes[b - 1]),
            (a, b) => {
                pairs.push((a, b));
                cm.ren(x1.nodes[a - 1], x2.nodes[b - 1])
            }
        };
    }
    // Index 0 counts the "no partner" side and may repeat.
    if let Some(a) = (1..=n1).find(|&a| seen1[a] != 1) {
        return fail(format!("t1 node {a} appears {} times", seen1[a]));
    }
    if let Some(b) = (1..=n2).find(|&b| seen2[b] != 1) {
        return fail(format!("t2 node {b} appears {} times", seen2[b]));
    }
    // Ancestors have the larger postorder and the smaller preorder id; a node
    // to the left has both smaller. Both relations must carry over.
    for (i, &(a, b)) in pairs.iter().enumerate() {
        for &(a2, b2) in &pairs[i + 1..] {
            let same_pre = x1.pre[a - 1].cmp(&x1.pre[a2 - 1]) == x2.pre[b - 1].cmp(&x2.pre[b2 - 1]);
            let same_post = a.cmp(&a2) == b.cmp(&b2);
            if !(same_pre && same_post) {
                return fail(format!(
                    "pairs ({a}, {b}) and ({a2}, {b2}) change ancestor or sibling order"
                ));
            }
        }
    }
    if (cost - d).abs() > 1e-3 * (1.0 + cost.abs().max(d.abs())) {
        return fail(format!("mapping costs {cost}, distance is {d}"));
    }
    Ok(())
}
