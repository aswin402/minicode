# Todo Quest - Product Requirements Document

## 1. Concept & Vision

**Todo Quest** is a gamified task management experience where daily tasks become heroic quests. Users create tasks that transform into adventures, earning XP and leveling up their character with each completion. The app combines RPG nostalgia with modern productivity mechanics—making the grind feel like a game worth playing.

**Core Fantasy**: You're an adventurer building your legend, one quest at a time.

## 2. Gamification Mechanics

### XP System
- Base XP per task: 10-50 XP based on difficulty tier
- Bonus XP for completing multiple tasks in a day
- XP is cumulative across all levels

### Leveling System
- Level formula: `level = floor(sqrt(totalXP / 100))`
- Each level requires progressively more XP
- Max level: 99 (reached at ~980,100 XP)

### Quest Tiers
| Tier | Label | Color | XP Reward | Example |
|------|-------|-------|-----------|---------|
| 1 | Minor Errand | Gray | 10 XP | Take out trash |
| 2 | Simple Task | Green | 20 XP | Read 10 pages |
| 3 | Hero's Duty | Blue | 35 XP | Finish project |
| 4 | Epic Quest | Purple | 50 XP | Learn new skill |

### Streak System
- Daily login + task completion maintains streak
- Streak multiplier: `1 + (streakDays * 0.1)`, max 2x at 10+ days
- Streak breaks if no tasks completed for 24 hours
- Visual: Fire icon with intensity based on streak length

## 3. Celebration States

### Completion Celebration
- **Visual**: Confetti burst, golden glow on completed task, progress bar animation
- **Audio**: Satisfying "ding" chime (Web Audio API generated)
- **Duration**: 800ms visual, 500ms audio

### Level Up Celebration
- **Visual**: Full-screen golden flash, animated "+1 Level" banner, character glow
- **Audio**: Triumphant fanfare (3-note ascending chord)
- **Duration**: 2000ms visual, 1500ms audio

### Streak Milestone (7, 30, 100 days)
- **Visual**: Fire particle effects, badge unlock animation
- **Audio**: Special achievement unlock sound
- **Duration**: 2500ms

## 4. Persistence Layer

### localStorage Schema
```json
{
  "todoQuest": {
    "character": { "name": "Hero", "totalXP": 0, "level": 1 },
    "streak": { "current": 0, "lastCompletion": null },
    "tasks": [{ "id": "uuid", "text": "...", "tier": 1, "completed": false, "createdAt": "ISO" }],
    "stats": { "totalCompleted": 0, "tasksToday": 0, "longestStreak": 0 },
    "dailyResetAt": "ISO date string (YYYY-MM-DD)"
  }
}
```

### Daily Reset Logic
- Check on app load: if `dailyResetAt !== today`, reset `tasksToday = 0` and update `dailyResetAt`
- Incomplete tasks persist; completed tasks can be cleared or archived

## 5. Component Architecture

### Core Components
1. **Header/Stats Bar** - Shows level, XP bar, streak count
2. **TaskInput** - Add new quest with tier selector
3. **TaskList** - Active quests with completion toggle
4. **LevelUpOverlay** - Celebration modal on level up
5. **AchievementToast** - Streak milestone notifications

### State Management
- Single source of truth: `gameState` object
- All mutations go through `updateGameState()` which triggers re-render and localStorage sync

## 6. Technical Stack

- **Framework**: Vanilla JS (no dependencies)
- **Styling**: Modular CSS with CSS variables
- **Audio**: Web Audio API (no external files)
- **Storage**: localStorage
- **Build**: Single HTML file with linked CSS/JS modules

## 7. Success Metrics

- Task completion rate visible and motivating
- Level progression feels rewarding (visual + audio feedback)
- Streak encourages daily engagement
- Zero external dependencies = instant load
