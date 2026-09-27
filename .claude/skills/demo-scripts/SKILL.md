---
name: demo-scripts
description: "Use when adding live code demos (+exec / +pty blocks, preambles) to Ostendo presentations."
---

# Demo Scripts

Syntax, languages, and limits: the "Code blocks" section of
[`docs/PRESENTATION_FORMAT.md`](../../../docs/PRESENTATION_FORMAT.md).
Implementation: `src/code/executor.rs`.

## Writing a demo that works on stage

- `+exec` for plain output; `+pty` when the program colors its output or
  checks for a terminal.
- Programs cannot read stdin; pass input as literals.
- Finish in a few seconds (hard limit 30 s). Compiled languages (Rust, C, C++,
  Go) compile on every run, so keep them small.
- Keep the visible code short; move imports and helpers into a
  `preamble_start: <lang>` block.
- Label blocks (`{label: "server.py"}`) when a slide has more than one.
- Walk the audience through the code before running it: `{1-2|4-6|all}`
  emphasizes each group in turn, and Ctrl+E works at any step.
- Run every block before presenting; `--validate` only checks that the
  language is runnable, not that the code works.
- Presenters can disable execution with `--no-exec`; the slide should still
  make sense without the output.
