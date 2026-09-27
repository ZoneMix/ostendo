# Presentation Format

An Ostendo presentation is one markdown file. Slides are separated by a line
containing only `---`; directives are HTML comments, so the file still reads
well anywhere markdown is rendered. Run `ostendo --validate talk.md` to check a
deck before presenting.

## Front matter

An optional first block between `---` lines holds `key: value` settings:

```markdown
---
title: Shipping Rust at Scale
author: Ada Lovelace
date: 2026-03-14
theme: nord
accent: "#88c0d0"
transition: fade
align: top
---
```

| Key | Effect |
|---|---|
| `title` | Shown in the status bar and used as the exported document title |
| `author` | Shown after the title in the status bar |
| `date` | Shown with the title and author in the overview (`o`) |
| `theme` | Theme slug (`ostendo --list-themes`); `--theme` overrides it |
| `accent` | Hex accent color for the deck's theme; ignored if it would be unreadable (below 3:1 contrast) |
| `transition` | Default transition for every slide: `fade`, `slide`, `dissolve` |
| `align` | Default alignment: `top`, `center`, `vcenter`, `hcenter` |

## Slide content

Elements render in the order they appear in the file.

- `# Title` — the slide title (first `#` heading). Other headings (`##`, `###`)
  render as bold text.
- The first line of text right after the title is the subtitle; later text
  becomes paragraphs (consecutive lines join into one paragraph).
- Lists: `-`, `*`, or `+` followed by a space; indent two spaces per level
  (three levels). Ordered items (`1.`, `2)`) keep their numbers, and task
  items (`- [ ] todo`, `- [x] done`) show a box or a check.
- Inline: `**bold**`, `*italic*` or `_italic_`, `` `code` ``, `~~strike~~`,
  nestable (`**bold with `code`**`). Underscores inside words (`snake_case`)
  stay literal.
- Links: `[text](https://…)`, `<https://…>`, and bare `https://…` URLs are
  underlined and clickable in terminals with hyperlink support (Kitty,
  Ghostty, iTerm2, WezTerm, GNOME Terminal, Windows Terminal).
- Tables: standard pipe tables; `:---`, `:---:`, `---:` set alignment. Wide
  tables shrink their columns and wrap cells.
- Block quotes: `> text`. A line starting with `— ` or `-- ` is shown as an
  attribution.
- Callouts (GitHub alert syntax): a quote whose first line is `[!NOTE]`,
  `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, or `[!CAUTION]` becomes a colored
  panel. Text after the marker replaces the heading: `> [!TIP] Pro move`.
- Images: `![alt](path)`, relative to the markdown file. The alt text becomes a
  caption in text-based rendering.
- Code fences, diagrams, and Mermaid blocks (below).

`---` inside a code fence or an HTML comment does not end the slide.

## Code blocks

````markdown
```python +exec {label: "fib.py"}
print([a for a in range(10)])
```
````

The info string is the language, then optional flags in any order:

| Flag | Effect |
|---|---|
| `+exec` | Ctrl+E runs the block; output streams underneath |
| `+pty` | Like `+exec`, in a pseudo-terminal (programs see a TTY and keep colors) |
| `{label: "name"}` | Label shown in the block's header |
| `{1,3-5\|7\|all}` | Emphasize lines 1 and 3–5, then 7, then none, one group per press of → (see [Building a slide](#building-a-slide)) |

Runnable languages: `python`, `bash`/`sh`, `javascript`/`node`, `ruby`, `rust`,
`c`, `cpp`/`c++`, `go`. Rust, C, C++, and Go snippets without a `main` are
wrapped in one (imports and helper functions are hoisted). Each run has a 30 s
limit and a 1 MB output cap, cannot read the keyboard, and is killed along with
its children when you leave the slide. Pressing Ctrl+E again after a run moves
to the next executable block on the slide. `--no-exec` disables execution.

Shared setup that should not appear on the slide goes in a preamble, prepended
to every block of that language on the slide:

```markdown
<!-- preamble_start: python -->
import math, random
<!-- preamble_end -->
```

## Building a slide

`<!-- pause -->` on its own line hides everything after it until the next
press of →. Code blocks with highlight groups (`{1-2|4|all}`) step through
their groups the same way, in source order with any pauses:

````markdown
# Rollout plan

- Ship behind a flag
<!-- pause -->
- Watch the error budget
<!-- pause -->

```rust {1|3-4|all}
let flag = Flag::new("new-parser");
if flag.enabled() {
    parse_v2(input)
}
```
````

Hidden content keeps its space, so centered slides do not shift as they
build. ← steps back; returning to a slide with ← shows it fully built. Dots
in the status bar show how far the current slide has built. Pauses inside
columns are ignored, and exports show every slide fully built.

## Diagrams

````markdown
```diagram style=bracket
# CI pipeline
Commit -> Build -> Test -> Deploy
: git push : cargo build : cargo test : k8s rollout
```
````

A `#` line is the title; each other line is a row of nodes joined by `->`; a
line starting with `:` annotates the nodes of the row above, one `: ` segment
per node. Styles: `box` (default), `bracket`, `vertical`. When a style does not
fit, labels are shortened and then the next narrower style is used.

A ```` ```mermaid ```` block is rendered to an image with
[mermaid-cli](https://github.com/mermaid-js/mermaid-cli) (`mmdc`) when it is
installed, and shown as source otherwise.

## Directives

Directives apply to the slide they are on.

### Layout and text

| Directive | Values | Effect |
|---|---|---|
| `<!-- section: Name -->` | text | Section shown in the status bar; later slides inherit it |
| `<!-- align: V -->` | `top`, `center`, `vcenter`, `hcenter` | Content alignment |
| `<!-- title_decoration: V -->` | `underline`, `box`, `banner`, `none` | Title style (themes may set a default) |
| `<!-- ascii_title -->` | | Title as FIGlet art, split across lines if needed |
| `<!-- text_scale: N -->` | 2–7 | Title drawn N× larger (Kitty's text sizing; ignored elsewhere) |
| `<!-- font_size: N -->` | -20–20 | Terminal font size for this slide; 1 is your normal size, each step is ±4 pt (Kitty remote control or Ghostty on macOS) |
| `<!-- fullscreen -->` | or `: false` | Hide the status bar on this slide |
| `<!-- show_section: V -->` | `true`, `false` | Show the section label above the title |
| `<!-- footer: text -->` | text | Footer line |
| `<!-- footer_align: V -->` | `left`, `center`, `right` | Footer alignment |
| `<!-- theme: slug -->` | theme slug | Theme for this slide only |

### Speaker notes

```markdown
<!-- notes: One line of notes -->

<!-- notes:
Several lines
of notes
-->
```

Press `n` to show notes under the slide; `N` / `P` scroll them.

### Images

These follow the image line they apply to.

| Directive | Values | Effect |
|---|---|---|
| `<!-- image_position: right -->` | `right` | Pin the image to the right; text wraps beside it |
| `<!-- image_scale: N -->` | 1–100 | Percentage of the available width |
| `<!-- image_render: V -->` | `kitty`, `iterm`, `sixel`, `ascii` | Force a protocol (`ascii` is character art) |
| `<!-- image_color: #hex -->` | hex color | Tint character-art images |

Images use the terminal's graphics protocol when available (Kitty, Ghostty,
iTerm2, WezTerm) and true-color half blocks everywhere else. Animated GIFs play.

### Columns

```markdown
<!-- column_layout: [2, 1] -->
<!-- column: 0 -->
**Left**
- takes two thirds
<!-- column: 1 -->
**Right**
- one third
<!-- reset_layout -->
```

Columns hold text, bullets, code, and one image each, in source order.
`<!-- column_separator: none -->` hides the divider;
`<!-- column_text_scale: N -->` (2–7) enlarges column text on Kitty.

### Animations

| Directive | Values |
|---|---|
| `<!-- transition: V -->` | `fade` (400 ms), `slide` (300 ms), `dissolve` (600 ms) |
| `<!-- animation: V -->` | `typewriter`, `fade_in`, `slide_down` (500 ms entrance) |
| `<!-- loop_animation: V -->` | `matrix`, `bounce`, `pulse`, `sparkle`, `spin` |

Transitions play when arriving at the slide; if the slide also has an entrance
animation, the transition only clears the previous slide and the entrance
reveals the new one. Loop animations run while the slide is shown and can be
limited to part of it: `sparkle(figlet)` affects only an `ascii_title`, and
`spin(image)` only character-art images. Repeat the directive to combine loops.
