//! Integration tests for the interactive /context visualizer & KV-cache diagnostics modal.

use minicode::config::Config;
use minicode::context::budget::ccr_cache::CcrCache;
use minicode::context::memory::progressive_memory::{
    MemoryTier, ProgressiveMemory, ProgressiveMemoryEntry,
};
use minicode::ui::modals::context_diagnostics::{
    render_context_diagnostics, ContextDiagnosticsData,
};
use minicode::ui::modals::ModalState;
use minicode::ui::theme::Theme;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tempfile::TempDir;

#[test]
fn test_context_diagnostics_data_gather() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path();

    // Create a dummy DOX rules file
    let agents_md = workspace.join("AGENTS.md");
    std::fs::write(&agents_md, "# Test Rules\nFollow Rust conventions.\n").unwrap();

    // Store something in CCR cache
    let ccr_id = CcrCache::store("Sample tool observation output for test");
    assert!(!ccr_id.is_empty());

    let mut config = Config::default();
    config.provider.default = "anthropic".to_string();
    config.provider.model = "claude-3-5-sonnet".to_string();
    config.provider.context_window = Some(200_000);

    let data = ContextDiagnosticsData::gather(workspace, &config, 50_000, 150_000, 35_000, 12);

    assert_eq!(data.model_name, "claude-3-5-sonnet");
    assert_eq!(data.provider_name, "anthropic");
    assert_eq!(data.context_limit, 200_000);
    assert_eq!(data.used_tokens, 50_000);
    assert_eq!(data.headroom_tokens, 150_000);
    assert_eq!(data.utilization_pct, 25.0);
    assert_eq!(data.cached_tokens, 35_000);
    assert!((data.cache_hit_pct - 70.0).abs() < 0.1);
    assert!(data.provider_cache_type.contains("Anthropic"));
    assert!(data.ttft_savings.contains("Faster"));
    assert!(data.cost_savings.contains("Discount"));
    assert!(data.dox_rules_files.contains(&"AGENTS.md".to_string()));
    assert_eq!(data.conversation_messages_count, 12);
    assert!(data.ccr_entries_count >= 1);
    assert!(data.ccr_total_bytes > 0);

    // Clean up CCR cache
    CcrCache::clear();
}

#[test]
fn test_context_modal_render_all_tabs() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path();

    let config = Config::default();
    let data = ContextDiagnosticsData::gather(workspace, &config, 25_000, 50_000, 10_000, 8);

    let theme = Theme::default();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    // Render Tab 0: Token Budget & Allocation
    terminal
        .draw(|f| {
            render_context_diagnostics(f, f.area(), &theme, &data, 0, 0);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let mut rendered_tab0 = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            rendered_tab0.push_str(buffer[(x, y)].symbol());
        }
        rendered_tab0.push('\n');
    }
    assert!(rendered_tab0.contains("Context Visualizer"));
    assert!(rendered_tab0.contains("Token Budget"));
    assert!(rendered_tab0.contains("Window Used"));

    // Render Tab 1: KV-Cache Diagnostics
    terminal
        .draw(|f| {
            render_context_diagnostics(f, f.area(), &theme, &data, 1, 0);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let mut rendered_tab1 = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            rendered_tab1.push_str(buffer[(x, y)].symbol());
        }
        rendered_tab1.push('\n');
    }
    assert!(rendered_tab1.contains("KV-Cache"));
    assert!(rendered_tab1.contains("Hit Rate"));

    // Render Tab 2: Memory & Storage
    terminal
        .draw(|f| {
            render_context_diagnostics(f, f.area(), &theme, &data, 2, 0);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let mut rendered_tab2 = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            rendered_tab2.push_str(buffer[(x, y)].symbol());
        }
        rendered_tab2.push('\n');
    }
    assert!(rendered_tab2.contains("Progressive Memory"));
    assert!(rendered_tab2.contains("Prune"));
}

#[test]
fn test_modal_state_context_diagnostics_lifecycle() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path();
    let config = Config::default();

    let data = ContextDiagnosticsData::gather(workspace, &config, 1000, 1000, 0, 2);

    let modal = ModalState::new_context_diagnostics(data);
    assert!(modal.is_active());

    if let ModalState::ContextDiagnostics {
        data: d,
        active_tab,
        scroll_offset,
    } = modal
    {
        assert_eq!(active_tab, 0);
        assert_eq!(scroll_offset, 0);
        assert_eq!(d.used_tokens, 1000);
    } else {
        panic!("Expected ModalState::ContextDiagnostics variant");
    }
}

#[test]
fn test_progressive_memory_decay_prune_integration() {
    let temp = TempDir::new().unwrap();
    let workspace = temp.path();

    let mut prog_mem = ProgressiveMemory::new();
    let mut old_entry = ProgressiveMemoryEntry::new(
        MemoryTier::L2ProjectFact,
        "old_dep",
        "tokio 0.2",
        0.5,
        "user",
    );
    // Artificially age the entry to 30 days ago
    let past = chrono::Utc::now() - chrono::Duration::days(30);
    old_entry.last_accessed = past.to_rfc3339();

    prog_mem.l2_project_facts.push(old_entry);
    prog_mem.save(workspace).unwrap();

    let mut loaded = ProgressiveMemory::load(workspace);
    assert_eq!(loaded.l2_project_facts.len(), 1);

    loaded.prune_decayed(0.15);
    assert_eq!(loaded.l2_project_facts.len(), 0);
}
