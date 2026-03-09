# Ostendo Development Guide

## Build & Test

```bash
cargo build --release          # Release build
cargo test                     # Run all tests (77 tests)
cargo run --release -- --validate presentations/test_presentation.md  # Validate presentation
```

## Architecture

```
src/
  main.rs              # CLI (clap), entry point
  render/engine.rs     # Core renderer, event loop, Presenter struct (~1700 lines)
  render/layout.rs     # WindowSize, terminal dimensions
  render/text.rs       # StyledLine/StyledSpan (virtual buffer)
  render/progress.rs   # Progress bar rendering
  markdown/parser.rs   # Markdown -> Vec<Slide> parser
  presentation/        # Slide struct, StateManager (JSON persistence)
  terminal/protocols.rs # Image protocol detection, FontSizeCapability
  theme/               # Theme registry, schema, color utilities
  code/                # Syntax highlighting, code execution
  image_util/          # Image loading, rendering (protocol-specific)
  image_util/mod.rs    # Image utility module
  watch.rs             # File watcher for hot reload
  remote/              # WebSocket remote control server
```

## Key Patterns

- **Virtual buffer**: Rendering builds `Vec<StyledLine>` in memory, then writes to terminal in one pass within `BeginSynchronizedUpdate`/`EndSynchronizedUpdate`
- **Smart redraw**: `render_frame()` tracks last-rendered state; timer-only ticks only update the status bar (no image re-emission)
- **Image caching**: `image_cache` keyed by `(path, content_width, protocol)` — stale entries are naturally unreachable
- **Hot reload**: Background `FileWatcher` polls every 500ms, triggers `try_reload()` which re-parses and preserves slide position

## Terminal Requirements

- **Recommended**: Kitty terminal — full feature support (native image protocol, per-slide font sizing via OSC 66, best rendering quality)
- **iTerm2**: Supports images via inline image protocol; no font sizing support
- **tmux**: Works with DCS passthrough for images; stale `KITTY_WINDOW_ID` may cause issues — unset it or start a fresh session
- **Other terminals**: Sixel or ASCII fallback for images; font sizing gracefully degrades
- Synchronized updates prevent flicker in most terminals

## Theme System

- 20 built-in themes in `themes/*.yaml`
- All themes must pass contrast ratio tests (WCAG 2.0):
  - text:bg >= 4.5:1
  - accent:bg >= 3.0:1
  - code_bg:bg >= 1.2:1
- Runtime theme switching via `:theme <slug>` command

## Test Presentation

`test_presentation.md` has 32 slides testing every feature. Each slide's speaker notes contain:
```
FEATURE: [name]
EXPECTED: [expected visual]
VERIFY: [what to check]
```

Use `AGENTS.md` feedback format: `Slide N - [Feature]: PASS/FAIL - [description]`

## Known Limitations

- Inline formatting markers spanning a wrap boundary will break (blockquotes and bullets)
- FIGlet ASCII titles overflow on narrow terminals with long text
- Font sizing via Kitty remote control protocol — Kitty terminal only
- Protocol images in tmux may have latency on first display
