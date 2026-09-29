//! Cost models for edit operations.

use crate::node::{Node, StringNodeData};

/// Costs of the three edit operations. Mirrors `costmodel.CostModel`.
pub trait CostModel<D> {
    /// Cost of deleting node `n`.
    fn del(&self, n: &Node<D>) -> f32;
    /// Cost of inserting node `n`.
    fn ins(&self, n: &Node<D>) -> f32;
    /// Cost of renaming node `n1` to `n2`.
    fn ren(&self, n1: &Node<D>, n2: &Node<D>) -> f32;
}

/// Unit cost model for string labels: deletion and insertion cost 1, rename
/// costs 1 for different labels and 0 for equal labels.
#[derive(Debug, Clone, Copy, Default)]
pub struct StringUnitCostModel;

impl CostModel<StringNodeData> for StringUnitCostModel {
    fn del(&self, _n: &Node<StringNodeData>) -> f32 {
        1.0
    }
    fn ins(&self, _n: &Node<StringNodeData>) -> f32 {
        1.0
    }
    fn ren(&self, n1: &Node<StringNodeData>, n2: &Node<StringNodeData>) -> f32 {
        if n1.node_data().same_label(n2.node_data()) {
            0.0
        } else {
            1.0
        }
    }
}

/// Cost model with a fixed cost per edit operation for string labels.
#[derive(Debug, Clone, Copy)]
pub struct PerEditOperationStringNodeDataCostModel {
    del_cost: f32,
    ins_cost: f32,
    ren_cost: f32,
}

impl PerEditOperationStringNodeDataCostModel {
    pub fn new(del_cost: f32, ins_cost: f32, ren_cost: f32) -> Self {
        Self {
            del_cost,
            ins_cost,
            ren_cost,
        }
    }
}

impl CostModel<StringNodeData> for PerEditOperationStringNodeDataCostModel {
    fn del(&self, _n: &Node<StringNodeData>) -> f32 {
        self.del_cost
    }
    fn ins(&self, _n: &Node<StringNodeData>) -> f32 {
        self.ins_cost
    }
    fn ren(&self, n1: &Node<StringNodeData>, n2: &Node<StringNodeData>) -> f32 {
        if n1.node_data().same_label(n2.node_data()) {
            0.0
        } else {
            self.ren_cost
        }
    }
}
