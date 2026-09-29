//! Regression tests for defects found after porting. Each documents where the
//! port intentionally differs from the Java reference implementation.

use apted::{BracketStringInputParser, PerEditOperationStringNodeDataCostModel, APTED};

/// Upstream `spf1` starts the minimum of `ren - ins` (and `ren - del`) at the
/// subtree's summed insertion (deletion) cost instead of +infinity. When
/// renames cost more than a deletion plus an insertion, the distance is
/// undercounted. `{b}` -> `{c{a}}`: delete b, insert c and a = 0.7 + 0.6.
#[test]
fn spf1_single_node_source_with_expensive_rename() {
    let p = BracketStringInputParser::new();
    let (t1, t2) = (p.from_string("{b}"), p.from_string("{c{a}}"));
    let cm = PerEditOperationStringNodeDataCostModel::new(0.7, 0.3, 1.9);
    let d = APTED::new(cm).compute_edit_distance(&t1, &t2);
    assert!((d - 1.3).abs() < 1e-5, "expected 1.3, got {d}");
}

/// Mirror case: single-node destination tree. `{c{a}}` -> `{b}`: delete c
/// and a, insert b = 0.6 + 0.7.
#[test]
fn spf1_single_node_destination_with_expensive_rename() {
    let p = BracketStringInputParser::new();
    let (t1, t2) = (p.from_string("{c{a}}"), p.from_string("{b}"));
    let cm = PerEditOperationStringNodeDataCostModel::new(0.3, 0.7, 1.9);
    let d = APTED::new(cm).compute_edit_distance(&t1, &t2);
    assert!((d - 1.3).abs() < 1e-5, "expected 1.3, got {d}");
}
