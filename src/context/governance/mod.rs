pub mod arch_parser;
pub mod arch_rules;
pub mod dead_code;
pub mod doc_synthesizer;
pub mod dox;
#[allow(clippy::module_inception)]
pub mod governance;
pub mod invariants;
pub mod smell_detector;
pub mod test_gap;

pub use governance::*;
