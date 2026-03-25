# Theme Authoring

Use when creating or modifying Ostendo themes.

## Theme File Location

Themes live in `themes/<slug>.yaml`. They are embedded at compile time via `build.rs`.
After creating/modifying a theme, run `cargo build --release` to include it.

## YAML Structure

```yaml
name: "My Theme"
slug: "my_theme"
dark_variant: "my_theme_light"     # optional: slug of the light variant
colors:
  background: "#1a1a2e"
  text: "#e0e0e0"
  accent: "#00ff88"
  code_background: "#2a2a3e"
gradient:                           # optional
  from: "#1a1a2e"
  to: "#0a0a1e"
  direction: "vertical"            # or "horizontal"
title_decoration: "underline"       # optional: underline, box, banner, none
```

## Required Fields

- `name`: Human-readable display name
- `slug`: URL-safe identifier (lowercase, underscores)
- `colors.background`: Main background hex color
- `colors.text`: Primary text hex color
- `colors.accent`: Accent color (titles, bullets, borders)
- `colors.code_background`: Code block background hex color

## WCAG Contrast Requirements

All themes must pass WCAG 2.0 contrast validation:

- Text vs background: >= 4.5:1 contrast ratio
- Accent vs background: >= 3.0:1 contrast ratio

The theme system validates these at load time. Run `cargo test` to verify.

## Dark/Light Pairs

To support the `D` toggle key:

1. Create both variants: `my_theme.yaml` and `my_theme_light.yaml`
2. Add `dark_variant: "my_theme_light"` to the dark theme
3. The registry automatically pairs them for toggle

## Gradient Backgrounds

Optional gradient fills the terminal background:

- `direction: "vertical"` -- top to bottom (most common)
- `direction: "horizontal"` -- left to right
- Colors interpolate per-row (vertical) or per-column (horizontal)

## Testing a Theme

```bash
cargo build --release
./target/release/ostendo --theme my_theme presentation.md
```

At runtime: `:theme my_theme` to switch, `D` to toggle dark/light.

## Color Utilities

The `theme/colors.rs` module provides:

- `hex_to_color()`: Parse hex string to crossterm Color
- `interpolate_color()`: Blend two colors by factor (0.0-1.0)
- `ensure_badge_contrast()`: Ensure help overlay badges are visible
- WCAG luminance and contrast ratio calculations

## Gotchas

- Slug must match filename (e.g., `my_theme.yaml` needs `slug: "my_theme"`)
- Code background should be visually distinct from main background
- Very dark backgrounds (near #000000) may need brighter accent colors
- Gradient `from` color should match `background` for seamless integration
- Test with FIGlet titles, code blocks, and tables to verify all elements
