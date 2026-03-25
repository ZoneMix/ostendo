> NOTE: The authoritative reference is now in `.claude/docs/DIRECTIVE_REFERENCE.md`.

# Ostendo Presentation Format

This document is the comprehensive human-readable reference for the Ostendo markdown presentation format.

## Front Matter

Every presentation begins with YAML front matter enclosed in `---` delimiters:

```markdown
---
title: My Presentation Title
theme: dracula
---
```

**Supported fields:**
- `title` - Presentation title (shown in window title)
- `theme` - Theme slug (see [THEME_GUIDE.md](THEME_GUIDE.md))

The front matter block must be the very first thing in the file.

## Slide Structure

After the front matter, slides are separated by `---` on its own line:

```markdown
---
title: Demo
theme: terminal_green
---

# First Slide

Content here

---

# Second Slide

More content

---

# Third Slide

Even more content
```

Each slide can contain:
- One title (`# Heading`)
- One subtitle (first non-empty, non-directive line after the title)
- Bullets
- Code blocks (multiple allowed)
- One image
- Tables (multiple allowed)
- Block quotes (multiple allowed)
- Directives (HTML comments)

## Titles

Use a single `# Heading` per slide:

```markdown
# My Slide Title
```

Only the first `#` heading in a slide is treated as the title. It renders in the theme's accent color with bold styling.

### ASCII Art Titles

Add the `<!-- ascii_title -->` directive to render the title as FIGlet ASCII art:

```markdown
# HACK
<!-- ascii_title -->
```

**Tips:**
- Keep titles under 15 characters to avoid overflow
- ALL CAPS often looks best with FIGlet
- The FIGlet font used is "Slant"

## Subtitles

The first non-empty line after the title that is not a directive becomes the subtitle:

```markdown
# Main Title

This line becomes the subtitle

- This is a bullet (not a subtitle)
```

Subtitles render in the theme's text color, slightly dimmer than regular content. Inline formatting (`**bold**`, `*italic*`, etc.) is supported in subtitles.

## Bullets

Use `-` or `*` with indentation for depth:

```markdown
- Top level bullet (depth 0)
  - Second level (depth 1, 2 spaces indent)
    - Third level (depth 2, 4 spaces indent)
- Another top level
  - With a sub-point
```

**Depth rendering:**
| Indent | Depth | Marker |
|--------|-------|--------|
| 0 spaces | 0 | `*` (accent color) |
| 2 spaces | 1 | `-` |
| 4 spaces | 2 | `>` |

Bullets support inline formatting: `**bold**`, `*italic*`, `` `code` ``, `~~strikethrough~~`.

Long bullets are automatically word-wrapped to fit the content width.

## Code Blocks

Standard fenced code blocks with optional modifiers:

````markdown
```python
def hello():
    print("Hello, world!")
```
````

### Language Tag

The language tag after the opening fence controls syntax highlighting. Supported languages include all syntect defaults:

`python`, `rust`, `bash`/`sh`, `json`, `yaml`, `toml`, `javascript`, `typescript`, `go`, `c`, `cpp`, `java`, `ruby`, `sql`, `html`, `css`, `xml`, `lua`, `haskell`, `swift`, `kotlin`, `scala`, `php`, `dart`, `zig`, `r`, and more.

### Executable Code Blocks

Add `+exec` to make a code block executable with `Ctrl+E`:

````markdown
```python +exec
print("This runs when you press Ctrl+E")
```
````

Add `+pty` for interactive programs that need a PTY:

````markdown
```bash +pty
htop
```
````

### Labels

Add a label that appears as a comment header above the code:

````markdown
```python +exec {label: "API request example"}
import requests
resp = requests.get("https://httpbin.org/get")
print(resp.status_code)
```
````

The label renders using the language's comment syntax (e.g., `# API request example` for Python, `// API request example` for Rust).

### Full Syntax

````
```<language> [+exec|+pty] [{label: "name"}]
code here
```
````

All three modifiers are optional and order matters: language first, then exec mode, then label.

## Images

```markdown
![alt text](path/to/image.png)
```

- Path is relative to the presentation markdown file
- Alt text is displayed as a caption below the image
- Supported formats: PNG, JPEG, GIF, BMP, WebP (anything the `image` crate supports)

### Image Directives

Control image rendering with directives on the same slide:

```markdown
![Network Diagram](assets/network.png)
<!-- image_render: ascii -->
<!-- image_position: right -->
<!-- image_scale: 60 -->
```

| Directive | Values | Default |
|-----------|--------|---------|
| `image_render` | `ascii`, `kitty`, `iterm`, `sixel` | CLI setting / auto |
| `image_position` | `left`, `right` | `below` (centered) |
| `image_scale` | `1` - `100` | `100` |

## Tables

Standard markdown pipe tables:

```markdown
| Name    | Role      | Status   |
|---------|-----------|----------|
| Alice   | Engineer  | Active   |
| Bob     | Designer  | On Leave |
```

### Column Alignment

Use colons in the separator row:

```markdown
| Left    | Center  | Right   |
|:--------|:-------:|--------:|
| data    | data    | data    |
```

- `:---` = left-aligned (default)
- `:---:` = center-aligned
- `---:` = right-aligned

## Block Quotes

```markdown
> This is a quoted block
> It can span multiple lines
```

Block quotes render with a vertical bar (`|`) in the accent color, and the text is italicized.

Multiple block quotes per slide are supported, each separated by a blank line.

## Inline Formatting

Supported within bullets, subtitles, block quotes, and table cells:

| Syntax | Result |
|--------|--------|
| `**bold**` | **bold** |
| `*italic*` | *italic* |
| `` `code` `` | inline code (with code background color) |
| `~~strikethrough~~` | ~~strikethrough~~ |

## Column Layouts

Create side-by-side content with column directives:

```markdown
<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

- Left column content
- More left content

<!-- column: 1 -->

- Right column content
- More right content

<!-- reset_layout -->
```

### Ratio Weights

The array in `column_layout` defines relative widths:

- `[1, 1]` = two equal columns (50/50)
- `[2, 1]` = left column is twice as wide (66/33)
- `[1, 1, 1]` = three equal columns (33/33/33)
- `[3, 1]` = left column is 75%, right is 25%

### Column Content

Each column can contain:
- Bullets
- Code blocks

Switch between columns with `<!-- column: N -->` (0-indexed).

Always end with `<!-- reset_layout -->` to return to normal flow.

## Directives Reference

All directives are HTML comments that Ostendo parses:

### Section

```markdown
<!-- section: Introduction -->
```

Sets a section label displayed in the status bar. Inherits to subsequent slides until a new `<!-- section: -->` is set.

### Timing

```markdown
<!-- timing: 2.5 -->
```

Sets expected timing in minutes for pace tracking. The timer in the status bar shows elapsed time.

### ASCII Title

```markdown
<!-- ascii_title -->
```

Renders the slide's `# Title` as FIGlet ASCII art using the Slant font. Place this directive anywhere on the slide.

### Font Size

```markdown
<!-- font_size: 3 -->
```

Sets font size for the slide (1-7). Uses Kitty remote control protocol, supported by kitty terminal. Can also be adjusted at runtime with `]` and `[` keys.

### Column Layout

See the [Column Layouts](#column-layouts) section above.

### Image Rendering

```markdown
<!-- image_render: ascii -->
```

Overrides the image rendering protocol for this slide. Useful when the global protocol doesn't work well for a specific image.

Values: `ascii`, `kitty`, `iterm`, `sixel`

### Image Position

```markdown
<!-- image_position: right -->
```

Places the image to the left or right of the content instead of below it.

Values: `left`, `right`

### Image Scale

```markdown
<!-- image_scale: 50 -->
```

Scales the image to a percentage of the available space.

Values: `1` to `100`

## Speaker Notes

### Single Line

```markdown
<!-- notes: Remember to demo the live API call here -->
```

### Multi-Line

```markdown
<!-- notes:
Key talking points:
- Explain the architecture decision
- Show the performance benchmarks
- Mention the team contributions
-->
```

Notes are toggled with `n` key or `:notes` command during presentation.

## Complete Example

````markdown
---
title: Quarterly Review
theme: frost_glass
---

# Q4 Review
<!-- section: overview -->
<!-- timing: 2.0 -->
<!-- ascii_title -->

Engineering team quarterly review

<!-- notes: Open with team accomplishments -->

---

# Key Metrics
<!-- timing: 3.0 -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

- Uptime: **99.97%**
- Deployments: 142
- Incidents: 3
  - 2 resolved < 1hr
  - 1 resolved < 4hr

<!-- column: 1 -->

- PRs merged: 487
- Test coverage: 89%
- Tech debt: -12%
  - Removed 3 legacy services

<!-- reset_layout -->

<!-- notes: Highlight the improvement in tech debt -->

---

# Architecture
<!-- section: technical -->
<!-- timing: 2.0 -->

![System Architecture](assets/architecture.png)
<!-- image_scale: 70 -->

> The new microservice boundary reduced cross-team dependencies by 40%

---

# Demo
<!-- timing: 1.5 -->

```python +exec {label: "health check"}
import json
status = {"api": "healthy", "db": "healthy", "cache": "healthy"}
print(json.dumps(status, indent=2))
```

---

# Summary

| Area       | Q3    | Q4    | Change |
|:-----------|:-----:|:-----:|-------:|
| Uptime     | 99.9% | 99.97%| +0.07% |
| Deploys    | 98    | 142   | +45%   |
| Coverage   | 82%   | 89%   | +7%    |

**Next steps:**
- Complete migration to new auth service
- Launch performance monitoring dashboard
- Hire 2 additional SREs

<!-- notes: End with concrete next steps and timeline -->
````
