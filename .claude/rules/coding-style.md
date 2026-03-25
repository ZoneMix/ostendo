# Rust Coding Style -- Ostendo

## Immutability

- Default to immutable bindings (`let`, not `let mut`) unless mutation is required
- Animation functions take `&lines` and return new `Vec<StyledLine>` -- never mutate in place
- Clone only when necessary (e.g., `slide.clone()` in render_frame to avoid borrow conflicts)

## Error Handling

- Use `Result<T>` with `anyhow` for all fallible operations
- Avoid `.unwrap()` in non-test code -- use `.unwrap_or()`, `.unwrap_or_default()`, or `?`
- `bail!()` for early-exit error paths
- `thiserror` for library-level error types

## Visibility

- Prefer `pub(crate)` over `pub` unless the item is part of the true public API
- Engine submodule methods are `pub(crate)` on `impl Presenter`
- Regex statics in `regex_patterns.rs` are `pub(crate)`

## Documentation

- Doc comments (`///`) on all public and `pub(crate)` items
- Module-level docs (`//!`) explaining purpose and architecture
- Include `# Parameters`, `# Returns`, `# Errors` sections where useful

## Module Organization

- Target <500 lines per file (3 files currently over 800 -- flagged in CLAUDE.md)
- Group by responsibility: one file per concern (font, state, navigation, etc.)
- Re-export submodule items at parent level when callers shouldn't know the layout

## Naming

- Types: `PascalCase` (e.g., `StyledSpan`, `AnimationState`)
- Functions/methods: `snake_case` (e.g., `render_frame`, `parse_slide`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `MAX_CODE_LENGTH`, `EXEC_TIMEOUT_SECS`)
- Enum variants: `PascalCase` (e.g., `TransitionType::SlideLeft`)

## Patterns

- `LazyLock<Regex>` for all compiled regexes (compile once, reuse)
- `StyledLine` / `StyledSpan` virtual buffer -- never write to terminal mid-frame
- `BeginSynchronizedUpdate` / `EndSynchronizedUpdate` for flicker-free output
- `BufWriter` with 256KB capacity for render output

## Linting

- `cargo clippy --all-targets` must produce 0 warnings
- `#[must_use]` on functions returning values that shouldn't be ignored
- `#[allow(dead_code)]` only when a field is used by future/conditional code

## Testing

- `#[cfg(test)] mod tests` in the same file as the code under test
- Test animation parsing, transition rendering, buffer operations
- Validate with: `cargo run --release -- --validate <file.md>`
