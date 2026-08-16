use minicode::ui::PtyDrawer;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

#[test]
fn test_pty_drawer_state_lifecycle_and_input() {
    let mut drawer = PtyDrawer::new();
    assert!(!drawer.is_open);

    drawer.toggle();
    assert!(drawer.is_open);

    drawer.handle_char('e');
    drawer.handle_char('c');
    drawer.handle_char('h');
    drawer.handle_char('o');
    drawer.handle_char(' ');
    drawer.handle_char('1');
    assert_eq!(drawer.input_buffer, "echo 1");

    drawer.handle_backspace();
    assert_eq!(drawer.input_buffer, "echo ");

    let submitted = drawer.submit_command();
    assert_eq!(submitted, Some("echo".to_string()));
    assert_eq!(drawer.input_buffer, "");

    drawer.append_output("hello from shell");
    assert!(drawer.history_lines.iter().any(|l| l.contains("hello from shell")));

    drawer.clear();
    assert!(drawer.history_lines.iter().any(|l| l.contains("cleared")));

    drawer.toggle();
    assert!(!drawer.is_open);
}

#[test]
fn test_pty_drawer_rendering_test_backend() {
    let mut drawer = PtyDrawer::new();
    drawer.is_open = true;
    drawer.append_output("Building release artifacts...");

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            drawer.render(frame, Rect::new(0, 0, 100, 30));
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = buffer
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();

    assert!(content.contains("Terminal Drawer"));
    assert!(content.contains("Building release artifacts"));
}
