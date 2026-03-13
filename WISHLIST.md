# Ostendo Wishlist

Future features and enhancement ideas for Ostendo, the terminal presentation tool. Contributions welcome! If you're interested in tackling one of these, open an issue to discuss the approach before diving in.

---

## 1. Runtime Element Editing

A modal editor for editing slide content in-place during a live presentation, saving changes back to the source markdown file. This eliminates the context switch of jumping to an external editor, re-saving, and waiting for hot reload when you spot a typo or want to adjust wording mid-talk.

Press `e` to enter edit mode on the current slide. The slide content transforms into an editable text buffer with vi-like keybindings: `h/j/k/l` for movement, `i` for insert mode, `dd` to delete a line, `yy`/`p` for yank/paste. A thin border or background tint distinguishes edit mode from presentation mode. Press `Esc` to exit edit mode, which writes the modified content back to the corresponding `---`-separated block in the markdown file and triggers an immediate re-render. The hot reload watcher should ignore this self-initiated write to avoid a redundant parse cycle.

Implementation would add an `EditBuffer` struct holding the cursor position, selection range, and a copy of the current slide's raw markdown lines. Keypress handling in `input.rs` would branch into the edit buffer's handler when active. On exit, the modified lines replace the original block in the file using byte-offset tracking from the parser.

## 2. Movable/Resizable Presenter Notes Buffer

Allow the speaker notes panel to be repositioned and resized dynamically during a presentation. Currently, presenter notes appear in a fixed location. This feature would support top, bottom, left-side, and right-side positioning, giving presenters control over their screen layout depending on their terminal size and personal preference.

Keyboard shortcuts (`Ctrl+Shift+Arrow`) would cycle the notes panel through the four positions. The split ratio between notes and slide content would be adjustable: `Ctrl+Shift+Plus/Minus` to grow or shrink the notes region in 5% increments, with a minimum of 15% and maximum of 50% of the terminal dimension. Mouse drag on the split border would also resize it for terminals that support mouse events.

The chosen position and split ratio should persist in the existing JSON state file managed by `StateManager`, so the layout preference survives across restarts. When notes are hidden (toggled off with `s`), the slide content reclaims the full viewport. Toggling notes back on restores the last-used position and size.

## 3. Status Bar Position

Support for placing the status bar at the top (current default), bottom, or hiding it entirely. Configurable via front matter (`status_bar: bottom`) or at runtime through a keybinding or command (`:status top|bottom|hidden`).

Bottom positioning is the more natural location for many presenters, matching the convention of most presentation tools and leaving the top of the screen clear for title content. Hidden mode is useful for distraction-free presentations or when recording screencasts where chrome should be minimized. The runtime toggle lets presenters adapt on the fly depending on the venue or audience.

Implementation touches `ui.rs` where the status bar is rendered. The bar's row position would be parameterized rather than hardcoded to row 0 or the last row. When set to bottom, the slide content viewport shifts up by one row; when hidden, the full terminal height is available for content. The preference should persist in the state file alongside theme and other runtime settings.

## 4. PPTX Export

Generate native PowerPoint (.pptx) files from Ostendo presentations, completing the export story alongside the existing HTML and PDF exporters. This is essential for sharing terminal presentations with non-technical stakeholders who expect traditional slide decks that open in PowerPoint, Google Slides, or Keynote.

Theme colors, fonts, and background gradients would map to PowerPoint slide masters and color schemes. Code blocks render as monospace text boxes with syntax highlighting preserved as individual colored runs. Images embed as native slide images at their original resolution. FIGlet ASCII art titles convert to large bold text or optionally render as embedded images to preserve their exact appearance. Mermaid diagrams export as PNG images generated during the export pass.

A Rust PPTX generation library (or a minimal Open XML writer operating on the OOXML zip structure directly) would handle the output. The export module at `src/export/pptx.rs` would follow the same pattern as `html.rs` and `pdf.rs`: accept the parsed slide vector and theme, iterate slides, and produce the output file. Invoked via `--export pptx` on the command line.

## 5. Presenter Display (Dual-Screen)

When running with two terminals or one terminal and one browser window, show the audience view on one screen and a presenter view on the other. The presenter view displays: the current slide, a preview of the next slide, speaker notes, an elapsed/remaining timer, and a thumbnail overview grid for quick navigation.

This extends the existing WebSocket remote control server to serve a dedicated "presenter mode" HTML page. The presenter opens `http://localhost:<port>/presenter` in a browser on their laptop screen while the terminal runs full-screen on the projector. The WebSocket connection streams slide content, notes, and timing data in real time. The presenter page renders a two-panel layout: current + next slide on the left, notes + timer + controls on the right.

This is a common requirement for conference settings where the speaker has a separate monitor facing them. The audience sees only the clean terminal presentation, while the presenter has full context about what's coming next and how much time remains. Navigation controls on the presenter page (arrow buttons, slide picker) send commands back through the WebSocket, so the presenter can drive the talk from either the terminal keyboard or the browser interface.

## 6. Slide Sorter / Reorder Mode

A visual grid mode that extends the current overview grid (`o` key) to allow drag-and-drop reordering of slides. In the current overview, slides are read-only thumbnails. This feature would make the grid interactive: arrow keys move the selection cursor, `Enter` or `Space` grabs the selected slide (highlighting it), arrow keys then move it to a new position, and a second `Enter`/`Space` drops it in place.

Changes write back to the source markdown file by reordering the `---`-separated slide blocks. A preview of the new order is shown before confirming the write, and the operation is undoable with `u` (restoring the previous file contents from a backup kept in memory). This enables fast restructuring of a presentation without leaving the tool or manually editing markdown. Particularly valuable during rehearsal when you realize the narrative flow needs adjustment.

The implementation builds on the overview grid rendering in `ui.rs` and adds an `OverviewInteraction` state machine in `input.rs` that tracks selection, grab, and drop phases. File rewriting uses the byte-offset ranges from the parser to splice blocks into the new order without disturbing front matter or content within slides.

## 7. Audience Polling / Q&A

Real-time audience interaction through the existing WebSocket remote control interface. Presenters define poll questions directly in their markdown using a directive format: `<!-- poll: What is your favorite language? | Rust | Python | Go | TypeScript -->`. When the slide renders, the poll appears as an ASCII-art ballot with labeled options. Audience members connected through the web remote see clickable buttons and submit their votes.

Results stream back through the WebSocket and render as live ASCII bar charts on the presenter's terminal, updating in real time as votes arrive. A `<!-- poll_duration: 30 -->` directive can set an auto-close timer with a countdown displayed on screen. After closing, the final results freeze in place. Multiple polls per presentation are supported, and results can be exported as part of the analytics data (see Presentation Analytics Dashboard below).

Q&A mode is activated with `<!-- qa: open -->`. Audience members type free-text questions through the remote UI, which queue in a scrollable panel overlay on the presenter's screen. The presenter can pin a question to display it full-screen, dismiss it, or mark it as answered. This transforms presentations from one-way broadcasts into interactive sessions, making Ostendo viable for workshops, lectures, and team meetings.

## 8. Incremental Reveal (Build Slides)

Progressive disclosure of bullets and content blocks, similar to PowerPoint's "Appear" animation. A `<!-- reveal: incremental -->` directive on a slide causes each top-level bullet point to appear one at a time on subsequent key presses (Right arrow or Space). The slide initially shows only the title (and any non-bullet content), then each press reveals the next bullet.

An optional `<!-- reveal: dim -->` variant dims previously revealed bullets to 40% opacity (using ANSI dim/faint attributes) as each new bullet appears, focusing the audience's attention on the current point. A `<!-- reveal: all -->` escape hatch shows everything at once for slides where you want the full list visible immediately. Paired with entrance animations, each revealed item can individually fade-in or typewriter-in rather than appearing instantly.

This is essential for presentations that build up complex arguments step by step, introduce agenda items one at a time, or walk through a process sequentially. Without incremental reveal, presenters must either show everything at once (losing suspense and focus) or manually split content across multiple slides (duplicating content and inflating slide count). The implementation adds a `reveal_index` counter to the slide rendering state, and `render_frame()` filters the content lines to only include items up to the current reveal index.

## 9. Slide Templates / Layouts

Pre-defined slide layouts such as "Title Slide", "Two Column", "Quote", "Full-Screen Image", "Section Divider", and "Code Focus". Templates are defined in YAML files alongside themes (in a `templates/` directory) and set default values for `font_size`, `align`, `title_decoration`, `column_layout`, `background`, and other per-slide directives automatically.

Users reference a template with `<!-- template: section_divider -->` at the top of a slide block. The template's defaults apply first, then any explicit directives on the slide override them. For example, the "Section Divider" template might set `font_size: 5`, `align: center`, `title_decoration: banner`, and `background: gradient`, producing a visually distinct break between presentation sections with zero manual directive configuration.

This reduces per-slide boilerplate, enforces visual consistency across a presentation, and makes it easier for new users to create professional-looking decks without memorizing every directive. Theme authors can bundle matching templates, so a theme like "corporate-blue" ships with templates that use its accent colors and preferred decorations. The parser resolves template references during the markdown-to-slide conversion pass, merging template defaults with slide-level overrides.

## 10. LaTeX Math Rendering

Render LaTeX math expressions inline (`$E=mc^2$`) and as display blocks (`$$\int_0^1 f(x)\,dx$$`) within slides. The parser detects `$...$` and `$$...$$` delimiters, extracts the LaTeX source, and renders it to a visual representation suitable for the terminal.

For ASCII-only terminals, the renderer would use a layout engine that converts LaTeX into Unicode math symbols and ASCII art structures (fractions as stacked lines, integrals as tall characters, superscripts/subscripts using Unicode). For protocol-capable terminals (Kitty, iTerm2), the math is rendered to a rasterized image via `katex` CLI or `latex`+`dvipng`, then displayed inline using the image protocol for maximum fidelity. A `<!-- math_render: ascii|image|auto -->` directive controls the mode, defaulting to auto-detection based on terminal capabilities.

This is essential for academic and technical presentations involving formulas, proofs, statistical models, or equations. Without math rendering, presenters must pre-render equations as images and embed them manually, which breaks the markdown-native workflow. A built-in math pipeline keeps everything in the source file and renders consistently across theme changes.

## 11. Presentation Analytics Dashboard

After a presentation ends (or on demand via a `--analytics` flag), display a summary dashboard showing: total presentation time, time spent per slide versus planned timing (from `<!-- timing: 2m -->` directives), slides that were skipped, slides that were revisited or lingered on, and audience engagement metrics if polling was enabled during the session.

The dashboard renders as a full-screen terminal view with ASCII bar charts for time distribution, a table of slide-by-slide statistics, and highlighted outliers (slides where you spent more than 2x the planned time, or less than 25%). Export the data as JSON (`--analytics-export analytics.json`) or as a markdown summary for inclusion in post-presentation notes.

This helps presenters improve their pacing over repeated deliveries of the same talk. By comparing analytics across rehearsals, you can identify sections that consistently run long and need trimming, or sections you rush through that might need more preparation. The timer system already built into the status bar provides the raw timing data; this feature adds collection, aggregation, and visualization on top of it.

## 12. Plugin System (Lua/WASM)

Allow custom slide renderers, animations, directives, and code executors through a plugin system. Plugins can be Lua scripts (lightweight and easy to author, embedded via the `rlua` or `mlua` crate) or WebAssembly modules (sandboxed, portable, language-agnostic). A plugin registers one or more extension points: a new `<!-- directive: ... -->` handler, a custom animation type, a custom code executor for additional languages, or a post-render hook that modifies the styled line buffer.

Plugins live in a `plugins/` directory (per-project or global at `~/.config/ostendo/plugins/`). A manifest file (`plugin.yaml`) declares the plugin name, version, supported extension points, and configuration schema. Ostendo discovers and loads plugins at startup, providing them with a sandboxed API that exposes: slide content (read-only for renderers, read-write for transformers), terminal dimensions, theme colors, and output functions for emitting styled lines or images.

This enables community-driven extension without forking the project. Examples: a `marp-compat` plugin that supports Marp-style directives, a `chartjs` plugin that renders chart specifications as ASCII graphs, a `live-code` plugin that connects to a running REPL and streams output to the slide, or a `custom-transition` plugin that implements new transition effects. The WASM sandbox ensures plugins cannot access the filesystem or network beyond what Ostendo explicitly grants.

## 13. Live Collaboration

Multiple presenters co-present from separate terminals connected to the same presentation session via WebSocket. One participant is the "driver" who controls slide navigation; others see slides advance in sync automatically. Any participant can request driver control, and the current driver can grant or deny it, enabling smooth handoffs between speakers.

Each connected terminal shows cursor presence indicators: small colored markers in the overview grid showing which slide each collaborator is currently viewing (for participants who have temporarily navigated away from the synced view to preview upcoming slides). A chat sidebar, toggled with `Ctrl+C`, provides a text channel for coordination between speakers without disrupting the audience view.

This is valuable for team presentations where different speakers take different sections, remote training sessions where an instructor needs to control what students see, and pair-presentation rehearsals where two people refine a talk together. The implementation extends the existing WebSocket remote server with session management, role assignment (driver/viewer), and a state synchronization protocol that broadcasts navigation events, cursor positions, and chat messages to all connected clients.
