# Todo Quest

A gamified task management game where completing daily tasks earns XP and levels up your character.

## Features

- **XP & Leveling System** - Earn XP for completing tasks, level up your character
- **Quest Tiers** - Choose difficulty: Minor (10 XP), Simple (20 XP), Heroic (35 XP), Epic (50 XP)
- **Streak Tracking** - Build daily streaks with XP multiplier bonuses (up to 2x at 10+ days)
- **Celebration States** - Visual confetti, floating XP popups, and triumphant sound effects
- **Local Persistence** - All progress saved to localStorage, survives browser refresh
- **Responsive Design** - Works on desktop and mobile

## Quick Start

### Option 1: Local File Opening
Simply open `index.html` in a modern browser. Due to ES module restrictions, you may need a local server.

### Option 2: Local Development Server
```bash
# Using Python
python3 -m http.server 8080

# Using Node.js (npx)
npx serve

# Using Bun
bunx serve
```

Then visit `http://localhost:8080`

## How to Play

1. **Create a Quest** - Enter a task and select its difficulty tier
2. **Complete Tasks** - Click the checkbox when you finish a task
3. **Earn XP** - Each completion awards XP with optional streak multiplier
4. **Level Up** - Accumulate XP to level up your character
5. **Build Streaks** - Complete at least one task daily to maintain your streak

## Game Mechanics

| Tier | XP Reward |
|------|-----------|
| Minor | 10 XP |
| Simple | 20 XP |
| Heroic | 35 XP |
| Epic | 50 XP |

### Streak Multiplier
- 1-9 days: 1.0 + (days × 0.1)
- 10+ days: 2.0 (max)

### Level Formula
```
level = floor(sqrt(totalXP / 100)) + 1
```

## Tech Stack

- Vanilla JavaScript (ES Modules)
- CSS Custom Properties
- Web Audio API (no external audio files)
- localStorage for persistence
- Canvas API for confetti effects

## Browser Support

- Chrome 80+
- Firefox 75+
- Safari 14+
- Edge 80+

## License

MIT
