# Contributing

Bug reports, fixes, themes, and example decks are welcome. For new features,
open an issue first; [WISHLIST.md](WISHLIST.md) lists ideas.

## Setup

Rust 1.89 or newer, then:

```bash
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run --release -- --validate presentations/examples/test_presentation.md
```

CI runs the same checks on Linux, macOS, and Windows; all must pass.
For visual changes, page through `presentations/examples/test_presentation.md`
in Kitty or Ghostty and in a terminal without graphics support.
`tests/integration/run_all.sh` drives the release build through tmux and
checks navigation, toggles, and exports (needs tmux).

## Code

- Keep changes small and focused; add only what a user-visible behavior needs.
- A bug fix comes with a regression test that fails without the fix. Test
  through public boundaries (`parse_presentation`, render and export
  functions, the CLI) rather than private helpers.
- Comments explain why, not what.
- No `.unwrap()` on anything derived from user input, the terminal, the
  filesystem, or the network.

[CLAUDE.md](CLAUDE.md) describes the architecture and its invariants, and the
[lean-audit skill](.claude/skills/lean-audit/SKILL.md) has the full review bar
for code, tests, comments, and docs.

## Themes and docs

Themes are YAML files in `themes/`; see [docs/THEME_GUIDE.md](docs/THEME_GUIDE.md)
for the schema and contrast rules. When behavior changes, update
[docs/PRESENTATION_FORMAT.md](docs/PRESENTATION_FORMAT.md) or the README in the
same change.
