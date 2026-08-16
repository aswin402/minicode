pub mod client;
pub mod diagnostics;
pub mod protocol;

pub use client::{LspClient, LspLocation};
pub use diagnostics::{DiagnosticItem, DiagnosticReport, FastCompilerChecker};

use crate::error::Result;
use std::path::Path;

/// High-level engine orchestrating 2-tier compiler diagnostics and LSP code navigation.
pub struct LspEngine;

impl LspEngine {
    /// Executes fast compiler and linter diagnostics across the workspace.
    pub async fn run_diagnostics(workspace_root: &Path) -> Result<DiagnosticReport> {
        FastCompilerChecker::check_workspace(workspace_root).await
    }

    /// Resolves definition location using active LSP server if available.
    pub async fn goto_definition(
        workspace_root: &Path,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>> {
        if let Some(client) = LspClient::auto_detect(workspace_root).await {
            client.goto_definition(file_path, line, character).await
        } else {
            Ok(Vec::new())
        }
    }

    /// Finds references across the workspace using active LSP server if available.
    pub async fn find_references(
        workspace_root: &Path,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>> {
        if let Some(client) = LspClient::auto_detect(workspace_root).await {
            client.find_references(file_path, line, character).await
        } else {
            Ok(Vec::new())
        }
    }
}
