//! Parsed slide model produced by `crate::markdown::parse_presentation` and consumed by the
//! renderer and exporters. `Option` directive fields are `None` unless the slide sets them.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use crate::render::animation::{EntranceAnimation, LoopAnimation, TransitionType};

/// `<!-- footer_align: left|center|right -->`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FooterAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// `<!-- align: top|center|vcenter|hcenter -->`; `center` centers on both axes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SlideAlignment {
    #[default]
    Top,
    Center,
    VCenter,
    HCenter,
}

/// Deck-wide settings from the front matter block. Empty strings mean "not set".
#[derive(Debug, Clone, Default)]
pub struct PresentationMeta {
    pub title: String,
    pub author: String,
    pub date: String,
    /// Hex color that replaces the theme accent.
    pub accent: String,
    pub default_alignment: Option<SlideAlignment>,
    /// Transition name (`fade`, `slide`, `dissolve`) for slides without their own.
    pub transition: String,
    pub theme: Option<String>,
    /// `duration: 20m`: planned talk length, for the pace shown by the timer.
    pub duration: Option<std::time::Duration>,
    /// `templates:` block: name -> directives (name, value) applied by
    /// `<!-- template: name -->`.
    pub templates: HashMap<String, Vec<(String, Option<String>)>>,
}

#[derive(Debug, Clone, Default)]
pub struct Slide {
    /// 1-based position in the deck.
    pub number: usize,
    /// Text of the first `# ` heading.
    pub title: String,
    /// Set by `<!-- section: name -->`; inherited from the previous slide otherwise.
    pub section: String,
    /// First plain-text line after the title, when nothing else precedes it.
    pub subtitle: String,
    /// Body elements in source order (title, subtitle and directives excluded).
    pub blocks: Vec<Block>,
    /// Plain-text paragraphs other than the subtitle; consecutive lines are joined by a space.
    pub paragraphs: Vec<String>,
    /// `-`/`*`/`+` and ordered (`1.`, `1)`) items outside columns, flattened across groups.
    /// Ordered items keep their number at the start of `text`.
    pub bullets: Vec<Bullet>,
    /// Ranges into `bullets`, one per run of list items not interrupted by another block.
    pub bullet_groups: Vec<Range<usize>>,
    pub code_blocks: Vec<CodeBlock>,
    /// Last `![alt](path)` outside columns.
    pub image: Option<SlideImage>,
    /// `<!-- ascii_title -->`: render the title as FIGlet art.
    pub ascii_title: bool,
    /// From `<!-- notes: ... -->`, single- or multi-line.
    pub notes: String,
    /// Present when the slide declares `<!-- column_layout: [..] -->`.
    pub columns: Option<ColumnLayout>,
    pub tables: Vec<Table>,
    pub block_quotes: Vec<BlockQuote>,
    /// `<!-- font_size: N -->`, clamped to -20..=20; applied via terminal font control.
    pub font_size: Option<i8>,
    /// `<!-- text_scale: N -->` (1-7): OSC 66 scale factor for the title.
    pub text_scale: Option<u8>,
    pub footer: Option<String>,
    pub footer_align: FooterAlign,
    pub alignment: Option<SlideAlignment>,
    /// `<!-- title_decoration: underline|box|banner|none -->`; overrides the theme default.
    pub title_decoration: Option<String>,
    /// `<!-- transition: fade|slide|dissolve -->`, played when navigating to this slide.
    pub transition: Option<TransitionType>,
    /// `<!-- animation: typewriter|fade_in|slide_down -->`.
    pub entrance_animation: Option<EntranceAnimation>,
    /// `<!-- loop_animation: name[(target)] -->`; the target (`figlet`, `image`) limits which
    /// lines animate.
    pub loop_animations: Vec<(LoopAnimation, Option<String>)>,
    /// `<!-- fullscreen -->` hides the status bar for this slide.
    pub fullscreen: Option<bool>,
    pub show_section: Option<bool>,
    /// Language -> code between `<!-- preamble_start: lang -->` and `<!-- preamble_end -->`,
    /// prepended to executable blocks of that language.
    pub code_preambles: HashMap<String, String>,
    pub mermaid_blocks: Vec<MermaidBlock>,
    pub diagram_blocks: Vec<DiagramBlock>,
    /// `<!-- theme: slug -->` for this slide only.
    pub theme_override: Option<String>,
    pub charts: Vec<Chart>,
    /// Text of each ```` ```qr ```` block, drawn as a QR code.
    pub qr_codes: Vec<String>,
    /// What each press of → does before the deck moves on, in source order.
    pub steps: Vec<Step>,
    /// Hash of the slide's source lines; hot reload uses it to find the slide
    /// an edit touched.
    pub fingerprint: u64,
    /// A `<!-- template: name -->` that names no built-in or deck template.
    pub missing_template: Option<String>,
}

/// One build step of a slide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// `<!-- pause -->`: `blocks[n..]` stay hidden until this step.
    Pause(usize),
    /// Code block `code` moves to its highlight group `group`.
    Highlight { code: usize, group: usize },
}

/// One body element of a slide; indexes point into the matching `Slide` vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Paragraph(usize),
    /// Index into `bullet_groups`.
    Bullets(usize),
    Code(usize),
    Table(usize),
    Quote(usize),
    Diagram(usize),
    Mermaid(usize),
    Chart(usize),
    Qr(usize),
    Image,
    Columns,
}

/// A ```` ```chart ```` block: one bar per `label: value` line, `# title`
/// optional.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub title: Option<String>,
    /// Label, value, and the value as written (`42 ms`).
    pub bars: Vec<(String, f64, String)>,
    /// `style=columns`: vertical bars instead of horizontal ones.
    pub columns: bool,
}

#[derive(Debug, Clone)]
pub struct ColumnLayout {
    /// Relative widths from `<!-- column_layout: [1, 2, 1] -->`; one entry per column.
    pub ratios: Vec<u8>,
    /// One entry per ratio, filled after `<!-- column: N -->` (0-based).
    pub contents: Vec<ColumnContent>,
    /// `false` after `<!-- column_separator: none -->`.
    pub separator: bool,
    /// `<!-- column_text_scale: N -->` (2-7): OSC 66 scale for columns without an image.
    pub text_scale: Option<u8>,
}

/// Column images always render as ASCII art because columns are merged as text rows.
#[derive(Debug, Clone)]
pub struct ColumnImage {
    /// Resolved against the presentation directory.
    pub path: String,
    /// Percentage of the column width (1-100).
    pub scale: Option<u8>,
    /// Hex tint for the ASCII art.
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ColumnContent {
    pub bullets: Vec<Bullet>,
    pub code_blocks: Vec<CodeBlock>,
    pub image: Option<ColumnImage>,
    /// Plain-text lines, typically a column header.
    pub text_lines: Vec<String>,
    pub tables: Vec<Table>,
    pub quotes: Vec<BlockQuote>,
    /// The column's content in source order.
    pub items: Vec<ColumnItem>,
    /// For each of `items`, how many `<!-- pause -->`s come before it on the
    /// slide; it shows once that many build steps have.
    pub pauses_before: Vec<usize>,
}

/// One entry of [`ColumnContent::items`], indexing into the column's lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnItem {
    Text(usize),
    Bullet(usize),
    Code(usize),
    Table(usize),
    Quote(usize),
    Image,
}

#[derive(Debug, Clone)]
pub struct Bullet {
    /// Item text with inline markdown intact.
    pub text: String,
    /// 0 for no indent, 1 for 2+ spaces, 2 for 4+ spaces.
    pub depth: usize,
}

impl Bullet {
    /// A task-list item (`[ ] text`, `[x] text`): whether it is done, and its text.
    pub fn task(&self) -> Option<(bool, &str)> {
        let rest = self.text.strip_prefix('[')?;
        let (mark, text) = rest.split_once("] ")?;
        match mark {
            " " => Some((false, text)),
            "x" | "X" => Some((true, text)),
            _ => None,
        }
    }

    /// Whether the item came from an ordered list (`1.` or `1)`).
    pub fn is_ordered(&self) -> bool {
        self.text.split_once(' ').is_some_and(|(marker, _)| {
            marker.len() >= 2
                && marker.ends_with(['.', ')'])
                && marker[..marker.len() - 1]
                    .bytes()
                    .all(|b| b.is_ascii_digit())
        })
    }
}

#[derive(Debug, Clone)]
pub struct CodeBlock {
    /// Language from the fence info string.
    pub language: String,
    pub code: String,
    /// From `{label: "name"}` on the fence line.
    pub label: String,
    /// `+exec` or `+pty` on the fence line.
    pub exec_mode: Option<ExecMode>,
    /// `{1,3-5|7|all}` on the fence line: groups of 1-based inclusive line
    /// ranges emphasized one step at a time. An empty group (`all`) emphasizes
    /// nothing; no groups means no emphasis at all.
    pub highlights: Vec<Vec<(usize, usize)>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecMode {
    /// Capture stdout/stderr.
    Exec,
    /// Run in a pseudo-terminal, keeping ANSI output.
    Pty,
}

#[derive(Debug, Clone)]
pub struct SlideImage {
    /// Resolved against the presentation directory.
    pub path: PathBuf,
    pub alt_text: String,
    pub position: ImagePosition,
    pub render_mode: ImageRenderMode,
    /// `<!-- image_scale: N -->` percentage (1-100).
    pub scale: u8,
    /// `<!-- image_color: #hex -->` tint for ASCII rendering; empty means none.
    pub color_override: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ImagePosition {
    #[default]
    Below,
    Left,
    Right,
}

/// `<!-- image_render: ascii|kitty|iterm|sixel -->`; `Auto` uses the detected protocol.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ImageRenderMode {
    #[default]
    Auto,
    Kitty,
    Iterm,
    Sixel,
    Ascii,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub headers: Vec<String>,
    pub alignments: Vec<TableAlign>,
    /// Cells per row; empty cells are kept.
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct BlockQuote {
    /// Lines with the leading `> ` removed (and the `[!KIND]` line of a callout).
    pub lines: Vec<String>,
    /// `> [!NOTE]` (GitHub alert syntax) with the heading to show: the text
    /// after the marker, or the kind's name.
    pub callout: Option<(Callout, String)>,
}

/// GitHub alert kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Callout {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Callout {
    pub fn name(self) -> &'static str {
        match self {
            Callout::Note => "Note",
            Callout::Tip => "Tip",
            Callout::Important => "Important",
            Callout::Warning => "Warning",
            Callout::Caution => "Caution",
        }
    }
}

/// A ```` ```mermaid ```` block, rendered to an image by the external `mmdc` CLI.
#[derive(Debug, Clone)]
pub struct MermaidBlock {
    pub source: String,
}

/// ```` ```diagram style=box|bracket|vertical ````.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DiagramStyle {
    #[default]
    Box,
    Bracket,
    Vertical,
}

/// A ```` ```diagram ```` block, rendered by the built-in ASCII diagram engine.
#[derive(Debug, Clone)]
pub struct DiagramBlock {
    pub source: String,
    pub style: DiagramStyle,
}
