# CLI, Protocol & Skills Reference — `minicode` 📖

## 1. CLI Commands & Flags

```bash
# Interactive Human TUI Mode (Default)
minicode

# Headless AI Agent Mode (Bidirectional NDJSON over stdin/stdout)
minicode --json-stream

# One-shot Non-Interactive Task Execution
minicode run "Refactor error handling in src/agent/provider.rs"

# Resume Previous Session
minicode --resume <session_id>
minicode --continue   # Resume the most recent session

# Custom Working Directory & Model Override
minicode --dir /path/to/project --model gemini-2.5-pro

# Autonomous Mode (Auto-approve file writes and shell execution)
minicode run "Run tests and fix broken asserts" --yes

# Plain/Accessible Mode (No TUI, scrolling REPL, screen-reader compatible)
minicode --plain

# Light Theme Override
minicode --light
```

### CLI Flag Reference
| Flag | Short | Description |
| :--- | :--- | :--- |
| `--dir <PATH>` | `-d` | Target workspace directory (defaults to cwd) |
| `--model <MODEL>`| `-m` | Override default model (e.g. `gemini-2.5-pro`, `claude-3-7-sonnet`, `gpt-4o`, `ollama/qwen2.5-coder`) |
| `--provider <PROV>`| `-p` | Set provider (`gemini`, `anthropic`, `openai`, `ollama`) |
| `--json-stream`| | Enable bidirectional NDJSON streaming for AI orchestrators |
| `--yes` | `-y` | Automatically approve file writes and shell commands |
| `--plain` | | Disable TUI, use scrolling REPL output (accessible, CI/CD compatible) |
| `--accessible` | | Alias for `--plain` |
| `--light` | | Force light color theme |
| `--resume <ID>` | | Resume a previous session by session ID |
| `--continue` | | Resume the most recent session |
| `--no-repo-map`| | Disable AST Tree-sitter codebase indexing |
| `--timeout <SEC>`| `-t` | Override default command execution timeout (default: 30s) |
| `--verbose` | `-v` | Enable verbose logging to `~/.config/minicode/logs/` |
| `--log-level <LVL>`| | Set log level: `error`, `warn`, `info`, `debug`, `trace` |
| `--config <FILE>`| | Use a specific config file instead of auto-detected |
| `--version` | `-V` | Print version information |
| `--help` | `-h` | Print full CLI help message |

---

## 2. Interactive Slash Commands (TUI Mode)

| Slash Command | Description |
| :--- | :--- |
| `/diff` | Toggle full-screen colorized review of all pending and recent file modifications |
| `/undo` | Revert all file changes from the last turn and truncate conversation |
| `/undo <turn_id>` | Revert to a specific turn checkpoint |
| `/retry` | Resubmit the last user prompt (useful after errors or bad generations) |
| `/model <name>`| Switch the active LLM provider or model on-the-fly |
| `/compact` | Trigger immediate context compression (masks older history) |
| `/skill <name>`| Manually load and inject a specific skill from `.skills/` |
| `/skills` | List all discovered project skills and rules |
| `/map` | Inspect the current AST Repo-Map skeleton and top PageRank symbols |
| `/save` | Manually save current session to disk |
| `/load <id>` | Load a previous session |
| `/sessions` | List all saved sessions |
| `/prompt` | Inspect the current raw system prompt (for debugging) |
| `/clear` | Clear the visual conversation timeline (retains thread in memory) |
| `/help` | Display keybindings and command reference |
| `/exit` | Exit the `minicode` session cleanly |

---

## 3. Headless NDJSON Event Protocol (AI Agents & IPC)

### 3.1. Stdout Events (minicode → orchestrator)
Every message emitted over `stdout` in `--json-stream` mode is a single JSON object terminated by `\n`:

#### `turn_start`
```json
{
  "event": "turn_start",
  "turn_id": 1,
  "timestamp": "2026-08-14T10:30:00Z",
  "model": "gemini-2.5-pro",
  "context_tokens": 1420
}
```

#### `stream_delta`
```json
{
  "event": "stream_delta",
  "turn_id": 1,
  "delta": "I will read `src/main.rs` to inspect the entrypoint."
}
```

#### `tool_call`
```json
{
  "event": "tool_call",
  "turn_id": 1,
  "tool_id": "call_9823",
  "tool": "read_file",
  "args": { "path": "src/main.rs", "start_line": 1, "end_line": 50 }
}
```

#### `tool_result`
```json
{
  "event": "tool_result",
  "turn_id": 1,
  "tool_id": "call_9823",
  "tool": "read_file",
  "success": true,
  "output": "1: fn main() {\n2:     println!(\"Hello\");\n3: }",
  "duration_ms": 2
}
```

#### `approval_request`
```json
{
  "event": "approval_request",
  "turn_id": 1,
  "tool_id": "call_9824",
  "tool": "exec_cmd",
  "args": { "command": "cargo test" },
  "reason": "shell_execution"
}
```

#### `file_modified`
```json
{
  "event": "file_modified",
  "turn_id": 1,
  "path": "src/tools/fs.rs",
  "action": "patched",
  "backup": ".minicode/backups/1/src/tools/fs.rs"
}
```

#### `turn_end`
```json
{
  "event": "turn_end",
  "turn_id": 1,
  "status": "complete",
  "total_tokens_used": 1840,
  "files_modified": ["src/main.rs"]
}
```

#### `error`
```json
{
  "event": "error",
  "turn_id": 2,
  "code": "rate_limited",
  "message": "429 Too Many Requests from provider",
  "retrying": true,
  "retry_after_ms": 5000
}
```

#### `heartbeat`
```json
{
  "event": "heartbeat",
  "timestamp": "2026-08-14T10:31:00Z",
  "status": "streaming",
  "turn_id": 2
}
```

### 3.2. Stdin Commands (orchestrator → minicode)
In `--json-stream` mode, `minicode` reads newline-delimited JSON commands from stdin:

#### Send a user message
```json
{"method": "user_input", "params": {"text": "Fix the failing test in src/tools/fs.rs"}}
```

#### Approve a pending tool call
```json
{"method": "tool_response", "params": {"tool_id": "call_9824", "action": "approve"}}
```

#### Reject a pending tool call
```json
{"method": "tool_response", "params": {"tool_id": "call_9824", "action": "reject", "reason": "Too destructive"}}
```

#### Abort current turn
```json
{"method": "abort", "params": {}}
```

#### Runtime configuration change
```json
{"method": "configure", "params": {"auto_approve": true}}
{"method": "configure", "params": {"model": "claude-3-7-sonnet"}}
```

---

## 4. Configuration File Format

### 4.1. Global Config (`~/.config/minicode/config.toml`)
```toml
# ~/.config/minicode/config.toml

[provider]
default = "gemini"              # gemini | anthropic | openai | ollama
model = "gemini-2.5-pro"        # Default model

[provider.ollama]
host = "http://localhost:11434"  # Ollama server URL

[agent]
auto_approve = false            # Auto-approve destructive actions
timeout = 30                    # Shell command timeout in seconds
map_tokens = 1024               # Repo-map token budget

[ui]
theme = "auto"                  # auto | dark | light
plain = false                   # Disable TUI (scrolling REPL)

[logging]
level = "info"                  # error | warn | info | debug | trace
file = true                     # Write logs to ~/.config/minicode/logs/
```

### 4.2. API Key Management
API keys are sourced from environment variables ONLY:
```bash
# Via shell environment
export GEMINI_API_KEY="AIza..."
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."

# Or via .env file in workspace root (auto-loaded)
# .env
GEMINI_API_KEY=AIza...
```
Keys are NEVER stored in config.toml or any committed file.

### 4.3. Priority Hierarchy (Highest → Lowest)
1. CLI flags (`--model gemini-2.5-pro`)
2. Environment variables (`MINICODE_MODEL=gemini-2.5-pro`)
3. Project-local `.minicode/config.toml`
4. Global `~/.config/minicode/config.toml`
5. Built-in defaults

---

## 5. Skills & Guidelines Specification

### 5.1. Automatic Discovery
`minicode` scans for project-level instructions in this order:
1. `AGENTS.md` in the workspace root.
2. `onpkg.json` → `agent_instructions.active_skills` under `onpkg_docs/`.
3. `.skills/*.md` files in the workspace root.
4. `SKILL.md` in the workspace root.

### 5.2. Standard SKILL.md Format
```markdown
---
name: rust-best-practices
description: Core idioms, error handling conventions, and concurrency patterns for Rust codebases.
version: "1.0"
triggers:
  - "*.rs"
  - "Cargo.toml"
depends_on: []
---

# Rust Guidelines
1. Always prefer `thiserror` for library error types and `anyhow` for application entrypoints.
2. Avoid `.unwrap()` and `.expect()` in non-test paths; use `?` operator.
3. Keep structs small and favor composition over large monolithic states.
```

### 5.3. Frontmatter Fields
| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `name` | string | yes | Unique skill identifier |
| `description` | string | yes | Human-readable description |
| `version` | string | no | Semver version of the skill |
| `triggers` | string[] | no | Glob patterns that activate this skill when matching files are in context |
| `depends_on` | string[] | no | Other skill names this skill requires |
