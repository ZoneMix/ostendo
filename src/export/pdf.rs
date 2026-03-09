use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

/// Detect available PDF converter: headless Chrome/Chromium or wkhtmltopdf.
pub fn detect_pdf_converter() -> Option<&'static str> {
    // Check for Chrome/Chromium
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

    // Check for wkhtmltopdf
    if which_exists("wkhtmltopdf") {
        return Some("wkhtmltopdf");
    }

    None
}

/// Export an HTML file to PDF.
///
/// Uses headless Chrome (`--print-to-pdf`) or wkhtmltopdf as fallback.
pub fn export_pdf(html_path: &Path, pdf_path: &Path) -> Result<()> {
    let converter = detect_pdf_converter()
        .ok_or_else(|| anyhow::anyhow!(
            "No PDF converter found. Install Chrome/Chromium or wkhtmltopdf."
        ))?;

    if converter == "wkhtmltopdf" {
        let status = Command::new("wkhtmltopdf")
            .arg("--enable-local-file-access")
            .arg("--page-size")
            .arg("A4")
            .arg("--orientation")
            .arg("Landscape")
            .arg(html_path.to_string_lossy().as_ref())
            .arg(pdf_path.to_string_lossy().as_ref())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()?;

        if !status.success() {
            bail!("wkhtmltopdf failed to convert HTML to PDF");
        }
    } else {
        // Chrome/Chromium headless
        let html_url = format!("file://{}", html_path.canonicalize()?.display());
        let pdf_arg = format!("--print-to-pdf={}", pdf_path.display());

        let status = Command::new(converter)
            .arg("--headless")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--print-to-pdf-no-header")
            .arg("--run-all-compositor-stages-before-draw")
            .arg("--virtual-time-budget=5000")
            .arg(&pdf_arg)
            .arg(&html_url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()?;

        if !status.success() {
            bail!("Chrome headless failed to convert HTML to PDF");
        }
    }

    Ok(())
}

fn which_exists(cmd: &str) -> bool {
    // Handle absolute paths (for macOS Chrome)
    if cmd.starts_with('/') {
        return Path::new(cmd).exists();
    }
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_which_exists_nonexistent() {
        assert!(!which_exists("definitely_not_a_real_command_12345"));
    }

    #[test]
    fn test_detect_pdf_converter_runs() {
        // Just verify it doesn't panic — result depends on system
        let _ = detect_pdf_converter();
    }
}
