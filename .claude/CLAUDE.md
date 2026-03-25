# Ostendo

**Version**: v0.4.1 | **Language**: Rust | **License**: GPL-3.0-only

AI-native terminal presentation tool -- markdown to slides in your terminal.

## Build & Run

```
cargo build --release
cargo test
cargo clippy --all-targets
./target/release/ostendo <file.md>
```

## Rendering Pipeline

```
Markdown source
  -> parse_presentation() [markdown/parser.rs]
  -> Vec<Slide> [presentation/slide.rs]
  -> render_frame() [render/engine/rendering.rs]
     1. Build Vec<StyledLine> virtual buffer
     2. Apply alignment (vertical/horizontal centering)
     3. Apply animation overlays (transition -> entrance -> loop)
     4. Clamp scroll viewport
     5. Flush atomically inside BeginSynchronizedUpdate/EndSynchronizedUpdate
  -> Terminal output
```

## Module Map

| Module | Responsibility |
|---|---|
| `main.rs` | CLI (clap), entry point |
| `markdown/parser.rs` | Markdown -> Vec<Slide> |
| `markdown/regex_patterns.rs` | ~30 LazyLock<Regex> directive patterns |
| `markdown/inline.rs` | Bold/italic/code/strikethrough |
| `markdown/tables.rs` | Table cell parsing, alignment |
| `presentation/slide.rs` | Slide struct, all content types |
| `presentation/state.rs` | StateManager (JSON persistence) |
| `render/engine/mod.rs` | Presenter struct, lifecycle |
| `render/engine/rendering.rs` | render_frame(), smart redraw |
| `render/engine/input.rs` | Event loop, key/mouse/remote |
| `render/engine/content.rs` | Tables, columns, FIGlet titles, exec output |
| `render/engine/ui.rs` | Status bar, help overlay, overview grid |
| `render/engine/font.rs` | Kitty RC / Ghostty AppleScript font control |
| `render/engine/state.rs` | Toggles, scale, theme, persistence |
| `render/engine/navigation.rs` | Slide movement, scrolling, animations |
| `render/animation/` | Transitions, entrances, loop animations |
| `render/text.rs` | StyledLine/StyledSpan virtual buffer |
| `terminal/protocols.rs` | Image protocol & font capability detection |
| `theme/` | Registry, WCAG validation, color utilities |
| `code/executor.rs` | Language execution, timeout, sandbox |
| `image_util/` | Protocol-specific image rendering |
| `diagram/` | ASCII diagram engine (box, bracket, vertical) |
| `export/` | HTML and PDF export |
| `remote/` | WebSocket server, auth, rate limiting |
| `watch.rs` | File watcher for hot reload |

## Key Constraints

- Immutable rendering: animation functions take `&lines`, return new buffer
- `line.content_type` must be preserved through centering (breaks sparkle/spin targeting)
- Protocol images must check `line_offset >= visible_start`
- Kitty images need clear on scroll offset change, not just slide change
- `font_size` range is -3 to 7 (negative = smaller than base font)
- ANSI escape codes from exec output are preserved (not stripped)
- 29 built-in themes embedded at compile time via build.rs
- Code execution sandboxed: process groups (`setsid`), 30s timeout, 64KB input / 1MB output

## Reference Files

| Topic | File |
|---|---|
| All directives | [docs/DIRECTIVE_REFERENCE.md](docs/DIRECTIVE_REFERENCE.md) |
| Keyboard shortcuts | [docs/KEYBOARD_SHORTCUTS.md](docs/KEYBOARD_SHORTCUTS.md) |
| CLI flags | [docs/CLI_FLAGS.md](docs/CLI_FLAGS.md) |
| Animations | [docs/ANIMATION_REFERENCE.md](docs/ANIMATION_REFERENCE.md) |
| Theme list | [docs/THEME_LIST.md](docs/THEME_LIST.md) |
| Architecture | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Presentation authoring | [skills/presentation-format.md](skills/presentation-format.md) |
| Theme creation | [skills/theme-authoring.md](skills/theme-authoring.md) |
| Demo runner | [skills/demo-scripts.md](skills/demo-scripts.md) |
| Coding style | [rules/coding-style.md](rules/coding-style.md) |
| Module architecture | [rules/architecture.md](rules/architecture.md) |
