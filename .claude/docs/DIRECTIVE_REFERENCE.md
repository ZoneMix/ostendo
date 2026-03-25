# Directive Reference

Complete list of HTML comment directives parsed by Ostendo. Every directive verified against `regex_patterns.rs`.

## Slide Content Directives

| Directive | Values | Default | Scope |
|---|---|---|---|
| `<!-- section: name -->` | any string | inherited from previous slide | Per-slide, inherits forward |
| `<!-- timing: N -->` | float (minutes) | 0.0 | Per-slide |
| `<!-- ascii_title -->` | (flag) | false | Per-slide |
| `<!-- notes: text -->` | any string | empty | Per-slide, single-line |
| `<!-- notes:\n...\n-->` | multi-line block | empty | Per-slide |
| `<!-- footer: text -->` | any string | none | Per-slide |
| `<!-- footer_align: V -->` | `left` `center` `right` | `left` | Per-slide |
| `<!-- align: V -->` | `top` `center` `vcenter` `hcenter` | `top` | Per-slide |
| `<!-- title_decoration: V -->` | `underline` `box` `banner` `none` | none (or theme default) | Per-slide |
| `<!-- fullscreen -->` | (flag) or `true`/`false` | false | Per-slide |
| `<!-- show_section: V -->` | `true` `false` | true | Per-slide |
| `<!-- theme: slug -->` | theme slug string | global theme | Per-slide override |

## Image Directives

| Directive | Values | Default |
|---|---|---|
| `<!-- image_position: V -->` | `left` `right` | `below` (implicit) |
| `<!-- image_render: V -->` | `ascii` `kitty` `iterm` `iterm2` `sixel` | `auto` |
| `<!-- image_scale: N -->` | 1-100 (percent) | 100 |
| `<!-- image_color: hex -->` | hex color string (e.g., `#FF5500`) | none |

## Font & Scale Directives

| Directive | Values | Default | Terminal Req |
|---|---|---|---|
| `<!-- font_size: N -->` | -3 to 7 (integer) | 0 (base) | Kitty or Ghostty |
| `<!-- font_transition: V -->` | `none` `fade` `dissolve` | `fade` | Kitty |
| `<!-- text_scale: N -->` | 1-7 (OSC 66 factor) | none | Kitty only |
| `<!-- title_scale: N -->` | 1-7 (OSC 66 factor) | none | Kitty only |
| `<!-- column_text_scale: N -->` | 2-7 (OSC 66 factor) | none | Kitty only |

## Column Layout Directives

| Directive | Values | Default |
|---|---|---|
| `<!-- column_layout: [ratios] -->` | comma-separated ints in brackets (e.g., `[1,2,1]`) | none |
| `<!-- column: N -->` | 0-based column index | none |
| `<!-- column_separator: V -->` | `none` (hides the visible separator) | visible (`true`) |
| `<!-- reset_layout -->` | (flag) | n/a |

## Animation Directives

| Directive | Values | Default |
|---|---|---|
| `<!-- transition: V -->` | `fade` `slide` `dissolve` | none (or front matter `transition` field) |
| `<!-- animation: V -->` | `typewriter` `fade_in` `slide_down` | none |
| `<!-- loop_animation: V -->` | `matrix` `bounce` `pulse` `sparkle` `spin` | none |
| `<!-- loop_animation: V(target) -->` | target: `figlet` or `image` | untargeted (all lines) |

Multiple `loop_animation` directives are allowed per slide.

## Code Preamble Directives

| Directive | Values |
|---|---|
| `<!-- preamble_start: lang -->` | language name (e.g., `python`, `rust`) |
| `<!-- preamble_end -->` | (flag, closes preamble block) |

Lines between start/end are prepended to executable code blocks of that language.

## Code Block Fence Modifiers

Parsed from the opening fence line (not HTML comments):

| Pattern | Effect |
|---|---|
| `` ```language `` | Syntax highlighting for `language` |
| `` ```language +exec `` | Block is executable with Ctrl+E |
| `` ```language +pty `` | Block runs in PTY mode (preserves ANSI) |
| `` ```language +exec {label: "name"} `` | Shows label above block |
| `` ```diagram style=bracket `` | Native diagram with bracket style |
| `` ```diagram style=vertical `` | Native diagram with vertical flow |
| `` ```mermaid `` | Mermaid diagram (requires `mmdc` CLI) |

## Front Matter Fields

YAML block between `---` delimiters at file start:

| Field | Type | Effect |
|---|---|---|
| `title` | string | Presentation title (status bar, HTML export title) |
| `author` | string | Author name (status bar footer) |
| `date` | string | Date string (status bar footer) |
| `accent` | hex color | Global accent color override |
| `transition` | string | Default transition for all slides (`fade`, `slide`, `dissolve`) |
| `align` / `alignment` | string | Default content alignment (`top`, `center`, `vcenter`, `hcenter`) |
