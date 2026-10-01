# QuestDo — Task Tracker 📋

## Implementation Plan

### Phase 1: Core Game Foundation
- [x] **1.1** Create `index.html` with HTML skeleton, meta tags, and CSS custom properties for dark fantasy theme
- [x] **1.2** Implement `GameState` singleton with player, tasks, streak state and localStorage persistence
- [x] **1.3** Build `SoundEngine` module with AudioContext, lazy init, and 5 procedural sound functions
- [x] **1.4** Create `index.css` with dark fantasy theme, responsive layout, XP bar styles, and animation keyframes

### Phase 2: UI & Rendering
- [x] **2.1** Build character class selection screen (Warrior/Mage/Rogue with flavor text and icons)
- [x] **2.2** Implement task input form with difficulty selector and keyboard support
- [x] **2.3** Create task list renderer with completion, streak display, and delete buttons
- [x] **2.4** Build XP bar component with smooth fill animation and level badge
- [x] **2.5** Add streak counter display with multiplier badge and fire effect

### Phase 3: Game Logic
- [x] **3.1** Implement XP system: `gainXP()`, level-up detection, character titles
- [x] **3.2** Build streak engine: `updateStreak()`, `getMultiplier()`, daily reset check
- [x] **3.3** Implement task completion flow: XP calculation with multiplier + class bonus, sound + effects
- [x] **3.4** Build perk system: class-specific perks by level threshold

### Phase 4: Visual Effects
- [x] **4.1** Implement canvas confetti particle system (60 particles, gravity physics, 2s lifespan)
- [x] **4.2** Create level-up overlay with animated title, perk reveal, and dismiss button
- [x] **4.3** Add task completion card animation and XP float text
- [x] **4.4** Implement streak fire animation (CSS keyframes) when streak ≥ 7

### Phase 5: Polish & Persistence
- [x] **5.1** Wire localStorage save/load for all state
- [x] **5.2** Add settings menu: volume toggle, export/import JSON
- [x] **5.3** Implement daily login reset: detect new day, update streaks
- [x] **5.4** Add accessibility: ARIA labels, focus management, keyboard shortcuts

### Phase 6: Testing & Verification
- [x] **6.1** Serve game with `python3 -m http.server` on port 8765
- [x] **6.2** Verify with browser automation: class selection, task add, task complete, XP gain, level-up
- [x] **6.3** Verify streak multiplier and class bonuses compute correctly
- [x] **6.4** Verify localStorage persistence across page reloads
- [x] **6.5** Verify all 5 sound effects play without external files
- [x] **6.6** Check for console errors
- [x] **6.7** Close test server and browser sessions

## Verification Results (Browser Automation)

| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| Class selection (Mage) | Mage → Lv1, +25 XP | Lv1, 25 XP | ✅ |
| Easy task XP | +10 XP (Mage) | 35 / 75 XP | ✅ |
| Medium task XP | +20 XP → 55 XP | 55 / 75 XP | ✅ |
| Epic task XP | +50 XP → 105 XP, Lv2 | 105 / 112 XP, Lv2 | ✅ |
| Level-up perk unlock | Quick Study for Mage | Quick Study chip visible | ✅ |
| localStorage persistence | Save on every change | Saves player, tasks, streak | ✅ |
| Sound effects (5 types) | All play without errors | ALL_SOUNDS_OK | ✅ |
| No console errors | Clean runtime | No errors detected | ✅ |

## Files Created
- `index.html` — HTML shell with splash loader
- `audio.js` — Web Audio API procedural synth (5 sound types, safe guards)
- `effects.js` — Canvas confetti, XP float text, level-up overlay
- `state.js` — Game state, XP engine, streak logic, perks, persistence
- `app.js` — UI rendering, event binding, onboarding, game screen

## Technical Notes
- Audio: `safePlay()` guard prevents errors when AudioContext is suspended (headless)
- XP curve: `XP(L) = 50 × 1.5^(L-1)` — exponential scaling
- Streak multiplier: 1× → 1.5× (7d) → 2× (14d) → 3× (30d), Rogue activates 30% sooner
- Class bonuses: Warrior +20% hard/epic; Mage +1.5× medium (Lv5 perk)
- Cache-busting: `?v=2` query param on script URLs for development
