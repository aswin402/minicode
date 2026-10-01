# QuestDo — Technical Specification 📋

## Core Features

### 1. Character System
- [x] Class selection: Warrior, Mage, Rogue (radio buttons, animated selection)
- [x] Class flavor text and stat bonuses displayed on selection
- [x] Player name input field
- [x] Avatar display (CSS emoji-based class icons: ⚔️ 🧙 🗡️)
- [x] Persistent state across sessions via localStorage

### 2. Task (Quest) Management
- [x] Add task with title input + Enter key support
- [x] Difficulty selector: Easy (10 XP), Medium (20 XP), Hard (30 XP), Epic (50 XP)
- [x] Task list with completion checkbox
- [x] Complete action: plays sound, adds XP, updates streak, shows particle effect
- [x] Delete task with confirmation (or undo within 5 seconds)
- [x] Empty state with motivational message

### 3. XP & Level System
- [x] XP bar with current/required display (e.g., "235 / 379 XP")
- [x] Smooth CSS animation on XP gain
- [x] Level number badge (glowing effect on level-up)
- [x] Level-up overlay with celebration animation and perks revealed
- [x] Character title by level bracket:
  - Lv 1-4: Apprentice
  - Lv 5-8: Adept
  - Lv 9-12: Master
  - Lv 13+: Legend

### 4. Streak System
- [x] Daily streak counter display
- [x] Multiplier badge: 1x / 1.5x / 2x / 3x
- [x] Streak milestone badges: 7-day, 14-day, 30-day
- [x] Streak freeze info on streak-break recovery
- [x] Per-task streak tracking (individual task chains)

### 5. Level-Up Perks (by class)
**Warrior Perks:**
- Lv 2: "Power Strike" — +5 bonus XP on epic tasks
- Lv 5: "Iron Will" — 1 free streak-save per week
- Lv 8: "Battle Fury" — Double XP on hard tasks on Tuesdays

**Mage Perks:**
- Lv 2: "Quick Study" — Start at Level 2 with 25 bonus XP
- Lv 5: "Arcane Wisdom" — Earn 1.5x XP on medium tasks
- Lv 8: "Spell Master" — Unlock "Meditation" task type (2x streak multiplier)

**Rogue Perks:**
- Lv 2: "Swift Feet" — Streak multiplier activates at 5 days (not 7)
- Lv 5: "Lucky Strike" — 10% chance of 3x random bonus XP
- Lv 8: "Shadow Step" — Complete 3 easy tasks = 1 free epic completion

### 6. Procedural Sound Effects (Web Audio API)
- [x] AudioContext singleton, lazy initialization on first user gesture
- [x] `playComplete()`: Triangle sweep 440→880Hz, 150ms, exponential decay
- [x] `playLevelUp()`: Square wave C4→E4→G4→C5 arpeggio, 600ms total
- [x] `playStreakBonus()`: Sine pulse 523→659→784Hz, 300ms
- [x] `playError()`: Low square 110Hz, 100ms, harsh
- [x] `playDailyReset()`: Sawtooth descend 220→55Hz, 200ms
- [x] Volume control (mute toggle)

### 7. Visual Celebration Effects
- [x] Confetti particle system (Canvas 2D, no library)
  - 60 particles per burst
  - Random colors from palette
  - Gravity + rotation physics
  - 2-second lifespan
- [x] Level-up screen: full-overlay with animated title text
- [x] XP bar fill animation: cubic-bezier spring
- [x] Streak fire effect (CSS animation when streak ≥ 7)
- [x] Task completion: card flip + sparkle effect

### 8. Persistence
- [x] Save to localStorage on every state change
- [x] Load on page init
- [x] Daily reset check: if new day, update streak dates
- [x] Export/import game state as JSON (settings menu)

### 9. UI/UX Details
- [x] Dark fantasy RPG theme with CSS variables
- [x] Responsive layout (mobile-first, max-width 600px)
- [x] Accessible: ARIA labels, keyboard navigation
- [x] Smooth transitions on all interactive elements
- [x] Loading state while fonts/assets initialize

## Acceptance Criteria
1. ✅ User can select a class, enter name, and begin questing — verified via browser automation
2. ✅ Adding and completing tasks correctly awards XP — verified XP math (easy=10, medium=20, hard=30, epic=50)
3. ✅ Level-up triggers sound, particles, and perk display — verified: Lv2 with Quick Study perk
4. ✅ Streak multiplier correctly scales XP gains — verified: 1→1.5→2→3× thresholds
5. ✅ All sounds play without external files — verified: ALL_SOUNDS_OK, no errors
6. ✅ Confetti fires on task complete and level-up — implemented in EffectsEngine
7. ✅ State persists across browser refresh — verified: localStorage saves player, tasks, streak
8. ✅ No console errors in production — verified: browser_debug_logs reports no errors
9. ✅ Works offline (no external dependencies) — verified: all assets inline, no CDN calls
