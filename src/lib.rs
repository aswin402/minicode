#![recursion_limit = "512"]
#![allow(
    clippy::manual_checked_ops,
    clippy::unnecessary_sort_by,
    clippy::collapsible_match,
    clippy::useless_borrows_in_formatting,
    clippy::question_mark
)]

pub mod agent;
pub mod app;
pub mod blocks;
pub mod config;
pub mod constants;
pub mod context;
pub mod dev;
pub mod error;
pub mod git;
pub mod logging;
pub mod lsp;
pub mod mcp;
pub mod sandbox;
pub mod security;
pub mod session;
pub mod tools;
pub mod ui;
pub mod utils;
pub mod vault;
