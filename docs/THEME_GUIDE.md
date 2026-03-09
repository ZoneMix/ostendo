# Ostendo Theme Guide

## Available Themes

### Dark Themes

| Slug | Name | Background | Accent | Text | Best For |
|------|------|------------|--------|------|----------|
| `terminal_green` | Terminal Green | #0D0D0D (near black) | #00FF88 (bright green) | #E0E0E0 | Hacker aesthetic, CTF talks |
| `amber_warning` | Amber Warning | #0D0D0D | #FFB000 (amber) | #E0E0E0 | Retro terminal, warnings |
| `arctic_blue` | Arctic Blue | #0A0E17 (dark navy) | #00D4FF (cyan) | #E0E0E0 | Clean tech presentations |
| `blood_moon` | Blood Moon | #1A0000 (deep red-black) | #CC0000 (red) | #E0C0C0 | Security, red team |
| `blueprint` | Blueprint | #0A1628 (dark blue) | #5BA4CF (soft blue) | #E0E0E0 | Architecture, planning |
| `catppuccin` | Catppuccin | #1E1E2E (mocha base) | #CBA6F7 (mauve) | #CDD6F4 | General purpose, popular |
| `cyber_red` | Cyber Red | #1A1A2E (dark indigo) | #FF4444 (bright red) | #E0E0E0 | Security, offensive |
| `dracula` | Dracula | #282A36 | #BD93F9 (purple) | #F8F8F2 | Developer talks |
| `frost_glass` | Frost Glass | #0F172A (dark slate) | #38BDF8 (sky blue) | #E2E8F0 | Modern, clean |
| `matrix` | Matrix | #000000 (pure black) | #00FF41 (matrix green) | #00FF41 | Maximum hacker vibes |
| `military_green` | Military Green | #1C2418 (olive dark) | #4A7C3F (army green) | #D0D0C0 | Military, tactical |
| `neon_purple` | Neon Purple | #13111C (dark purple) | #A855F7 (bright purple) | #E0E0E0 | Creative, modern |
| `nord` | Nord | #2E3440 (polar night) | #88C0D0 (frost) | #D8DEE9 | Calm, professional |
| `outrun` | Outrun | #1A0A2E (deep purple) | #FF2975 (hot pink) | #E0D0FF | Retro-futuristic |
| `solarized` | Solarized | #002B36 (base03) | #B58900 (yellow) | #839496 | Classic, readable |
| `sunset_warm` | Sunset Warm | #1A0A0A (dark warm) | #FF6B35 (orange) | #E0E0E0 | Warm, energetic |
| `vaporwave` | Vaporwave | #1A0033 (deep purple) | #FF6EC7 (pink) | #E0E0FF | Aesthetic, creative |

### Light Themes

| Slug | Name | Background | Accent | Text | Best For |
|------|------|------------|--------|------|----------|
| `clean_light` | Clean Light | #F5F5F0 (off-white) | #2563EB (blue) | #1A1A1A | Business, corporate |
| `minimal_mono` | Minimal Mono | #FFFFFF (white) | #E63946 (red) | #1A1A1A | Minimalist |
| `paper` | Paper | #FAF8F5 (warm white) | #6B4C3B (brown) | #2C2C2C | Academic, reading |

## Theme Schema

Each theme is defined in a YAML file under `themes/`:

```yaml
name: "My Theme"
slug: "my_theme"
colors:
  background: "#1A1A1A"
  accent: "#00FF88"
  text: "#E0E0E0"
  code_background: "#2A2A2A"
fonts:
  heading: "JetBrains Mono"
  body: "Fira Code"
  code: ""
layout: "left"
visual_style: "bold"
```

### Color Fields

| Field | Used For |
|-------|----------|
| `background` | Main slide background, status bar text |
| `accent` | Titles, bullet markers, status bar background, section headers |
| `text` | Body text, bullet text, table content |
| `code_background` | Code block background, status bar, command bar, notes panel |

### Font Fields

Font fields (`heading`, `body`, `code`) are informational hints. Terminal rendering uses the terminal's configured font. These fields help theme authors document the intended typography.

### Layout & Visual Style

- `layout`: `"left"` (default) - content alignment
- `visual_style`: `"bold"` (default) - title rendering style

## Selecting a Theme

### In Front Matter

```markdown
---
title: My Talk
theme: dracula
---
```

### Via CLI

```bash
ostendo presentation.md --theme nord
```

CLI flag overrides front matter.

### At Runtime

Press `:` to enter command mode, then:

```
:theme catppuccin
```

### Listing Themes

```bash
ostendo --list-themes
```

### Theme Name Display

Press `T` during a presentation to toggle the current theme name in the status bar.

## Theme Selection Guide

| Context | Recommended Themes |
|---------|-------------------|
| Security / Red Team | `blood_moon`, `cyber_red`, `terminal_green`, `matrix` |
| Security / Blue Team | `arctic_blue`, `blueprint`, `frost_glass` |
| Developer Conference | `dracula`, `catppuccin`, `nord`, `solarized` |
| Business / Corporate | `clean_light`, `paper`, `minimal_mono` |
| Creative / Design | `vaporwave`, `outrun`, `neon_purple` |
| Workshop / Hands-on | `terminal_green`, `frost_glass`, `catppuccin` |
| Military / Government | `military_green`, `amber_warning` |
| General Purpose | `nord`, `frost_glass`, `catppuccin` |

## How Themes Affect Rendering

1. **Background** fills the entire terminal
2. **Accent color** is used for:
   - Slide titles (bold)
   - Bullet markers (`*`, `-`, `>`)
   - Code block labels
   - Status bar background
   - Block quote vertical bars
   - Section headers
   - Progress bar
3. **Text color** is used for:
   - Body text in bullets
   - Table content
   - Subtitles
   - Image captions
4. **Code background** creates a distinct rectangle for code blocks, and is also used for:
   - Status bar timer/progress area
   - Command bar
   - Notes panel
   - Keyboard shortcut badges in help screen
