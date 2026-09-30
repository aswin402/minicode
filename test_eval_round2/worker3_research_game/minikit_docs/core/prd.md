# Product Requirements Document (PRD) 🚀

## Project Overview
*Project: `worker3_research_game` — Interactive web-based "Todo Quest" productivity game.*
*Built with: Vanilla HTML/CSS/JS (no framework), Web Audio API, localStorage.*

---

## 1. Concept & Vision

**Todo Quest** is a gamified task management experience that transforms mundane to-do items into heroic adventures. Players create "quests" (tasks) that, when completed, reward experience points (XP), build daily streaks, and unlock levels. The experience is rooted in **productivity psychology** — specifically BJ Fogg's Behavior Model, Nir Eyal's Hook Model, and Charles Duhigg's Habit Loop — wrapped in a dark-fantasy aesthetic with warm gold accents reminiscent of ancient tomes and treasure maps.

The emotional core: every completed task feels like a small victory, punctuated by procedural audio celebrations. Streaks create identity investment ("I am someone who finishes things"), and level-ups provide long-term aspirational hooks.

---

## 2. Gamification Mechanics Research

### 2.1 Psychological Foundations

#### The Habit Loop (Charles Duhigg)
Every habit consists of three stages, all of which Todo Quest explicitly addresses:
- **Cue** → A new day resets the quest board. A blinking "New Quest" button or streak counter acts as a "trigger" reminding the player to engage.
- **Routine** → Completing a quest is the core behavior. It must feel effortless and rewarding.
- **Reward** → Immediate feedback: a chime sound, a satisfying checkmark animation, XP counter ticking up, and the quest card fading out. Streak counters reinforce the investment.

#### BJ Fogg's Behavior Model (B = MAP)
For a behavior to occur: **Motivation + Ability + Prompt** must converge at the same moment.
- **Motivation**: XP, levels, streak multipliers, achievement badges, lore snippets unlocked at milestones.
- **Ability**: Quest creation is one-field simple ("Enter your quest..."). Completion is a single click.
- **Prompt**: Daily streak display, pulsing "Complete Quest" button, ambient XP counter.

#### Nir Eyal's Hook Model (Trigger → Action → Variable Reward → Investment)
- **Trigger**: Time-based (daily reset) and emotional (low streak = anxiety → action).
- **Action**: The simplest behavior: type quest title + click "Accept Quest".
- **Variable Reward**: XP is deterministic, but level-up fanfare timing is unpredictable. Streak bonuses are variable (multiplier increases with streak length).
- **Investment**: The longer the player maintains their streak, the more they have to lose — creating powerful identity investment.

### 2.2 XP Formula

```
base_xp = 50                          # Base XP per completed quest

streak_multiplier = min(1 + (current_streak_days * 0.1), 3.0)
# Streak 0→1: 1.1x | Streak 5: 1.5x | Streak 10: 2.0x | Streak 20+: 3.0x (cap)

xp_gained = floor(base_xp * streak_multiplier)
```

**Examples:**
| Streak Days | Multiplier | XP Earned |
|---|---|---|
| 0 | 1.0x | 50 |
| 1 | 1.1x | 55 |
| 5 | 1.5x | 75 |
| 10 | 2.0x | 100 |
| 20+ | 3.0x | 150 (cap) |

### 2.3 Level Progression

Levels are cumulative XP thresholds. Each level unlocks a sense of progression and milestones.

```
level_thresholds = [0, 100, 250, 500, 900, 1500, 2300, 3400, 5000, 7200, 10200, ...]
# Formula: thresholds[n] = 50 * n * (n + 1)  (triangular numbers * 2)

level = largest index where total_xp >= level_thresholds[index]
xp_progress = (total_xp - level_thresholds[level]) / (level_thresholds[level+1] - level_thresholds[level])
```

**Level Table (first 15 levels):**
| Level | Title | Total XP Required |
|---|---|---|
| 1 | Novice Adventurer | 0 |
| 2 | Apprentice | 100 |
| 3 | Journeyman | 250 |
| 4 | Skilled Seeker | 500 |
| 5 | Seasoned Hero | 900 |
| 6 | Veteran Explorer | 1,500 |
| 7 | Elite Champion | 2,300 |
| 8 | Master Tactician | 3,400 |
| 9 | Grand Strategist | 5,000 |
| 10 | Legendary Legend | 7,200 |
| 11 | Mythic Overlord | 10,200 |
| 12 | Ascended Sage | 13,600 |
| 13 | Celestial Guardian | 17,400 |
| 14 | Eternal Archon | 21,600 |
| 15+ | Divine Overmind | continuing pattern |

### 2.4 Streak System

**Streak Rules:**
- A streak is maintained if the player completes at least 1 quest per calendar day (UTC).
- Streak resets to 0 if a day is missed.
- On app load, if `lastActiveDate` < today (UTC), streak is checked:
  - If `lastActiveDate` == yesterday: streak continues.
  - Otherwise: streak resets to 0.
- Streak counter is prominently displayed as both a number and a flame icon.

**Streak Milestone Bonuses:**
| Streak Days | Bonus Description |
|---|---|
| 3 | Bronze Streak badge |
| 7 | Silver Streak badge + subtle bonus visual |
| 14 | Gold Streak badge |
| 30 | Platinum Streak badge |
| 100 | Diamond Streak badge + special fanfare |

### 2.5 Celebration Feedback System

Celebrations are multi-layered to create a dopamine cascade:

#### Layer 1 — Quest Completion (every quest)
- **Visual**: Card fades out (opacity 0, translateY -20px, 400ms ease-out). A green checkmark "pops" in.
- **Audio**: Completion chime — a rising two-note arpeggio (C5→E5→G5) in 300ms.
- **XP Fly-up**: A "+N XP" badge floats up from the quest card and fades at the top.

#### Layer 2 — Level-Up (when crossing a level threshold)
- **Visual**: Full-screen golden flash overlay (opacity 0.3 → 0, 600ms). Large level-up text centered.
- **Audio**: Fanfare — a triumphant 6-note ascending melody with reverb-like decay (Web Audio API oscillator).
- **UI**: Level badge pulses with glow animation. Level-up modal briefly shows title earned.

#### Layer 3 — Streak Milestone (3, 7, 14, 30, 100 days)
- **Visual**: Fire/flame particle animation behind the streak counter.
- **Audio**: Special descending then ascending arpeggio (royal fanfare).
- **UI**: Streak badge animates in with golden glow.

#### Layer 4 — Streak Failure Recovery
- **Visual**: Streak resets with a brief red flash on the counter.
- **Audio**: Subtle descending tone (not punishing, just informative).
- **UI**: Encouraging message: "A new day brings new possibilities. Start fresh!"

### 2.6 Achievement Badges

Badges are visual milestones stored in localStorage. Types:
- **Streak Badges**: 3-day, 7-day, 14-day, 30-day, 100-day
- **Level Badges**: Level 5, 10, 15, 20
- **Quest Count Badges**: 10 quests, 50 quests, 100 quests, 500 quests

---

## 3. localStorage Data Schema

All game state is persisted in `localStorage` under the key `todoquest_save`.

```json
{
  "version": "1.0",
  "player": {
    "xp": 0,
    "level": 1,
    "title": "Novice Adventurer",
    "total_quests_completed": 0,
    "current_streak": 0,
    "longest_streak": 0,
    "last_active_date": "2025-01-15",
    "badges": ["streak_3", "level_5"]
  },
  "quests": [
    {
      "id": "uuid-v4-string",
      "title": "Write unit tests for auth module",
      "completed": false,
      "created_at": "2025-01-15T10:00:00Z"
    }
  ]
}
```

**Key Implementation Notes:**
- `last_active_date` is stored as ISO date string in UTC.
- Quest `id` is generated via `crypto.randomUUID()`.
- `completed` quests are removed from the array on completion (not flagged).
- `badges` array stores string IDs of earned badges.
- Schema version is stored to enable future migrations.

---

## 4. Feature List

### Core Features
1. **Quest Creation**: Single-field input ("Accept Quest" button) with instant feedback.
2. **Quest List**: Scrollable list of active (incomplete) quests with checkmark-to-complete.
3. **XP System**: Real-time XP gain with animated counter and fly-up effect.
4. **Level System**: Level display with progress bar and title.
5. **Streak System**: Daily streak counter with streak-break detection.
6. **Celebration Engine**: Multi-layer celebrations (quest, level-up, streak milestone).
7. **Procedural Audio**: Web Audio API engine — chimes, fanfares, level-up (no external files).
8. **localStorage Persistence**: Auto-save on every state change, auto-load on startup.
9. **Reset Option**: Button to clear all data and start fresh.

### UI Components
1. **Header**: Title, current level badge, XP progress bar.
2. **Stats Bar**: Streak flame, total quests completed counter, level title.
3. **Quest Input**: Styled input field + "Accept Quest" button.
4. **Quest Board**: Scrollable list of quest cards.
5. **Quest Card**: Title, completion checkbox with animation, hover effects.
6. **Level-Up Modal**: Overlay celebrating level transitions.
7. **Streak Celebration**: Particle fire effect on streak milestones.
8. **Footer**: Reset button, lore flavor text.

---

## 5. Design Language

### Aesthetic: Dark Fantasy / Ancient Tome
Inspired by fantasy RPG interfaces, treasure maps, and candlelit libraries.

### Color Palette (MiniBlocks "Midnight Coffee" + custom gold)
| Token | Hex | Usage |
|---|---|---|
| `--bg` | `#12100E` | Page background (deep dark brown-black) |
| `--surface` | `#1E1A16` | Card backgrounds |
| `--surface-raised` | `#2A2520` | Hover states, elevated cards |
| `--accent` | `#D4A017` | Gold accent — XP, buttons, borders |
| `--accent-bright` | `#FFD700` | Highlights, level-up flash |
| `--text` | `#F0E6D3` | Primary text (warm parchment) |
| `--text-muted` | `#8A7F72` | Secondary text, labels |
| `--success` | `#4ADE80` | Completion checkmark |
| `--danger` | `#EF4444` | Reset/clear actions |
| `--streak-flame` | `#FF6B35` | Streak fire color |

### Typography
- **Primary Font**: `Cinzel` (Google Fonts) — for headings, level titles (fantasy serif).
- **Body Font**: `Crimson Text` (Google Fonts) — for quest titles, body text (readable serif).
- **Mono Accent**: `Courier Prime` — for XP numbers (typewriter feel).

### Spatial System
- Base unit: 8px
- Card padding: 16px
- Section gaps: 24px
- Border radius: 8px (cards), 4px (inputs/buttons)

### Motion Philosophy
- All transitions: 200-400ms, `ease-out` or custom cubic-bezier
- Quest completion: card fade + fly-up XP badge
- Level-up: full-screen golden pulse + modal
- Hover: subtle scale(1.02) and shadow lift
- XP counter: smooth number tick animation

---

## 6. Technical Approach

### File Structure
```
worker3_research_game/
├── index.html       # UI structure, Google Fonts, CSS
├── styles.css       # All styles (embedded in index.html or separate)
├── audio.js         # Web Audio API procedural sound engine
├── app.js           # Game logic, state management, rendering
├── minikit_docs/core/prd.md  # This document
└── minikit_docs/core/todo.md # Task tracker
```

### Dependencies
- **Google Fonts**: Cinzel, Crimson Text, Courier Prime (CDN)
- **No external JS libraries** — pure vanilla JS
- **No external audio files** — all sound procedural via Web Audio API

### Audio Engine (audio.js)
- Single `AudioEngine` class
- `playChime()`: Quest completion (rising arpeggio)
- `playLevelUp()`: Level threshold crossed (fanfare)
- `playStreakMilestone()`: Streak bonus achieved
- `playStreakBreak()`: Streak lost
- All sounds generated with OscillatorNode + GainNode + BiquadFilterNode

### Game Engine (app.js)
- `GameState` class: XP, level, streak, quests array
- `render()`: Re-renders entire UI from state
- Event delegation on quest list for performance
- Auto-save to localStorage on every mutation
- Date comparison logic for streak (UTC day boundary)
