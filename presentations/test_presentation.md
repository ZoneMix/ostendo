---
title: Ostendo Feature Test
theme: terminal_green
---

# Ostendo Feature Test
<!-- section: intro -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

A systematic test of all rendering features — one per slide.

- Navigate: arrow keys or h/l
- Help: press ?
- Quit: press q

<!-- notes:
FEATURE: Title slide
EXPECTED: Title in accent color, subtitle text, bullet list with bullet chars
VERIFY: Title renders bold in accent color, bullets have correct indent
TALKING POINTS:
- Welcome everyone to the Ostendo feature test presentation
- This presentation systematically tests every rendering feature
- Each slide focuses on one specific capability
- Speaker notes support scrolling — press Shift+N to scroll down, Shift+P to scroll up
- The notes panel fills the entire bottom section with a consistent background
- Navigation uses vim-style keys: h/l for slides, j/k for content scroll
- Press 'n' to toggle this notes panel on and off
- Press '?' for the full help menu with all keybindings
-->

---

<!-- ascii_title -->
# ASCII Art
<!-- section: formatting -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

This title should render as large FIGlet ASCII art

<!-- notes:
FEATURE: ASCII art title
EXPECTED: Title "ASCII Art" renders in FIGlet slant font using accent color
VERIFY: Large multi-line ASCII characters visible, not plain text
-->

---

# Bullet Depths
<!-- section: formatting -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- Top level bullet
- Another top level
  - Second level indent
  - Another second level
    - Third level deep
    - Another deep bullet
- Back to top level

<!-- notes:
FEATURE: Bullet depth levels
EXPECTED: Three distinct indent levels with different bullet markers
VERIFY: Level 1 (•), level 2 (◦), level 3 (▪) at increasing indents
-->

---

# Bold Text
<!-- section: formatting -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- This line has **bold text** inline
- **Entire line is bold**
- Normal text then **bold at end**

<!-- notes:
FEATURE: Bold text
EXPECTED: Double-asterisk text renders with bold attribute
VERIFY: Bold text appears brighter/heavier than normal text
-->

---

# Italic Text
<!-- section: formatting -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- This line has *italic text* inline
- *Entire line is italic*
- Normal text then *italic at end*

<!-- notes:
FEATURE: Italic text
EXPECTED: Single-asterisk text renders with italic attribute
VERIFY: Italic text appears slanted (terminal-dependent)
-->

---

# Strikethrough Text
<!-- section: formatting -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- This line has ~~strikethrough text~~ inline
- ~~Entire line is struck through~~
- Normal text then ~~strikethrough at end~~

<!-- notes:
FEATURE: Strikethrough text
EXPECTED: Double-tilde text renders with strikethrough attribute
VERIFY: Strikethrough line visible through text (terminal-dependent)
-->

---

# Inline Code
<!-- section: formatting -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- This line has `inline code` with background
- `Code at start` of line
- Normal text then `code at end`

<!-- notes:
FEATURE: Inline code
EXPECTED: Backtick text renders with code_bg background color
VERIFY: Code spans have visible background distinct from page background
-->

---

# Mixed Inline Formatting
<!-- section: formatting -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- **Bold** and *italic* and `code` together
- **Bold *and italic* mixed** in one span
- Text with ~~strike~~ and **bold** and `code`

<!-- notes:
FEATURE: Mixed inline formatting
EXPECTED: Multiple formatting types coexist on same line
VERIFY: Each format type renders correctly without bleeding into others
-->

---

# Code Block: Python
<!-- section: code -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

```python {label: "example.py"}
def fibonacci(n):
    """Generate Fibonacci sequence."""
    a, b = 0, 1
    for _ in range(n):
        yield a
        a, b = b, a + b

for num in fibonacci(8):
    print(num, end=" ")
```

<!-- notes:
FEATURE: Code block with syntax highlighting
EXPECTED: Python code with colored keywords, strings, comments in a bordered box
VERIFY: def/for/print colored differently, label "example.py" visible above code
TALKING POINTS:
- Code blocks support syntax highlighting for many languages
- The label directive {label: "example.py"} shows a filename above the block
- Code blocks are rendered inside a bordered box with the code_bg color
- Keywords like def, for, print should be colored differently
- String literals should have their own distinct color
- Comments (lines starting with #) should be dimmed
- The code block respects the current content scale setting
-->

---

# Executable Code
<!-- section: code -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

```python +exec {label: "hello.py"}
import sys
print("Hello from Ostendo!")
print(f"Python {sys.version_info.major}.{sys.version_info.minor}")
```

- Press Ctrl+E to execute
- Output streams below the code block

<!-- notes:
FEATURE: Executable code block (+exec)
EXPECTED: Code block with +exec marker, Ctrl+E executes and shows output
VERIFY: +exec badge visible, execution produces output below code block
TALKING POINTS:
- The +exec flag makes a code block executable with Ctrl+E
- Output streams below the code block in real-time
- Supports Python, Bash, and other interpreted languages
- Execution happens in a subprocess with stdout/stderr captured
- The execution output persists until you navigate away from the slide
- Great for live demos during presentations
- Security note: only execute code you trust — it runs with your user permissions
- The +pty flag is also supported for interactive terminal programs
-->

---

# Multiple Code Blocks
<!-- section: code -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

```rust {label: "example.rs"}
fn main() {
    println!("Hello, Rust!");
}
```

```bash {label: "usage.sh"}
cargo run
ostendo --help
```

<!-- notes:
FEATURE: Multiple code blocks on one slide
EXPECTED: Two separate code blocks with different languages and labels
VERIFY: Both blocks render with correct syntax highlighting and labels
-->

---

# Basic Table
<!-- section: tables -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

| Feature | Status | Notes |
| --- | --- | --- |
| Bullets | Done | All depths |
| Code | Done | Syntax highlighting |
| Tables | Done | With borders |
| Images | Done | Multi-protocol |
| Columns | Done | 2 and 3 col |

<!-- notes:
FEATURE: Basic table
EXPECTED: Table with box-drawing borders, header row bold/accented
VERIFY: Columns aligned, borders use box chars (┌─┬─┐ etc)
-->

---

# Table Alignment
<!-- section: tables -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

| Left Aligned | Center Aligned | Right Aligned |
| :--- | :---: | ---: |
| alpha | bravo | charlie |
| delta | echo | foxtrot |
| golf | hotel | india |

<!-- notes:
FEATURE: Table column alignment
EXPECTED: Left/center/right alignment per column header markers
VERIFY: Left col left-aligned, center col centered, right col right-aligned
-->

---

# Short Blockquote
<!-- section: quotes -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

> The only truly secure system is one that is powered off.
> -- Gene Spafford

<!-- notes:
FEATURE: Short blockquote
EXPECTED: Text with left border pipe in accent color, italic text
VERIFY: "│" border visible in accent color, text is italic
-->

---

# Long Blockquote (Wrapping)
<!-- section: quotes -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

> This is a very long blockquote line that should wrap automatically at the terminal width boundary while maintaining the left border pipe character and proper indentation throughout the wrapped lines to test the text wrapping functionality added in batch 1

<!-- notes:
FEATURE: Long blockquote with text wrapping
EXPECTED: Long text wraps at terminal width, each wrapped line has the │ prefix
VERIFY: No horizontal truncation, all wrapped lines show │ border
-->

---

# Blockquote with Formatting
<!-- section: quotes -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

> This quote has **bold text** and *italic text* and `inline code` mixed in

> A second blockquote below the first

<!-- notes:
FEATURE: Blockquote with inline formatting
EXPECTED: Bold, italic, code formatting visible within blockquote
VERIFY: Formatting renders inside blockquote, both quotes show borders
-->

---

# Two Equal Columns
<!-- section: columns -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->
- Left column item 1
- Left column item 2
- Left column item 3

<!-- column: 1 -->
- Right column item 1
- Right column item 2
- Right column item 3

<!-- reset_layout -->

<!-- notes:
FEATURE: Two equal columns
EXPECTED: Content split into two equal-width columns
VERIFY: Left and right columns render side by side with equal width
-->

---

# Asymmetric Columns (2:1)
<!-- section: columns -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

<!-- column_layout: [2, 1] -->

<!-- column: 0 -->
- This wider column gets 2/3 width
- Good for main content
- Has more space for longer text

<!-- column: 1 -->
- Narrow 1/3
- Sidebar info
- Compact

<!-- reset_layout -->

<!-- notes:
FEATURE: Asymmetric columns (2:1 ratio)
EXPECTED: Left column is approximately twice as wide as right column
VERIFY: Width ratio visually ~2:1, content doesn't overflow
-->

---

# Three Columns
<!-- section: columns -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

<!-- column_layout: [1, 1, 1] -->

<!-- column: 0 -->
- Col 1 A
- Col 1 B

<!-- column: 1 -->
- Col 2 A
- Col 2 B

<!-- column: 2 -->
- Col 3 A
- Col 3 B

<!-- reset_layout -->

<!-- notes:
FEATURE: Three column layout
EXPECTED: Content split into three equal-width columns
VERIFY: Three columns render side by side, none overlap
-->

---

# Columns with Code
<!-- section: columns -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

<!-- column_layout: [3, 2] -->

<!-- column: 0 -->

```python +exec {label: "col_exec.py"}
import math
print(f"Pi = {math.pi:.4f}")
print(f"e  = {math.e:.4f}")
```

<!-- column: 1 -->
- Code in columns
- Ctrl+E to execute
- Output streams below

<!-- reset_layout -->

<!-- notes:
FEATURE: Columns with code block
EXPECTED: Code block renders within column bounds, executable
VERIFY: Code block fits in left column, right column has bullets
-->

---

# Image: Auto Protocol
<!-- section: images -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

![Opus](../images/opus.png)

<!-- notes:
FEATURE: Image with auto protocol detection
EXPECTED: Image renders using detected protocol (iTerm2 in iTerm/tmux)
VERIFY: Image visible, not garbled, correct aspect ratio
-->

---

# Image: DakotaCon Logo (SVG)
<!-- section: images -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

![DakotaCon Logo](../images/dakotacon.svg)
<!-- image_scale: 50 -->

<!-- notes:
FEATURE: SVG image rendering
EXPECTED: SVG logo rasterized and rendered using detected protocol
VERIFY: SVG renders cleanly, no artifacts, correct aspect ratio
-->

---

# Image: ASCII Art (PNG)
<!-- section: images -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->
<!-- image_render: ascii -->

![Opus ASCII](../images/opus.png)

- Forced ASCII mode via directive

<!-- notes:
FEATURE: Image ASCII rendering (PNG)
EXPECTED: Image renders using ASCII characters
VERIFY: Recognizable image shape in ASCII chars
-->

---

# Image: ASCII Art (SVG)
<!-- section: images -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->
<!-- image_render: ascii -->

![DakotaCon ASCII](../images/dakotacon.svg)
<!-- image_scale: 60 -->

- SVG rendered as ASCII art

<!-- notes:
FEATURE: SVG image in ASCII rendering mode
EXPECTED: SVG rasterized then rendered as ASCII characters
VERIFY: Recognizable shape, clean rendering from SVG source
-->

---

<!-- font_size: 7 -->
# Font Size Directive
<!-- section: display -->
<!-- timing: 0.5 -->

This slide uses `<!-- font_size: 7 -->` directive (+12pt offset)

- Font size only changes in Kitty terminal (OSC 66)
- In other terminals, this has no visible effect
- Use ] and [ keys to adjust (Kitty only)

<!-- notes:
FEATURE: Font size directive
EXPECTED: In Kitty: larger font. In iTerm2/tmux: no visible change (graceful degradation)
VERIFY: No errors regardless of terminal, slide renders normally
-->

---

# Section Transition
<!-- section: sections -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- This slide starts a new section: "sections"
- Section name appears in status bar area
- Use J/K to jump between sections

<!-- notes:
FEATURE: Section transitions
EXPECTED: Section label "sections" visible, J/K navigate between sections
VERIFY: Section name shows in slide content area
-->

---

# Speaker Notes
<!-- section: display -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- Press 'n' to toggle the notes panel
- Notes appear at the bottom of the screen
- Up to 5 lines of notes visible

<!-- notes:
FEATURE: Speaker notes panel
EXPECTED: Pressing 'n' shows notes panel at bottom with consistent background color
VERIFY: Notes panel fills entire bottom area, background covers all rows
TALKING POINTS:
- Press 'n' to toggle the notes panel visibility
- The notes panel occupies 7 rows at the bottom of the screen
- The separator line shows "Notes" with a scroll indicator if content overflows
- Press Shift+N to scroll notes down, Shift+P to scroll notes up
- The notes background should fill the ENTIRE reserved area — no gaps
- Notes scroll position resets when changing slides
- Notes can contain any text — use them for talking points, reminders, timing cues
- This slide's notes are intentionally long to test the scrolling feature
- You should see a scroll indicator like [1/4] in the separator line
- Keep scrolling to verify all lines are accessible
- This is line 11 of the notes
- This is line 12 — if you can see this, notes scrolling works correctly
-->

---

# Timing
<!-- section: display -->
<!-- timing: 2.0 -->
<!-- font_size: 6 -->

- This slide has 2.0 minute timing set
- Timer starts on first slide navigation
- Timer shows in status bar as HH:MM:SS
- Use `:timer` to start, `:timer reset` to reset

<!-- notes:
FEATURE: Slide timing
EXPECTED: Timer visible in status bar, timing directive parsed
VERIFY: Timer counts up in status bar after navigating slides
-->

---

# Theme Switching
<!-- section: display -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- Use `:theme <slug>` to switch themes live
- Try: `:theme dracula` or `:theme cyberpunk`
- All colors update immediately
- Use `--list-themes` CLI flag to see all

<!-- notes:
FEATURE: Theme switching
EXPECTED: :theme command changes all colors (bg, text, accent, code_bg)
VERIFY: Colors change after :theme command, no rendering artifacts
-->

---

# Scale Adjustment
<!-- section: display -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

- Press + to increase content scale
- Press - to decrease content scale
- Default scale: 80%
- Range: 50% to 200%

<!-- notes:
FEATURE: Content scale adjustment
EXPECTED: +/- keys change content width with visible margin changes
VERIFY: Content area widens/narrows, margins adjust symmetrically
-->

---

# Scroll Test: Bullets
<!-- section: scrolling -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

This slide has enough content to require scrolling. Use j/k or arrow keys.

- Bullet 1: The quick brown fox jumps over the lazy dog
- Bullet 2: Pack my box with five dozen liquor jugs
- Bullet 3: How vexingly quick daft zebras jump
- Bullet 4: The five boxing wizards jump quickly
- Bullet 5: Jackdaws love my big sphinx of quartz
- Bullet 6: Lorem ipsum dolor sit amet, consectetur adipiscing elit
- Bullet 7: Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua
- Bullet 8: Ut enim ad minim veniam, quis nostrud exercitation ullamco
- Bullet 9: Duis aute irure dolor in reprehenderit in voluptate velit
- Bullet 10: Excepteur sint occaecat cupidatat non proident
  - Nested 10a: First nested item under bullet 10
  - Nested 10b: Second nested item under bullet 10
    - Deep 10b-i: Third level nesting test
    - Deep 10b-ii: Another third level item
- Bullet 11: Sunt in culpa qui officia deserunt mollit anim id est laborum
- Bullet 12: Curabitur pretium tincidunt lacus
- Bullet 13: Nulla gravida orci a odio
- Bullet 14: Nullam varius, turpis et commodo pharetra
- Bullet 15: Est eros bibendum elit, nec luctus magna felis sollicitudin mauris
- Bullet 16: Integer in mauris eu nibh euismod gravida
- Bullet 17: Duis ac tellus et risus vulputate vehicula
- Bullet 18: Donec lobortis risus a elit
- Bullet 19: Etiam tempor, sapien in ultrices porttitor
- Bullet 20: Scroll should work smoothly through all of these

<!-- notes:
FEATURE: Scrolling with many bullets
EXPECTED: Content extends beyond terminal height, j/k scrolls smoothly
VERIFY: Can scroll down to see all 20 bullets, scroll up returns to top
TALKING POINTS:
- This slide tests the scroll system with 20 bullet points
- Use j/k or arrow keys to scroll the main content up and down
- The status bar at the top should remain steady during scrolling (no flicker)
- Nested bullets at different depths should maintain their indentation while scrolling
- Ctrl+D scrolls down half a page, Ctrl+U scrolls up half a page
- The scroll position resets when navigating to a different slide
- These notes themselves should be scrollable with Shift+N and Shift+P
- Note: if the font is very large, fewer bullets will be visible at once
- The scroll offset is tracked separately from the notes scroll offset
- Both scroll systems work independently of each other
-->

---

# Scroll Test: Code + Bullets + Quotes
<!-- section: scrolling -->
<!-- timing: 0.5 -->
<!-- font_size: 6 -->

Mixed content that requires scrolling through different element types.

- First bullet before the code block
- Second bullet with **bold** formatting

```python {label: "long_example.py"}
# This is a longer code block to test scrolling through code
import os
import sys
import json
from pathlib import Path

def process_data(input_file, output_file):
    """Process data from input to output."""
    with open(input_file, 'r') as f:
        data = json.load(f)

    results = []
    for item in data.get('items', []):
        name = item.get('name', 'unknown')
        value = item.get('value', 0)
        results.append({
            'name': name.upper(),
            'value': value * 2,
            'processed': True
        })

    with open(output_file, 'w') as f:
        json.dump({'results': results}, f, indent=2)

    return len(results)

if __name__ == '__main__':
    count = process_data('input.json', 'output.json')
    print(f"Processed {count} items")
```

> This blockquote appears after the code block to test scrolling through mixed content types including bullets, code, and quotes all on one slide

- Bullet after the blockquote
- Another bullet to verify scroll position
- Final bullet — if you can see this, scrolling works through mixed content

| Header A | Header B | Header C |
| --- | --- | --- |
| Row 1 Col A | Row 1 Col B | Row 1 Col C |
| Row 2 Col A | Row 2 Col B | Row 2 Col C |
| Row 3 Col A | Row 3 Col B | Row 3 Col C |

<!-- notes:
FEATURE: Scrolling through mixed content types
EXPECTED: Code block, blockquote, bullets, and table all scrollable
VERIFY: Can scroll through all element types without rendering glitches
TALKING POINTS:
- This slide combines multiple content types: bullets, code, blockquotes, and tables
- Scrolling should transition smoothly between different element types
- The code block should maintain its syntax highlighting while scrolled
- The blockquote should keep its left border character when partially visible
- Tables should maintain column alignment during scrolling
- No rendering artifacts should appear at the top or bottom edges
- The scroll position should clamp properly — can't scroll past the content
-->

---

# Summary Checklist
<!-- section: final -->
<!-- timing: 1.0 -->
<!-- font_size: 6 -->

All features tested across 32 slides:

- Formatting: title, ASCII art, bullets, bold, italic, strike, code, mixed
- Code: syntax highlighting, execution, multiple blocks, labels
- Tables: basic, alignment
- Quotes: short, long (wrapping), formatted
- Columns: 2-col, asymmetric, 3-col, with code
- Images: auto, ASCII
- Display: font size, sections, notes, timing, themes, scale
- Scrolling: bullet overflow, mixed content overflow

<!-- notes:
FEATURE: Summary/checklist
EXPECTED: Clean bullet list summarizing all features
VERIFY: All bullet text visible, no truncation, proper indentation
TALKING POINTS:
- This is the final slide — summarizes all features tested
- Use this as a reference checklist when verifying the presentation tool
- Each category maps to a group of slides in the presentation
- Formatting covers slides 1-8 (title, ASCII art, bullets, bold, italic, strike, code, mixed)
- Code covers slides 9-11 (syntax highlighting, execution, multiple blocks)
- Tables cover slides 12-13 (basic rendering, column alignment)
- Quotes cover slides 14-16 (short, long wrapping, formatted)
- Columns cover slides 17-20 (2-col, asymmetric, 3-col, with code)
- Images cover slides 20-21 (auto protocol, ASCII)
- Display covers slides 24-29 (font size, sections, notes, timing, themes, scale)
- Scrolling covers slides 30-31 (bullet overflow, mixed content overflow)
- Thank you for testing Ostendo! Report issues at the project repository.
-->
