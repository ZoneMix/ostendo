mod markdown;
mod presentation;
mod render;
mod remote;
mod terminal;
mod theme;
mod code;
mod third_party;
mod image_util;
mod export;
mod watch;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "ostendo", about = "Terminal-based presentation tool", version)]
pub struct Cli {
    /// Path to the markdown presentation file
    pub file: Option<PathBuf>,

    /// Theme slug to use
    #[arg(short, long, default_value = "terminal_green")]
    pub theme: String,

    /// Start at specific slide number
    #[arg(short, long, default_value_t = 1)]
    pub slide: usize,

    /// Image render mode (auto, kitty, iterm, sixel, ascii)
    #[arg(long, default_value = "auto")]
    pub image_mode: String,

    /// List available themes and exit
    #[arg(long)]
    pub list_themes: bool,

    /// Enable WebSocket remote control
    #[arg(long)]
    pub remote: bool,

    /// Remote control port
    #[arg(long, default_value_t = 8765)]
    pub remote_port: u16,

    /// Validate presentation without running TUI
    #[arg(long)]
    pub validate: bool,

    /// Print slide count and exit
    #[arg(long)]
    pub count: bool,

    /// Export slide titles to stdout (one per line)
    #[arg(long)]
    pub export_titles: bool,

    /// Detect and print image protocol, then exit
    #[arg(long)]
    pub detect_protocol: bool,

    /// Override content scale (50-200, default 80)
    #[arg(long)]
    pub scale: Option<u8>,

    /// Start with fullscreen mode (no status bar)
    #[arg(long)]
    pub fullscreen: bool,

    /// Start with timer running
    #[arg(long)]
    pub timer: bool,

    /// Export presentation to HTML or PDF
    #[arg(long, value_name = "FORMAT")]
    pub export: Option<String>,

    /// Output path for export (default: presentation name with new extension)
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let registry = theme::ThemeRegistry::load();

    if cli.list_themes {
        println!("Available themes:");
        for name in registry.list() {
            println!("  {}", name);
        }
        return Ok(());
    }

    if cli.detect_protocol {
        let proto = terminal::protocols::detect_protocol();
        println!("{:?}", proto);
        return Ok(());
    }

    let file = cli.file.unwrap_or_else(|| {
        eprintln!("Error: no presentation file specified");
        std::process::exit(1);
    });

    let theme = registry
        .get(&cli.theme)
        .unwrap_or_else(|| registry.get("terminal_green").expect("default theme missing"));

    let source = std::fs::read_to_string(&file)?;
    let (meta, slides) = markdown::parse_presentation(&source, file.parent())?;

    if slides.is_empty() {
        anyhow::bail!("No slides found in {:?}", file);
    }

    if cli.count {
        println!("{}", slides.len());
        return Ok(());
    }

    if cli.export_titles {
        for slide in &slides {
            println!("{}", if slide.title.is_empty() { "(untitled)" } else { &slide.title });
        }
        return Ok(());
    }

    if cli.validate {
        println!("Presentation: {:?}", file);
        println!("Slides: {}", slides.len());
        println!("Theme: {}", cli.theme);
        let mut issues = Vec::new();
        for slide in &slides {
            // Check image paths
            if let Some(ref img) = slide.image {
                if !img.path.exists() {
                    issues.push(format!(
                        "Slide {}: image not found: {:?}", slide.number, img.path
                    ));
                }
            }
            // Check for empty slides
            if slide.title.is_empty() && slide.bullets.is_empty()
                && slide.code_blocks.is_empty() && slide.tables.is_empty()
            {
                issues.push(format!("Slide {}: appears empty (no title, bullets, code, or tables)", slide.number));
            }
        }
        if issues.is_empty() {
            println!("Status: OK - no issues found");
        } else {
            println!("Issues found: {}", issues.len());
            for issue in &issues {
                println!("  - {}", issue);
            }
        }
        return Ok(());
    }

    // Export mode
    if let Some(ref format) = cli.export {
        let output_path = cli.output.clone().unwrap_or_else(|| {
            let stem = file.file_stem().unwrap_or_default().to_string_lossy();
            match format.as_str() {
                "pdf" => PathBuf::from(format!("{}.pdf", stem)),
                _ => PathBuf::from(format!("{}.html", stem)),
            }
        });
        match format.as_str() {
            "html" => {
                export::html::export_html(&slides, &theme, &output_path)?;
                println!("Exported HTML to {:?}", output_path);
            }
            "pdf" => {
                // First export to HTML, then convert to PDF
                let html_path = std::env::temp_dir().join("ostendo_export.html");
                export::html::export_html(&slides, &theme, &html_path)?;
                export::pdf::export_pdf(&html_path, &output_path)?;
                let _ = std::fs::remove_file(&html_path);
                println!("Exported PDF to {:?}", output_path);
            }
            _ => {
                eprintln!("Unknown export format: {}. Use 'html' or 'pdf'.", format);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Start remote control server if requested
    let remote_channels = if cli.remote {
        eprintln!("Remote control: http://127.0.0.1:{}", cli.remote_port);
        let (rx, tx) = remote::server::RemoteServer::start(cli.remote_port);
        Some((rx, tx))
    } else {
        None
    };

    let mut presenter = render::Presenter::new(
        slides, meta, theme, cli.slide.saturating_sub(1), &file, &cli.image_mode, remote_channels,
    );
    if cli.fullscreen {
        presenter.set_fullscreen(true);
    }
    if cli.timer {
        presenter.start_timer();
    }
    if let Some(scale) = cli.scale {
        presenter.set_default_scale(scale.clamp(50, 200));
    }
    presenter.run()
}
