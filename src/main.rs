//! Ostendo: present markdown slides in the terminal.

mod code;
mod diagram;
mod export;
mod image_util;
mod markdown;
mod math;
mod presentation;
mod remote;
mod render;
mod terminal;
mod theme;
mod watch;

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};

use presentation::{PresentationMeta, Slide};
use render::text::printable;
use terminal::protocols::ImageProtocol;
use theme::{Theme, ThemeRegistry};

const DEFAULT_THEME: &str = "terminal_green";

#[derive(Parser)]
#[command(
    name = "ostendo",
    version,
    about = "Present markdown slides in your terminal"
)]
struct Cli {
    /// Markdown presentation to show
    #[arg(required_unless_present_any = ["list_themes", "detect_protocol"])]
    file: Option<PathBuf>,

    /// Theme slug (overrides the front matter; see --list-themes)
    #[arg(short, long, value_name = "SLUG")]
    theme: Option<String>,

    /// Slide to start on (default: where you left off)
    #[arg(short, long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
    slide: Option<u32>,

    /// Image protocol
    #[arg(long, value_enum, default_value_t = ImageMode::Auto)]
    image_mode: ImageMode,

    /// Content width as a percentage of the terminal
    #[arg(long, value_name = "PERCENT", default_value_t = 80, value_parser = clap::value_parser!(u8).range(40..=100))]
    scale: u8,

    /// Hide the status bar
    #[arg(long)]
    fullscreen: bool,

    /// Start the timer immediately
    #[arg(long)]
    timer: bool,

    /// Never run code blocks (hides the Ctrl+E hints)
    #[arg(long)]
    no_exec: bool,

    /// Serve a browser remote control on 127.0.0.1
    #[arg(long)]
    remote: bool,

    /// Port for --remote
    #[arg(long, value_name = "PORT", default_value_t = 8765, requires = "remote")]
    remote_port: u16,

    /// Require this token for remote connections ([A-Za-z0-9._~-])
    #[arg(long, value_name = "TOKEN", requires = "remote")]
    remote_token: Option<String>,

    /// Let the remote run code blocks (needs --remote-token: without one, any
    /// local process could run the deck's code as you)
    #[arg(long, requires = "remote_token")]
    remote_exec: bool,

    /// Serve a voting page for poll slides to everyone on the network
    #[arg(long)]
    audience: bool,

    /// Port for --audience
    #[arg(
        long,
        value_name = "PORT",
        default_value_t = 8766,
        requires = "audience"
    )]
    audience_port: u16,

    /// Check the presentation and exit (non-zero status on problems)
    #[arg(long)]
    validate: bool,

    /// With --validate: also report slides that need scrolling in a terminal
    /// this size (e.g. 100x30)
    #[arg(long, value_name = "COLSxROWS", value_parser = parse_size, requires = "validate")]
    size: Option<(u16, u16)>,

    /// Record the talk as an asciicast file (replay with asciinema)
    #[arg(long, value_name = "FILE")]
    record: Option<PathBuf>,

    /// Write the presentation to a file and exit
    #[arg(long, value_enum, value_name = "FORMAT")]
    export: Option<ExportFormat>,

    /// Output path for --export (default: next to the input)
    #[arg(short, long, value_name = "PATH", requires = "export")]
    output: Option<PathBuf>,

    /// List themes and exit
    #[arg(long)]
    list_themes: bool,

    /// Print the slide count and exit
    #[arg(long)]
    count: bool,

    /// Print slide titles, one per line, and exit
    #[arg(long)]
    export_titles: bool,

    /// Print time per slide from past runs (timer on, a minute or longer) and exit
    #[arg(long)]
    report: bool,

    /// Print the image protocol this terminal supports and exit
    #[arg(long)]
    detect_protocol: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum ImageMode {
    Auto,
    Kitty,
    Iterm,
    Sixel,
    /// Colored half blocks (any true-color terminal)
    Blocks,
    /// Character-ramp art
    Ascii,
}

#[derive(Clone, Copy, ValueEnum)]
enum ExportFormat {
    Html,
    Pdf,
    Pptx,
}

fn main() -> std::process::ExitCode {
    match run(Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        // `ostendo --list-themes | head` closing the pipe early is not an error.
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe) =>
        {
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let registry = ThemeRegistry::load();

    let mut stdout = std::io::stdout();
    if cli.list_themes {
        return list_themes(&registry, &mut stdout);
    }
    if cli.detect_protocol {
        println!("{:?}", terminal::protocols::detect_protocol());
        return Ok(());
    }

    let file = cli.file.clone().context("no presentation file given")?;
    let source = std::fs::read_to_string(&file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    let (meta, slides) = markdown::parse_presentation(&source, file.parent())?;
    if slides.is_empty() {
        bail!("{} has no slides", file.display());
    }

    if cli.count {
        writeln!(stdout, "{}", slides.len())?;
        return Ok(());
    }
    if cli.export_titles {
        for slide in &slides {
            writeln!(
                stdout,
                "{}",
                if slide.title.is_empty() {
                    "(untitled)".into()
                } else {
                    printable(&slide.title)
                }
            )?;
        }
        return Ok(());
    }
    if cli.report {
        let titles: Vec<String> = slides
            .iter()
            .map(|s| printable(&s.title).into_owned())
            .collect();
        let runs = presentation::rehearsal::load(&file);
        write!(
            stdout,
            "{}",
            presentation::rehearsal::report(&titles, &runs, meta.duration)
        )?;
        return Ok(());
    }
    if cli.validate {
        return validate(
            &file,
            &meta,
            &slides,
            &registry,
            cli.theme.as_deref(),
            cli.size,
        );
    }

    let requested = cli.theme.clone().or_else(|| meta.theme.clone());
    let theme = resolve_theme(&registry, requested.as_deref())?;

    if let Some(format) = cli.export {
        let ext = match format {
            ExportFormat::Html => "html",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Pptx => "pptx",
        };
        let output = cli
            .output
            .clone()
            .unwrap_or_else(|| file.with_extension(ext));
        let title = if meta.title.is_empty() {
            &slides[0].title
        } else {
            &meta.title
        };
        let mut theme = theme;
        let bg = theme::colors::hex_to_color(&theme.colors.background);
        let readable = theme::colors::hex_to_color(&meta.accent)
            .zip(bg)
            .is_some_and(|(accent, bg)| theme::colors::accent_is_readable(accent, bg));
        if readable {
            theme.colors.accent = meta.accent.clone();
        }
        match format {
            ExportFormat::Html => export::html::export_html(&slides, &theme, title, &output)?,
            ExportFormat::Pdf => export::pdf::export_pdf(&slides, &theme, title, &output)?,
            ExportFormat::Pptx => {
                export::pptx::export_pptx(&slides, &meta, &theme, title, &output)?
            }
        }
        println!("Wrote {}", output.display());
        return Ok(());
    }

    let (commands, remote_rx) = std::sync::mpsc::channel();
    let states = if cli.remote {
        let states =
            remote::server::start(cli.remote_port, cli.remote_token.clone(), commands.clone())?;
        let fragment = cli
            .remote_token
            .as_ref()
            .map(|t| format!("/#token={t}"))
            .unwrap_or_default();
        eprintln!(
            "Remote control: http://127.0.0.1:{}{fragment}",
            cli.remote_port
        );
        Some(states)
    } else {
        None
    };
    let audience = if cli.audience {
        let (polls, url) = remote::audience::start(cli.audience_port, commands)?;
        eprintln!("Audience voting: {url}");
        Some((polls, url))
    } else {
        None
    };
    let remote = (states.is_some() || audience.is_some()).then(|| remote::Links {
        commands: remote_rx,
        states,
        audience,
    });

    render::Presenter::new(render::PresenterConfig {
        slides,
        meta,
        theme,
        theme_explicit: requested.is_some(),
        start: cli.slide.map(|n| n as usize - 1),
        presentation_path: file,
        image_protocol: match cli.image_mode {
            // A recording shows images only as text cells.
            ImageMode::Auto if cli.record.is_some() => Some(ImageProtocol::Blocks),
            ImageMode::Auto => None,
            ImageMode::Kitty => Some(ImageProtocol::Kitty),
            ImageMode::Iterm => Some(ImageProtocol::Iterm2),
            ImageMode::Sixel => Some(ImageProtocol::Sixel),
            ImageMode::Blocks => Some(ImageProtocol::Blocks),
            ImageMode::Ascii => Some(ImageProtocol::Ascii),
        },
        remote,
        allow_exec: !cli.no_exec,
        allow_remote_exec: cli.remote_exec,
        fullscreen: cli.fullscreen,
        timer: cli.timer,
        scale: cli.scale,
        record: cli.record,
    })
    .run()
}

fn parse_size(value: &str) -> Result<(u16, u16), String> {
    let (cols, rows) = value
        .split_once(['x', 'X'])
        .ok_or("expected COLSxROWS, like 100x30")?;
    let cols: u16 = cols
        .trim()
        .parse()
        .map_err(|_| "columns must be a number")?;
    let rows: u16 = rows.trim().parse().map_err(|_| "rows must be a number")?;
    if cols < 20 || rows < 5 {
        return Err("the smallest size is 20x5".into());
    }
    Ok((cols, rows))
}

fn resolve_theme(registry: &ThemeRegistry, slug: Option<&str>) -> Result<Theme> {
    let slug = slug.unwrap_or(DEFAULT_THEME);
    registry.get(slug).with_context(|| {
        format!(
            "{}; run `ostendo --list-themes`",
            unknown_theme(registry, slug)
        )
    })
}

fn unknown_theme(registry: &ThemeRegistry, slug: &str) -> String {
    let slugs = registry.list();
    let slugs: Vec<&str> = slugs.iter().map(String::as_str).collect();
    markdown::parser::unknown("theme", slug, &slugs)
}

fn list_themes(registry: &ThemeRegistry, out: &mut impl Write) -> Result<()> {
    let color = std::io::stdout().is_terminal();
    for slug in registry.list() {
        let Some(theme) = registry.get(&slug) else {
            continue;
        };
        let swatch = if color {
            let rgb =
                |hex: &str| theme::colors::hex_to_color(hex).and_then(theme::colors::color_to_rgb);
            match (rgb(&theme.colors.background), rgb(&theme.colors.accent)) {
                (Some((br, bg, bb)), Some((ar, ag, ab))) => {
                    format!("\x1b[48;2;{br};{bg};{bb}m\x1b[38;2;{ar};{ag};{ab}m ■■ \x1b[0m ")
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };
        writeln!(out, "{swatch}{slug:<24} {}", theme.name)?;
    }
    Ok(())
}

/// Reports problems that would show up during the talk.
fn validate(
    file: &Path,
    meta: &PresentationMeta,
    slides: &[Slide],
    registry: &ThemeRegistry,
    cli_theme: Option<&str>,
    size: Option<(u16, u16)>,
) -> Result<()> {
    // (file line, message); a deck-wide problem points at line 1.
    let mut issues: Vec<(usize, String)> = meta
        .warnings
        .iter()
        .map(|w| (w.line, w.message.clone()))
        .collect();
    let theme = cli_theme.or(meta.theme.as_deref()).unwrap_or(DEFAULT_THEME);
    if registry.get(theme).is_none() {
        issues.push((1, unknown_theme(registry, theme)));
    }
    for slide in slides {
        let n = slide.number;
        let mut issue =
            |message: String| issues.push((slide.line, format!("slide {n}: {message}")));
        let images = slide.image.iter().map(|i| i.path.clone()).chain(
            slide
                .columns
                .iter()
                .flat_map(|c| &c.contents)
                .filter_map(|c| c.image.as_ref().map(|i| PathBuf::from(&i.path))),
        );
        for path in images {
            if !path.exists() {
                issue(format!("image not found: {}", path.display()));
            }
        }
        if let Some(slug) = &slide.theme_override {
            if registry.get(slug).is_none() {
                issue(unknown_theme(registry, slug));
            }
        }
        let columns = slide.columns.iter().flat_map(|c| &c.contents);
        for cb in slide
            .code_blocks
            .iter()
            .chain(columns.flat_map(|c| &c.code_blocks))
        {
            if cb.exec_mode.is_some() && !code::executor::is_supported(&cb.language) {
                issue(format!("cannot run '{}' code", cb.language));
            }
        }
        if slide.title.is_empty() && slide.subtitle.is_empty() && slide.blocks.is_empty() {
            issue("empty".to_string());
        }
        for data in &slide.qr_codes {
            if qrcode::QrCode::new(data.as_bytes()).is_err() {
                issue("too much text for a QR code".to_string());
            }
        }
        if slide.charts.iter().any(|c| c.bars.is_empty()) {
            issue("chart without `label: value` lines".to_string());
        }
        if slide.polls.iter().any(|p| p.options.len() < 2) {
            issue("poll with fewer than two options".to_string());
        }
    }
    if let Some((cols, rows)) = size {
        let theme =
            resolve_theme(registry, Some(theme)).or_else(|_| resolve_theme(registry, None))?;
        let config = render::PresenterConfig {
            slides: slides.to_vec(),
            meta: meta.clone(),
            theme,
            theme_explicit: true,
            start: Some(0),
            presentation_path: file.to_path_buf(),
            image_protocol: Some(ImageProtocol::Blocks),
            remote: None,
            allow_exec: true,
            allow_remote_exec: false,
            fullscreen: false,
            timer: false,
            scale: 80,
            record: None,
        };
        for (n, extra) in render::overflowing_slides(config, cols, rows) {
            issues.push((
                slides.get(n - 1).map_or(1, |s| s.line),
                format!("slide {n}: {extra} row(s) too tall for {cols}x{rows} (it will scroll)"),
            ));
        }
    }
    let count = match slides.len() {
        1 => "1 slide".to_string(),
        n => format!("{n} slides"),
    };
    println!("{}: {count}, theme {theme}", file.display());
    if issues.is_empty() {
        println!("OK");
        return Ok(());
    }
    // `file:line: message`, which editors and terminals can jump to.
    issues.sort_by_key(|&(line, _)| line);
    for (line, message) in &issues {
        println!("{}:{line}: {}", file.display(), printable(message));
    }
    bail!("{} problem(s) found", issues.len())
}
