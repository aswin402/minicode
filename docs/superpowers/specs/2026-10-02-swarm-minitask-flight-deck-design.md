# Swarm MiniTask Integration & Dynamic Multi-Style Flight Deck — Design Specification

**Phase:** Phase 153  
**Status:** Approved  
**Version Target:** `minicode v0.3.45`  
**Authors:** Aswin (Lead), Antigravity AI Engine  
**Review Target:** Dual-mode human/AI agent operations (`minitask`, `/tasks`, `/theme`)

---

## 1. Executive Summary & Vision

In Phase 152 (`v0.3.44`), we introduced **Swarm Cross-Worker Inter-Agent Messaging**, allowing concurrent subagents in isolated Git worktrees to communicate asynchronously over a durable message bus (`bus.jsonl`).

However, during execution, developers and AI supervisors face a visibility blindspot:
1. **Human Developer:** A plain CLI stdout stream interleaves log lines between multiple workers, obscuring task progress, bottlenecks, and real-time inter-worker contract negotiations.
2. **AI Agent (minicode):** The agent needs a standardized mechanism to observe running swarms, query worker heartbeats, and inspect live communication without shelling out to raw terminal utilities.

**Phase 153** bridges this gap by unifying Swarm execution with **`minitask`** (minicode's process registry and supervisor) and introducing an interactive **Swarm Flight Deck** inside the Task Tracker modal (`/tasks`, `Ctrl+T`, `F7`).

### Core Value Pillars
- **Unified Supervisor (`minitask`):** Swarm orchestrators and child workers register directly into `MiniDevRegistry` as `DevProcessType::Swarm`. Both AI agents (`minitask(action="list")`) and humans can observe and manage them.
- **Dynamic Theming (100% Theme-Adaptive):** All visual components bind dynamically to the active `Theme` (`Catppuccin`, `TokyoNight`, `Nord`, `Dracula`, `Soft Dark`, etc.). No hardcoded ANSI escape sequences.
- **5 Selectable TUI Styles:** The user can select their preferred aesthetic in `/theme` settings or `/swarm-style`, with **Style 3 (Minimal + Stylish)** as the default:
  1. `stylish` (Minimal + Stylish / Nordic Neo-TUI) — **DEFAULT ⭐**
  2. `gitgraph` (Minimal + GitGraph Pipeline / Topological DAG)
  3. `modern` (Clean Modernist / Floating Card Aesthetic)
  4. `minimal` (Ultra-Minimalist / Whitespace Tree)
  5. `cockpit` (High-Density Cockpit / 3-Column Split)
- **Live Message Bus Tail:** Real-time rendering of `bus.jsonl` transactions (`query_interface`, `publish_contract`, `coordination_note`) as visual message bubbles inside the TUI.

---

## 2. Architecture & Data Flow

```
                                  ┌────────────────────────────────┐
                                  │      SwarmScheduler (CLI)      │
                                  └───────────────┬────────────────┘
                                                  │
                 ┌────────────────────────────────┴────────────────────────────────┐
                 ▼                                                                 ▼
   ┌───────────────────────────┐                                     ┌───────────────────────────┐
   │    Spawn Child Workers    │                                     │  Register in MiniTask     │
   │  (Git Worktree Processes) │                                     │      (DevRegistry)        │
   └─────────────┬─────────────┘                                     └─────────────┬─────────────┘
                 │                                                                 │
                 ▼                                                                 ▼
   ┌───────────────────────────┐                                     ┌───────────────────────────┐
   │    SwarmMessageBus        │◀─────────── Tailed in Real Time ─── │  ProcessesTab::Swarm      │
   │      (bus.jsonl)          │                                     │  (Interactive Flight Deck)│
   └───────────────────────────┘                                     └─────────────┬─────────────┘
                                                                                   │
                                                     ┌─────────────────────────────┴─────────────────────────────┐
                                                     ▼                                                           ▼
                                       ┌───────────────────────────┐                               ┌───────────────────────────┐
                                       │   Human TUI (/tasks)      │                               │   AI Agent Tools          │
                                       │   Active Swarm Style:     │                               │   minitask(action="list") │
                                       │   Stylish | GitGraph |... │                               │   minitask(action="status")│
                                       └───────────────────────────┘                               └───────────────────────────┘
```

---

## 3. Configuration & Theme Settings

### 3.1 `UiConfig` Additions (`src/config.rs`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,

    #[serde(default = "default_animation")]
    pub animation: String,

    #[serde(default = "default_todo_style")]
    pub todo_style: String,

    #[serde(default = "default_swarm_style")]
    pub swarm_style: String,

    #[serde(default)]
    pub plain: bool,
}

fn default_swarm_style() -> String {
    "stylish".to_string()
}
```

### 3.2 Swarm Dashboard Style Enum
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SwarmDashboardStyle {
    #[default]
    Stylish,
    GitGraph,
    Modern,
    Minimal,
    Cockpit,
}

impl SwarmDashboardStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stylish => "stylish",
            Self::GitGraph => "gitgraph",
            Self::Modern => "modern",
            Self::Minimal => "minimal",
            Self::Cockpit => "cockpit",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "gitgraph" | "git" | "dag" | "pipeline" => Self::GitGraph,
            "modern" | "cards" | "floating" => Self::Modern,
            "minimal" | "clean" | "tree" => Self::Minimal,
            "cockpit" | "dense" | "mission" => Self::Cockpit,
            _ => Self::Stylish,
        }
    }
}
```

### 3.3 `/theme` Modal 4th Tab (`SwarmStyles`)
`ThemeModalTab` in `src/ui/modals/mod.rs`:
```rust
pub enum ThemeModalTab {
    Themes,
    Animations,
    TodoStyles,
    SwarmStyles,
}
```
In `src/ui/modals/theme_select.rs`:
`SWARM_STYLE_OPTIONS` provides live previews of all 5 styles. Navigation between tabs is seamless via `Left`/`Right` or `Tab`.

---

## 4. MiniDev / MiniTask Integration

### 4.1 Process Category Expansion (`src/dev/models.rs`)
```rust
pub enum DevProcessType {
    Frontend,
    Backend,
    Script,
    Docker,
    Chrome,
    Worker,
    Cron,
    Timer,
    /// Autonomous multi-agent swarm or swarm worker
    Swarm,
}
```

### 4.2 Swarm Scheduler Process Registration (`src/agent/swarm/scheduler.rs`)
1. **Swarm Parent Registration:** Upon plan execution start, `SwarmScheduler` registers the swarm into `MiniDevRegistry` with metadata:
   - `id`: `format!("swarm-{}", plan.id)`
   - `name`: `plan.title`
   - `process_type`: `DevProcessType::Swarm`
   - `command`: `format!("minicode swarm run --auto-merge {}", plan.title)`
2. **Worker Registration:** When each worker process spawns:
   - `id`: `format!("worker-{}", task.id)`
   - `name`: `task.role_title`
   - `process_type`: `DevProcessType::Swarm`
   - `command`: `format!("worker: {}", task.id)`
3. **Heartbeat & Log Ring-Buffer:** Child worker process stdout is hooked to the ring-buffer log recorder so `minitask(action="logs", id="worker-t1")` streams real-time worker logs.
4. **Cleanup:** On completion, processes are cleanly marked as completed in the registry.

---

## 5. The 5 Swarm TUI Styles & Layouts

All styles take `frame: &mut Frame`, `area: Rect`, `theme: &Theme`, and `data: &SwarmDeckData`.

### Style 1: Modernist (`modern`)
- **Container:** Rounded borders (`╭─╮`) using `theme.border`.
- **Worker Cards:** 2-column or 3-column split cards with status pills `[RUNNING 65%]` in `theme.success` and target files in `theme.muted`.
- **Bus Feed:** Bordered box at bottom with pill tags `[❓ QUERY]`, `[📜 CONTRACT]`.

### Style 2: Ultra-Minimalist (`minimal`)
- **Container:** Zero borders. Pure whitespace indentation.
- **Worker Nodes:** Tree guide lines (`│`, `├─`, `└─`) in `theme.muted`.
- **Progress:** Minimal text fractions (`turn 3/5 · 42s · 14.2k tokens`).
- **Bus Feed:** Indented list with clean ASCII arrow symbols (`──?`, `──!`).

### Style 3: Minimal + Stylish (`stylish`) — **DEFAULT ⭐**
- **Container:** Thin square borders (`┌─┐`) in `theme.border`.
- **Header:** Electric title banner in `theme.secondary` and `theme.primary` with time pill.
- **Worker Rows:** Micro-progress meters (`━╾────────────── 50%`) with cursor `❯` in `theme.primary`.
- **Bus Feed:** Dedicated message section with indented quote bars and intent badges in `theme.primary` and `theme.warning`.

### Style 4: Minimal + GitGraph Pipeline (`gitgraph`)
- **Container:** Clean top & bottom borders.
- **Pipeline Nodes:** Literal topological branch tracks:
  - Trunk: `● [HEAD: main]` in `theme.primary`
  - Forking branches: `├─┬─● [t1_calculator]` in `theme.success`
  - Comms arcs: `│ ├───💬 query_interface t2 ──▶ t1` in `theme.warning`
  - Auto-merge barrier: `├───● [auto-merge] Synchronization Barrier`
  - Downstream wave: `└───○ [t3_tests] ⏳ Pending (Wave 2)` in `theme.muted`
- **Interactive Node Inspector:** Bottom card showing worktree path, files modified, and last turn action of the focused node.

### Style 5: High-Density Cockpit (`cockpit`)
- **Container:** Single-screen 3-column split layout:
  - **Column 1 (Left, 30%):** Worker pool with turn count, duration, tokens, and active tool call.
  - **Column 2 (Middle, 35%):** Live inter-worker message bus tail (`bus.jsonl`).
  - **Column 3 (Right, 35%):** Live streaming output / stdout preview of the selected worker.

---

## 6. Interactive Keyboard Controls

Inside `/tasks` on the `[Swarm]` tab:
- `Tab`: Switch between `/tasks` tabs (`All`, `Servers`, `Workers`, `Logs`, `Telemetry`, `Swarm`).
- `↑` / `↓`: Navigate between workers or DAG nodes.
- `Space` / `Enter`: Open fullscreen log viewer for the selected worker.
- `m`: Open dedicated inter-worker message bus modal.
- `k`: Terminate the selected worker (sends SIGTERM via `MiniDevRegistry`).
- `r`: Open preview of `REPORT.md` (Swarm Executive Report).
- `s`: Cycle swarm style (`stylish` -> `gitgraph` -> `modern` -> `minimal` -> `cockpit`).
- `Esc` / `q`: Close modal and return to main chat / TUI.

---

## 7. Quality Gates & Invariants

1. **Tool Count Invariant:** `TOTAL_TOOL_COUNT` = 186 in `src/constants.rs` remains untouched.
2. **Pure Rust Networking & Tokio:** Zero blocking disk I/O on async threads.
3. **No `.unwrap()` or `.expect()`:** Complete error resilience via `thiserror` and `anyhow::Result`.
4. **Theme Dynamic Integrity:** Zero hardcoded colors; all renderers query `&Theme`.
5. **Concurrency Safety:** Cargo commands use `-j 1` on check/test/clippy; `-j 2` on release builds.
