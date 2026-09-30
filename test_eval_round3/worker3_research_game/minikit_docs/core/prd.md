# Product Requirements Document (PRD) 🚀
## QuestDo: Gamified Todo RPG Game

### Project Overview
**Project:** `worker3_research_game` — QuestDo Gamified Todo RPG  
**Type:** Interactive browser-based productivity game  
**Core Concept:** Transform mundane todo lists into an engaging RPG adventure with character progression, streaks, and sound feedback

---

## 1. Core Objectives & User Stories

### User Stories
- **As a user**, I want to create, edit, and complete todos that award XP so I feel rewarded for productivity
- **As a user**, I want to choose a character class (Warrior, Mage, Rogue) with unique stats affecting gameplay
- **As a user**, I want to maintain daily streaks that multiply XP rewards to build consistent habits
- **As a user**, I want to level up and unlock perks that enhance my gameplay experience
- **As a user**, I want to hear procedural sound effects that provide satisfying audio feedback
- **As a user**, I want to see visual celebration effects on achievements to feel accomplished

### Core Objectives

| ID | Objective | Success Criteria |
|----|-----------|------------------|
| O1 | **Character System** | 3 character classes with distinct base stats and passive bonuses |
| O2 | **XP & Leveling** | Dynamic XP bars with level-scaled progression curve |
| O3 | **Streak Mechanics** | Daily streak tracking with multiplier bonuses (1.5x at 7 days, 2x at 14 days) |
| O4 | **Perk System** | Level-gated perks that provide tangible gameplay benefits |
| O5 | **Sound Synthesis** | Procedural Web Audio sounds for task completion, level-up, streak milestones |
| O6 | **Visual Effects** | Particle explosions, screen flashes, and animated celebrations |

---

## 2. Gamification Mechanics Research

### 2.1 XP (Experience Points) System
Based on Self-Determination Theory and XP Habit Ladder research:

| Difficulty | Base XP | Examples |
|------------|---------|----------|
| Easy | 10 XP | Quick tasks, 5-minute chores |
| Medium | 25 XP | Standard tasks, 15-30 min work |
| Hard | 50 XP | Complex tasks, 1+ hour projects |
| Epic | 100 XP | Major achievements, multi-day goals |

**Leveling Curve (Exponential):**
- Level N requires: `baseXP * (1.5 ^ (N-1))` total XP
- Level 1→2: 100 XP | Level 5→6: ~759 XP | Level 10→11: ~5,764 XP

### 2.2 Habit Streak System
Research from Duolingo & Trophy data shows:
- **Streak preservation**: "Lazy Day" variant (2-min task) preserves streak without XP
- **Streak tiers**: Daily (1), Weekly (7), Monthly (30)
- **Multipliers**:
  - 7-day streak: 1.5x XP bonus
  - 14-day streak: 2x XP bonus  
  - 30-day streak: 3x XP bonus

### 2.3 Character Classes

| Class | Base Stats | Passive Bonus | Playstyle |
|-------|------------|---------------|-----------|
| **Warrior** | STR: 15, INT: 8, AGI: 10 | +20% XP from hard/epic tasks | Steady grind, high yield |
| **Mage** | STR: 8, INT: 15, AGI: 10 | +1 bonus XP per level | Consistent, reliable |
| **Rogue** | STR: 10, INT: 10, AGI: 15 | +10% streak bonus | Fast accumulation |

### 2.4 Level-Up Perks

| Level | Perk | Effect |
|-------|------|--------|
| 2 | Quick Learner | +5% XP gain |
| 3 | Double Task | Can complete 2 tasks/day for bonus XP |
| 5 | Habit Architect | +10% streak multiplier |
| 7 | Power Hour | First task each day = 2x XP |
| 10 | Master Achiever | Unlock epic-tier tasks |

---

## 3. Technical Implementation

### 3.1 Web Audio Sound Design
**Procedural Synthesis (No External Assets):**

| Event | Sound Type | Synthesis |
|-------|-----------|-----------|
| Task Complete | Bright chime | Sine wave, 523Hz→1047Hz sweep, 150ms |
| Level Up | Triumphant fanfare | 3 stacked oscillators, major chord arpeggio |
| Streak Milestone | Victory burst | Sawtooth + filter sweep, 300ms |
| Achievement Unlock | Crystal ding | Triangle wave, high frequency, reverb |

**Audio Chain:**
```
OscillatorNode → GainNode → BiquadFilterNode → AudioContext.destination
```

### 3.2 Visual Effects
- **Task Complete**: Green checkmark pulse + particle burst
- **Level Up**: Golden radial glow + floating "+1 LEVEL" text
- **Streak**: Fire/flame particles along streak counter
- **Achievement**: Trophy icon bounce + confetti explosion

---

## 4. Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Task Completion Rate | >70% | Completed/Total tasks |
| Streak Retention | >60% | Users maintaining 7+ day streaks |
| Sound Engagement | >50% | Users enabling sound |
| Level Progression | 1 level/week | Average user level gain |

---

## 5. Non-Goals
- No multiplayer/leaderboard (single-player focus)
- No persistent cloud storage (localStorage only)
- No external audio file dependencies
