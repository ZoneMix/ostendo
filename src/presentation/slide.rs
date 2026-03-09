use std::path::PathBuf;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Slide {
    pub number: usize,
    pub title: String,
    pub section: String,
    pub subtitle: String,
    pub bullets: Vec<Bullet>,
    pub code_blocks: Vec<CodeBlock>,
    pub image: Option<SlideImage>,
    pub ascii_title: bool,
    pub notes: String,
    pub timing_minutes: f64,
    pub columns: Option<ColumnLayout>,
    pub tables: Vec<Table>,
    pub block_quotes: Vec<BlockQuote>,
    pub font_size: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct ColumnLayout {
    pub ratios: Vec<u8>,
    pub contents: Vec<ColumnContent>,
}

#[derive(Debug, Clone)]
pub struct ColumnContent {
    pub bullets: Vec<Bullet>,
    pub code_blocks: Vec<CodeBlock>,
}

#[derive(Debug, Clone)]
pub struct Bullet {
    pub text: String,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
    pub label: String,
    pub exec_mode: Option<ExecMode>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecMode {
    Exec,
    Pty,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SlideImage {
    pub path: PathBuf,
    pub alt_text: String,
    pub position: ImagePosition,
    pub render_mode: ImageRenderMode,
    pub scale: u8,
    pub color_override: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ImagePosition {
    #[default]
    Below,
    Left,
    Right,
}

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
    pub lines: Vec<String>,
}

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
        }
    }
}
