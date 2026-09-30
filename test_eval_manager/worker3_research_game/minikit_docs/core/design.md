# Todo Quest - Design Specification

## 1. Visual Design Language

### Aesthetic Direction
**Fantasy RPG meets modern minimalism** — Dark parchment backgrounds, golden accents reminiscent of classic RPGs, clean UI with subtle texture. Think Diablo character screen meets Notion.

### Color Palette
```
Background:   #0F0C1B (deep space purple-black)
Surface:      #1F1A3A (card backgrounds, elevated surfaces)
Accent:       #FFD700 (gold - XP, levels, celebrations)
Accent Alt:   #FF007F (hot pink - streaks, urgent)
Text Primary: #E8E6E3 (off-white, easy on eyes)
Text Muted:   #8B8B8B (secondary info)
Tier Colors:
  - Tier 1: #9CA3AF (gray)
  - Tier 2: #22C55E (green)
  - Tier 3: #3B82F6 (blue)
  - Tier 4: #A855F7 (purple)
```

### Typography
- **Headings**: 'Cinzel', serif (fantasy RPG feel)
- **Body**: 'Inter', sans-serif (clean readability)
- **Monospace accents**: 'JetBrains Mono' for XP numbers

### Spatial System
- Base unit: 8px
- Card padding: 24px
- Gap between cards: 16px
- Border radius: 12px (cards), 8px (buttons), 50% (avatars)

### Motion Philosophy
- **Micro-interactions**: 200ms ease-out for hovers, button presses
- **Celebrations**: 600-800ms with spring easing for bounces
- **Level up**: 2000ms dramatic reveal with staggered text
- **Progress bars**: Smooth 400ms fill animation

## 2. Layout Structure

### Page Architecture
```
┌─────────────────────────────────────────┐
│  HEADER: Character + Level + XP Bar     │
├─────────────────────────────────────────┤
│  STREAK BAR: Fire icon + day count      │
├─────────────────────────────────────────┤
│  ┌─────────────────────────────────┐    │
│  │  TASK INPUT: Input + Tier Select│    │
│  └─────────────────────────────────┘    │
│  ┌─────────────────────────────────┐    │
│  │  TASK CARD 1                    │    │
│  │  [✓] Quest text        [Tier]   │    │
│  └─────────────────────────────────┘    │
│  ┌─────────────────────────────────┐    │
│  │  TASK CARD 2                    │    │
│  └─────────────────────────────────┘    │
│  ...                                    │
└─────────────────────────────────────────┘
```

### Responsive Strategy
- Mobile-first: Single column, full-width cards
- Tablet+: Max-width 600px centered, more breathing room
- Touch targets: Minimum 44px for mobile accessibility

## 3. Component Specifications

### XP Progress Bar
- Height: 8px with rounded ends
- Background: Surface color
- Fill: Gradient from Accent (#FFD700) to #FFA500
- Animation: Width transition 400ms ease-out
- Label: "Level X • Y/Z XP to next level"

### Task Card
- Background: Surface with 1px border (rgba white 10%)
- Left border: 4px colored by tier
- Hover: Subtle lift (translateY -2px) + glow
- Completed: Strike-through text, opacity 0.6, checkmark animation

### Level Up Overlay
- Full viewport modal with backdrop blur
- Center: Large "LEVEL UP!" text with glow
- Subtitle: "You are now Level X"
- Animation: Scale from 0.8 → 1.0, fade in, particle burst

### Streak Counter
- Fire emoji/icon with count
- Color intensity increases with streak length
- Pulse animation when active
- Tooltip: "X day streak! Next milestone: Y days"

## 4. Sound Design (Web Audio API)

### Completion Sound
- Frequency: 880Hz → 1100Hz (pleasant upward jump)
- Duration: 150ms
- Waveform: Sine with quick fade

### Level Up Fanfare
- Notes: C5 (523Hz) → E5 (659Hz) → G5 (784Hz)
- Duration: 100ms per note, slight overlap
- Waveform: Triangle for warm tone

### Streak Milestone
- Ascending arpeggio: C5 → E5 → G5 → C6
- Duration: 80ms per note
- Waveform: Square for bright achievement feel

## 5. State Visual Indicators

### XP Gain Animation
- "+X XP" floats up from completed task
- Fades out over 1 second
- Color: Gold

### Streak Warning
- When streak at risk (接近 midnight): Pulsing red glow
- When broken: Sad trombone (optional) + streak reset to 0

### Empty State
- Illustration: Sword with "No quests yet" text
- CTA: "Add your first quest to begin your adventure!"

## 6. File Structure

```
/todo-quest/
├── index.html          # Main HTML structure
├── styles.css          # All styling with CSS variables
├── app.js              # Application logic
├── audio.js            # Web Audio sound generation
└── README.md           # Quick start guide
```
