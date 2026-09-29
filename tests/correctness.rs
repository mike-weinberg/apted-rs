//! Port of `CorrectnessTest.java`: correctness of distance and mapping
//! computation for the unit-cost model and single string-value labels. For
//! the mapping, only its cost is verified against the correct distance.

mod common;

use apted::{BracketStringInputParser, StringUnitCostModel, APTED};
use common::{expect_eq, for_each_case};

const CASES: &str = "correctness_test_cases.json";

/// Parse trees from bracket notation, convert back to strings and verify
/// equality with the input.
#[test]
fn parsing_bracket_notation_to_string_node_data() {
    for_each_case(CASES, |tc| {
        let parser = BracketStringInputParser::new();
        let t1 = parser.from_string(&tc.t1);
        let t2 = parser.from_string(&tc.t2);
        expect_eq(tc.t1.clone(), t1.to_string(), "t1")?;
        expect_eq(tc.t2.clone(), t2.to_string(), "t2")
    });
}

/// Compute TED and compare to the correct value, in both directions.
#[test]
fn distance_unit_cost_string_node_data_cost_model() {
    for_each_case(CASES, |tc| {
        let parser = BracketStringInputParser::new();
        let t1 = parser.from_string(&tc.t1);
        let t2 = parser.from_string(&tc.t2);
        let mut apted = APTED::new(StringUnitCostModel);
        // This cast is safe due to unit cost.
        let result = apted.compute_edit_distance(&t1, &t2) as i32;
        expect_eq(tc.d, result, "d(t1,t2)")?;
        // Verify the symmetric case.
        let result = apted.compute_edit_distance(&t2, &t1) as i32;
        expect_eq(tc.d, result, "d(t2,t1)")
    });
}

/// Compute TED with the strategy fixed to left paths in the left-hand tree,
/// which triggers spf_L.
#[test]
fn distance_unit_cost_string_node_data_cost_model_spf_l() {
    for_each_case(CASES, |tc| {
        let parser = BracketStringInputParser::new();
        let t1 = parser.from_string(&tc.t1);
        let t2 = parser.from_string(&tc.t2);
        let mut apted = APTED::new(StringUnitCostModel);
        let result = apted.compute_edit_distance_spf_test(&t1, &t2, 0) as i32;
        expect_eq(tc.d, result, "d")
    });
}

/// Compute TED with the strategy fixed to right paths in the left-hand tree,
/// which triggers spf_R.
#[test]
fn distance_unit_cost_string_node_data_cost_model_spf_r() {
    for_each_case(CASES, |tc| {
        let parser = BracketStringInputParser::new();
        let t1 = parser.from_string(&tc.t1);
        let t2 = parser.from_string(&tc.t2);
        let mut apted = APTED::new(StringUnitCostModel);
        let result = apted.compute_edit_distance_spf_test(&t1, &t2, 1) as i32;
        expect_eq(tc.d, result, "d")
    });
}

/// Compute the minimum-cost edit mapping and compare its cost to the correct
/// TED value.
#[test]
fn mapping_cost_unit_cost_string_node_data_cost_model() {
    for_each_case(CASES, |tc| {
        let parser = BracketStringInputParser::new();
        let t1 = parser.from_string(&tc.t1);
        let t2 = parser.from_string(&tc.t2);
        let mut apted = APTED::new(StringUnitCostModel);
        // TED must be computed before the mapping.
        apted.compute_edit_distance(&t1, &t2);
        let mapping = apted.compute_edit_mapping();
        let result = apted.mapping_cost(&mapping) as i32;
        expect_eq(tc.d, result, "mapping cost")
    });
}
