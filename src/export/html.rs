//! Self-contained HTML export.
//!
//! Produces one HTML file with inline CSS, JavaScript and base64 images that
//! opens in any browser without a server. Code blocks are emitted as plain
//! `<pre><code class="language-…">` without highlighting. Arrow keys, space
//! and `h`/`l` navigate, `N` toggles speaker notes, and `@media print` rules
//! lay out one slide per page for printing and PDF export.

use anyhow::Result;
use base64::Engine;
use std::path::Path;

use crate::presentation::Slide;
use crate::theme::Theme;

/// Write `slides` styled with `theme` to `output_path` as a single HTML file.
pub fn export_html(slides: &[Slide], theme: &Theme, output_path: &Path) -> Result<()> {
    let bg = &theme.colors.background;
    let text = &theme.colors.text;
    let accent = &theme.colors.accent;
    let code_bg = &theme.colors.code_background;

    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    html.push_str("<title>Presentation</title>\n");

    html.push_str("<style>\n");
    html.push_str(&format!(r#"
:root {{
    --bg: {bg};
    --text: {text};
    --accent: {accent};
    --code-bg: {code_bg};
}}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ background: var(--bg); color: var(--text); font-family: monospace; }}
.slide {{
    display: none;
    width: 100vw;
    height: 100vh;
    padding: 5vh 8vw;
    overflow: hidden;
}}
.slide.active {{ display: flex; flex-direction: column; justify-content: flex-start; }}
.slide h1 {{ color: var(--accent); font-size: 2.5em; margin-bottom: 0.5em; font-weight: bold; }}
.slide .subtitle {{ color: var(--text); font-size: 1.2em; margin-bottom: 1em; opacity: 0.8; }}
.slide ul {{ list-style: none; padding-left: 1em; }}
.slide li {{ margin: 0.3em 0; }}
.slide li::before {{ content: "* "; color: var(--accent); }}
.slide pre {{
    background: var(--code-bg);
    padding: 1em;
    border-radius: 4px;
    overflow: hidden;
    word-wrap: break-word;
    white-space: pre-wrap;
    max-width: 100%;
    margin: 0.5em 0;
    font-size: 0.9em;
}}
.slide code {{ font-family: monospace; }}
.slide blockquote {{
    border-left: 3px solid var(--accent);
    padding-left: 1em;
    font-style: italic;
    opacity: 0.8;
    margin: 0.5em 0;
}}
.slide table {{
    border-collapse: collapse;
    margin: 0.5em 0;
}}
.slide th, .slide td {{
    border: 1px solid var(--accent);
    padding: 0.3em 0.8em;
    text-align: left;
}}
.slide th {{ color: var(--accent); font-weight: bold; }}
.slide .notes {{ display: none; }}
.slide .notes.visible {{
    display: block;
    background: var(--code-bg);
    padding: 1em;
    margin-top: auto;
    border-top: 2px solid var(--accent);
    font-size: 0.8em;
}}
.slide img {{ max-width: 80%; max-height: 50vh; margin: 1em 0; }}
.progress {{
    position: fixed;
    bottom: 0;
    left: 0;
    height: 3px;
    background: var(--accent);
    transition: width 0.3s;
}}
.slide-counter {{
    position: fixed;
    bottom: 8px;
    right: 12px;
    font-size: 0.8em;
    color: var(--accent);
}}
@media print {{
    .slide {{ display: flex !important; flex-direction: column; justify-content: flex-start; page-break-after: always; height: 100vh; overflow: hidden; }}
    .slide:last-child {{ page-break-after: avoid; }}
    .progress, .slide-counter {{ display: none; }}
}}
@page {{ size: landscape; margin: 0; }}
"#));
    html.push_str("</style>\n</head>\n<body>\n");

    for (i, slide) in slides.iter().enumerate() {
        let active = if i == 0 { " active" } else { "" };
        html.push_str(&format!(
            "<div class=\"slide{}\" data-slide=\"{}\">\n",
            active, i
        ));

        if !slide.title.is_empty() {
            html.push_str(&format!("<h1>{}</h1>\n", escape_html(&slide.title)));
        }

        if !slide.subtitle.is_empty() {
            html.push_str(&format!(
                "<div class=\"subtitle\">{}</div>\n",
                escape_html(&slide.subtitle)
            ));
        }

        if !slide.bullets.is_empty() {
            html.push_str("<ul>\n");
            for bullet in &slide.bullets {
                let indent = "  ".repeat(bullet.depth);
                html.push_str(&format!(
                    "{}<li>{}</li>\n",
                    indent,
                    escape_html(&bullet.text)
                ));
            }
            html.push_str("</ul>\n");
        }

        for cb in &slide.code_blocks {
            html.push_str(&format!(
                "<pre><code class=\"language-{}\">{}</code></pre>\n",
                escape_html(&cb.language),
                escape_html(&cb.code)
            ));
        }

        for bq in &slide.block_quotes {
            html.push_str("<blockquote>\n");
            for line in &bq.lines {
                html.push_str(&format!("<p>{}</p>\n", escape_html(line)));
            }
            html.push_str("</blockquote>\n");
        }

        for table in &slide.tables {
            html.push_str("<table>\n<thead><tr>\n");
            for header in &table.headers {
                html.push_str(&format!("<th>{}</th>", escape_html(header)));
            }
            html.push_str("\n</tr></thead>\n<tbody>\n");
            for row in &table.rows {
                html.push_str("<tr>");
                for cell in row {
                    html.push_str(&format!("<td>{}</td>", escape_html(cell)));
                }
                html.push_str("</tr>\n");
            }
            html.push_str("</tbody></table>\n");
        }

        if let Some(img) = &slide.image {
            if let Some(uri) = image_data_uri(&img.path) {
                let alt = escape_html(&img.alt_text);
                html.push_str(&format!("<img src=\"{uri}\" alt=\"{alt}\">\n"));
            }
        }

        if !slide.notes.is_empty() {
            html.push_str(&format!(
                "<div class=\"notes\">{}</div>\n",
                escape_html(&slide.notes).replace('\n', "<br>")
            ));
        }

        html.push_str("</div>\n");
    }

    html.push_str("<div class=\"progress\" id=\"progress\"></div>\n");
    html.push_str("<div class=\"slide-counter\" id=\"counter\"></div>\n");

    html.push_str("<script>\n");
    html.push_str(
        r#"
let current = 0;
const slides = document.querySelectorAll('.slide');
const total = slides.length;

function showSlide(n) {
    slides[current].classList.remove('active');
    current = Math.max(0, Math.min(n, total - 1));
    slides[current].classList.add('active');
    document.getElementById('progress').style.width = ((current + 1) / total * 100) + '%';
    document.getElementById('counter').textContent = (current + 1) + '/' + total;
}

document.addEventListener('keydown', (e) => {
    switch(e.key) {
        case 'ArrowRight': case ' ': case 'l': showSlide(current + 1); break;
        case 'ArrowLeft': case 'h': showSlide(current - 1); break;
        case 'n': case 'N':
            document.querySelectorAll('.notes').forEach(n =>
                n.classList.toggle('visible'));
            break;
    }
});

showSlide(0);
"#,
    );
    html.push_str("</script>\n");
    html.push_str("</body>\n</html>\n");

    std::fs::write(output_path, html)?;
    Ok(())
}

/// Inline an image file as a data URI. Anything that is not recognizably an
/// image is skipped: markdown can point `![](…)` at any path, and embedding
/// e.g. `~/.aws/credentials` would leak it into a file meant for sharing.
fn image_data_uri(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let is_svg = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"));
    let mime = if is_svg {
        "image/svg+xml"
    } else {
        image::guess_format(&data).ok()?.to_mime_type()
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
    Some(format!("data:{mime};base64,{encoded}"))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::{ImagePosition, ImageRenderMode, SlideImage};

    fn export(slides: &[Slide]) -> String {
        let theme = crate::theme::ThemeRegistry::load()
            .get("terminal_green")
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.html");
        export_html(slides, &theme, &path).unwrap();
        std::fs::read_to_string(&path).unwrap()
    }

    fn slide_with_image(path: &Path) -> Slide {
        Slide {
            image: Some(SlideImage {
                path: path.to_path_buf(),
                alt_text: String::new(),
                position: ImagePosition::Below,
                render_mode: ImageRenderMode::Auto,
                scale: 100,
                color_override: String::new(),
            }),
            ..Slide::default()
        }
    }

    #[test]
    fn test_export_html_basic() {
        let content = export(&[Slide {
            number: 1,
            title: "Test <script>alert(1)</script> & Slide".to_string(),
            ..Slide::default()
        }]);
        assert!(content.starts_with("<!DOCTYPE html>"));
        assert!(content.contains("Test &lt;script&gt;alert(1)&lt;/script&gt; &amp; Slide"));
        assert!(!content.contains("<script>alert(1)"));
    }

    #[test]
    fn embeds_images_but_not_arbitrary_files() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("pixel.png");
        image::RgbaImage::new(1, 1).save(&png).unwrap();
        let secret = dir.path().join("credentials");
        std::fs::write(&secret, "aws_secret_access_key=hunter2").unwrap();

        let content = export(&[slide_with_image(&png), slide_with_image(&secret)]);
        assert_eq!(content.matches("<img ").count(), 1);
        assert!(content.contains("<img src=\"data:image/png;base64,"));
    }
}
