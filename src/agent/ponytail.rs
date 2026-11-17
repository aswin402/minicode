//! Native Ponytail Minimalist Engineering Mode (`dietrichgebert/ponytail` parity for `minicode`).
//!
//! Configurable just like `CavemanMode` (`ponytail_mode: bool` + `ponytail_level: "lite" | "full" | "ultra"`),
//! persisted in `config.toml` and symlink-safe `.minicode/.ponytail-active` flag files, toggleable in
//! `/settings` -> **Autonomy**, `update_agent_config`, or natural language, and propagated across
//! both the Orchestrator and `MiniPub` subagent workers.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const PONYTAIL_SENTINEL: &str = "Think like the laziest senior dev in the room";
const MAX_FLAG_BYTES: u64 = 64;

/// Supported Ponytail minimalist engineering intensity levels (`lite`, `full`, `ultra`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PonytailLevel {
    Off,
    /// `lite`: Build what is asked cleanly and note any lazier/simpler alternative in one line.
    Lite,
    /// `full` (default): Strictly enforce the 7-Rung YAGNI Ladder, reuse existing symbols first,
    /// mark deliberate scaling ceilings with `// shortcut: <ceiling>, <when to upgrade>`, and finish all callers.
    #[default]
    Full,
    /// `ultra`: YAGNI extremist — challenge unrequested scope, prefer deleting code/dependencies
    /// over adding abstractions, and push back before building heavy infrastructure.
    Ultra,
}

impl PonytailLevel {
    /// Parses a mode/level string loosely.
    #[must_use]
    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "disabled" | "false" | "none" | "stop" => Self::Off,
            "lite" | "light" | "low" => Self::Lite,
            "full" | "on" | "enabled" | "true" | "default" | "ponytail" | "medium" => Self::Full,
            "ultra" | "max" | "high" | "extreme" => Self::Ultra,
            _ => Self::Full,
        }
    }

    /// Parses an active level string, mapping `"off"` to `Self::Full` when the boolean toggle is enabled.
    #[must_use]
    pub fn from_str_active(s: &str) -> Self {
        let parsed = Self::from_str_loose(s);
        if parsed == Self::Off {
            Self::Full
        } else {
            parsed
        }
    }

    /// Detects natural-language Ponytail activation, level switch, or deactivation triggers
    /// without requiring any slash command.
    #[must_use]
    pub fn detect_natural_trigger(user_message: &str) -> Option<Self> {
        let lower = user_message.trim().to_ascii_lowercase();
        if lower.is_empty() || lower.len() > 240 {
            return None;
        }
        if lower.contains("stop ponytail")
            || lower.contains("disable ponytail")
            || lower.contains("ponytail off")
        {
            return Some(Self::Off);
        }
        if lower.contains("ponytail ultra") || lower.contains("ultra ponytail") {
            return Some(Self::Ultra);
        }
        if lower.contains("ponytail lite") || lower.contains("lite ponytail") {
            return Some(Self::Lite);
        }
        if lower.contains("enable ponytail")
            || lower.contains("use ponytail")
            || lower.contains("ponytail mode")
            || lower.contains("ponytail full")
            || lower.contains("ponytail on")
            || lower.contains("laziest senior dev")
        {
            return Some(Self::Full);
        }
        None
    }

    /// Canonical configuration string (`"off"`, `"lite"`, `"full"`, `"ultra"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Lite => "lite",
            Self::Full => "full",
            Self::Ultra => "ultra",
        }
    }

    /// Cycles through active intensity levels (`lite -> full -> ultra -> lite`).
    #[must_use]
    pub const fn cycle_next(self) -> Self {
        match self {
            Self::Lite => Self::Full,
            Self::Full => Self::Ultra,
            Self::Ultra | Self::Off => Self::Lite,
        }
    }

    /// Whether Ponytail Mode is active.
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// Resolves the active `PonytailLevel` for `workspace_dir` from `Config` and `.minicode/.ponytail-active`.
#[must_use]
pub fn resolve_ponytail_level(workspace_dir: &Path) -> PonytailLevel {
    let app_cfg = crate::config::Config::load_for_workspace(workspace_dir);
    let flag_opt = read_flag_safe(&default_flag_path(workspace_dir));
    if let Some(flag_lvl) = flag_opt {
        return flag_lvl;
    }
    if app_cfg.agent.ponytail_mode {
        PonytailLevel::from_str_active(&app_cfg.agent.ponytail_level)
    } else {
        PonytailLevel::Off
    }
}

/// Builds the `<ponytail_mode level="...">` system prompt block when Ponytail Mode is enabled.
#[must_use]
pub fn build_ponytail_system_block(enabled: bool, level_str: &str) -> String {
    if !enabled {
        return String::new();
    }
    let level = PonytailLevel::from_str_active(level_str);
    let intensity_rules = match level {
        PonytailLevel::Off => return String::new(),
        PonytailLevel::Lite => {
            "Level: **lite** — Build what the user asked cleanly while climbing the 7-Rung Ladder for internal implementation details, and note any lazier/simpler alternative in 1 line at the end."
        }
        PonytailLevel::Full => {
            "Level: **full** — Enforce the minimal complete change: climb Rungs 1->7 strictly, reuse existing workspace symbols before writing new helpers, state any speculative bloat skipped, update every caller (`open_callers == 0`), and include a 1-line Honesty Footer."
        }
        PonytailLevel::Ultra => {
            "Level: **ultra** — YAGNI extremist: prefer deleting dead code, consolidating duplicates, or using config/stdlib over writing new abstractions. Challenge unrequested complexity via `ask_user` before building heavy layers when a minimal diff suffices."
        }
    };

    format!(
        "\n<ponytail_mode level=\"{}\">\n\
  {}: The best code is the code you never wrote.\n\
  - **Intensity**: {}\n\
  - **7-Rung Solution Ladder**:\n\
    1. **Delete / Config**: Solve by deleting dead code or adjusting existing config first.\n\
    2. **Reuse Existing Symbol**: Check `<reusable_symbol_map>` and `repo_map(mode=\"reuse\")` before creating any new helper, type, or component.\n\
    3. **Standard Library**: Use stdlib primitives before external dependencies.\n\
    4. **Native Platform Feature**: Use built-in OS, browser, HTML/CSS, or SQL features directly.\n\
    5. **Installed Dependency**: Use crates/packages already in `Cargo.toml` / `package.json`.\n\
    6. **One-Liner**: Write one clear readable line when it solves the root cause.\n\
    7. **Minimum Working Code + `shortcut:` Ledger**: Write the smallest complete diff; annotate deliberate scaling ceilings with `// shortcut: <ceiling>, <when to upgrade>`.\n\
  - **Never Cut Safety**: Keep trust-boundary validation, data-loss error handling, security, accessibility, and caller completeness (`open_callers == 0`).\n\
  - **Honesty Footer**: End non-trivial coding turns with 1 line noting any shortcut ceiling taken or `Not checked:` path.\n\
</ponytail_mode>\n",
        level.as_str(),
        PONYTAIL_SENTINEL,
        intensity_rules
    )
}

/// Builds a compact Zone B recency reminder (`<ponytail_active level="...">`) when Ponytail Mode is active.
#[must_use]
pub fn build_ponytail_recency_block(enabled: bool, level_str: &str) -> String {
    if !enabled {
        return String::new();
    }
    let level = PonytailLevel::from_str_active(level_str);
    let hint = match level {
        PonytailLevel::Lite => {
            "Reuse existing `<reusable_symbol_map>` symbols first; note any simpler alternative in 1 line."
        }
        PonytailLevel::Full => {
            "Climb 7-Rung YAGNI Ladder (`Delete/Config -> Reuse symbol -> Stdlib -> Native -> Installed dep -> 1-liner -> Minimal diff + shortcut:`), keep `open_callers == 0`."
        }
        PonytailLevel::Ultra => {
            "YAGNI extremist: prefer deleting/reusing over new code; push back on unrequested abstractions; keep diff minimal and `open_callers == 0`."
        }
        PonytailLevel::Off => return String::new(),
    };
    format!(
        "  <ponytail_active level=\"{}\">\n    {} (`{}`): {}\n  </ponytail_active>\n",
        level.as_str(),
        PONYTAIL_SENTINEL,
        level.as_str(),
        hint
    )
}

/// Builds a compact Ponytail subagent contract for `MiniPub` workers when Ponytail Mode is enabled.
#[must_use]
pub fn ponytail_subagent_block(enabled: bool, level_str: &str) -> String {
    if !enabled {
        return String::new();
    }
    let level = PonytailLevel::from_str_active(level_str);
    format!(
        "\n\n<ponytail_subagent level=\"{}\">\n\
        {}: Reuse `<reusable_symbol_map>` symbols first, avoid single-use wrappers/traits, update all callers of modified signatures, and mark deliberate YAGNI limits with `// shortcut: <ceiling>, <when to upgrade>`.\n\
        </ponytail_subagent>",
        level.as_str(),
        PONYTAIL_SENTINEL
    )
}

/// Default workspace path for the `.ponytail-active` state flag.
#[must_use]
pub fn default_flag_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".minicode").join(".ponytail-active")
}

/// Reads `.ponytail-active` safely (`O_NOFOLLOW` on Unix, rejecting symlinks and oversized files).
/// Note: Unlike `.caveman-active`, `"off"` stored in `.ponytail-active` explicitly represents user-disabled state!
#[must_use]
pub fn read_flag_safe(flag_path: &Path) -> Option<PonytailLevel> {
    let meta = fs::symlink_metadata(flag_path).ok()?;
    if meta.file_type().is_symlink() || !meta.file_type().is_file() || meta.len() > MAX_FLAG_BYTES {
        return None;
    }

    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(flag_path)
            .ok()?
    };
    #[cfg(not(unix))]
    let file = OpenOptions::new().read(true).open(flag_path).ok()?;

    let mut buf = String::new();
    file.take(MAX_FLAG_BYTES).read_to_string(&mut buf).ok()?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PonytailLevel::from_str_loose(trimmed))
}

/// Writes `.ponytail-active` atomically without following symlinks (`0600` on Unix).
pub fn write_flag_safe(flag_path: &Path, level: PonytailLevel) -> std::io::Result<()> {
    if let Ok(meta) = fs::symlink_metadata(flag_path) {
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Refusing to write .ponytail-active through symlink",
            ));
        }
    }
    if let Some(parent) = flag_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = flag_path.with_extension(format!("tmp.{}", std::process::id()));
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp_path)?;
        f.write_all(level.as_str().as_bytes())?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        fs::write(&tmp_path, level.as_str().as_bytes())?;
    }
    fs::rename(&tmp_path, flag_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ponytail_level_parsing_and_cycling() {
        assert_eq!(PonytailLevel::from_str_loose("off"), PonytailLevel::Off);
        assert_eq!(PonytailLevel::from_str_loose("lite"), PonytailLevel::Lite);
        assert_eq!(PonytailLevel::from_str_loose("full"), PonytailLevel::Full);
        assert_eq!(PonytailLevel::from_str_loose("ultra"), PonytailLevel::Ultra);

        assert_eq!(PonytailLevel::Lite.cycle_next(), PonytailLevel::Full);
        assert_eq!(PonytailLevel::Full.cycle_next(), PonytailLevel::Ultra);
        assert_eq!(PonytailLevel::Ultra.cycle_next(), PonytailLevel::Lite);

        assert!(!PonytailLevel::Off.is_enabled());
        assert!(PonytailLevel::Lite.is_enabled());
        assert!(PonytailLevel::Full.is_enabled());
        assert!(PonytailLevel::Ultra.is_enabled());
    }

    #[test]
    fn test_ponytail_natural_triggers_and_system_blocks() {
        assert_eq!(
            PonytailLevel::detect_natural_trigger("enable ponytail"),
            Some(PonytailLevel::Full)
        );
        assert_eq!(
            PonytailLevel::detect_natural_trigger("switch to ponytail ultra"),
            Some(PonytailLevel::Ultra)
        );
        assert_eq!(
            PonytailLevel::detect_natural_trigger("use ponytail lite"),
            Some(PonytailLevel::Lite)
        );
        assert_eq!(
            PonytailLevel::detect_natural_trigger("disable ponytail"),
            Some(PonytailLevel::Off)
        );
        assert_eq!(
            PonytailLevel::detect_natural_trigger("refactor src/main.rs"),
            None
        );

        assert!(build_ponytail_system_block(false, "full").is_empty());
        let ultra_block = build_ponytail_system_block(true, "ultra");
        assert!(ultra_block.contains("<ponytail_mode level=\"ultra\">"));
        assert!(ultra_block.contains("YAGNI extremist"));
    }

    #[test]
    fn test_ponytail_flag_roundtrip_and_disable() {
        let tmp = tempfile::tempdir().unwrap();
        let flag = default_flag_path(tmp.path());
        write_flag_safe(&flag, PonytailLevel::Ultra).unwrap();
        assert_eq!(read_flag_safe(&flag), Some(PonytailLevel::Ultra));
        assert_eq!(resolve_ponytail_level(tmp.path()), PonytailLevel::Ultra);

        write_flag_safe(&flag, PonytailLevel::Off).unwrap();
        assert_eq!(read_flag_safe(&flag), Some(PonytailLevel::Off));
        assert_eq!(resolve_ponytail_level(tmp.path()), PonytailLevel::Off);
    }
}
