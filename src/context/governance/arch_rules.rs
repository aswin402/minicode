//! Declarative architectural boundary rules engine and layered dependency validator.

use crate::context::arch_parser::ImportReference;
use crate::context::layers::{ArchitecturalLayer, LayerClassifier};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Specific boundary violation details when an import crosses a prohibited layer boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryViolation {
    /// Relative path of the source file containing the invalid import.
    pub source_file: String,
    /// Architectural layer of the source file.
    pub source_layer: ArchitecturalLayer,
    /// Canonical target module that was illegally imported.
    pub target_module: String,
    /// Architectural layer of the target module.
    pub target_layer: ArchitecturalLayer,
    /// Machine-readable rule identifier (e.g. "R1_CORE_NO_UI").
    pub rule_id: String,
    /// Human-readable explanation of why this dependency is forbidden.
    pub message: String,
    /// Source line number where the violation occurred.
    pub line_number: usize,
}

/// Architectural boundary rule engine that validates dependency graphs against clean architecture axioms.
#[derive(Debug, Clone, Default)]
pub struct ArchRuleEngine;

impl ArchRuleEngine {
    /// Checks a single import reference against architectural boundary policies.
    #[must_use]
    pub fn check_import(
        &self,
        source_file: &str,
        import_ref: &ImportReference,
    ) -> Option<BoundaryViolation> {
        let source_path = Path::new(source_file);
        let target_path = Path::new(&import_ref.canonical_module);

        let source_layer = LayerClassifier::classify_path(source_path);
        let target_layer = LayerClassifier::classify_path(target_path);

        // Normalize paths for sub-path pattern checks
        let source_norm = source_file.replace('\\', "/");
        let target_norm = import_ref.canonical_module.replace('\\', "/");

        // Self-module or intra-layer imports are always permitted
        if source_norm.starts_with(&target_norm) || target_norm.starts_with(&source_norm) {
            return None;
        }

        // Rule R4: Provider Wire Protocol Isolation
        // Wire providers in src/agent/providers/ must only depend on networking/protocols, not UI, session, or app
        if source_norm.contains("src/agent/providers/")
            && (target_norm.contains("src/ui/")
                || target_norm.contains("src/app/")
                || target_norm.contains("src/session/"))
        {
            return Some(BoundaryViolation {
                source_file: source_file.to_string(),
                source_layer,
                target_module: import_ref.canonical_module.clone(),
                target_layer,
                rule_id: "R4_PROVIDER_ISOLATION".to_string(),
                message: format!(
                    "Provider Isolation: Wire protocol provider `{}` cannot depend on `{}`",
                    source_file, import_ref.canonical_module
                ),
                line_number: import_ref.line_number,
            });
        }

        // Rule R3: Utility Layer Purity
        // Fundamental utilities (strings, constants, errors) must be independent leaves and not import upper layers
        if source_layer == ArchitecturalLayer::Utility
            && (target_layer == ArchitecturalLayer::Ui
                || target_layer == ArchitecturalLayer::Service
                || target_layer == ArchitecturalLayer::Data)
        {
            return Some(BoundaryViolation {
                source_file: source_file.to_string(),
                source_layer,
                target_module: import_ref.canonical_module.clone(),
                target_layer,
                rule_id: "R3_UTILITY_PURITY".to_string(),
                message: format!(
                    "Utility Purity: Foundation utility `{}` cannot import upper-layer `{}`",
                    source_file, import_ref.canonical_module
                ),
                line_number: import_ref.line_number,
            });
        }

        // Rule R1: Presentation Layer Non-Inversion
        // No lower core layers (Service, Data) may depend on Presentation (UI, App, Main)
        if (source_layer == ArchitecturalLayer::Service || source_layer == ArchitecturalLayer::Data)
            && (target_layer == ArchitecturalLayer::Ui
                || target_norm.starts_with("src/ui")
                || target_norm.starts_with("src/app"))
        {
            return Some(BoundaryViolation {
                source_file: source_file.to_string(),
                source_layer,
                target_module: import_ref.canonical_module.clone(),
                target_layer,
                rule_id: "R1_CORE_NO_UI".to_string(),
                message: format!(
                    "Boundary Inversion: Core layer `{}` ({}) cannot import Presentation layer `{}`",
                    source_file,
                    source_layer.display_name(),
                    import_ref.canonical_module
                ),
                line_number: import_ref.line_number,
            });
        }

        // Rule R2: Data Layer Cannot Depend on Service/Agent Loop
        // Persistence / Data (session, git, context graph) must not call into interactive agent loop or mutating tools
        if source_layer == ArchitecturalLayer::Data
            && (target_layer == ArchitecturalLayer::Service
                || target_norm.starts_with("src/agent")
                || target_norm.starts_with("src/tools")
                || target_norm.starts_with("src/services")
                || target_norm.starts_with("src/service"))
        {
            // Allowed exception: Data types or interfaces in agent/types.rs or agent/models.rs
            if !target_norm.contains("types") && !target_norm.contains("models") {
                return Some(BoundaryViolation {
                    source_file: source_file.to_string(),
                    source_layer,
                    target_module: import_ref.canonical_module.clone(),
                    target_layer,
                    rule_id: "R2_DATA_NO_SERVICE".to_string(),
                    message: format!(
                        "Layer Violation: Data layer `{}` cannot depend on Service/Agent layer `{}`",
                        source_file, import_ref.canonical_module
                    ),
                    line_number: import_ref.line_number,
                });
            }
        }

        None
    }

    /// Evaluates all files in a workspace against architectural boundary rules.
    #[must_use]
    pub fn evaluate_workspace(
        &self,
        files_with_imports: &[(String, Vec<ImportReference>)],
    ) -> Vec<BoundaryViolation> {
        let mut violations = Vec::new();
        for (source_file, imports) in files_with_imports {
            for imp in imports {
                if let Some(violation) = self.check_import(source_file, imp) {
                    violations.push(violation);
                }
            }
        }
        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_r1_core_cannot_import_ui() {
        let engine = ArchRuleEngine;
        let imp = ImportReference {
            raw: "use crate::ui::view::render;".to_string(),
            canonical_module: "src/ui/view".to_string(),
            top_level_module: "src/ui".to_string(),
            line_number: 10,
        };
        let violation = engine.check_import("src/tools/exec.rs", &imp);
        assert!(violation.is_some());
        let v = violation.unwrap();
        assert_eq!(v.rule_id, "R1_CORE_NO_UI");
    }

    #[test]
    fn test_rule_r2_data_cannot_import_agent_loop() {
        let engine = ArchRuleEngine;
        let imp = ImportReference {
            raw: "use crate::agent::loop::AgentLoop;".to_string(),
            canonical_module: "src/agent/loop".to_string(),
            top_level_module: "src/agent".to_string(),
            line_number: 15,
        };
        let violation = engine.check_import("src/session/store.rs", &imp);
        assert!(violation.is_some());
        let v = violation.unwrap();
        assert_eq!(v.rule_id, "R2_DATA_NO_SERVICE");
    }

    #[test]
    fn test_rule_r4_provider_isolation() {
        let engine = ArchRuleEngine;
        let imp = ImportReference {
            raw: "use crate::ui::modal::render_modal;".to_string(),
            canonical_module: "src/ui/modal".to_string(),
            top_level_module: "src/ui".to_string(),
            line_number: 5,
        };
        let violation = engine.check_import("src/agent/providers/gemini.rs", &imp);
        assert!(violation.is_some());
        let v = violation.unwrap();
        assert_eq!(v.rule_id, "R4_PROVIDER_ISOLATION");
    }

    #[test]
    fn test_allowed_ui_imports_service() {
        let engine = ArchRuleEngine;
        let imp = ImportReference {
            raw: "use crate::agent::types::AgentEvent;".to_string(),
            canonical_module: "src/agent/types".to_string(),
            top_level_module: "src/agent".to_string(),
            line_number: 12,
        };
        let violation = engine.check_import("src/ui/view.rs", &imp);
        assert!(violation.is_none());
    }

    #[test]
    fn test_allowed_service_imports_utility() {
        let engine = ArchRuleEngine;
        let imp = ImportReference {
            raw: "use crate::utils::strings::truncate_chars;".to_string(),
            canonical_module: "src/utils/strings".to_string(),
            top_level_module: "src/utils".to_string(),
            line_number: 8,
        };
        let violation = engine.check_import("src/agent/loop.rs", &imp);
        assert!(violation.is_none());
    }
}
