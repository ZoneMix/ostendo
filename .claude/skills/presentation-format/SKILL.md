---
name: presentation-format
description: "Use when creating or editing Ostendo markdown presentations: front matter, slide elements, directives, columns, code execution."
---

# Presentation Format

The complete syntax is in [`docs/PRESENTATION_FORMAT.md`](../../../docs/PRESENTATION_FORMAT.md).
Read it before writing a deck; do not guess directive names — unknown
directives are silently ignored.

## Workflow

1. Front matter: `title`, `author`, `theme` (`ostendo --list-themes`).
2. One idea per slide, slides separated by `---`.
3. `ostendo --validate deck.md` — must report no problems (missing images,
   unknown themes, unrunnable `+exec` languages, empty slides).
4. Present it and page through every slide at the terminal size you will use;
   `presentations/examples/test_presentation.md` shows every feature.

## Writing slides that look good

- Titles: a few words. `<!-- ascii_title -->` suits short titles (under ~15
  characters); longer ones split across lines.
- At most five or six bullets; split the slide instead of scrolling.
- Group slides with `<!-- section: Name -->`; later slides inherit it.
- Code on a slide should fit without scrolling (about 15 lines). Hide imports
  in a `preamble_start`/`preamble_end` block.
- Images: one per slide, or one per column. `image_position: right` puts text
  beside it.
- Put speaker notes on every slide: `<!-- notes: ... -->`.
- Use `align: center` for title and section-break slides only.
- Animations sparingly: a transition in front matter is enough for most decks;
  loops (`sparkle(figlet)`, `matrix`) belong on title slides.
