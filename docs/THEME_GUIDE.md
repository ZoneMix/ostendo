# Themes

Ostendo ships with the themes in [`themes/`](../themes/), embedded into the
binary at build time. `ostendo --list-themes` prints each one with a color
swatch.

## Choosing a theme

In order of precedence:

1. `--theme nord` on the command line
2. `theme: nord` in the deck's front matter
3. The last theme you switched to while presenting this deck
4. `terminal_green`

While presenting, `:theme <slug>` switches theme, `D` swaps a theme for its
dark or light counterpart, and `T` shows the theme name in the status bar. A
single slide can use another theme with `<!-- theme: slug -->`.

## Theme files

```yaml
name: "Nord"
slug: "nord"
light_variant: "nord_light"
colors:
  background: "#2E3440"
  text: "#D8DEE9"
  accent: "#88C0D0"
  code_background: "#3B4252"
gradient:
  from: "#2E3440"
  to: "#242933"
title_decoration: "underline"
```

| Field | Required | Used for |
|---|---|---|
| `name` | yes | Display name (status bar, `--list-themes`) |
| `slug` | no | Identifier; defaults to the file name |
| `colors.background` | yes | Page background |
| `colors.text` | yes | Body text; muted text (captions, labels) is blended from it |
| `colors.accent` | yes | Titles, bullets, borders, progress bar |
| `colors.code_background` | no | Code blocks; also the status bar, notes, and prompt when it stands apart from the background (default `#1A1A1A`) |
| `gradient` | no | Background fades from `from` (top) to `to` (bottom) |
| `title_decoration` | no | Default title style: `underline`, `box`, `banner`, `none` |
| `light_variant` / `dark_variant` | no | Slug of the counterpart `D` switches to |

Code highlighting picks a dark or light syntax palette from the brightness of
`code_background`.

## Rules

- **Contrast.** Against the background, text must reach 4.5:1 and the accent
  3:1 (WCAG 2.0); the code background needs 1.2:1 so code blocks stand out.
  `cargo test` checks every built-in theme.
- **Pairs.** Give the dark theme `light_variant` and the light one
  `dark_variant` so `D` works in both directions.
- **Gradients.** Keep both ends close to `background`: contrast is checked
  against `background` only.

A front-matter `accent:` overrides the theme's accent for one deck, but only
when it still reaches 3:1 against the background.

## Adding a theme

1. Create `themes/<slug>.yaml`.
2. `cargo test` — a file that fails to parse or misses a contrast minimum is
   named in the failure.
3. `cargo run --release -- --theme <slug> presentations/examples/test_presentation.md`
   and look at titles, code, tables, and the status bar.
