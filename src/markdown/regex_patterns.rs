//! Compiled regex patterns for the Markdown parser.
//!
//! Each `static` is a lazily-compiled `Regex` wrapped in `LazyLock`. The pattern is compiled
//! exactly once on first access and reused for every subsequent call, avoiding the overhead
//! of recompiling inside a loop.

use regex::Regex;
use std::sync::LazyLock;

/// Matches the opening of a fenced code block with an optional language, exec mode, and label.
/// Capture groups: (1) language, (2) `+exec` or `+pty`, (3) label text.
/// Example: `` ```python +exec {label: "demo.py"} ``
pub(crate) static FENCE_OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^```(\w*)\s*(\+exec|\+pty)?\s*(?:\{label:\s*"([^"]*)"\s*\})?\s*$"#).unwrap()
});

/// Matches the opening of a diagram code block with an optional style parameter.
/// Capture group: (1) style name (e.g., "bracket", "vertical").
/// Example: `` ```diagram style=bracket ``
pub(crate) static DIAGRAM_FENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^```diagram\s*(?:style=(\w+))?\s*$"#).unwrap()
});

/// Matches the closing of a fenced code block (three backticks on their own line).
/// Example: `` ``` ``
pub(crate) static FENCE_CLOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^```\s*$").unwrap());

// ── Directive regex patterns ──

/// Matches `<!-- section: <name> -->`. Assigns the slide to a named section.
/// Example: `"<!-- section: intro -->"`
pub(crate) static SECTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*section:\s*(\S+)\s*-->").unwrap());

/// Matches `<!-- timing: <minutes> -->`. Sets the speaker-time budget for a slide.
/// Example: `"<!-- timing: 2.5 -->"`
pub(crate) static TIMING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*timing:\s*([\d.]+)\s*-->").unwrap());

/// Matches `<!-- ascii_title -->`. Enables FIGlet ASCII-art rendering for the slide title.
/// Example: `"<!-- ascii_title -->"`
pub(crate) static ASCII_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*ascii_title\s*-->").unwrap());

/// Matches `<!-- image_position: left|right -->`. Places the image to the left or right of text.
/// Example: `"<!-- image_position: left -->"`
pub(crate) static IMAGE_POS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*image_position:\s*(left|right)\s*-->").unwrap());

/// Matches `<!-- image_render: ascii|kitty|iterm|iterm2|sixel -->`. Forces a specific image protocol.
/// Example: `"<!-- image_render: kitty -->"`
pub(crate) static IMAGE_RENDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*image_render:\s*(ascii|kitty|iterm2?|sixel)\s*-->").unwrap());

/// Matches `<!-- image_scale: <percent> -->`. Scales the image (1-100%).
/// Example: `"<!-- image_scale: 50 -->"`
pub(crate) static IMAGE_SCALE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*image_scale:\s*(\d+)\s*-->").unwrap());

/// Matches `<!-- image_color: <color> -->`. Overrides the tint color for ASCII-art images.
/// Example: `"<!-- image_color: #FF5500 -->"`
pub(crate) static IMAGE_COLOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*image_color:\s*(\S+)\s*-->").unwrap());

/// Matches the start of a multi-line speaker notes block. The block ends at `-->`.
/// Example: `"<!-- notes:"`  (note: no closing `-->` on the same line)
pub(crate) static NOTES_MULTI_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*notes:\s*$").unwrap());

/// Matches a single-line speaker note. Capture group (1) is the note text.
/// Example: `"<!-- notes: Remember to demo this -->"`
pub(crate) static NOTES_SINGLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*notes:\s*(.*?)\s*-->\s*$").unwrap());

/// Matches the closing `-->` of a multi-line notes block.
/// Example: `"-->"`
pub(crate) static NOTES_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-->\s*$").unwrap());

/// Matches any single-line HTML comment. Used as a catch-all to skip unrecognized directives.
/// Example: `"<!-- any comment here -->"`
pub(crate) static HTML_COMMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--.*-->\s*$").unwrap());

/// Matches `<!-- column_layout: [ratios] -->`. Defines a multi-column layout with width ratios.
/// Capture group (1) is the comma-separated ratio list.
/// Example: `"<!-- column_layout: [1,2,1] -->"`
pub(crate) static COLUMN_LAYOUT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*column_layout:\s*\[([^\]]+)\]\s*-->").unwrap());

/// Matches `<!-- column: <index> -->`. Switches content output to the given column index.
/// Example: `"<!-- column: 0 -->"`
pub(crate) static COLUMN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*column:\s*(\d+)\s*-->").unwrap());

/// Matches `<!-- column_separator: none -->`. Hides the visible `│` separator between columns.
/// Capture group (1) is the value (currently only `"none"` is meaningful).
/// Example: `"<!-- column_separator: none -->"`
pub(crate) static COLUMN_SEPARATOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*column_separator:\s*(\w+)\s*-->").unwrap());

/// Matches `<!-- reset_layout -->`. Ends the current column layout and returns to normal flow.
/// Example: `"<!-- reset_layout -->"`
pub(crate) static RESET_LAYOUT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*reset_layout\s*-->").unwrap());

/// Matches `<!-- font_size: <n> -->`. Sets the terminal font size delta (-3 to 7).
/// Negative values shrink below base; positive values enlarge. Kitty terminal only.
/// Example: `"<!-- font_size: 3 -->"`
pub(crate) static FONT_SIZE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*font_size:\s*(-?\d+)\s*-->").unwrap());

/// Matches `<!-- font_transition: <mode> -->`. Controls how font size changes animate.
/// Use `none` for instant changes. Kitty terminal only.
/// Example: `"<!-- font_transition: none -->"`
pub(crate) static FONT_TRANSITION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*font_transition:\s*(\w+)\s*-->").unwrap());

/// Matches `<!-- text_scale: <n> -->`. Scales title + subtitle via OSC 66 protocol (1-7).
/// Example: `"<!-- text_scale: 3 -->"`
pub(crate) static TEXT_SCALE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*text_scale:\s*(\d+)\s*-->").unwrap());

/// Matches `<!-- title_scale: <n> -->`. Scales title only via OSC 66 protocol (1-7).
/// Example: `"<!-- title_scale: 5 -->"`
pub(crate) static TITLE_SCALE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*title_scale:\s*(\d+)\s*-->").unwrap());

/// Matches a Markdown level-1 heading (`# Title text`). Capture group (1) is the title.
/// Example: `"# Welcome to My Presentation"`
pub(crate) static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#\s+(.+)$").unwrap());

/// Matches a Markdown image reference. Capture groups: (1) alt text, (2) file path.
/// Example: `"![architecture diagram](images/arch.png)"`
pub(crate) static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^!\[([^\]]*)\]\(([^)]+)\)\s*$").unwrap());

/// Matches a Markdown bullet point (`-` or `*`). Capture groups: (1) leading whitespace (for
/// nesting depth), (2) bullet text. Depth: 0 spaces = level 0, 2+ = level 1, 4+ = level 2.
/// Example: `"  - Nested bullet item"`
pub(crate) static BULLET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)[-*]\s*(.*)$").unwrap());

/// Matches the `---` slide separator on its own line. The `(?m)` flag enables multi-line mode
/// so that `^` and `$` match at line boundaries, not just the start/end of the whole string.
/// Example: `"---"`
pub(crate) static SLIDE_SEPARATOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^---\s*$").unwrap());

/// Matches a Markdown table data row (pipe-delimited cells). Does not distinguish header from body.
/// Example: `"| Name | Value |"`
pub(crate) static TABLE_ROW_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\|(.+)\|\s*$").unwrap());

/// Matches a Markdown table separator row with dashes and optional colons for alignment.
/// Example: `"| :--- | :---: | ---: |"`
pub(crate) static TABLE_SEP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\|[\s:]*-+[\s:]*(\|[\s:]*-+[\s:]*)*\|\s*$").unwrap());

/// Matches a Markdown block quote line. Capture group (1) is the text after `>`.
/// Example: `"> This is a quote"`
pub(crate) static BLOCKQUOTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^>\s?(.*)$").unwrap());

/// Matches `<!-- footer: <text> -->`. Sets a per-slide footer message.
/// Example: `"<!-- footer: Confidential -->"`
pub(crate) static FOOTER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*footer:\s*(.*?)\s*-->").unwrap());

/// Matches `<!-- footer_align: left|center|right -->`. Controls footer text alignment.
/// Example: `"<!-- footer_align: center -->"`
pub(crate) static FOOTER_ALIGN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*footer_align:\s*(left|center|right)\s*-->").unwrap());

/// Matches `<!-- align: top|center|vcenter|hcenter -->`. Controls vertical/horizontal alignment.
/// `center` = both axes, `vcenter` = vertical only, `hcenter` = horizontal only.
/// Example: `"<!-- align: center -->"`
pub(crate) static ALIGN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*align:\s*(top|center|vcenter|hcenter)\s*-->").unwrap());

/// Matches `<!-- title_decoration: underline|box|banner|none -->`. Adds visual decoration to
/// the slide title.
/// Example: `"<!-- title_decoration: box -->"`
pub(crate) static TITLE_DECORATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*title_decoration:\s*(underline|box|banner|none)\s*-->").unwrap());

/// Matches `<!-- transition: fade|slide|dissolve -->`. Sets the transition animation when
/// navigating to this slide.
/// Example: `"<!-- transition: dissolve -->"`
pub(crate) static TRANSITION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*transition:\s*(fade|slide|dissolve)\s*-->").unwrap());

/// Matches `<!-- animation: typewriter|fade_in|slide_down -->`. Sets a one-shot entrance
/// animation that plays when the slide first appears.
/// Example: `"<!-- animation: typewriter -->"`
pub(crate) static ANIMATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*animation:\s*(typewriter|fade_in|slide_down)\s*-->").unwrap());

/// Matches `<!-- loop_animation: <type>[(target)] -->`. Sets a continuous animation that runs
/// while the slide is displayed. Optional `(target)` limits the effect to `figlet` or `image`.
/// Multiple loop animations are allowed on a single slide.
/// Example: `"<!-- loop_animation: sparkle(figlet) -->"`
pub(crate) static LOOP_ANIMATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*loop_animation:\s*(matrix|bounce|pulse|sparkle|spin)(?:\((\w+)\))?\s*-->").unwrap());

/// Matches `<!-- fullscreen -->` or `<!-- fullscreen: true|false -->`. When enabled, the slide
/// hides the status bar and uses the full terminal height.
/// Example: `"<!-- fullscreen: true -->"`
pub(crate) static FULLSCREEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*fullscreen(?::\s*(true|false))?\s*-->").unwrap());

/// Matches `<!-- show_section: true|false -->`. Controls whether the section name appears
/// in the status bar for this slide.
/// Example: `"<!-- show_section: false -->"`
pub(crate) static SHOW_SECTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*show_section:\s*(true|false)\s*-->").unwrap());

/// Matches `<!-- theme: <slug> -->`. Overrides the presentation theme for this slide only.
/// The slug must match a theme file in the `themes/` directory.
/// Example: `"<!-- theme: cyber_red -->"`
pub(crate) static THEME_OVERRIDE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*theme:\s*(\S+)\s*-->").unwrap());

/// Matches `<!-- preamble_start: <language> -->`. Begins a code preamble block for the given
/// language. Lines between this and `<!-- preamble_end -->` are prepended to executable code blocks.
/// Example: `"<!-- preamble_start: python -->"`
pub(crate) static PREAMBLE_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*preamble_start:\s*(\w+)\s*-->").unwrap());

/// Matches `<!-- preamble_end -->`. Ends the current code preamble block.
/// Example: `"<!-- preamble_end -->"`
pub(crate) static PREAMBLE_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*<!--\s*preamble_end\s*-->").unwrap());
