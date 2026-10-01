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

#[inline]
fn contains_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|&p| text.contains(p))
}

#[inline]
fn ends_with_any(text: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|&s| text.ends_with(s))
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
        const UI_PATHS: &[&str] = &[
            "/ui/",
            "/presentation/",
            "/views/",
            "/components/",
            "/templates/",
            "/widgets/",
        ];
        const UI_EXTS: &[&str] = &[".tsx", ".jsx", ".vue", ".svelte", ".css", ".scss"];
        const UI_FILES: &[&str] = &[
            "view.rs",
            "modal.rs",
            "input.rs",
            "theme.rs",
            "diff_view.rs",
        ];

        if contains_any(&path_str, UI_PATHS)
            || ends_with_any(&file_name, UI_EXTS)
            || UI_FILES.contains(&file_name.as_str())
        {
            return ArchitecturalLayer::Ui;
        }

        // 2. API & Protocols
        const API_PATHS: &[&str] = &[
            "/api/",
            "/routes/",
            "/controllers/",
            "/endpoints/",
            "/server/",
            "/rpc/",
            "/protocol/",
        ];
        const API_FILES: &[&str] = &["main.rs", "server.rs", "routes.rs", "protocol.rs"];

        if contains_any(&path_str, API_PATHS) || API_FILES.contains(&file_name.as_str()) {
            return ArchitecturalLayer::Api;
        }

        // 3. Data & Persistence
        const DATA_PATHS: &[&str] = &[
            "/data/",
            "/models/",
            "/db/",
            "/database/",
            "/schema/",
            "/session/",
            "/store/",
            "/storage/",
            "/memory/",
            "/repository/",
        ];
        const DATA_FILES: &[&str] = &["store.rs", "db.rs", "schema.rs", "graph.rs", "repomap.rs"];

        if contains_any(&path_str, DATA_PATHS) || DATA_FILES.contains(&file_name.as_str()) {
            return ArchitecturalLayer::Data;
        }

        // 4. Utility & Support
        const UTIL_PATHS: &[&str] = &["/utils/", "/util/", "/helpers/", "/common/"];
        const UTIL_FILES: &[&str] = &[
            "error.rs",
            "constants.rs",
            "types.rs",
            "config.rs",
            "format.rs",
        ];

        if contains_any(&path_str, UTIL_PATHS) || UTIL_FILES.contains(&file_name.as_str()) {
            return ArchitecturalLayer::Utility;
        }

        // 5. Default to Service (Core logic, agent loop, tools, scaffolder)
        ArchitecturalLayer::Service
    }

    /// Classifies an individual symbol taking into account both its declaring file and symbol attributes
    pub fn classify_symbol(path: &Path, symbol_name: &str, _kind: &str) -> ArchitecturalLayer {
        let sym_lower = symbol_name.to_lowercase();

        // Specific symbol name overrides
        const UI_SYMS: &[&str] = &["widget", "view", "render", "modal", "component"];
        if contains_any(&sym_lower, UI_SYMS) {
            return ArchitecturalLayer::Ui;
        }

        const API_SYMS: &[&str] = &["route", "handler", "endpoint", "api", "rpc", "webhook"];
        if contains_any(&sym_lower, API_SYMS) {
            return ArchitecturalLayer::Api;
        }

        const UTIL_SYMS: &[&str] = &["helper", "format_", "constant", "util"];
        if sym_lower.ends_with("error") || contains_any(&sym_lower, UTIL_SYMS) {
            return ArchitecturalLayer::Utility;
        }

        const DATA_SYMS: &[&str] = &[
            "store", "model", "schema", "table", "record", "entity", "query",
        ];
        if contains_any(&sym_lower, DATA_SYMS) {
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

    #[test]
    fn test_symbol_classification() {
        let p = PathBuf::from("src/arbitrary.rs");
        assert_eq!(
            LayerClassifier::classify_symbol(&p, "render_card_view", "fn"),
            ArchitecturalLayer::Ui
        );
        assert_eq!(
            LayerClassifier::classify_symbol(&p, "handle_login_endpoint", "fn"),
            ArchitecturalLayer::Api
        );
        assert_eq!(
            LayerClassifier::classify_symbol(&p, "user_account_schema", "struct"),
            ArchitecturalLayer::Data
        );
        assert_eq!(
            LayerClassifier::classify_symbol(&p, "format_iso_time", "fn"),
            ArchitecturalLayer::Utility
        );
    }
}
