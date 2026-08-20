use minicode::config::Config;
use minicode::ui::modal::ModalState;
use minicode::ui::Theme;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;
use std::fs;
use std::path::PathBuf;

fn create_temp_workspace() -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!(
        "minicode_integration_theme_{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).unwrap();
    temp_dir
}

#[test]
fn test_all_theme_palettes_and_detection() {
    let themes = Theme::list_themes();
    assert_eq!(themes.len(), 9);

    // Verify all theme ids are unique
    let mut ids: Vec<String> = themes.iter().map(|t| t.id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 9);

    // Verify Theme::detect maps correctly
    let tokyo = Theme::detect("tokyo-night");
    assert_eq!(tokyo.brand_accent, Color::Rgb(122, 162, 247));

    let catppuccin = Theme::detect("catppuccin");
    assert_eq!(catppuccin.brand_accent, Color::Rgb(203, 166, 247));

    let nord = Theme::detect("nord");
    assert_eq!(nord.brand_accent, Color::Rgb(136, 192, 208));

    let gruvbox = Theme::detect("gruvbox");
    assert_eq!(gruvbox.brand_accent, Color::Rgb(250, 189, 47));

    let dracula = Theme::detect("dracula");
    assert_eq!(dracula.brand_accent, Color::Rgb(189, 147, 249));

    let cyberpunk = Theme::detect("cyberpunk");
    assert_eq!(cyberpunk.brand_accent, Color::Rgb(0, 255, 102));

    let aura = Theme::detect("aura");
    assert_eq!(aura.brand_accent, Color::Rgb(162, 119, 255));
}

#[test]
fn test_theme_modal_rendering() {
    let theme = Theme::aura_dark();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    let modal = ModalState::new_theme_select("catppuccin");

    terminal
        .draw(|f| {
            let area = f.area();
            modal.render(f, area, &theme);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();

    assert!(content.contains("Theme Switcher"));
    assert!(content.contains("Aura Dark"));
    assert!(content.contains("Tokyo Night"));
    assert!(content.contains("Catppuccin Mocha"));
    assert!(content.contains("Apply & Save"));
    assert!(content.contains("Cancel"));
}

#[test]
fn test_config_save_theme_persistence() {
    let ws = create_temp_workspace();
    let minicode_dir = ws.join(".minicode");
    fs::create_dir_all(&minicode_dir).unwrap();

    let mut config = Config::default();
    config.ui.theme = "tokyo-night".to_string();

    // Save configuration to workspace .minicode/config.toml
    config.save(Some(&ws)).unwrap();

    // Verify file written
    let config_file = minicode_dir.join("config.toml");
    assert!(config_file.exists());

    // Load configuration back and verify persistence
    let loaded = Config::load(Some(&ws), None).unwrap();
    assert_eq!(loaded.ui.theme, "tokyo-night");

    let _ = fs::remove_dir_all(&ws);
}
