//! Rust port of APTED (All Path Tree Edit Distance) by Mateusz Pawlik and
//! Nikolaus Augsten. Ported from the canonical Java implementation at
//! <https://github.com/DatabaseGroup/apted> (MIT License).
//!
//! References:
//! - M. Pawlik and N. Augsten. Efficient Computation of the Tree Edit
//!   Distance. ACM Transactions on Database Systems (TODS) 40(1). 2015.
//! - M. Pawlik and N. Augsten. Tree edit distance: Robust and memory-
//!   efficient. Information Systems 56. 2016.

#![forbid(unsafe_code)]

pub mod cost_model;
pub mod distance;
pub mod error;
pub mod node;
pub mod parser;

pub use cost_model::{CostModel, PerEditOperationStringNodeDataCostModel, StringUnitCostModel};
pub use distance::{AllPossibleMappingsTED, APTED};
pub use error::{estimated_peak_bytes, TedError};
pub use node::{Node, NodeIndexer, StringNodeData};
pub use parser::{BracketStringInputParser, ParseError};
