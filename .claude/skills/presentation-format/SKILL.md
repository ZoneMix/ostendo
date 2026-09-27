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
3. `ostendo --validate deck.md --size 100x30` — must report no problems
   (missing images, unknown themes, unrunnable `+exec` languages, empty
   slides, and slides too tall to fit without scrolling at that size).
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
- Build arguments with `<!-- pause -->` and walk through code with
  `{1-3|5|all}` groups instead of splitting one idea across slides.
- Use callouts (`> [!WARNING]`) for the one thing the audience must not
  miss; one per slide.
- Numbers that compare belong in a ```` ```chart ````; end a talk with a
  ```` ```qr ```` of the slides' URL.
- Formulas go in `$$ … $$` on their own lines; keep inline `$…$` short, as
  it stays on one line (`a/b`, `x²`).
- Start title and section slides with `<!-- template: title -->` /
  `<!-- template: section -->` instead of repeating their directives.
- Set `duration:` in the front matter so the timer shows the pace, and check
  `ostendo deck.md --report` after rehearsals.
