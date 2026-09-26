//! PDF export by printing the HTML export with headless Chrome/Chromium, or
//! `wkhtmltopdf` as a fallback. Neither is bundled; a missing converter is
//! reported as an error.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::presentation::Slide;
use crate::theme::Theme;

/// Find an installed PDF converter, preferring Chrome/Chromium.
pub fn detect_pdf_converter() -> Option<&'static str> {
    let chrome_names = [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ];
    for name in &chrome_names {
        if which_exists(name) {
            return Some(name);
        }
    }
    if which_exists("wkhtmltopdf") {
        return Some("wkhtmltopdf");
    }
    None
}

/// Render `slides` to a temporary HTML file and print it to `pdf_path`.
pub fn export_pdf(slides: &[Slide], theme: &Theme, pdf_path: &Path) -> Result<()> {
    let converter = detect_pdf_converter().ok_or_else(|| {
        anyhow::anyhow!("No PDF converter found. Install Chrome/Chromium or wkhtmltopdf.")
    })?;

    // Chrome picks the renderer from the extension, so the file must end in .html.
    let html = tempfile::Builder::new()
        .prefix("ostendo-export-")
        .suffix(".html")
        .tempfile()?;
    super::html::export_html(slides, theme, html.path())?;

    let mut command = Command::new(converter);
    if converter == "wkhtmltopdf" {
        command
            .args(["--enable-local-file-access", "--page-size", "A4"])
            .args(["--orientation", "Landscape"])
            .arg(html.path())
            .arg(pdf_path);
    } else {
        command
            .args(["--headless", "--disable-gpu", "--print-to-pdf-no-header"])
            .arg("--run-all-compositor-stages-before-draw")
            .arg("--virtual-time-budget=5000")
            .arg(format!("--print-to-pdf={}", pdf_path.display()))
            .arg(format!("file://{}", html.path().canonicalize()?.display()));
    }

    // `output()` drains stderr while waiting; `status()` with a piped stderr
    // deadlocks once the converter fills the pipe buffer.
    let output = command.stdin(Stdio::null()).output()?;
    if !output.status.success() {
        bail!(
            "{converter} failed to convert HTML to PDF ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Absolute paths (the macOS Chrome bundle) are checked directly; bare
/// command names are looked up on `PATH` with `which`.
fn which_exists(cmd: &str) -> bool {
    if cmd.starts_with('/') {
        return Path::new(cmd).exists();
    }
    Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
