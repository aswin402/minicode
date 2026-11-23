//! Project-level configuration loader & inline suppression parser for `minidoctor`.

use crate::types::Severity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Persisted or embedded configuration for `minidoctor`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DoctorConfig {
    pub lint: bool,
    pub dead_code: bool,
    pub security: bool,
    pub write_md: bool,
    pub threshold: u8,
    pub ignore_rules: Vec<String>,
    pub ignore_files: Vec<String>,
    pub severity_overrides: HashMap<String, String>,
}

impl Default for DoctorConfig {
    fn default() -> Self {
        Self {
            lint: true,
            dead_code: true,
            security: true,
            write_md: true,
            threshold: 75,
            ignore_rules: Vec::new(),
            ignore_files: Vec::new(),
            severity_overrides: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct RawDoctorJson {
    lint: Option<bool>,
    #[serde(alias = "deadCode")]
    dead_code: Option<bool>,
    security: Option<bool>,
    #[serde(alias = "writeMd")]
    write_md: Option<bool>,
    threshold: Option<u8>,
    ignore: Option<RawIgnoreBlock>,
    rules: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawIgnoreBlock {
    rules: Option<Vec<String>>,
    files: Option<Vec<String>>,
}

impl DoctorConfig {
    /// Loads configuration by checking `.minicode/minidoctor.toml`, `minidoctor.toml`,
    /// `minidoctor.config.json`, `package.json`, and `pyproject.toml`.
    #[must_use]
    pub fn load(workspace: &Path) -> Self {
        let mut cfg = Self::default();

        // 1. TOML config files (`.minicode/minidoctor.toml` or `minidoctor.toml`)
        for rel in [".minicode/minidoctor.toml", "minidoctor.toml"] {
            let path = workspace.join(rel);
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(raw) = toml::from_str::<RawDoctorJson>(&text) {
                    cfg.apply_raw(raw);
                    return cfg;
                }
            }
        }

        // 2. JSON config files (`minidoctor.config.json`, `py_doctor.config.json`, `hono_doctor.config.json`, `flutter_doctor.config.json`)
        for rel in [
            ".minicode/minidoctor.config.json",
            "minidoctor.config.json",
            "py_doctor.config.json",
            "hono_doctor.config.json",
            "flutter_doctor.config.json",
        ] {
            let path = workspace.join(rel);
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(raw) = serde_json::from_str::<RawDoctorJson>(&text) {
                    cfg.apply_raw(raw);
                    return cfg;
                }
            }
        }

        // 3. Embedded in `package.json` (`minidoctor` or `honoDoctor`)
        if let Ok(text) = fs::read_to_string(workspace.join("package.json")) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                for key in ["minidoctor", "miniDoctor", "honoDoctor"] {
                    if let Some(sub) = val.get(key) {
                        if let Ok(raw) = serde_json::from_value::<RawDoctorJson>(sub.clone()) {
                            cfg.apply_raw(raw);
                            return cfg;
                        }
                    }
                }
            }
        }

        // 4. Embedded in `pyproject.toml` (`[tool.minidoctor]` or `[tool.py_doctor]`)
        if let Ok(text) = fs::read_to_string(workspace.join("pyproject.toml")) {
            if let Ok(val) = toml::from_str::<toml::Value>(&text) {
                if let Some(tool) = val.get("tool") {
                    for key in ["minidoctor", "py_doctor"] {
                        if let Some(sub) = tool.get(key) {
                            if let Ok(raw) = sub.clone().try_into::<RawDoctorJson>() {
                                cfg.apply_raw(raw);
                                return cfg;
                            }
                        }
                    }
                }
            }
        }

        cfg
    }

    fn apply_raw(&mut self, raw: RawDoctorJson) {
        if let Some(v) = raw.lint {
            self.lint = v;
        }
        if let Some(v) = raw.dead_code {
            self.dead_code = v;
        }
        if let Some(v) = raw.security {
            self.security = v;
        }
        if let Some(v) = raw.write_md {
            self.write_md = v;
        }
        if let Some(v) = raw.threshold {
            self.threshold = v.min(100);
        }
        if let Some(ignore) = raw.ignore {
            if let Some(rules) = ignore.rules {
                self.ignore_rules.extend(rules);
            }
            if let Some(files) = ignore.files {
                self.ignore_files.extend(files);
            }
        }
        if let Some(rules) = raw.rules {
            self.severity_overrides.extend(rules);
        }
    }

    /// Returns true if a rule is ignored via config or `"off"` severity override.
    #[must_use]
    pub fn is_rule_ignored(&self, rule_id: &str) -> bool {
        if self
            .ignore_rules
            .iter()
            .any(|r| r == rule_id || (r.ends_with('/') && rule_id.starts_with(r.as_str())))
        {
            return true;
        }
        self.severity_overrides.get(rule_id).is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "off" | "disable" | "none"
            )
        })
    }

    /// Resolves effective severity for a rule after config overrides.
    #[must_use]
    pub fn effective_severity(&self, rule_id: &str, default: Severity) -> Severity {
        self.severity_overrides
            .get(rule_id)
            .and_then(|raw| Severity::from_str_loose(raw))
            .unwrap_or(default)
    }

    /// Matches workspace-relative path against `ignore_files` patterns (supports `*`, `**`, prefix dirs, suffix globs).
    #[must_use]
    pub fn is_file_ignored(&self, rel_path: &str) -> bool {
        let norm = rel_path.replace('\\', "/");
        for pat in &self.ignore_files {
            let p = pat.trim().replace('\\', "/");
            if p.is_empty() {
                continue;
            }
            if glob_match_simple(&p, &norm) {
                return true;
            }
        }
        false
    }
}

/// Checks if a given 1-based source line is suppressed via inline comment:
/// `minidoctor-ignore`, `minidoctor-ignore <rule_id>`, or previous-line `minidoctor-ignore-next-line`.
#[must_use]
pub fn is_line_suppressed(source: &str, line_1based: usize, rule_id: &str) -> bool {
    if line_1based == 0 {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    let idx = line_1based.saturating_sub(1);
    if let Some(cur) = lines.get(idx) {
        if line_has_suppression_marker(cur, "minidoctor-ignore", rule_id)
            && !cur.contains("minidoctor-ignore-next-line")
        {
            return true;
        }
    }
    if idx > 0 {
        if let Some(prev) = lines.get(idx - 1) {
            if line_has_suppression_marker(prev, "minidoctor-ignore-next-line", rule_id) {
                return true;
            }
        }
    }
    false
}

fn line_has_suppression_marker(line: &str, marker: &str, rule_id: &str) -> bool {
    let Some(pos) = line.find(marker) else {
        return false;
    };
    let tail = line[pos + marker.len()..].trim();
    let tail = tail.trim_start_matches(':').trim();
    if tail.is_empty() || tail.starts_with("*/") || tail.starts_with("--") {
        return true;
    }
    tail.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .any(|tok| tok == rule_id || tok == "*")
}

/// Lightweight glob matcher supporting prefix directories (`migrations/`), `**/*.ext`, and `*` wildcards.
pub(crate) fn glob_match_simple(pattern: &str, path: &str) -> bool {
    let pat = pattern.trim_start_matches("./");
    let p = path.trim_start_matches("./");
    if pat.ends_with('/') {
        let dir = pat.trim_end_matches('/');
        return p == dir || p.starts_with(pat) || p.contains(&format!("/{dir}/"));
    }
    if let Some(suffix) = pat.strip_prefix("**/") {
        if glob_match_simple(suffix, p) {
            return true;
        }
        return p
            .match_indices('/')
            .any(|(i, _)| glob_match_simple(suffix, &p[i + 1..]));
    }
    if let Some(prefix) = pat.strip_suffix("/**") {
        return p == prefix || p.starts_with(&format!("{prefix}/"));
    }
    wildcard_match(pat, p)
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_idx, mut match_idx) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_idx = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(si) = star_idx {
            pi = si + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_and_inline_suppression() {
        assert!(glob_match_simple("migrations/", "app/migrations/001.py"));
        assert!(glob_match_simple("**/*.g.dart", "lib/models/user.g.dart"));
        assert!(glob_match_simple(
            "lib/generated/**",
            "lib/generated/a/b.dart"
        ));
        assert!(!glob_match_simple("migrations/", "app/models/user.py"));

        let src = "let x = 1;\n// minidoctor-ignore-next-line rust/unwrap-in-non-test\nlet y = opt.unwrap();\nlet z = a.unwrap(); // minidoctor-ignore\n";
        assert!(is_line_suppressed(src, 3, "rust/unwrap-in-non-test"));
        assert!(!is_line_suppressed(src, 3, "other/rule"));
        assert!(is_line_suppressed(src, 4, "rust/unwrap-in-non-test"));
    }
}
