//! Modal dialog state management and rendering dispatch.

pub mod api_key;
pub mod architecture;
pub mod code_explorer;
pub mod command_catalog;
pub mod common;
pub mod exit_confirm;
pub mod git_diff;
pub mod help;
pub mod provider_select;
pub mod session_browser;
pub mod stack_select;
pub mod theme_select;
pub mod undo_checkpoint;
pub mod workspace_analysis;

#[allow(unused_imports)]
pub use command_catalog::{CommandCatalogItem, COMMAND_CATALOG_ITEMS};
#[allow(unused_imports)]
pub use common::{compute_scroll_offset, format_time_ago, TurnCheckpointInfo};

use crate::agent::models::ModelInfo;
use crate::ui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::Frame;

#[derive(Debug, Clone)]
pub enum ModalState {
    None,
    ApiKeyInput {
        provider: String,
        env_var: String,
        input: String,
        cursor: usize,
    },
    ProviderSelect {
        providers: Vec<String>,
        selected_index: usize,
    },
    ModelSelect {
        provider: String,
        models: Vec<ModelInfo>,
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
        loading: bool,
    },
    UndoCheckpoint {
        checkpoints: Vec<TurnCheckpointInfo>,
        selected_index: usize,
    },
    ThemeSelect {
        themes: Vec<crate::ui::theme::ThemeInfo>,
        selected_index: usize,
    },
    SessionBrowser {
        sessions: Vec<crate::session::store::SessionMetadata>,
        selected_index: usize,
        cached_summary: Option<crate::session::store::SessionSummary>,
    },
    StackSelect {
        stacks: Vec<crate::tools::onpkg::stacks::Stack>,
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
    },
    CommandCatalog {
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
    },
    CodeExplorer {
        symbols: Vec<crate::context::explorer::CodeExploreMatch>,
        filtered_indices: Vec<usize>,
        selected_index: usize,
        filter: String,
        active_tab: usize,
    },
    GitDiff {
        diff_files: Vec<crate::git::GitDiffFile>,
        selected_file_index: usize,
        scroll_offset: usize,
        staged_view: bool,
    },
    Help,
    StreamingSelect {
        selected_index: usize,
        current_streaming: bool,
    },
    Approval(crate::ui::approval::ApprovalModalState),
    WorkspaceAnalysis {
        workspace_path: String,
        is_indexed: bool,
        cached_symbols_count: usize,
        cached_files_count: usize,
        selected_index: usize,
    },
    ExitConfirm {
        workspace_name: String,
        selected_yes: bool,
    },
    ArchitectureAudit {
        report: Box<crate::context::governance::ArchitectureReport>,
        active_tab: usize,
        selected_index: usize,
        scroll_offset: usize,
    },
}

impl ModalState {
    pub fn is_active(&self) -> bool {
        !matches!(self, ModalState::None)
    }

    pub fn new_architecture_audit(report: crate::context::governance::ArchitectureReport) -> Self {
        Self::ArchitectureAudit {
            report: Box::new(report),
            active_tab: 0,
            selected_index: 0,
            scroll_offset: 0,
        }
    }

    pub fn new_exit_confirm(workspace_root: &std::path::Path) -> Self {
        let workspace_name = workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("minicode")
            .to_string();
        Self::ExitConfirm {
            workspace_name,
            selected_yes: false,
        }
    }

    pub fn new_api_key_input(provider: String, env_var: String, initial: Option<String>) -> Self {
        let input = initial.unwrap_or_default();
        let cursor = input.len();
        ModalState::ApiKeyInput {
            provider,
            env_var,
            input,
            cursor,
        }
    }

    pub fn new_provider_select() -> Self {
        let providers = vec![
            "anthropic".to_string(),
            "openrouter".to_string(),
            "gemini".to_string(),
            "openai".to_string(),
            "deepseek".to_string(),
            "groq".to_string(),
            "minimax".to_string(),
            "z.ai".to_string(),
            "together".to_string(),
            "mistral".to_string(),
            "ollama".to_string(),
            "lmstudio".to_string(),
            "vllm".to_string(),
            "localhost".to_string(),
        ];
        ModalState::ProviderSelect {
            providers,
            selected_index: 0,
        }
    }

    pub fn new_model_select(provider: String, models: Vec<ModelInfo>) -> Self {
        let count = models.len();
        ModalState::ModelSelect {
            provider,
            models,
            filtered_indices: (0..count).collect(),
            selected_index: 0,
            filter: String::new(),
            loading: false,
        }
    }

    pub fn new_undo_checkpoint(manifests: Vec<crate::session::backup::BackupManifest>) -> Self {
        let mut checkpoints = Vec::new();
        for (i, m) in manifests.into_iter().enumerate() {
            let time_ago = format_time_ago(&m.timestamp);
            let prompt = m
                .user_prompt
                .unwrap_or_else(|| format!("Turn #{}", m.turn_id));
            let files = m.files.into_iter().map(|f| f.original_path).collect();
            checkpoints.push(TurnCheckpointInfo {
                turn_id: m.turn_id,
                prompt,
                timestamp: m.timestamp,
                time_ago,
                files,
                is_latest: i == 0,
            });
        }
        ModalState::UndoCheckpoint {
            checkpoints,
            selected_index: 0,
        }
    }

    pub fn new_theme_select(active_theme_id: &str) -> Self {
        let themes = crate::ui::theme::Theme::list_themes();
        let selected_index = themes
            .iter()
            .position(|t| t.id == active_theme_id || active_theme_id.starts_with(&t.id))
            .unwrap_or(0);
        ModalState::ThemeSelect {
            themes,
            selected_index,
        }
    }

    pub fn new_session_browser(
        sessions: Vec<crate::session::store::SessionMetadata>,
        cached_summary: Option<crate::session::store::SessionSummary>,
    ) -> Self {
        ModalState::SessionBrowser {
            sessions,
            selected_index: 0,
            cached_summary,
        }
    }

    pub fn new_stack_select() -> Self {
        let stacks = crate::tools::onpkg::scaffolder::OnpkgScaffolder::get_all_stacks();
        let count = stacks.len();
        ModalState::StackSelect {
            stacks,
            filtered_indices: (0..count).collect(),
            selected_index: 0,
            filter: String::new(),
        }
    }

    pub fn new_command_catalog() -> Self {
        let count = COMMAND_CATALOG_ITEMS.len();
        ModalState::CommandCatalog {
            filtered_indices: (0..count).collect(),
            selected_index: 0,
            filter: String::new(),
        }
    }

    pub fn new_code_explorer(workspace_root: &std::path::Path) -> Self {
        let mut graph = crate::context::graph::CodeGraph::new();
        if let Err(e) = graph.build_graph(workspace_root) {
            tracing::warn!(error = %e, "Failed to build code graph for CodeExplorer modal");
        }
        let res = crate::context::explorer::CodeExploreEngine::explore(
            workspace_root,
            &graph,
            "",
            None,
            2,
            true,
        )
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to run AST explore for CodeExplorer modal");
            crate::context::explorer::CodeExploreResult {
                query: String::new(),
                target_symbol: None,
                matches: Vec::new(),
                summary: String::new(),
            }
        });
        let count = res.matches.len();
        ModalState::CodeExplorer {
            symbols: res.matches,
            filtered_indices: (0..count).collect(),
            selected_index: 0,
            filter: String::new(),
            active_tab: 0,
        }
    }

    pub fn new_git_diff(diff_files: Vec<crate::git::GitDiffFile>, staged_view: bool) -> Self {
        ModalState::GitDiff {
            diff_files,
            selected_file_index: 0,
            scroll_offset: 0,
            staged_view,
        }
    }

    pub fn update_filter(&mut self) {
        if let ModalState::ModelSelect {
            models,
            filtered_indices,
            selected_index,
            filter,
            ..
        } = self
        {
            let f = filter.trim().to_lowercase();
            *filtered_indices = models
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    if f.is_empty() {
                        true
                    } else {
                        Self::contains_ci(&m.id, &f) || Self::contains_ci(&m.name, &f)
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        } else if let ModalState::StackSelect {
            stacks,
            filtered_indices,
            selected_index,
            filter,
        } = self
        {
            let f = filter.trim().to_lowercase();
            *filtered_indices = stacks
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    if f.is_empty() {
                        true
                    } else {
                        Self::contains_ci(&s.name, &f)
                            || Self::contains_ci(&s.runtime, &f)
                            || Self::contains_ci(&s.description, &f)
                            || s.packages.iter().any(|p| Self::contains_ci(p, &f))
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        } else if let ModalState::CommandCatalog {
            filtered_indices,
            selected_index,
            filter,
        } = self
        {
            let f = filter.trim().to_lowercase();
            *filtered_indices = COMMAND_CATALOG_ITEMS
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    if f.is_empty() {
                        true
                    } else {
                        Self::contains_ci(c.name, &f)
                            || Self::contains_ci(c.category, &f)
                            || Self::contains_ci(c.description, &f)
                            || Self::contains_ci(c.shortcut, &f)
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        } else if let ModalState::CodeExplorer {
            symbols,
            filtered_indices,
            selected_index,
            filter,
            ..
        } = self
        {
            let f = filter.trim().to_lowercase();
            *filtered_indices = symbols
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    if f.is_empty() {
                        true
                    } else {
                        Self::contains_ci(&s.symbol_name, &f)
                            || Self::contains_ci(&s.kind, &f)
                            || Self::contains_ci(&s.file_path, &f)
                            || Self::contains_ci(&s.signature, &f)
                            || Self::contains_ci(s.layer.display_name(), &f)
                    }
                })
                .map(|(i, _)| i)
                .collect();

            if *selected_index >= filtered_indices.len() {
                *selected_index = filtered_indices.len().saturating_sub(1);
            }
        }
    }

    pub fn new_streaming_select(current_streaming: bool) -> Self {
        let selected_index = if current_streaming { 0 } else { 1 };
        ModalState::StreamingSelect {
            selected_index,
            current_streaming,
        }
    }

    pub fn new_workspace_analysis(workspace_root: &std::path::Path) -> Self {
        let graph_file = crate::context::graph_store::GraphStore::graph_file_path(workspace_root);
        let is_indexed = graph_file.exists();
        let (cached_symbols_count, cached_files_count) = if is_indexed {
            if let Ok(Some(snap)) =
                crate::context::graph_store::GraphStore::load_snapshot(workspace_root)
            {
                let sym_count = snap
                    .nodes
                    .iter()
                    .filter(|n| n.kind != crate::context::graph::SymbolKind::File)
                    .count();
                let file_count = snap.file_hashes.len();
                (sym_count, file_count)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        let workspace_path = if let Ok(home) = std::env::var("HOME") {
            let p_str = workspace_root.display().to_string();
            if let Some(rest) = p_str.strip_prefix(&home) {
                format!("~{}", rest)
            } else {
                p_str
            }
        } else {
            workspace_root.display().to_string()
        };

        ModalState::WorkspaceAnalysis {
            workspace_path,
            is_indexed,
            cached_symbols_count,
            cached_files_count,
            selected_index: 0,
        }
    }

    /// Efficient zero-allocation case-insensitive substring search for ASCII text.
    fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
        if needle_lower.is_empty() {
            return true;
        }
        if haystack.len() < needle_lower.len() {
            return false;
        }
        if haystack.is_ascii() && needle_lower.is_ascii() {
            let needle_bytes = needle_lower.as_bytes();
            haystack.as_bytes().windows(needle_bytes.len()).any(|w| {
                w.iter()
                    .zip(needle_bytes.iter())
                    .all(|(a, b)| a.to_ascii_lowercase() == *b)
            })
        } else {
            haystack.to_lowercase().contains(needle_lower)
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        match self {
            ModalState::None => {}
            ModalState::ApiKeyInput {
                provider,
                env_var,
                input,
                ..
            } => {
                api_key::render_api_key_input(frame, area, theme, provider, env_var, input);
            }
            ModalState::ProviderSelect {
                providers,
                selected_index,
            } => {
                provider_select::render_provider_select(
                    frame,
                    area,
                    theme,
                    providers,
                    *selected_index,
                );
            }
            ModalState::ModelSelect {
                provider,
                models,
                filtered_indices,
                selected_index,
                filter,
                loading,
            } => {
                provider_select::render_model_select(
                    frame,
                    area,
                    theme,
                    provider,
                    models,
                    filtered_indices,
                    *selected_index,
                    filter,
                    *loading,
                );
            }
            ModalState::UndoCheckpoint {
                checkpoints,
                selected_index,
            } => {
                undo_checkpoint::render_undo_checkpoint(
                    frame,
                    area,
                    theme,
                    checkpoints,
                    *selected_index,
                );
            }
            ModalState::ThemeSelect {
                themes,
                selected_index,
            } => {
                theme_select::render_theme_select(frame, area, theme, themes, *selected_index);
            }
            ModalState::SessionBrowser {
                sessions,
                selected_index,
                cached_summary,
            } => {
                session_browser::render_session_browser(
                    frame,
                    area,
                    theme,
                    sessions,
                    *selected_index,
                    cached_summary.as_ref(),
                );
            }
            ModalState::StreamingSelect {
                selected_index,
                current_streaming,
            } => {
                theme_select::render_streaming_select(
                    frame,
                    area,
                    theme,
                    *selected_index,
                    *current_streaming,
                );
            }
            ModalState::Help => {
                help::render_help(frame, area, theme);
            }
            ModalState::StackSelect {
                stacks,
                filtered_indices,
                selected_index,
                filter,
            } => {
                stack_select::render_stack_select(
                    frame,
                    area,
                    theme,
                    stacks,
                    filtered_indices,
                    *selected_index,
                    filter,
                );
            }
            ModalState::CommandCatalog {
                filtered_indices,
                selected_index,
                filter,
            } => {
                command_catalog::render_command_catalog(
                    frame,
                    area,
                    theme,
                    filtered_indices,
                    *selected_index,
                    filter,
                );
            }
            ModalState::GitDiff {
                diff_files,
                selected_file_index,
                scroll_offset,
                staged_view,
            } => {
                git_diff::render_git_diff(
                    frame,
                    area,
                    theme,
                    diff_files,
                    *selected_file_index,
                    *scroll_offset,
                    *staged_view,
                );
            }
            ModalState::CodeExplorer {
                symbols,
                filtered_indices,
                selected_index,
                filter,
                active_tab,
            } => {
                code_explorer::render_code_explorer(
                    frame,
                    area,
                    theme,
                    symbols,
                    filtered_indices,
                    *selected_index,
                    filter,
                    *active_tab,
                );
            }
            ModalState::Approval(approval_state) => {
                approval_state.render(frame, area, theme);
            }
            ModalState::WorkspaceAnalysis {
                workspace_path,
                is_indexed,
                cached_symbols_count,
                cached_files_count,
                selected_index,
            } => {
                workspace_analysis::render_workspace_analysis(
                    frame,
                    area,
                    theme,
                    workspace_path,
                    *is_indexed,
                    *cached_symbols_count,
                    *cached_files_count,
                    *selected_index,
                );
            }
            ModalState::ExitConfirm {
                workspace_name,
                selected_yes,
            } => {
                exit_confirm::render_exit_confirm(
                    frame,
                    area,
                    theme,
                    workspace_name,
                    *selected_yes,
                );
            }
            ModalState::ArchitectureAudit {
                report,
                active_tab,
                selected_index,
                scroll_offset,
            } => {
                architecture::render_architecture_audit(
                    frame,
                    area,
                    theme,
                    report,
                    *active_tab,
                    *selected_index,
                    *scroll_offset,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::TempDir;

    #[test]
    fn test_new_workspace_analysis_unindexed() {
        let temp_dir = TempDir::new().unwrap();
        let modal = ModalState::new_workspace_analysis(temp_dir.path());
        match modal {
            ModalState::WorkspaceAnalysis {
                is_indexed,
                selected_index,
                ..
            } => {
                assert!(!is_indexed);
                assert_eq!(selected_index, 0);
            }
            _ => panic!("Expected WorkspaceAnalysis variant"),
        }
    }

    #[test]
    fn test_workspace_analysis_render_without_panic() {
        let temp_dir = TempDir::new().unwrap();
        let modal = ModalState::new_workspace_analysis(temp_dir.path());
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_exit_confirm_initial_state_and_render() {
        let temp_dir = TempDir::new().unwrap();
        let modal = ModalState::new_exit_confirm(temp_dir.path());
        match modal {
            ModalState::ExitConfirm {
                selected_yes,
                ref workspace_name,
            } => {
                assert!(!selected_yes, "Default should be Nope for safety");
                assert!(!workspace_name.is_empty());
            }
            _ => panic!("Expected ExitConfirm variant"),
        }

        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();

        let modal_yes = ModalState::ExitConfirm {
            workspace_name: "test_ws".to_string(),
            selected_yes: true,
        };
        terminal
            .draw(|f| {
                let area = f.area();
                modal_yes.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_session_browser_render() {
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Test empty
        let empty_modal = ModalState::SessionBrowser {
            sessions: vec![],
            selected_index: 0,
            cached_summary: None,
        };
        terminal
            .draw(|f| {
                let area = f.area();
                empty_modal.render(f, area, &theme);
            })
            .unwrap();

        // Test with sessions
        let meta1 = crate::session::store::SessionMetadata {
            id: "20260831T142903Z-10f7a7e0".to_string(),
            created_at: "2026-08-31T14:29:03Z".to_string(),
            workspace: "minicode".to_string(),
            path: "/tmp/s1.jsonl".to_string(),
            event_count: 42,
            preview: "Fix permission asking modal responsive layout".to_string(),
        };
        let meta2 = crate::session::store::SessionMetadata {
            id: "20260831T142845Z-20a8b9c1".to_string(),
            created_at: "2026-08-31T14:28:45Z".to_string(),
            workspace: "minicode".to_string(),
            path: "/tmp/s2.jsonl".to_string(),
            event_count: 160,
            preview: "Implement floating spotlight command palette".to_string(),
        };

        let populated_modal = ModalState::SessionBrowser {
            sessions: vec![meta1, meta2],
            selected_index: 0,
            cached_summary: None,
        };
        terminal
            .draw(|f| {
                let area = f.area();
                populated_modal.render(f, area, &theme);
            })
            .unwrap();
    }

    #[test]
    fn test_architecture_audit_render() {
        let theme = Theme::default();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let report = crate::context::governance::ArchitectureReport {
            health_score: 95,
            total_files: 12,
            total_loc: 1500,
            circular_cycles: vec![vec!["mod_a".into(), "mod_b".into()]],
            layer_violations: vec![],
            coupling_metrics: vec![],
            god_files: vec![],
            fan_out_spikes: vec![],
        };

        let modal = ModalState::new_architecture_audit(report);
        terminal
            .draw(|f| {
                let area = f.area();
                modal.render(f, area, &theme);
            })
            .unwrap();
    }
}
