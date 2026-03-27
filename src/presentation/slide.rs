//! Core data structures representing a parsed presentation slide and its content elements.
//!
//! This module defines [`Slide`] and all the types it contains — bullets, code blocks,
//! images, tables, columns, block quotes, and Mermaid/diagram blocks. The Markdown
//! parser (`crate::markdown::parser`) produces a `Vec<Slide>` which the render engine
//! then walks to display each slide on screen.
//!
//! Most fields on `Slide` map directly to HTML comment directives that the author
//! places in their Markdown source (e.g., `<!-- font_size: 3 -->`). Fields that are
//! `Option<T>` default to `None` and are only set when the author explicitly provides
//! the corresponding directive.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::render::animation::{EntranceAnimation, LoopAnimation, TransitionType};

/// Alignment for the per-slide footer bar.
///
/// Set via `<!-- footer_align: left|center|right -->` on a slide.
/// Defaults to `Left` when no directive is present.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FooterAlign {
    /// Footer text is left-aligned (the default).
    #[default]
    Left,
    /// Footer text is centered horizontally.
    Center,
    /// Footer text is right-aligned.
    Right,
}

/// Alignment for slide content.
///
/// Controls how the body content of a slide is positioned within the terminal
/// viewport. Set via `<!-- align: top|center|vcenter|hcenter -->`.
/// Defaults to `Top` (content starts at the top of the slide area).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SlideAlignment {
    /// Content starts at the top of the viewport (the default).
    #[default]
    Top,
    /// Content is centered both vertically and horizontally.
    Center,
    /// Content is vertically centered only.
    VCenter,
    /// Content is horizontally centered only.
    HCenter,
}

/// Metadata extracted from front matter (the YAML block between `---` delimiters).
///
/// Front matter appears at the very top of a presentation Markdown file and provides
/// global settings that apply to the entire presentation (title, author, default
/// theme accent color, etc.).
#[derive(Debug, Clone, Default)]
pub struct PresentationMeta {
    /// Presentation title, shown in the status bar and exported HTML `<title>`.
    pub title: String,
    /// Author name, displayed in the status bar footer area.
    pub author: String,
    /// Date string, displayed alongside the author in the footer.
    pub date: String,
    /// Global accent color override as a hex string (e.g., `"#FF5733"`).
    /// When set, this replaces the theme's accent color.
    pub accent: String,
    /// Default content alignment applied to all slides unless overridden per-slide.
    pub default_alignment: Option<SlideAlignment>,
    /// Default slide transition type name (e.g., `"fade"`, `"slide-left"`).
    pub transition: String,
    /// Raw key-value pairs from front matter for forward-compatibility.
    /// Any YAML key that doesn't match a known field is stored here so that
    /// future versions can consume it without changing this struct.
    pub pairs: Vec<(String, String)>,
}

/// A single slide in the presentation.
///
/// Each Markdown `---` separator produces a new `Slide`. The parser fills in
/// whichever fields are present in the Markdown source; everything else gets
/// the value from the [`Default`] implementation (empty strings, empty `Vec`s,
/// `None`, etc.).
///
/// The render engine reads these fields to decide what to draw on each frame.
#[derive(Debug, Clone)]
pub struct Slide {
    /// Zero-based slide index within the presentation.
    pub number: usize,
    /// The slide title, parsed from the first `# Heading` on the slide.
    pub title: String,
    /// Section name this slide belongs to, parsed from `## Section` headings.
    /// Shown in the status bar when `show_section` is enabled.
    pub section: String,
    /// Subtitle line, parsed from `### Subtitle` headings on the slide.
    pub subtitle: String,
    /// Bullet point items parsed from Markdown unordered/ordered lists.
    /// Each [`Bullet`] carries its text and nesting depth.
    pub bullets: Vec<Bullet>,
    /// Fenced code blocks found on this slide. Multiple blocks are allowed;
    /// the user can cycle through executable ones with Ctrl+E.
    pub code_blocks: Vec<CodeBlock>,
    /// An optional image reference (only one image per slide is supported).
    /// Set when the slide contains a Markdown `![alt](path)` image.
    pub image: Option<SlideImage>,
    /// When `true`, the title is rendered as large ASCII art using FIGlet fonts.
    /// Set via `<!-- ascii_title -->` directive.
    pub ascii_title: bool,
    /// Speaker notes for this slide, parsed from a trailing `Notes:` block.
    /// These are hidden during presentation and only visible in presenter mode.
    pub notes: String,
    /// Suggested timing in minutes for this slide (for pacing). Parsed from
    /// `<!-- timing: 2.5 -->` directives.
    /// Parsed and tested; renderer support pending.
    #[allow(dead_code)]
    pub timing_minutes: f64,
    /// Multi-column layout, present when the slide uses a `<!-- columns: ... -->` directive.
    /// Contains the width ratios and per-column content.
    pub columns: Option<ColumnLayout>,
    /// Markdown tables found on this slide. Each [`Table`] has headers, alignments,
    /// and data rows.
    pub tables: Vec<Table>,
    /// Block quote elements (lines prefixed with `>` in Markdown).
    pub block_quotes: Vec<BlockQuote>,
    /// Terminal font size offset (-3 to 7). Set via `<!-- font_size: N -->`.
    /// Negative values shrink below the base font size. Only effective in Kitty terminal.
    pub font_size: Option<i8>,
    /// Per-slide text scale percentage (e.g., 80 = 80%). Set via `<!-- text_scale: N -->`.
    /// Controls the OSC 66 text scaling for body content.
    pub text_scale: Option<u8>,
    /// Per-slide title scale percentage. Set via `<!-- title_scale: N -->`.
    /// Controls the OSC 66 text scaling for the slide title.
    /// Parsed and tested; renderer support pending.
    #[allow(dead_code)]
    pub title_scale: Option<u8>,
    /// Custom footer text displayed at the bottom of the slide.
    /// Set via `<!-- footer: Your text here -->`.
    pub footer: Option<String>,
    /// Alignment of the footer bar. Set via `<!-- footer_align: left|center|right -->`.
    /// Defaults to `FooterAlign::Left`.
    pub footer_align: FooterAlign,
    /// Content alignment override for this specific slide.
    /// Set via `<!-- align: center|vcenter|hcenter|top -->`.
    pub alignment: Option<SlideAlignment>,
    /// Title decoration style (e.g., `"underline"`, `"box"`, `"banner"`).
    /// Set via `<!-- title_decoration: style -->` or inherited from the theme.
    pub title_decoration: Option<String>,
    /// Slide transition animation played when navigating *to* this slide.
    /// Set via `<!-- transition: fade|slide-left|dissolve -->`.
    pub transition: Option<TransitionType>,
    /// One-time entrance animation played when the slide first appears.
    /// Set via `<!-- entrance: typewriter|fade_in|slide_down -->`.
    pub entrance_animation: Option<EntranceAnimation>,
    /// Continuously running loop animations applied to this slide.
    /// Each entry is a `(LoopAnimation, Option<target>)` tuple. The optional
    /// target string restricts the animation to specific content types
    /// (e.g., `"figlet"`, `"image"`). Set via `<!-- loop: sparkle(figlet) -->`.
    pub loop_animations: Vec<(LoopAnimation, Option<String>)>,
    /// When `Some(true)`, the slide is rendered in fullscreen mode (no status bar).
    /// Set via `<!-- fullscreen: true -->`.
    pub fullscreen: Option<bool>,
    /// Whether the section name is shown in the status bar for this slide.
    /// Set via `<!-- show_section: true|false -->`.
    pub show_section: Option<bool>,
    /// Per-language code preambles that are prepended to executable code blocks.
    /// Keyed by language name (e.g., `"rust"`, `"python"`).
    /// Set via `<!-- preamble_rust: ... -->` directives.
    pub code_preambles: HashMap<String, String>,
    /// Mermaid diagram blocks parsed from ` ```mermaid ` fenced code blocks.
    /// Each block's `source` is sent to the `mmdc` CLI for rendering.
    pub mermaid_blocks: Vec<MermaidBlock>,
    /// Native diagram blocks parsed from ` ```diagram ` fenced code blocks.
    /// These are rendered as ASCII box diagrams using the built-in diagram engine.
    pub diagram_blocks: Vec<DiagramBlock>,
    /// Per-slide theme override slug. When set, the renderer switches to this
    /// theme for the duration of this slide. Set via `<!-- theme: slug -->`.
    pub theme_override: Option<String>,
    /// Controls how font size changes are applied. When set to `"none"`, font
    /// size changes take effect instantly without an animated transition.
    /// Set via `<!-- font_transition: none -->`.
    pub font_transition: Option<String>,
    /// Plain text lines that appear after the bullet list on a slide.
    /// Captured by the parser when a non-directive, non-heading text line
    /// follows an existing bullet list (outside of a column context).
    pub trailing_text: Vec<String>,
}

/// Describes a multi-column layout for a slide.
///
/// Columns are defined with `<!-- columns: 1:2:1 -->` where the numbers
/// represent relative width ratios. The `contents` vector holds one
/// [`ColumnContent`] per column.
#[derive(Debug, Clone)]
pub struct ColumnLayout {
    /// Relative width ratios for each column (e.g., `[1, 2, 1]` means the
    /// middle column is twice as wide as the outer ones).
    pub ratios: Vec<u8>,
    /// Content for each column, in left-to-right order. The length of this
    /// `Vec` (Rust's growable array type) matches the length of `ratios`.
    pub contents: Vec<ColumnContent>,
    /// Whether to show the visible `│` separator between columns.
    /// Defaults to `true`. Set to `false` via `<!-- column_separator: none -->`.
    pub separator: bool,
    /// OSC 66 text scale factor for non-image columns (2-7). When set, bullet
    /// and FIGlet text in text columns is scaled up while image columns remain
    /// at the base font size for detail. Set via `<!-- column_text_scale: N -->`.
    pub text_scale: Option<u8>,
}

/// An image reference within a column of a multi-column layout.
///
/// Column images are always rendered as ASCII art because the column merge
/// logic operates on `StyledLine` virtual buffers, which protocol-based
/// rendering (Kitty, iTerm2, Sixel) cannot participate in.
#[derive(Debug, Clone)]
pub struct ColumnImage {
    /// Filesystem path to the image file (resolved to absolute during parsing).
    pub path: String,
    /// Rendering mode override (e.g., `"ascii"`, `"kitty"`). Column images
    /// are always rendered as ASCII regardless of this value, but it is
    /// preserved for potential future use.
    pub render_mode: Option<String>,
    /// Image scale as a percentage (1-100). Controls how much of the column
    /// width the image occupies.
    pub scale: Option<u8>,
    /// Hex color override for ASCII art rendering (e.g., `"#FF0000"`).
    pub color: Option<String>,
}

/// Content within a single column of a multi-column layout.
///
/// Each column can independently contain bullet points, code blocks,
/// and an optional image rendered as ASCII art.
#[derive(Debug, Clone)]
pub struct ColumnContent {
    /// Bullet points appearing in this column.
    pub bullets: Vec<Bullet>,
    /// Code blocks appearing in this column.
    pub code_blocks: Vec<CodeBlock>,
    /// Optional image to render as ASCII art within this column.
    pub image: Option<ColumnImage>,
    /// Plain text lines (e.g., column headers) that appear before bullets
    /// within this column.
    pub text_lines: Vec<String>,
}

/// A single bullet point item from a Markdown list.
///
/// Parsed from lines starting with `-`, `*`, `+`, or numbered prefixes like `1.`.
#[derive(Debug, Clone)]
pub struct Bullet {
    /// The text content of the bullet, with any inline Markdown formatting preserved.
    pub text: String,
    /// Nesting depth (0 = top-level, 1 = first indent, etc.).
    /// Determined by the leading whitespace in the Markdown source.
    pub depth: usize,
}

/// A fenced code block parsed from Markdown.
///
/// Code blocks are delimited by triple backticks (` ``` `) in the source.
/// They can optionally be marked as executable with `+exec` or `+pty` annotations.
#[derive(Debug, Clone)]
pub struct CodeBlock {
    /// The language identifier from the opening fence (e.g., `"rust"`, `"python"`).
    /// Used for syntax highlighting and to determine the compiler/interpreter for execution.
    pub language: String,
    /// The raw source code content between the opening and closing fences.
    pub code: String,
    /// An optional label displayed above the code block.
    /// Parsed from a trailing comment or annotation on the fence line.
    pub label: String,
    /// If set, this code block is executable. `Exec` runs the code and captures
    /// stdout; `Pty` runs it in a pseudo-terminal for interactive output.
    /// Triggered via `+exec` or `+pty` annotations on the fence line.
    pub exec_mode: Option<ExecMode>,
}

/// Execution mode for a code block.
///
/// Determines how the code is run when the user triggers execution (Ctrl+E).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecMode {
    /// Standard execution: compile (if needed) and capture stdout/stderr.
    Exec,
    /// PTY execution: run in a pseudo-terminal, preserving ANSI escape codes
    /// and interactive formatting in the output.
    Pty,
}

/// An image reference on a slide, parsed from `![alt](path)` Markdown syntax.
///
/// The renderer uses the `render_mode` and terminal capabilities to decide
/// which image protocol to use (Kitty, iTerm inline, Sixel, or ASCII fallback).
#[derive(Debug, Clone)]
pub struct SlideImage {
    /// Filesystem path to the image file (absolute or relative to the presentation file).
    pub path: PathBuf,
    /// Alt text from the Markdown image syntax. Displayed when the image cannot be rendered.
    pub alt_text: String,
    /// Where the image appears relative to the slide content.
    pub position: ImagePosition,
    /// Which rendering protocol to use. `Auto` lets the renderer detect the best option.
    pub render_mode: ImageRenderMode,
    /// Image scale as a percentage (e.g., 100 = original size, 50 = half size).
    /// Adjusted via `>` and `<` keys during presentation.
    pub scale: u8,
    /// Hex color override for ASCII art rendering (e.g., `"#FF0000"`).
    /// Set via `<!-- image_color: #hex -->`. Empty string means no override.
    pub color_override: String,
}

/// Position of an image relative to the slide's text content.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ImagePosition {
    /// Image appears below the text content (the default).
    #[default]
    Below,
    /// Image appears to the left of the text, with content wrapping to the right.
    Left,
    /// Image appears to the right of the text, with content wrapping to the left.
    Right,
}

/// Rendering protocol used to display an image in the terminal.
///
/// Different terminal emulators support different image protocols. `Auto` is
/// recommended — it lets the renderer probe the terminal and pick the best
/// available protocol.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ImageRenderMode {
    /// Automatically detect the best protocol for the current terminal.
    #[default]
    Auto,
    /// Use the Kitty graphics protocol (best quality, Kitty terminal only).
    Kitty,
    /// Use the iTerm2 inline image protocol.
    Iterm,
    /// Use the Sixel graphics protocol (wide terminal support).
    Sixel,
    /// Render the image as colored ASCII art (works in all terminals).
    Ascii,
}

/// A Markdown table with headers, column alignments, and data rows.
///
/// Parsed from pipe-delimited Markdown table syntax:
/// ```text
/// | Header 1 | Header 2 |
/// |----------|----------|
/// | cell     | cell     |
/// ```
#[derive(Debug, Clone)]
pub struct Table {
    /// Column header labels from the first row of the table.
    pub headers: Vec<String>,
    /// Per-column alignment (left, center, or right), determined by the
    /// colon placement in the separator row (e.g., `:---:` for center).
    pub alignments: Vec<TableAlign>,
    /// Data rows. Each inner `Vec<String>` contains one cell per column.
    pub rows: Vec<Vec<String>>,
}

/// Horizontal alignment for a table column.
///
/// Determined by the colon syntax in the Markdown table separator row:
/// - `---` or `:---` = Left
/// - `:---:` = Center
/// - `---:` = Right
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableAlign {
    /// Left-aligned column.
    Left,
    /// Center-aligned column.
    Center,
    /// Right-aligned column.
    Right,
}

/// A block quote element, parsed from lines starting with `>` in Markdown.
///
/// Block quotes are rendered with a vertical accent-colored bar on the left
/// and slightly indented text.
#[derive(Debug, Clone)]
pub struct BlockQuote {
    /// The text lines within the block quote, with the leading `> ` stripped.
    pub lines: Vec<String>,
}

/// A Mermaid diagram block, parsed from ` ```mermaid ` fenced code blocks.
///
/// At render time the `source` is passed to the external `mmdc` (Mermaid CLI)
/// tool which produces an SVG or PNG image that is then displayed using the
/// terminal's image protocol.
#[derive(Debug, Clone)]
pub struct MermaidBlock {
    /// The raw Mermaid diagram definition (e.g., `graph TD; A-->B`).
    pub source: String,
}

/// Visual style for a native diagram block.
///
/// Controls how diagram nodes and edges are drawn in ASCII art.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DiagramStyle {
    /// Nodes drawn with box borders: `+-----+` (the default).
    #[default]
    Box,
    /// Nodes drawn with square brackets: `[Node]`.
    Bracket,
    /// Layout flows top-to-bottom instead of left-to-right.
    Vertical,
}

/// A native ASCII diagram block, parsed from ` ```diagram ` fenced code blocks.
///
/// Unlike Mermaid blocks, these are rendered entirely by the built-in diagram
/// engine without requiring any external CLI tool.
#[derive(Debug, Clone)]
pub struct DiagramBlock {
    /// The raw diagram definition text.
    pub source: String,
    /// The visual style to use when rendering this diagram.
    pub style: DiagramStyle,
}

/// Provides sensible defaults for a [`Slide`] so that the parser only needs to
/// set the fields that are explicitly present in the Markdown source.
///
/// All `String` fields default to empty, all `Vec` fields to empty vectors,
/// and all `Option` fields to `None`. Numeric fields default to zero.
impl Default for Slide {
    fn default() -> Self {
        Self {
            number: 0,
            title: String::new(),
            section: String::new(),
            subtitle: String::new(),
            bullets: Vec::new(),
            code_blocks: Vec::new(),
            image: None,
            ascii_title: false,
            notes: String::new(),
            timing_minutes: 0.0,
            columns: None,
            tables: Vec::new(),
            block_quotes: Vec::new(),
            font_size: None,
            text_scale: None,
            title_scale: None,
            footer: None,
            footer_align: FooterAlign::Left,
            alignment: None,
            title_decoration: None,
            transition: None,
            entrance_animation: None,
            loop_animations: Vec::new(),
            fullscreen: None,
            show_section: None,
            code_preambles: HashMap::new(),
            mermaid_blocks: Vec::new(),
            diagram_blocks: Vec::new(),
            theme_override: None,
            font_transition: None,
            trailing_text: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Slide::default ---

    #[test]
    fn slide_default_number_is_zero() {
        assert_eq!(Slide::default().number, 0);
    }

    #[test]
    fn slide_default_strings_are_empty() {
        let s = Slide::default();
        assert!(s.title.is_empty());
        assert!(s.section.is_empty());
        assert!(s.subtitle.is_empty());
        assert!(s.notes.is_empty());
    }

    #[test]
    fn slide_default_vecs_are_empty() {
        let s = Slide::default();
        assert!(s.bullets.is_empty());
        assert!(s.code_blocks.is_empty());
        assert!(s.tables.is_empty());
        assert!(s.block_quotes.is_empty());
        assert!(s.loop_animations.is_empty());
        assert!(s.mermaid_blocks.is_empty());
        assert!(s.diagram_blocks.is_empty());
        assert!(s.code_preambles.is_empty());
    }

    #[test]
    fn slide_default_options_are_none() {
        let s = Slide::default();
        assert!(s.image.is_none());
        assert!(s.columns.is_none());
        assert!(s.font_size.is_none());
        assert!(s.text_scale.is_none());
        assert!(s.title_scale.is_none());
        assert!(s.footer.is_none());
        assert!(s.alignment.is_none());
        assert!(s.title_decoration.is_none());
        assert!(s.transition.is_none());
        assert!(s.entrance_animation.is_none());
        assert!(s.fullscreen.is_none());
        assert!(s.show_section.is_none());
        assert!(s.theme_override.is_none());
        assert!(s.font_transition.is_none());
    }

    #[test]
    fn slide_default_ascii_title_is_false() {
        assert!(!Slide::default().ascii_title);
    }

    #[test]
    fn slide_default_timing_is_zero() {
        assert_eq!(Slide::default().timing_minutes, 0.0);
    }

    #[test]
    fn slide_default_footer_align_is_left() {
        assert_eq!(Slide::default().footer_align, FooterAlign::Left);
    }

    // --- FooterAlign ---

    #[test]
    fn footer_align_default_is_left() {
        assert_eq!(FooterAlign::default(), FooterAlign::Left);
    }

    #[test]
    fn footer_align_variants_are_distinct() {
        assert_ne!(FooterAlign::Left, FooterAlign::Center);
        assert_ne!(FooterAlign::Center, FooterAlign::Right);
        assert_ne!(FooterAlign::Left, FooterAlign::Right);
    }

    // --- SlideAlignment ---

    #[test]
    fn slide_alignment_default_is_top() {
        assert_eq!(SlideAlignment::default(), SlideAlignment::Top);
    }

    #[test]
    fn slide_alignment_variants_are_distinct() {
        assert_ne!(SlideAlignment::Top, SlideAlignment::Center);
        assert_ne!(SlideAlignment::VCenter, SlideAlignment::HCenter);
    }

    // --- ImagePosition ---

    #[test]
    fn image_position_default_is_below() {
        assert_eq!(ImagePosition::default(), ImagePosition::Below);
    }

    #[test]
    fn image_position_variants_are_distinct() {
        assert_ne!(ImagePosition::Below, ImagePosition::Left);
        assert_ne!(ImagePosition::Left, ImagePosition::Right);
    }

    // --- ImageRenderMode ---

    #[test]
    fn image_render_mode_default_is_auto() {
        assert_eq!(ImageRenderMode::default(), ImageRenderMode::Auto);
    }

    #[test]
    fn image_render_mode_variants_are_distinct() {
        assert_ne!(ImageRenderMode::Auto, ImageRenderMode::Kitty);
        assert_ne!(ImageRenderMode::Iterm, ImageRenderMode::Sixel);
        assert_ne!(ImageRenderMode::Sixel, ImageRenderMode::Ascii);
    }

    // --- ColumnContent ---

    #[test]
    fn column_content_with_image_field() {
        let col = ColumnContent {
            bullets: Vec::new(),
            code_blocks: Vec::new(),
            image: Some(ColumnImage {
                path: "/some/path.png".to_string(),
                render_mode: None,
                scale: Some(80),
                color: None,
            }),
            text_lines: Vec::new(),
        };
        assert!(col.image.is_some());
        let img = col.image.unwrap();
        assert_eq!(img.path, "/some/path.png");
        assert_eq!(img.scale, Some(80));
    }

    #[test]
    fn column_content_no_image() {
        let col = ColumnContent {
            bullets: vec![Bullet { text: "item".to_string(), depth: 0 }],
            code_blocks: Vec::new(),
            image: None,
            text_lines: Vec::new(),
        };
        assert!(col.image.is_none());
        assert_eq!(col.bullets.len(), 1);
    }

    // --- SlideImage position variants ---

    #[test]
    fn slide_image_with_left_position() {
        let img = SlideImage {
            path: std::path::PathBuf::from("/img.png"),
            alt_text: "alt".to_string(),
            position: ImagePosition::Left,
            render_mode: ImageRenderMode::Auto,
            scale: 100,
            color_override: String::new(),
        };
        assert_eq!(img.position, ImagePosition::Left);
    }

    #[test]
    fn slide_image_with_right_position() {
        let img = SlideImage {
            path: std::path::PathBuf::from("/img.png"),
            alt_text: String::new(),
            position: ImagePosition::Right,
            render_mode: ImageRenderMode::Ascii,
            scale: 50,
            color_override: "#FF0000".to_string(),
        };
        assert_eq!(img.position, ImagePosition::Right);
        assert_eq!(img.render_mode, ImageRenderMode::Ascii);
        assert_eq!(img.scale, 50);
    }

    // --- DiagramStyle ---

    #[test]
    fn diagram_style_default_is_box() {
        assert_eq!(DiagramStyle::default(), DiagramStyle::Box);
    }

    // --- ExecMode ---

    #[test]
    fn exec_mode_variants_are_distinct() {
        assert_ne!(ExecMode::Exec, ExecMode::Pty);
    }

    // --- Bullet ---

    #[test]
    fn bullet_stores_text_and_depth() {
        let b = Bullet { text: "hello".to_string(), depth: 2 };
        assert_eq!(b.text, "hello");
        assert_eq!(b.depth, 2);
    }

    // --- Slide clone ---

    #[test]
    fn slide_clone_is_independent() {
        let original = Slide { title: "Original".to_string(), ..Default::default() };
        let mut cloned = original.clone();
        cloned.title = "Cloned".to_string();
        assert_eq!(original.title, "Original");
        assert_eq!(cloned.title, "Cloned");
    }
}
