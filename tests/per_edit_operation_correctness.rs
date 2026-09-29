//! Port of `PerEditOperationCorrectnessTest.java`: distance for single
//! string-value labels and a per-edit-operation cost model, checked against
//! the brute-force AllPossibleMappingsTED.

mod common;

use apted::{
    AllPossibleMappingsTED, BracketStringInputParser, PerEditOperationStringNodeDataCostModel,
    APTED,
};
use common::for_each_case;

/// Costs differ from the unit cost model on purpose.
fn cost_model() -> PerEditOperationStringNodeDataCostModel {
    PerEditOperationStringNodeDataCostModel::new(0.4, 0.4, 0.6)
}

#[test]
fn distance_per_edit_operation_string_node_data_cost_model() {
    for_each_case("mini.json", |tc| {
        let parser = BracketStringInputParser::new();
        let t1 = parser.from_string(&tc.t1);
        let t2 = parser.from_string(&tc.t2);
        let result = APTED::new(cost_model()).compute_edit_distance(&t1, &t2);
        let correct = AllPossibleMappingsTED::new(cost_model()).compute_edit_distance(&t1, &t2);
        if (correct - result).abs() <= 0.0001 {
            Ok(())
        } else {
            Err(format!("expected {correct}, got {result}"))
        }
    });
}
