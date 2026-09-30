//! Regression tests for defects found after porting. Each documents where the
//! port intentionally differs from the Java reference implementation.

use apted::{
    AllPossibleMappingsTED, BracketStringInputParser, PerEditOperationStringNodeDataCostModel,
    APTED,
};

/// Upstream `spf1` starts the minimum of `ren - ins` (and `ren - del`) at the
/// subtree's summed insertion (deletion) cost instead of +infinity. The
/// distance is undercounted when no labels match, every `ren - ins` exceeds
/// the summed insertion cost and that sum is below `del` (see the README).
/// `{b}` -> `{c{a}}`: delete b, insert c and a = 0.7 + 0.6.
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

/// Upstream `AllPossibleMappingsTED` starts its minimum at `size1 + size2`,
/// which is only an upper bound when every deletion and insertion costs at
/// most 1. With costs above 1 it returned that cap instead of the distance.
/// Found by the differential fuzz target. `{a{a{a{a}}}}` -> `{a}`: delete
/// three nodes at 1.75 each.
#[test]
fn brute_force_with_operation_costs_above_one() {
    let p = BracketStringInputParser::new();
    let (t1, t2) = (p.from_string("{a{a{a{a}}}}"), p.from_string("{a}"));
    let cm = PerEditOperationStringNodeDataCostModel::new(1.75, 1.75, 0.0);
    let bf = AllPossibleMappingsTED::new(cm).compute_edit_distance(&t1, &t2);
    let d = APTED::new(cm).compute_edit_distance(&t1, &t2);
    assert!(
        (bf - 5.25).abs() < 1e-5,
        "brute force: expected 5.25, got {bf}"
    );
    assert!((d - 5.25).abs() < 1e-5, "apted: expected 5.25, got {d}");
}
