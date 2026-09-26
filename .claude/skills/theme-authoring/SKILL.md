---
name: theme-authoring
description: "Use when creating or modifying Ostendo themes in themes/*.yaml: schema, WCAG contrast rules, dark/light pairing, gradients."
---

# Theme Authoring

Schema, contrast rules, and steps are in
[`docs/THEME_GUIDE.md`](../../../docs/THEME_GUIDE.md). The schema is
`src/theme/schema.rs`; unknown YAML keys are ignored, so check field names
there.

## Checklist

- `themes/<slug>.yaml`, embedded by `build.rs` — no registration needed.
- `cargo test`: `every_builtin_theme_parses` and
  `builtin_themes_meet_contrast_minimums` must pass (text 4.5:1, accent 3:1,
  code background 1.2:1 against the background).
- Dark/light pairs point at each other with `light_variant` / `dark_variant`;
  `default_and_variant_slugs_resolve` checks the slugs exist.
- Gradients run top to bottom; keep both ends near `background`.
- Look at it: `cargo run --release -- --theme <slug> presentations/examples/test_presentation.md`
  and check titles, code highlighting (picked from `code_background`
  brightness), tables, and the status bar.
