use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

/// Origin of a discovered workspace script or runnable recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptSource {
    Justfile,
    PackageJson,
    Makefile,
    Cargo,
    Deno,
    PyProject,
    StaticWeb,
}

impl fmt::Display for ScriptSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Justfile => write!(f, "justfile"),
            Self::PackageJson => write!(f, "package.json"),
            Self::Makefile => write!(f, "Makefile"),
            Self::Cargo => write!(f, "Cargo.toml"),
            Self::Deno => write!(f, "deno.json"),
            Self::PyProject => write!(f, "pyproject.toml"),
            Self::StaticWeb => write!(f, "index.html"),
        }
    }
}

/// A runnable script target discovered dynamically from the project repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceScript {
    /// Human or shorthand name of the script (e.g. "dev", "start", "build", "test", "lint")
    pub name: String,
    /// Exact shell invocation to run it (e.g. "just dev", "bun run dev", "cargo run")
    pub command: String,
    /// Origin manifest or task runner defining this target
    pub source: ScriptSource,
    /// True if this target represents a long-running development server, preview server, or daemon
    pub is_server: bool,
    /// Optional docstring or raw command snippet
    pub description: Option<String>,
}

pub struct WorkspaceScriptDiscovery;

impl WorkspaceScriptDiscovery {
    /// Discovers all runnable scripts across the workspace (justfile, package.json, Makefile, Cargo.toml, deno.json, pyproject.toml, index.html).
    pub fn discover(workspace_root: &Path) -> Vec<WorkspaceScript> {
        let mut scripts = Vec::new();

        // 1. justfile / Justfile (Highest priority task runner when present)
        Self::scan_justfile(workspace_root, &mut scripts);

        // 2. package.json (Node / Bun / Deno ecosystems)
        Self::scan_package_json(workspace_root, &mut scripts);

        // 3. Makefile / GNUmakefile
        Self::scan_makefile(workspace_root, &mut scripts);

        // 4. Cargo.toml (Rust)
        Self::scan_cargo(workspace_root, &mut scripts);

        // 5. deno.json / deno.jsonc
        Self::scan_deno(workspace_root, &mut scripts);

        // 6. Python pyproject.toml / requirements.txt
        Self::scan_python(workspace_root, &mut scripts);

        // 7. Static HTML workspace fallback (when index.html exists and no other runner defines dev/serve)
        if scripts.is_empty() {
            Self::scan_static_web(workspace_root, &mut scripts);
        }

        scripts
    }

    /// Looks up a specific script target by name (e.g. "dev", "build", "test").
    pub fn find_script(workspace_root: &Path, name: &str) -> Option<WorkspaceScript> {
        let all = Self::discover(workspace_root);
        let trimmed = name.trim();
        // Exact ASCII case-insensitive match first (zero allocations)
        if let Some(found) = all.iter().find(|s| s.name.eq_ignore_ascii_case(trimmed)) {
            return Some(found.clone());
        }
        let clean = trimmed.to_lowercase();
        // Substring / alias match (e.g. "server" -> "dev")
        all.into_iter().find(|s| {
            let s_lower = s.name.to_lowercase();
            s_lower.contains(&clean) || clean.contains(&s_lower)
        })
    }

    /// Dynamically discovers the primary development server script for the workspace.
    pub fn find_dev_server(workspace_root: &Path) -> Option<WorkspaceScript> {
        let all = Self::discover(workspace_root);
        // Priority order for dev server targets: dev -> start -> serve -> run -> preview
        for target in &["dev", "start", "serve", "server", "run", "preview"] {
            if let Some(s) = all.iter().find(|s| s.name.eq_ignore_ascii_case(target)) {
                return Some(s.clone());
            }
        }
        // Fallback: any script marked as a server
        all.into_iter().find(|s| s.is_server)
    }

    /// Tests whether an arbitrary shell command executes a long-running dev server, daemon, or web process.
    pub fn is_server_command(workspace_root: &Path, command_str: &str) -> bool {
        let trimmed = command_str.trim();
        if trimmed.is_empty() {
            return false;
        }

        let lower = trimmed.to_lowercase();

        // 0. Never classify one-shot CLI utilities, HTTP probes, or bounded commands as daemons
        let non_daemon_prefixes = [
            "curl ", "wget ", "timeout ", "sleep ", "ss ", "netstat ", "lsof ", "fuser ", "git ",
            "gh ", "ls ", "find ", "rg ", "grep ", "cat ", "head ", "tail ", "sed ", "awk ", "jq ",
            "ps ", "kill ", "pkill ", "killall ", "pgrep ", "echo ", "printf ", "which ", "mkdir ",
            "rm ", "cp ", "mv ",
        ];
        if non_daemon_prefixes.iter().any(|p| lower.starts_with(p)) {
            return false;
        }

        // 1. Explicit background operator at the end of a simple command
        if trimmed.ends_with('&') && !trimmed.ends_with("&&") {
            return true;
        }

        // 2. Universal server executables & flags (language-agnostic)
        // Note: Do NOT include short ambiguous flags like `-w ` (which matches `curl -w`, `grep -w`, etc.).
        let universal_patterns = [
            "http.server",
            "simplehttpserver",
            "http-server",
            "uvicorn ",
            "gunicorn ",
            "flask run",
            "fastapi dev",
            "fastapi run",
            "cargo watch",
            "docker compose up",
            "docker-compose up",
            "vite preview",
            "next dev",
            "astro dev",
            "remix dev",
            "--watch",
        ];
        if universal_patterns.iter().any(|pat| lower.contains(pat)) {
            return true;
        }

        // 3. Match against dynamically discovered project scripts
        let all_scripts = Self::discover(workspace_root);
        let runner_prefixes = [
            "bun ", "npm ", "pnpm ", "yarn ", "just ", "make ", "cargo ", "deno ", "npx ", "bunx ",
        ];
        let starts_with_runner = runner_prefixes.iter().any(|r| lower.starts_with(r));

        for s in all_scripts {
            if !s.is_server {
                continue;
            }
            let s_cmd_lower = s.command.to_lowercase();
            if lower == s_cmd_lower
                || lower
                    .strip_prefix(&s_cmd_lower)
                    .is_some_and(|rest| rest.starts_with(' '))
            {
                return true;
            }
            // Check if invoked via task runner (e.g. "bun run dev", "npm run dev", "just dev")
            if starts_with_runner {
                let s_name_lower = s.name.to_lowercase();
                if let Some(prefix) = lower.strip_suffix(&s_name_lower) {
                    if prefix.ends_with(" run ") || prefix.ends_with(' ') {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Formats discovered scripts into a human- and agent-readable markdown table.
    pub fn format_catalog(scripts: &[WorkspaceScript]) -> String {
        if scripts.is_empty() {
            return "ℹ No runnable script targets (justfile, package.json, Makefile, Cargo.toml) discovered in workspace.".to_string();
        }

        let mut out = String::from("### Discovered Workspace Script Targets 📜\n\n");
        out.push_str("| Target | Source | Command | Type |\n");
        out.push_str("|---|---|---|---|\n");
        for s in scripts {
            let kind = if s.is_server {
                "⚡ Dev Server / Persistent"
            } else {
                "🛠️ Task / Build"
            };
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` | {} |\n",
                s.name, s.source, s.command, kind
            ));
        }
        out.push_str("\n💡 *Tip: Run any target directly with `minitask(action=\"start\", script=\"<target>\")`.*");
        out
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal Scanners
    // ─────────────────────────────────────────────────────────────────────────

    fn scan_justfile(workspace_root: &Path, scripts: &mut Vec<WorkspaceScript>) {
        for name in &["justfile", "Justfile", ".justfile"] {
            let path = workspace_root.join(name);
            if !path.is_file() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('#') || trimmed.starts_with('@') || trimmed.is_empty() {
                        continue;
                    }
                    if let Some(idx) = trimmed.find(':') {
                        let recipe_part = trimmed[..idx].trim();
                        let recipe = recipe_part.split_whitespace().next().unwrap_or("");
                        // Recipes in justfiles are alphanumeric with dashes/underscores
                        if !recipe.is_empty()
                            && recipe
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                        {
                            let is_server = matches!(
                                recipe.to_lowercase().as_str(),
                                "dev" | "serve" | "server" | "start" | "watch" | "preview" | "run"
                            );
                            scripts.push(WorkspaceScript {
                                name: recipe.to_string(),
                                command: format!("just {}", recipe),
                                source: ScriptSource::Justfile,
                                is_server,
                                description: None,
                            });
                        }
                    }
                }
            }
            break;
        }
    }

    fn scan_package_json(workspace_root: &Path, scripts: &mut Vec<WorkspaceScript>) {
        let pkg_path = workspace_root.join("package.json");
        if !pkg_path.is_file() {
            return;
        }

        let Ok(content) = fs::read_to_string(&pkg_path) else {
            return;
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else {
            return;
        };
        let Some(scripts_obj) = parsed.get("scripts").and_then(|s| s.as_object()) else {
            return;
        };

        let pm = Self::detect_node_pm(workspace_root);

        for (key, val) in scripts_obj {
            let cmd_str = val.as_str().unwrap_or("").trim();
            let lower_key = key.to_lowercase();
            let lower_cmd = cmd_str.to_lowercase();

            let is_server = if matches!(
                lower_key.as_str(),
                "build" | "compile" | "lint" | "check" | "test" | "format"
            ) {
                false
            } else if matches!(
                lower_key.as_str(),
                "dev" | "start" | "serve" | "server" | "preview" | "watch"
            ) {
                true
            } else {
                (lower_cmd.contains("vite") && !lower_cmd.contains("vite build"))
                    || lower_cmd.contains("next dev")
                    || lower_cmd.contains("remix dev")
                    || lower_cmd.contains("astro dev")
                    || lower_cmd.contains("webpack serve")
                    || lower_cmd.contains("http-server")
                    || lower_cmd.starts_with("serve ")
                    || lower_cmd == "serve"
            };

            let command = if key == "start" {
                format!("{} start", pm)
            } else if key == "test" {
                format!("{} test", pm)
            } else {
                format!("{} run {}", pm, key)
            };

            scripts.push(WorkspaceScript {
                name: key.clone(),
                command,
                source: ScriptSource::PackageJson,
                is_server,
                description: if !cmd_str.is_empty() {
                    Some(cmd_str.to_string())
                } else {
                    None
                },
            });
        }
    }

    fn scan_makefile(workspace_root: &Path, scripts: &mut Vec<WorkspaceScript>) {
        for name in &["Makefile", "makefile", "GNUmakefile"] {
            let path = workspace_root.join(name);
            if !path.is_file() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('#') || trimmed.is_empty() || trimmed.starts_with('.') {
                        continue;
                    }
                    if let Some(idx) = trimmed.find(':') {
                        let target = trimmed[..idx].trim();
                        if !target.is_empty()
                            && target
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                            && target != "PHONY"
                        {
                            let is_server = matches!(
                                target.to_lowercase().as_str(),
                                "dev" | "serve" | "server" | "start" | "run" | "watch"
                            );
                            scripts.push(WorkspaceScript {
                                name: target.to_string(),
                                command: format!("make {}", target),
                                source: ScriptSource::Makefile,
                                is_server,
                                description: None,
                            });
                        }
                    }
                }
            }
            break;
        }
    }

    fn scan_cargo(workspace_root: &Path, scripts: &mut Vec<WorkspaceScript>) {
        let cargo_path = workspace_root.join("Cargo.toml");
        if !cargo_path.is_file() {
            return;
        }

        let is_server = if let Ok(content) = fs::read_to_string(&cargo_path) {
            let lower = content.to_lowercase();
            lower.contains("axum")
                || lower.contains("actix-web")
                || lower.contains("rocket")
                || lower.contains("warp")
                || lower.contains("poem")
                || lower.contains("salvo")
                || lower.contains("tide")
        } else {
            false
        };

        scripts.push(WorkspaceScript {
            name: "run".to_string(),
            command: "cargo run".to_string(),
            source: ScriptSource::Cargo,
            is_server,
            description: Some("Compile and run primary binary target".to_string()),
        });

        scripts.push(WorkspaceScript {
            name: "check".to_string(),
            command: "cargo check".to_string(),
            source: ScriptSource::Cargo,
            is_server: false,
            description: Some("Type-check Rust crate without emitting binaries".to_string()),
        });

        scripts.push(WorkspaceScript {
            name: "test".to_string(),
            command: "cargo test".to_string(),
            source: ScriptSource::Cargo,
            is_server: false,
            description: Some("Execute unit and integration tests".to_string()),
        });
    }

    fn scan_deno(workspace_root: &Path, scripts: &mut Vec<WorkspaceScript>) {
        for name in &["deno.json", "deno.jsonc"] {
            let path = workspace_root.join(name);
            if !path.is_file() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(tasks) = parsed.get("tasks").and_then(|t| t.as_object()) {
                        for (task_name, task_val) in tasks {
                            let is_server = matches!(
                                task_name.to_lowercase().as_str(),
                                "dev" | "start" | "serve" | "server" | "preview" | "watch"
                            );
                            scripts.push(WorkspaceScript {
                                name: task_name.clone(),
                                command: format!("deno task {}", task_name),
                                source: ScriptSource::Deno,
                                is_server,
                                description: task_val.as_str().map(|s| s.to_string()),
                            });
                        }
                    }
                }
            }
            break;
        }
    }

    fn scan_python(workspace_root: &Path, scripts: &mut Vec<WorkspaceScript>) {
        let pyproject = workspace_root.join("pyproject.toml");
        let reqs = workspace_root.join("requirements.txt");

        let has_fastapi = (pyproject.is_file()
            && fs::read_to_string(&pyproject)
                .map(|c| c.contains("fastapi") || c.contains("uvicorn"))
                .unwrap_or(false))
            || (reqs.is_file()
                && fs::read_to_string(&reqs)
                    .map(|c| c.contains("fastapi") || c.contains("uvicorn"))
                    .unwrap_or(false));

        if has_fastapi {
            let entry = if workspace_root.join("main.py").is_file()
                && !workspace_root.join("app/main.py").is_file()
            {
                "main.py"
            } else {
                "app/main.py"
            };
            scripts.push(WorkspaceScript {
                name: "dev".to_string(),
                command: format!("fastapi dev {}", entry),
                source: ScriptSource::PyProject,
                is_server: true,
                description: Some("FastAPI development server with live reload".to_string()),
            });
        }
    }

    fn scan_static_web(workspace_root: &Path, scripts: &mut Vec<WorkspaceScript>) {
        if !workspace_root.join("index.html").is_file() {
            return;
        }
        let cmd = if which::which("python3").is_ok() {
            "python3 -m http.server 8000".to_string()
        } else {
            "npx --yes serve -l 3000 .".to_string()
        };
        for name in ["dev", "serve"] {
            scripts.push(WorkspaceScript {
                name: name.to_string(),
                command: cmd.clone(),
                source: ScriptSource::StaticWeb,
                is_server: true,
                description: Some("Static HTML/CSS/JS local HTTP server".to_string()),
            });
        }
    }

    fn detect_node_pm(workspace_root: &Path) -> &'static str {
        if workspace_root.join("bun.lock").exists() || workspace_root.join("bun.lockb").exists() {
            "bun"
        } else if workspace_root.join("pnpm-lock.yaml").exists() {
            "pnpm"
        } else if workspace_root.join("yarn.lock").exists() {
            "yarn"
        } else if workspace_root.join("package-lock.json").exists() {
            "npm"
        } else if which::which("bun").is_ok() {
            "bun"
        } else {
            "npm"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_discover_justfile() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let just_content = "
# Recipes
dev:
    bun run dev

build:
    bun run build

test filter='':
    cargo test {{filter}}
";
        fs::write(ws.join("justfile"), just_content).unwrap();

        let scripts = WorkspaceScriptDiscovery::discover(ws);
        assert_eq!(scripts.len(), 3);
        assert_eq!(scripts[0].name, "dev");
        assert_eq!(scripts[0].command, "just dev");
        assert!(scripts[0].is_server);

        let dev_script = WorkspaceScriptDiscovery::find_dev_server(ws).unwrap();
        assert_eq!(dev_script.command, "just dev");
    }

    #[test]
    fn test_discover_package_json() {
        let dir = tempdir().unwrap();
        let ws = dir.path();

        let pkg_content = r#"{
            "scripts": {
                "dev": "vite",
                "build": "tsc -b && vite build",
                "preview": "vite preview"
            }
        }"#;
        fs::write(ws.join("package.json"), pkg_content).unwrap();
        // Simulate bun lock
        fs::write(ws.join("bun.lock"), "").unwrap();

        let scripts = WorkspaceScriptDiscovery::discover(ws);
        assert!(scripts
            .iter()
            .any(|s| s.name == "dev" && s.command == "bun run dev" && s.is_server));
        assert!(scripts
            .iter()
            .any(|s| s.name == "build" && s.command == "bun run build" && !s.is_server));

        let is_server = WorkspaceScriptDiscovery::is_server_command(ws, "bun run dev");
        assert!(is_server);
        let is_build_server = WorkspaceScriptDiscovery::is_server_command(ws, "bun run build");
        assert!(!is_build_server);

        // Ensure curl -w / timeout / grep -w are never misclassified as background daemons
        assert!(!WorkspaceScriptDiscovery::is_server_command(
            ws,
            r#"timeout 10 curl -sS --noproxy '*' --max-time 6 -o /dev/null -w "code:%{http_code}\n" http://127.0.0.1:4173/"#
        ));
        assert!(!WorkspaceScriptDiscovery::is_server_command(
            ws,
            r#"curl -sS -w "%{http_code}" http://localhost:5173/"#
        ));
    }
}
