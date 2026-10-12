# UI & Terminal Design System Specification — `minicode` 🎨

## 1. Core Design Philosophy
- **Stream-Centric, Never Dashboard-Heavy:** `minicode` explicitly avoids rigid bento-box dashboards, complex nested panes, and icon clutter. The terminal interface is designed as an elegant, vertical **interactive stream** matching modern minimalist agent aesthetics.
- **Aura Theme Design System:** Built with the official [Aura Theme](https://github.com/daltonmenezes/aura-theme) color palette by Dalton Menezes.
- **High Information Density & Low Visual Noise:** Tool calls are shown as concise, single-line collapsible folds (`• Running <cmd>` → `✔ You approved minicode to run <cmd>` → `• Ran <cmd>` with indented `└ ...` output details).
- **Fast Visual Recognition:** Immediate visual distinction between user input, assistant thought/plan, code blocks, tool executions, and diff patches.

---

## 2. Terminal Layout Hierarchy (Ratatui)

```text
• Ran git status --short --branch
  └ ## main...origin/main [ahead 1]
    M src/grounding.rs

• Running rustfmt --edition 2021 src/grounding.rs
✔ You approved minicode to run rustfmt --edition 2021 src/grounding.rs this time

• Ran rustfmt --edition 2021 src/grounding.rs
  └ (no output)

• Working (4s • esc to interrupt)
─────────────────────────────────────────────────────────────────────────────
› Implement {feature}
  liquid/lfm-2.5-2.6b:free · ~/programming/my_project · main [default]
```

---

## 3. Component Architecture & State Structure

### 3.1. Top-Level Widgets
1. **`TimelineView` (`src/ui/view.rs`)**:
   - Streaming timeline containing user prompts (`› `), assistant markdown responses (`### `, `• `), tool start notifications, tool approval tags, and folded output blocks.
   - Smooth auto-scrolling with manual `PageUp`/`PageDown` override.
   - Live execution timer (`• Working (Xs • esc to interrupt)`).
2. **`InputDock` (`src/ui/input.rs`)**:
   - Elevated background surface (`#29263c`).
   - Prefix chevron `› ` in Aura Purple (`#a277ff`).
   - `Enter` or `Shift+Enter` for prompt submission.
   - `Ctrl+J` or `Alt+Enter` for inserting newlines.
3. **`StatusWidgets` (`src/ui/status.rs`)**:
   - Single-line bottom bar: `<model> · <workspace_path> · <git_branch> [default]`.
4. **`ConfigMenu` (`src/ui/configure.rs`)**:
   - Step-by-step interactive CLI setup wizard for provider, model, API keys, and approval policy.

---

## 4. Official Aura Color Palette (`src/ui/theme.rs`)

| Token | Hex | RGB | Usage |
| :--- | :--- | :--- | :--- |
| **Primary Background** | `#15141b` | `(21, 20, 27)` | Main terminal background canvas |
| **Elevated Surface** | `#21202e` | `(33, 32, 46)` | Cards, dialog frames |
| **Input Surface** | `#29263c` | `(41, 38, 60)` | Input dock elevated background |
| **Brand Accent (Purple)** | `#a277ff` | `(162, 119, 255)` | Chevrons (`› `), markdown titles, key highlights |
| **Success (Mint Green)** | `#61ffca` | `(97, 255, 202)` | Tool approval checks (`✔`), diff additions (`+`), git branch |
| **Destructive (Coral Red)** | `#ff6767` | `(255, 103, 103)` | Diff deletions (`-`), failed tool outputs, errors |
| **Warning (Warm Orange)** | `#ffca85` | `(255, 202, 133)` | Tool names, model tags in status line |
| **Highlight (Pink)** | `#f694ff` | `(246, 148, 255)` | Markdown H3 section titles (`### `) |
| **Info (Cyan Blue)** | `#82e2ff` | `(130, 226, 255)` | Workspace directory paths, diff headers |
| **Text Primary** | `#edecee` | `(237, 236, 238)` | Body text, prompt content |
| **Muted Slate** | `#8a8a93` | `(138, 138, 147)` | Fold lines, secondary metadata, timers |
| **Subtle Border** | `#3d375e` | `(61, 55, 94)` | Horizontal turn separators, input outline |

---

## 5. Keyboard Navigation & Hotkeys

| Keybinding | Action |
| :--- | :--- |
| `Enter` | Submit prompt / Confirm suggested action |
| `Ctrl + J` | Insert newline in multi-line prompt editor |
| `Alt + Enter` | Insert newline in multi-line prompt editor |
| `Ctrl + C` / `Esc` | Instantly interrupt running stream or cancel running tool process |
| `PageUp` / `PageDown` | Scroll conversation timeline up / down |
| `/clear` | Clear terminal conversation screen |
| `/exit` / `/quit` | Exit `minicode` session cleanly |

---

## 6. Accessibility & Plain Mode

When invoked with `--plain` or `--accessible`, `minicode` bypasses the full-screen `ratatui` alternate buffer entirely and outputs a clean scrolling REPL interface compatible with screen readers and piping.
