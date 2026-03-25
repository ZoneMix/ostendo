# Ostendo Codebase Assessment Report

**Version**: v0.4.1 | **Date**: 2026-03-25 | **Tests**: 173 passing

## Clippy Warnings (7)

| # | File:Line | Warning | Fix |
|---|-----------|---------|-----|
| 1 | `content.rs:581` | `vec_init_then_push` | Use `vec![...]` |
| 2 | `content.rs:644` | `redundant_closure` | `.and_then(hex_to_color)` |
| 3 | `input.rs:301` | `unnecessary_map_or` | `.is_some_and(...)` |
| 4 | `rendering.rs:1160` | `single_match` | Use `if` |
| 5 | `mod.rs:589` | `too_many_arguments` (9/7) | `PresenterConfig` struct |
| 6 | `mod.rs:641` | `needless_range_loop` | `.iter().enumerate()` |
| 7 | `executor.rs:220` | `manual_flatten` | `.lines().flatten()` |

## Test Coverage Gaps

34 source files have 0 tests. ~35-40% coverage vs 80% target.

**Critical untested**: server.rs, highlight.rs, inline.rs, protocols.rs, ascii_art.rs, loops.rs, content.rs, ui.rs, input.rs, slide.rs

## Dead Code (45 #[allow(dead_code)])

- `executor.rs`: 10 — entire sync execution path dead (only streaming used)
- `render/mod.rs:2`: blanket module-level suppression
- `engine/mod.rs`: 4 — two 80-line dead functions (TODO v0.6)
- `kitty.rs`: 9 — protocol enum variants for completeness

## Security

- 3 unsafe blocks — all justified (setsid + kill), 1 missing SAFETY comment
- 99 .unwrap() — 39 LazyLock, 38 tests, 21 production (medium concern)
- 0 panic! in production, 0 hardcoded paths
- 1 design note: unknown language falls back to sh silently

## Files Over 500 Lines (8)

| Lines | File | Action |
|---|---|---|
| 1721 | engine/mod.rs | Extract types, image_cache, output_helpers |
| 1391 | rendering.rs | Decompose render_frame (951 lines) |
| 1248 | parser.rs | Extract parse_slide (682 lines) |
| 901 | executor.rs | Remove dead sync path |
| 736 | content.rs | Extract columns, table_render |
| 723 | remote/html.rs | include_str!() from assets/ |
| 537 | input.rs | Extract mouse/command handlers |
| 498 | loops.rs | Borderline — consider per-animation files |
