# Ostendo Agent Instructions

You are a presentation builder. Read this file, then the user's instructions and data, then build an Ostendo markdown presentation.

## Terminal Recommendation

**Kitty terminal is recommended** for full feature support: native image protocol, per-slide font sizing (OSC 66), and best rendering quality. iTerm2 supports images but not font sizing. Other terminals fall back to Sixel or ASCII for images.

## Quick Start Workflow

1. Read the user's instructions and any provided data/content
2. Create `presentations/<name>/presentation.md`
3. Copy any image assets to `presentations/<name>/images/` (or reference `images/diagram.png` from the project root)
4. Write the presentation markdown following the format spec below
5. Validate with `ostendo --validate presentations/<name>/presentation.md`

## Project Structure

```
presentations/
  my-talk/
    presentation.md
    images/
      diagram.png
      screenshot.png
```

## Presentation Format Spec

### Front Matter

Every presentation starts with YAML front matter:

```
---
title: My Presentation
theme: terminal_green
---
```

### Slide Separator

Slides are separated by `---` on its own line (same syntax as front matter delimiter, but outside the front matter block).

### Slide Elements

**Title:** `# Heading` (one per slide, first `#` heading found)

**Subtitle:** The first non-empty, non-directive line after the title.

**Bullets:**
```markdown
- Top-level bullet (depth 0)
  - Indented bullet (depth 1, 2 spaces)
    - Deep bullet (depth 2, 4 spaces)
```
Indent with 0, 2, or 4 spaces for depth 0, 1, 2.

**Code Blocks:**
````markdown
```python +exec {label: "demo"}
print("Hello, world!")
```
````

- Language tag: any syntect-supported language (`python`, `rust`, `bash`, `json`, `yaml`, `toml`, `javascript`, `typescript`, `go`, `c`, `cpp`, `ruby`, `java`, `sql`, `html`, `css`, etc.)
- `+exec` flag: makes block executable with Ctrl+E
- `+pty` flag: runs in PTY mode (interactive programs)
- `{label: "name"}`: optional label shown as a comment above the code block
- All three modifiers are optional

**Images:**
```markdown
![alt text](images/image.png)
```
Path is relative to the presentation file.

**Tables:** Standard markdown pipe tables with optional alignment:
```markdown
| Left   | Center  | Right  |
|:-------|:-------:|-------:|
| data   | data    | data   |
```

**Block Quotes:**
```markdown
> This is a quoted block
> that can span multiple lines
```

**Inline Formatting:**
- `**bold**`
- `*italic*`
- `` `inline code` ``
- `~~strikethrough~~`

## Directives Reference

Directives are HTML comments parsed by Ostendo. Place them on their own line within a slide.

| Directive | Description |
|-----------|-------------|
| `<!-- section: name -->` | Section label shown in header; inherits to subsequent slides until changed |
| `<!-- timing: 1.0 -->` | Timing in minutes for pace tracking |
| `<!-- ascii_title -->` | Render the slide title as FIGlet ASCII art (keep title < 15 chars) |
| `<!-- font_size: N -->` | Font size offset for this slide (Kitty remote control, adjusts relative to base font) |
| `<!-- column_layout: [1, 1] -->` | Start a column layout with ratio weights |
| `<!-- column: 0 -->` | Switch content to column N (0-indexed) |
| `<!-- reset_layout -->` | End column layout |
| `<!-- image_render: ascii\|kitty\|iterm\|sixel -->` | Per-slide image rendering mode |
| `<!-- image_position: left\|right -->` | Image position (default: below content) |
| `<!-- image_scale: 50 -->` | Image scale percentage (1-100) |
| `<!-- notes: speaker notes here -->` | Single-line speaker notes |
| Multi-line notes: | See below |

**Multi-line notes:**
```markdown
<!-- notes:
Line 1 of speaker notes
Line 2 of speaker notes
-->
```

## Available Themes

| Slug | Name | Style |
|------|------|-------|
| `terminal_green` | Terminal Green | Dark, green accent (#00FF88) |
| `amber_warning` | Amber Warning | Dark, amber accent (#FFB000) |
| `arctic_blue` | Arctic Blue | Dark navy, cyan accent (#00D4FF) |
| `blood_moon` | Blood Moon | Deep red/black, red accent (#CC0000) |
| `blueprint` | Blueprint | Dark blue, soft blue accent (#5BA4CF) |
| `catppuccin` | Catppuccin | Mocha palette, purple accent (#CBA6F7) |
| `clean_light` | Clean Light | Light background, blue accent (#2563EB) |
| `cyber_red` | Cyber Red | Dark indigo, red accent (#FF4444) |
| `dracula` | Dracula | Classic Dracula, purple accent (#BD93F9) |
| `frost_glass` | Frost Glass | Dark slate, sky blue accent (#38BDF8) |
| `matrix` | Matrix | Pure black, green text (#00FF41) |
| `military_green` | Military Green | Olive dark, muted green (#4A7C3F) |
| `minimal_mono` | Minimal Mono | White background, red accent (#E63946) |
| `neon_purple` | Neon Purple | Dark purple, bright purple (#A855F7) |
| `nord` | Nord | Nord palette, frost blue (#88C0D0) |
| `outrun` | Outrun | Deep purple, hot pink (#FF2975) |
| `paper` | Paper | Warm white, brown accent (#6B4C3B) |
| `solarized` | Solarized | Solarized dark, yellow accent (#B58900) |
| `sunset_warm` | Sunset Warm | Dark warm, orange accent (#FF6B35) |
| `vaporwave` | Vaporwave | Deep purple, pink accent (#FF6EC7) |

Set the theme in front matter (`theme: dracula`) or via CLI (`--theme dracula`).

## CLI Reference

| Flag | Description | Default |
|------|-------------|---------|
| `<file>` | Path to markdown presentation file | (required) |
| `-t, --theme <slug>` | Theme slug to use | `terminal_green` |
| `-s, --slide <N>` | Start at specific slide number | `1` |
| `--image-mode <mode>` | Image render mode: `auto`, `kitty`, `iterm`, `sixel`, `ascii` | `auto` |
| `--list-themes` | List available themes and exit | |
| `--remote` | Enable WebSocket remote control | |
| `--remote-port <N>` | Remote control port | `8765` |
| `--validate` | Validate presentation without running TUI | |
| `--count` | Print slide count and exit | |
| `--export-titles` | Export slide titles to stdout (one per line) | |
| `--detect-protocol` | Detect and print image protocol, then exit | |
| `--scale <N>` | Override content scale (50-200) | `80` |
| `--fullscreen` | Start with fullscreen mode (no status bar) | |
| `--timer` | Start with timer running | |

## Keyboard Shortcuts

**Navigation:**
- `h` / `Left` / `Backspace` - Previous slide
- `l` / `Right` / `Space` - Next slide
- `j` / `Down` - Scroll down
- `k` / `Up` - Scroll up
- `J` (shift) - Next section
- `K` (shift) - Previous section
- `Ctrl+D` / `Ctrl+U` - Half page down/up
- `g` + number + `Enter` - Go to slide N

**Display:**
- `n` - Toggle speaker notes
- `f` - Toggle fullscreen (hide status bar)
- `T` - Toggle theme name in status bar
- `?` - Show/hide help
- `o` - Slide overview
- `Shift+N` / `Shift+P` - Scroll speaker notes down/up

**Font & Scale:**
- `+` / `=` - Increase content scale
- `-` - Decrease content scale
- `]` / `[` - Increase/decrease font size (Kitty remote control)
- `Ctrl/Cmd+0` - Reset font size

**Code Execution:**
- `Ctrl+E` - Execute code block marked with `+exec`

**Commands (`:` mode):**
- `:theme <slug>` - Switch theme
- `:goto <N>` - Jump to slide N
- `:notes` - Toggle notes panel
- `:timer` / `:timer reset` - Start/reset timer
- `:overview` - Slide overview grid
- `:help` - Show help
- `q` / `Ctrl+C` - Quit

## Best Practices for AI Agents

1. **Keep titles concise** - especially when using `<!-- ascii_title -->` (FIGlet art needs short text, < 15 characters)
2. **Use sections** - `<!-- section: name -->` groups slides logically in the status bar
3. **Add speaker notes** - `<!-- notes: ... -->` on every slide helps the presenter
4. **Set timing** - `<!-- timing: 2.0 -->` enables pace tracking
5. **Validate** - always run `ostendo --validate <file>` after generating
6. **Use columns** for side-by-side comparisons (e.g., before/after, attack/defense)
7. **Code blocks** - use `+exec` only for safe, deterministic, fast-running code
8. **Image paths** - always relative to the presentation file, store in `images/` subdirectory
9. **Theme selection** - match the theme to the presentation tone (e.g., `cyber_red` for security, `clean_light` for business)
10. **One title per slide** - only the first `# Heading` is used as the slide title

## Complete Slide Example

````markdown
---
title: Security Assessment Results
theme: cyber_red
---

# Overview
<!-- section: intro -->
<!-- timing: 2.0 -->
<!-- ascii_title -->

Quarterly security assessment findings

- 3 Critical findings
- 7 High findings
- 12 Medium findings

<!-- notes: Start with the high-level summary before diving into details -->

---

# Attack Surface
<!-- section: findings -->
<!-- timing: 3.0 -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

- External endpoints: 47
- Internal services: 128
- Cloud assets: 89

<!-- column: 1 -->

- APIs tested: 34
- Auth bypasses: 2
- SQLi vectors: 1

<!-- reset_layout -->

<!-- notes: Emphasize the scope of testing -->

---

# Exploit Demo
<!-- timing: 1.5 -->

```python +exec {label: "SQLi PoC"}
import requests
url = "https://example.com/api/search"
payload = "' OR 1=1--"
print(f"Testing: {payload}")
```

> Always test in isolated environments

---

# Results Summary

| Severity | Count | Remediated |
|:---------|:-----:|:----------:|
| Critical | 3     | 1          |
| High     | 7     | 3          |
| Medium   | 12    | 8          |

![Risk Matrix](images/risk-matrix.png)
<!-- image_scale: 60 -->
````

## Test Presentation Feedback Format

When reviewing `test_presentation.md`, use this structured format for each slide:

```
Slide N - [Feature]: PASS/FAIL - [description if FAIL]
```

Example:
```
Slide 1 - Title slide: PASS
Slide 2 - ASCII art title: PASS
Slide 14 - Short blockquote: FAIL - border character not visible
Slide 15 - Long blockquote wrapping: FAIL - text truncated at terminal width
```

## Known Limitations

- Inline formatting markers spanning a wrap boundary will break (blockquotes and bullets)
- FIGlet ASCII titles overflow on narrow terminals with long text
- Font sizing via Kitty remote control protocol — Kitty terminal only
- Protocol images in tmux may have latency on first display
