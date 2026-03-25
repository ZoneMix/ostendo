# Presentation Format

Use when creating or editing Ostendo markdown presentations.

## Quick Start

1. Create `presentations/<name>/presentation.md`
2. Add YAML front matter between `---` delimiters
3. Separate slides with `---` on its own line
4. Validate: `ostendo --validate presentation.md`

## Front Matter

```
---
title: My Talk
theme: terminal_green
author: Name
date: 2025-01-01
accent: "#FF5500"
transition: fade
align: center
---
```

## Slide Elements

- **Title**: `# Heading` (first `#` on slide)
- **Subtitle**: first non-empty, non-directive line after title
- **Bullets**: `- text` (0/2/4 spaces for depth 0/1/2)
- **Code**: fenced with backticks, optional `+exec`, `+pty`, `{label: "name"}`
- **Images**: `![alt](path)` (path relative to presentation file)
- **Tables**: standard pipe tables with optional `:---:` alignment
- **Block quotes**: `> text`
- **Diagrams**: ```` ```diagram ```` or ```` ```mermaid ````
- **Inline**: `**bold**`, `*italic*`, `` `code` ``, `~~strike~~`

## Essential Directives

```
<!-- section: name -->         Section label (inherits forward)
<!-- timing: 2.0 -->           Minutes for pacing
<!-- ascii_title -->            FIGlet art title (keep < 15 chars)
<!-- notes: text -->            Speaker notes (single-line)
<!-- column_layout: [1,1] -->  Start columns
<!-- column: 0 -->             Switch to column N
<!-- reset_layout -->          End columns
<!-- transition: fade -->      Slide transition
<!-- animation: typewriter --> Entrance animation
<!-- loop_animation: sparkle(figlet) -->  Loop animation
```

See `.claude/docs/DIRECTIVE_REFERENCE.md` for the complete list.

## Column Layout

```markdown
<!-- column_layout: [1, 2] -->
<!-- column: 0 -->
- Left column (1/3 width)
<!-- column: 1 -->
- Right column (2/3 width)
<!-- reset_layout -->
```

Columns support bullets, code blocks, and images. Column images render as ASCII art.
Use `<!-- column_separator: none -->` to hide the vertical separator.
Use `<!-- column_text_scale: 3 -->` to scale text in non-image columns (Kitty only).

## Code Execution

````markdown
```python +exec {label: "demo"}
print("Hello")
```
````

Supported: Python, Bash, JavaScript, Ruby, Rust, C, C++, Go.
Rust/C/C++/Go auto-wrap snippets without `main`.
PTY mode (`+pty`) preserves ANSI output.

## Best Practices

- Keep FIGlet titles under 15 characters
- One image per slide (use columns for side-by-side)
- Add `<!-- notes: ... -->` on every slide
- Set `<!-- timing: N -->` for pace tracking
- Use `<!-- section: name -->` to group slides logically
- Validate before presenting: `ostendo --validate <file>`
- Match theme to tone: `cyber_red` for security, `clean_light` for business
- Use `+exec` only for safe, fast, deterministic code
