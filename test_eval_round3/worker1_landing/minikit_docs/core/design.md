# Design Specification 🎨

## Design System

### Color Palette (Sleek Corporate Theme)
Based on Tailwind CSS with custom design tokens:

```css
/* CSS Variables */
:root {
  --bg-primary: #222831;      /* Dark background */
  --bg-surface: #393E46;       /* Card/surface background */
  --accent: #00ADB5;           /* Primary accent (teal) */
  --accent-hover: #00C9D1;     /* Accent hover state */
  --text-primary: #EEEEEE;     /* Primary text */
  --text-secondary: #9CA3AF;   /* Secondary/muted text */
  --border: #4B5563;           /* Border color */
}
```

### Typography
- **Headings**: Inter (Google Font), bold weight
- **Body**: Inter, regular/medium weight
- **Fallbacks**: system-ui, -apple-system, sans-serif

### Spacing System
- Base unit: 4px (Tailwind default)
- Section padding: py-24 (96px)
- Container max-width: 7xl (1280px)
- Card padding: p-6 to p-8 (24-32px)

### Component Specifications

#### Navbar
- Fixed position, blurred background on scroll
- Logo on left, nav links centered, CTA button on right
- Mobile: hamburger menu with slide-out drawer

#### Hero Section
- Two-column layout (text left, visual right)
- Large heading with gradient text effect
- Two CTA buttons: primary (filled) and secondary (outlined)
- Subtle floating animation on visual element

#### Feature Cards
- Grid: 3 columns on desktop, 2 on tablet, 1 on mobile
- Icon with accent color background
- Title and description
- Subtle hover lift effect

#### Pricing Cards
- 3 columns: Free, Pro (highlighted), Enterprise
- Pro tier has accent border and "Popular" badge
- Feature checklist with check icons
- CTA button at bottom of each card

#### Testimonials
- 2-column grid
- Quote icon, testimonial text, author photo + name + role
- Subtle background differentiation

#### Footer
- 4-column layout on desktop
- Logo, navigation links, social icons
- Copyright notice

## Responsive Breakpoints
- **Mobile**: < 640px (sm)
- **Tablet**: 640px - 1024px (md, lg)
- **Desktop**: > 1024px (xl, 2xl)

## Animations
- Scroll-triggered fade-in for sections
- Hover states with transform and shadow transitions
- Smooth scroll behavior for anchor links
