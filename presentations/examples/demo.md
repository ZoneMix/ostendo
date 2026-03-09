---
title: Ostendo Feature Showcase
theme: frost_glass
---

# Ostendo
<!-- section: intro -->
<!-- timing: 1.0 -->
<!-- ascii_title -->

AI-native terminal presentations from markdown

<!-- notes:
Welcome to the Ostendo feature showcase.
This presentation demonstrates every major capability.
Press 'n' to toggle these speaker notes on/off.
-->

---

# What is Ostendo?
<!-- timing: 2.0 -->

A Rust-powered presentation tool that renders **beautiful slide decks** directly in your terminal.

- **Markdown-native** -- write slides in plain text
- **20 built-in themes** with runtime hot-switching
- Executable code blocks with *real-time* streaming output
- Multiple image protocols: `Kitty`, `iTerm2`, `Sixel`, `ASCII`
- Column layouts, tables, ~~PowerPoint~~, block quotes
- Speaker notes and pace tracking
- WebSocket remote control from any device

> Everything you see is rendered in the terminal. No browser. No electron. Just your shell.

<!-- notes: Emphasize this runs over SSH too - great for remote demos -->

---

# Image: Auto Protocol
<!-- section: images -->
<!-- timing: 1.5 -->

Ostendo auto-detects the best image protocol for your terminal.

![Opus - auto detected protocol](../../images/opus.png)
<!-- image_scale: 50 -->

<!-- notes: This image uses whatever protocol was auto-detected (kitty, iterm, sixel, or ascii) -->

---

# Image: ASCII Art
<!-- timing: 1.5 -->

Pure ASCII character rendering with `<!-- image_render: ascii -->` -- the universal fallback.

![Opus - ascii art rendering](../../images/opus.png)
<!-- image_render: ascii -->
<!-- image_scale: 45 -->

<!-- notes: ASCII art maps pixel brightness to characters. Works everywhere including over serial. -->

---

# Image: Scaled
<!-- timing: 1.5 -->

Control image size with `<!-- image_scale: N -->` (1-100):

![Opus at 30% scale](../../images/opus.png)
<!-- image_scale: 30 -->

> Scale keeps images from dominating the slide

<!-- notes: The scale directive accepts 1-100 as a percentage of available width -->

---

# Code Execution
<!-- section: code -->
<!-- timing: 2.0 -->

Mark code blocks with `+exec` and press **Ctrl+E** to run:

```python +exec {label: "Feature Matrix"}
import json

features = {
    "themes": 20,
    "image_protocols": ["kitty", "iterm2", "sixel", "ascii"],
    "code_exec": True,
    "columns": True,
    "tables": True,
    "remote_control": True,
}

print(json.dumps(features, indent=2))
```

<!-- notes: Press Ctrl+E to execute this block live. Output streams in real-time below the code. -->

---

# Multi-Language Highlighting
<!-- timing: 2.0 -->

Syntax highlighting via syntect supports 30+ languages:

```rust {label: "Rust example"}
fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}
```

```bash {label: "Shell example"}
#!/bin/bash
for service in nginx postgres redis; do
    systemctl status "$service" | grep -q "active (running)" \
        && echo "[OK] $service" \
        || echo "[FAIL] $service"
done
```

<!-- notes: The label appears as a comment in the language's native syntax above the code block -->

---

# Executable Bash
<!-- timing: 1.5 -->

```bash +exec {label: "system info"}
echo "Date:     $(date '+%Y-%m-%d %H:%M:%S')"
echo "Host:     $(hostname)"
echo "Shell:    $SHELL"
echo "Terminal: $TERM"
echo "Rust:     $(rustc --version 2>/dev/null || echo 'not found')"
```

<!-- notes: +exec works with any language that has an interpreter on the system -->

---

# Column Layouts
<!-- section: layouts -->
<!-- timing: 2.0 -->

Use `<!-- column_layout: [ratio, ratio] -->` for side-by-side content:

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Attack Surface**
- External endpoints: 47
- Internal services: 128
- Cloud assets: 89
  - AWS: 52
  - GCP: 37

<!-- column: 1 -->

**Findings**
- Critical: 3
- High: 7
- Medium: 12
  - Remediated: 8
  - Open: 4

<!-- reset_layout -->

<!-- notes: Columns are great for attack/defense comparisons in security reports -->

---

# Columns + Code
<!-- timing: 2.0 -->

Columns can contain code blocks too:

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Vulnerable**

```python {label: "bad.py"}
query = f"SELECT * FROM users "
        f"WHERE id = {user_id}"
cursor.execute(query)
```

<!-- column: 1 -->

**Secure**

```python {label: "good.py"}
query = "SELECT * FROM users "
        "WHERE id = %s"
cursor.execute(query, (user_id,))
```

<!-- reset_layout -->

> Always use parameterized queries

<!-- notes: Side-by-side code comparison is one of the most powerful uses of columns -->

---

# Weighted Columns
<!-- timing: 1.5 -->

Adjust ratios for asymmetric layouts with `[2, 1]`:

<!-- column_layout: [2, 1] -->
<!-- column: 0 -->

- Ostendo renders presentations entirely in the terminal
- No GUI dependencies, no browser, no electron
- Works over SSH, in tmux, in containers
- Themes are YAML files with 4 color fields

<!-- column: 1 -->

**Stats**
- 20 themes
- 62 tests
- 0 deps on X11

<!-- reset_layout -->

<!-- notes: The left column gets 2/3 width, right gets 1/3 -->

---

# Data Tables
<!-- section: tables -->
<!-- timing: 1.5 -->

Standard markdown pipe tables with column alignment:

| Severity | Count | Remediated | SLA       |
|:---------|:-----:|:----------:|----------:|
| Critical | 3     | 1          | 72 hours  |
| High     | 7     | 3          | 2 weeks   |
| Medium   | 12    | 8          | 30 days   |
| Low      | 23    | 19         | Next cycle|

<!-- notes: Tables support left, center, and right alignment via colon syntax in the separator row -->

---

# Block Quotes
<!-- section: formatting -->
<!-- timing: 1.0 -->

Use `>` for callouts and emphasis:

> The only truly secure system is one that is powered off, cast in a block of concrete, and sealed in a lead-lined room with armed guards.
> -- Gene Spafford

> Defense in depth is not about building one perfect wall. It is about making the attacker climb many imperfect ones.

<!-- notes: Block quotes render with an accent-colored vertical bar and italic text -->

---

# Inline Formatting
<!-- timing: 1.0 -->

All standard markdown inline styles work in bullets, subtitles, and quotes:

- This text is **bold** for emphasis
- This text is *italic* for nuance
- This is `inline code` with a background highlight
- This is ~~strikethrough~~ for corrections
- Mix them: **bold with `code` inside** and *italic with ~~struck~~ words*

> Inline formatting works inside block quotes too: **bold**, *italic*, `code`

<!-- notes: Inline formatting is parsed everywhere except code blocks -->

---

# Content Scaling
<!-- section: display -->
<!-- timing: 1.0 -->

Press `+` and `-` to scale content width in real-time.

- Scale controls how much of the terminal width is used
- Content stays centered with equal margins
- Code blocks, bullets, and images all respect the scale
- Default is 80% of terminal width

> Try pressing `+` and `-` right now to see the effect

<!-- notes: Scale range is 50-200%. It affects all slides globally. -->

---

# Image + Table
<!-- section: images -->
<!-- timing: 1.5 -->

![Opus Logo](../../images/opus.png)
<!-- image_scale: 40 -->

| Feature | Protocol | Quality |
|:--------|:---------|:------:|
| Kitty   | Native   | Best   |
| iTerm2  | Inline   | Good   |
| Sixel   | Bitmap   | Good   |
| ASCII   | Text     | Basic  |

<!-- notes: Images and tables can coexist on the same slide -->

---

# Theme Showcase
<!-- section: meta -->
<!-- timing: 2.0 -->

Switch themes live with `:theme <slug>` -- try these:

| Theme | Slug | Vibe |
|:------|:-----|:-----|
| Terminal Green | `terminal_green` | Hacker aesthetic |
| Dracula | `dracula` | Developer favorite |
| Cyber Red | `cyber_red` | Red team reports |
| Nord | `nord` | Calm and professional |
| Catppuccin | `catppuccin` | Modern and popular |
| Matrix | `matrix` | Maximum hacker |
| Clean Light | `clean_light` | Corporate / bright rooms |
| Outrun | `outrun` | Retro synthwave |

<!-- notes:
Press : then type 'theme dracula' to switch.
Press T to toggle the theme name in the status bar.
All 20 themes are available - run ostendo --list-themes to see them all.
-->

---

# Navigation Cheatsheet
<!-- section: reference -->
<!-- timing: 1.5 -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Movement**
- `h`/`l` or arrows: prev/next slide
- `j`/`k` or arrows: scroll up/down
- `J`/`K`: next/prev section
- `g` + N + Enter: go to slide N
- `Ctrl+D`/`Ctrl+U`: half page

<!-- column: 1 -->

**Actions**
- `?`: this help
- `n`: toggle speaker notes
- `f`: toggle fullscreen
- `o`: slide overview
- `Ctrl+E`: execute code
- `+`/`-`: scale content
- `:`  command mode
- `q`: quit

<!-- reset_layout -->

<!-- notes: Press ? during any presentation to see the full help screen -->

---

# Built with Opus
<!-- section: closing -->
<!-- timing: 1.0 -->

![Built with Claude Opus 4.6](../../images/opus.png)
<!-- image_scale: 40 -->

> Built with Claude Code Max (Opus 4.6) in a weekend.

<!-- notes: Ostendo was built entirely with AI-driven development using Claude Code Max -->

---

# Thank You
<!-- timing: 0.5 -->
<!-- ascii_title -->

- See `AGENTS.md` to have AI build presentations for you
- See `docs/` for the complete format reference
- Press `?` for the full keyboard shortcut help

> Built with Rust. Rendered in your terminal. Driven by AI.

<!-- notes:
This presentation demonstrated:
- ASCII art titles (FIGlet)
- Bullets with depth levels
- Inline formatting (bold, italic, code, strikethrough)
- Code blocks with syntax highlighting
- Executable code (+exec)
- Code labels
- Images with auto and ascii render modes
- Image scaling
- Column layouts (equal and weighted)
- Columns with code blocks
- Data tables with alignment
- Block quotes
- Font size directive
- Speaker notes (single and multi-line)
- Timing directives
- Section directives
- Theme switching reference
-->
