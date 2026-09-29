//! Tree edit distance algorithms.

mod all_possible_mappings_ted;
mod apted;
mod matrix;

pub use all_possible_mappings_ted::AllPossibleMappingsTED;
pub use apted::APTED;
