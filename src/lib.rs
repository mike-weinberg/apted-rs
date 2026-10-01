#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cost_model;
pub mod distance;
pub mod error;
pub mod node;
pub mod parser;

pub use cost_model::{CostModel, PerEditOperationStringNodeDataCostModel, StringUnitCostModel};
#[doc(hidden)]
pub use distance::AllPossibleMappingsTED;
pub use distance::APTED;
pub use error::{estimated_peak_bytes, TedError};
pub use node::{Node, StringNodeData};
pub use parser::{BracketStringInputParser, ParseError};
