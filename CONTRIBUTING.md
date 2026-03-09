# Contributing to Ostendo

## Origin Story

Ostendo was built in a weekend as a "PowerPoint killer for hackers" — an AI-native terminal presentation tool written entirely through AI-agent driven development using Claude Code Max (Opus 4.6). The entire codebase, all 20 themes, image protocol support, and documentation were generated through conversation with Claude.

## Contributions Welcome

We're open to contributions of all kinds:

- Bug fixes
- New features (see [WISHLIST.md](WISHLIST.md) for ideas)
- New themes
- Documentation improvements
- Presentation examples

## Getting Started

1. Fork the repository
2. Clone your fork
3. Build and test:

```bash
cargo build --release
cargo test                # 77 tests should pass
cargo clippy              # No warnings
```

## Development Setup

- **Rust 1.75+** (uses `LazyLock`)
- **Kitty terminal** recommended for full feature testing (native images, font sizing)
- iTerm2 works for image testing but lacks font sizing support

## Code Contributions

- Follow existing code style and patterns
- Add tests for new functionality
- Ensure `cargo clippy` produces no warnings
- All 77 existing tests must continue to pass
- Run `ostendo --validate` against test presentations before submitting

## Theme Contributions

Themes are YAML files in `themes/`. All themes must meet WCAG 2.0 contrast requirements:

- text:bg ratio >= 4.5:1
- accent:bg ratio >= 3.0:1
- code_bg:bg ratio >= 1.2:1

See [docs/THEME_GUIDE.md](docs/THEME_GUIDE.md) for the full theme schema.

## Documentation

- Update relevant docs when changing behavior
- `AGENTS.md` is the primary format reference for AI agents building presentations
- `docs/PRESENTATION_FORMAT.md` is the human-readable format reference

## Ideas?

Check [WISHLIST.md](WISHLIST.md) for planned features and enhancement ideas.
