#[allow(dead_code)]
pub mod guard;
#[allow(dead_code)]
pub mod selector;

#[allow(unused_imports)]
pub use guard::TerminalGuard;
#[allow(unused_imports)]
pub use selector::{next_index, prev_index, InteractiveSelector, SelectorItem};
