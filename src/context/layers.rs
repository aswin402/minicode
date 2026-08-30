use serde::{Deserialize, Serialize};
use std::path::Path;

/// Core architectural tiers for classifying codebase components and symbols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitecturalLayer {
    Ui,
    Api,
    Service,
    Data,
    Utility,
}

impl ArchitecturalLayer {
    /// Returns the human-readable display title for this architectural layer
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Ui => "UI & Presentation",
            Self::Api => "API & Protocols",
            Self::Service => "Core Services & Agent",
            Self::Data => "Data & Persistence",
            Self::Utility => "Utilities & Support",
        }
    }

    /// Returns a short icon/emoji badge for terminal and TUI rendering
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Ui => "🎨 UI",
            Self::Api => "🌐 API",
            Self::Service => "⚙️ Service",
            Self::Data => "💾 Data",
            Self::Utility => "🔧 Utility",
        }
    }

    /// Returns a brief description of what lives in this layer
    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Ui => "Terminal views, components, widgets, modals, and user presentation",
            Self::Api => "CLI entrypoints, HTTP routes, JSON-RPC streaming, and protocol handlers",
            Self::Service => {
                "Agent execution loop, tool orchestrators, background workers, and business logic"
            }
            Self::Data => "Session store, AST graphs, state models, repositories, and cache layers",
            Self::Utility => {
                "Error handling, constants, string utilities, and cross-cutting helpers"
            }
        }
    }
}

/// Classifier engine that categorizes file paths and AST symbols into architectural layers
pub struct LayerClassifier;

impl LayerClassifier {
    /// Classifies a file path into its primary architectural layer based on path conventions and file extensions
    pub fn classify_path(path: &Path) -> ArchitecturalLayer {
        let path_str = path.to_string_lossy().to_lowercase();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        // 1. UI & Presentation
        if path_str.contains("/ui/")
            || path_str.contains("/views/")
            || path_str.contains("/components/")
            || path_str.contains("/templates/")
            || path_str.contains("/widgets/")
            || file_name.ends_with(".tsx")
            || file_name.ends_with(".jsx")
            || file_name.ends_with(".vue")
            || file_name.ends_with(".svelte")
            || file_name.ends_with(".css")
            || file_name.ends_with(".scss")
            || file_name == "view.rs"
            || file_name == "modal.rs"
            || file_name == "input.rs"
            || file_name == "theme.rs"
            || file_name == "diff_view.rs"
        {
            return ArchitecturalLayer::Ui;
        }

        // 2. API & Protocols
        if path_str.contains("/api/")
            || path_str.contains("/routes/")
            || path_str.contains("/controllers/")
            || path_str.contains("/endpoints/")
            || path_str.contains("/server/")
            || path_str.contains("/rpc/")
            || file_name == "main.rs"
            || file_name == "server.rs"
            || file_name == "routes.rs"
            || file_name == "protocol.rs"
            || path_str.contains("/protocol/")
        {
            return ArchitecturalLayer::Api;
        }

        // 3. Data & Persistence
        if path_str.contains("/models/")
            || path_str.contains("/db/")
            || path_str.contains("/database/")
            || path_str.contains("/schema/")
            || path_str.contains("/session/")
            || path_str.contains("/store/")
            || path_str.contains("/storage/")
            || path_str.contains("/memory/")
            || path_str.contains("/repository/")
            || file_name == "store.rs"
            || file_name == "db.rs"
            || file_name == "schema.rs"
            || file_name == "graph.rs"
            || file_name == "repomap.rs"
        {
            return ArchitecturalLayer::Data;
        }

        // 4. Utility & Support
        if path_str.contains("/utils/")
            || path_str.contains("/util/")
            || path_str.contains("/helpers/")
            || path_str.contains("/common/")
            || file_name == "error.rs"
            || file_name == "constants.rs"
            || file_name == "types.rs"
            || file_name == "config.rs"
            || file_name == "format.rs"
        {
            return ArchitecturalLayer::Utility;
        }

        // 5. Default to Service (Core logic, agent loop, tools, scaffolder)
        ArchitecturalLayer::Service
    }

    /// Classifies an individual symbol taking into account both its declaring file and symbol attributes
    pub fn classify_symbol(path: &Path, symbol_name: &str, _kind: &str) -> ArchitecturalLayer {
        let sym_lower = symbol_name.to_lowercase();

        // Specific symbol name overrides
        if sym_lower.contains("widget")
            || sym_lower.contains("view")
            || sym_lower.contains("render")
            || sym_lower.contains("modal")
            || sym_lower.contains("component")
        {
            return ArchitecturalLayer::Ui;
        }

        if sym_lower.contains("route")
            || sym_lower.contains("handler")
            || sym_lower.contains("endpoint")
            || sym_lower.contains("api")
            || sym_lower.contains("rpc")
            || sym_lower.contains("webhook")
        {
            return ArchitecturalLayer::Api;
        }

        if sym_lower.ends_with("error")
            || sym_lower.contains("helper")
            || sym_lower.contains("format_")
            || sym_lower.contains("constant")
            || sym_lower.contains("util")
        {
            return ArchitecturalLayer::Utility;
        }

        if sym_lower.contains("store")
            || sym_lower.contains("model")
            || sym_lower.contains("schema")
            || sym_lower.contains("table")
            || sym_lower.contains("record")
            || sym_lower.contains("entity")
            || sym_lower.contains("query")
        {
            return ArchitecturalLayer::Data;
        }

        // Fallback to the file's primary layer
        Self::classify_path(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_layer_classification() {
        assert_eq!(
            LayerClassifier::classify_path(&PathBuf::from("src/ui/modal.rs")),
            ArchitecturalLayer::Ui
        );
        assert_eq!(
            LayerClassifier::classify_path(&PathBuf::from("src/main.rs")),
            ArchitecturalLayer::Api
        );
        assert_eq!(
            LayerClassifier::classify_path(&PathBuf::from("src/session/store.rs")),
            ArchitecturalLayer::Data
        );
        assert_eq!(
            LayerClassifier::classify_path(&PathBuf::from("src/error.rs")),
            ArchitecturalLayer::Utility
        );
        assert_eq!(
            LayerClassifier::classify_path(&PathBuf::from("src/agent/loop.rs")),
            ArchitecturalLayer::Service
        );
    }
}
