# Technical Specifications & Invariant Rules 📐
## QuestDo: Gamified Todo RPG Game

---

## 1. Core Engineering Invariants

| Invariant | Rule | Enforcement |
|-----------|------|-------------|
| **Zero Runtime Errors** | No unhandled exceptions in game logic | Try-catch around all state mutations |
| **Audio Context Safety** | Handle AudioContext unlock on mobile | User gesture required before play |
| **LocalStorage Bounds** | Limit save data to 5MB | Compress older data, prune old tasks |
| **Animation Performance** | Target 60fps | Use CSS transforms, not layout properties |
| **Sound Muting** | Respect user's mute preference | Persist preference, default to ON |

---

## 2. Technical Interface Contracts

### 2.1 Game State Object
```typescript
interface GameState {
  version: string;           // "1.0"
  character: Character;
  xp: XPState;
  streak: StreakState;
  perks: string[];
  tasks: Task[];
  settings: Settings;
  lastSave: string;           // ISO 8601
}

interface Character {
  class: "warrior" | "mage" | "rogue";
  name: string;
  stats: { str: number; int: number; agi: number };
}

interface XPState {
  current: number;           // XP in current level
  total: number;            // Total XP earned
  level: number;            // Current level (starts at 1)
}

interface StreakState {
  current: number;           // Current consecutive days
  longest: number;          // All-time best
  lastCompletedDate: string; // YYYY-MM-DD
  multiplier: number;       // 1.0, 1.5, 2.0, or 3.0
}

interface Task {
  id: string;
  text: string;
  difficulty: "easy" | "medium" | "hard" | "epic";
  completed: boolean;
  completedAt?: string;
  createdAt: string;
}
```

### 2.2 Audio Context Contract
```javascript
// Must be called after user gesture
async function initAudio(): Promise<AudioContext>

// Sound event types
type SoundEvent = "taskComplete" | "levelUp" | "streak7" | "streak14" | "streak30" | "achievement"

// Play a sound event
function playSound(event: SoundEvent): void
```

### 2.3 Visual Effect Contract
```javascript
interface EffectOptions {
  x?: number;               // Origin X (default: center)
  y?: number;               // Origin Y (default: center)
  color?: string;           // Override color
  count?: number;           // Particle count
  duration?: number;        // Effect duration in ms
}

// Trigger visual effect
function triggerEffect(name: string, options?: EffectOptions): void
```

---

## 3. XP & Leveling Formulas

### 3.1 XP for Task Completion
```javascript
const BASE_XP = { easy: 10, medium: 25, hard: 50, epic: 100 };

function calculateXP(task, character, streakMultiplier) {
  let xp = BASE_XP[task.difficulty];
  
  // Class bonuses
  if (character.class === "warrior" && (task.difficulty === "hard" || task.difficulty === "epic")) {
    xp *= 1.2; // +20% XP for warrior on hard/epic
  }
  if (character.class === "mage") {
    xp += character.stats.int; // +INT bonus
  }
  if (character.class === "rogue") {
    xp *= (1 + character.stats.agi / 100); // +AGI% bonus
  }
  
  // Streak multiplier
  xp *= streakMultiplier;
  
  // Level bonus (+5% per level)
  xp *= (1 + (xpState.level - 1) * 0.05);
  
  return Math.floor(xp);
}
```

### 3.2 Level Thresholds
```javascript
const LEVEL_THRESHOLDS = [0, 100, 250, 500, 900, 1450, 2275, 3513, 5370, 8155];

// Level N requires LEVEL_THRESHOLDS[N] total XP
// Level 1 = 0 XP, Level 2 = 100 XP, Level 3 = 250 XP, etc.
```

---

## 4. Streak Multiplier Table

| Streak Days | Multiplier | Bonus |
|-------------|------------|-------|
| 1-6 | 1.0x | None |
| 7-13 | 1.5x | +50% XP |
| 14-29 | 2.0x | +100% XP |
| 30+ | 3.0x | +200% XP |

**Streak Reset Rule:** If `lastCompletedDate !== yesterday`, reset streak to 1 (if completed today) or 0 (if not).

---

## 5. Perk Definitions

| Level | Perk ID | Perk Name | Effect |
|-------|---------|-----------|--------|
| 2 | quick_learner | Quick Learner | +5% XP gain (stacks with base) |
| 3 | double_task | Double Task | First 2 tasks/day give +10 XP bonus |
| 5 | habit_architect | Habit Architect | +10% streak multiplier bonus |
| 7 | power_hour | Power Hour | First task each day = 2x XP |
| 10 | master_achiever | Master Achiever | Unlock epic-tier tasks |

---

## 6. Character Class Stats

| Class | STR | INT | AGI | Passive Ability |
|-------|-----|-----|-----|----------------|
| Warrior | 15 | 8 | 10 | +20% XP on hard/epic tasks |
| Mage | 8 | 15 | 10 | +INT bonus to all XP |
| Rogue | 10 | 10 | 15 | +15% streak multiplier |

---

## 7. Browser Compatibility

| Feature | Minimum Version | Fallback |
|---------|-----------------|----------|
| Web Audio API | All modern browsers | Silent mode |
| CSS Animations | All modern browsers | No effects |
| localStorage | All modern browsers | Session-only |
| CSS Grid | Chrome 57, FF 52, Safari 10.1 | Flexbox |

---

## 8. Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| First Contentful Paint | <1s | Performance API |
| Time to Interactive | <2s | Performance API |
| Animation Frame Rate | 60fps | requestAnimationFrame |
| Memory Usage | <50MB | performance.memory |
| Save Operation | <50ms | Performance.now() |
