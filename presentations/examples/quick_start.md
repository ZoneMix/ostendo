---
title: Quick Start Guide
author: Your Name
date: 2026-03-09
accent: "#00BFFF"
transition: fade
---

# Quick Start Guide
<!-- section: intro -->
<!-- font_size: 6 -->
<!-- ascii_title -->

Get up and running with Ostendo in minutes

- Write slides in **Markdown**
- Present in your **terminal**
- No GUI required

<!-- notes: This is a minimal example showing the core features of Ostendo. -->

---

# Formatting Basics
<!-- section: content -->
<!-- font_size: 6 -->

Ostendo supports standard Markdown inline formatting:

- **Bold text** for emphasis
- *Italic text* for nuance
- `inline code` with background highlight
- ~~Strikethrough~~ for corrections
- Mix them: **bold with `code` inside** and *italic with ~~struck~~ words*

> Block quotes render with an accent-colored left border

<!-- notes: All standard Markdown formatting works in bullets, subtitles, and block quotes. -->

---

# Live Code Execution
<!-- section: code -->
<!-- font_size: 6 -->

Mark code blocks with `+exec` and press **Ctrl+E** to run:

```python +exec {label: "hello.py"}
import sys
from datetime import datetime

print(f"Hello from Ostendo!")
print(f"Python {sys.version_info.major}.{sys.version_info.minor}")
print(f"Current time: {datetime.now().strftime('%H:%M:%S')}")
```

- Output streams in real-time below the code block
- Supports Python, Bash, Ruby, JavaScript, Go, C, C++

<!-- notes: Press Ctrl+E to execute. The output appears directly below the code block. -->

---

# Columns and Images
<!-- section: layout -->
<!-- font_size: 6 -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Left Column**
- Use `<!-- column_layout: [1, 1] -->` for equal columns
- Adjust ratios like `[2, 1]` for asymmetric layouts
- Up to 3 columns supported

<!-- column: 1 -->

**Right Column**
- Images: `![alt](path.png)`
- Scale: `<!-- image_scale: 50 -->`
- ASCII mode: `<!-- image_render: ascii -->`

<!-- reset_layout -->

> Columns are great for side-by-side comparisons

<!-- notes: Column layouts split the slide horizontally. Ratios control relative widths. -->

---

# Thank You
<!-- section: closing -->
<!-- font_size: 6 -->
<!-- ascii_title -->

- Run: `ostendo quick_start.md`
- Help: press `?` during presentation
- Themes: `:theme dracula` to switch live
- Export: `ostendo --export html quick_start.md`

> Built with Rust. Rendered in your terminal.

<!-- notes:
Key commands:
- h/l or arrows: navigate slides
- n: toggle speaker notes
- Ctrl+E: execute code
- +/-: scale content
- :: command mode
- q: quit
-->
