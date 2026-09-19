use crossterm::{
    cursor::{Hide, Show},
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, stdout, Write};

/// RAII guard that enables raw terminal mode, hides the cursor, and enables bracketed paste.
/// When dropped, it automatically restores normal terminal mode, shows the cursor,
/// and disables bracketed paste.
pub struct TerminalGuard;

impl TerminalGuard {
    /// Enables raw terminal mode, hides the cursor, and enables bracketed paste.
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        if let Err(e) = execute!(out, Hide, EnableBracketedPaste) {
            let _ = disable_raw_mode();
            return Err(e);
        }
        let _ = out.flush();
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        let _ = execute!(out, Show, DisableBracketedPaste);
        let _ = disable_raw_mode();
        let _ = out.flush();
    }
}
