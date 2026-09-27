//! Self-contained HTML export.
//!
//! Produces one HTML file with inline CSS, JavaScript and base64 images that
//! opens in any browser without a server. Slide bodies follow source order,
//! like the terminal renderer. Code blocks are emitted as plain
//! `<pre><code class="language-…">` without highlighting. Arrow keys, space
//! and `h`/`l` navigate, `N` toggles speaker notes, and `@media print` rules
//! lay out one slide per page for printing and PDF export.

use anyhow::Result;
use base64::Engine;
use std::fmt::Write as _;
use std::path::Path;

use crate::presentation::{
    Block, BlockQuote, Bullet, Chart, ColumnContent, ColumnItem, Slide, Table,
};
use crate::theme::Theme;

/// Write `slides` styled with `theme` to `output_path` as a single HTML file
/// titled `title`.
pub fn export_html(slides: &[Slide], theme: &Theme, title: &str, output_path: &Path) -> Result<()> {
    let bg = &theme.colors.background;
    let text = &theme.colors.text;
    let accent = &theme.colors.accent;
    let code_bg = &theme.colors.code_background;

    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    let _ = writeln!(html, "<title>{}</title>", escape_html(title));

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
.slide li::before {{ content: "• "; color: var(--accent); }}
.slide li.d1 {{ padding-left: 1.5em; }}
.slide li.d1::before {{ content: "◦ "; }}
.slide li.d2 {{ padding-left: 3em; }}
.slide li.d2::before {{ content: "▪ "; }}
.slide li.ordered::before {{ content: none; }}
.slide p {{ margin: 0.4em 0; }}
.slide :not(pre) > code {{ background: var(--code-bg); padding: 0 0.3em; border-radius: 3px; }}
.slide .columns {{ display: grid; gap: 2em; margin: 0.5em 0; }}
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
.slide pre.math {{ background: none; width: fit-content; margin: 0.5em auto; white-space: pre; line-height: 1.2; font-size: 1.1em; }}
.slide blockquote {{
    border-left: 3px solid var(--accent);
    padding-left: 1em;
    font-style: italic;
    opacity: 0.8;
    margin: 0.5em 0;
}}
.slide .chart {{ margin: 0.5em 0; }}
.slide .chart figcaption {{ opacity: 0.7; margin-bottom: 0.4em; }}
.slide .chart .bar {{ display: grid; grid-template-columns: 8em 1fr auto; gap: 0.6em; align-items: center; }}
.slide .chart .bar i {{ display: block; height: 0.9em; background: var(--accent); border-radius: 2px; }}
.slide .chart .bar b {{ font-weight: normal; opacity: 0.7; }}
.slide svg.qr {{ width: 12em; height: 12em; display: block; margin: 0.5em auto; }}
.slide .callout {{
    --tone: #4493f8;
    border-left: 4px solid var(--tone);
    background: color-mix(in srgb, var(--tone) 10%, transparent);
    padding: 0.4em 1em;
    margin: 0.5em 0;
}}
.slide .callout.tip {{ --tone: #3fb950; }}
.slide .callout.important {{ --tone: #ab7df8; }}
.slide .callout.warning {{ --tone: #d29922; }}
.slide .callout.caution {{ --tone: #f85149; }}
.slide .callout .heading {{ color: var(--tone); font-weight: bold; margin: 0.2em 0; }}
.slide a {{ color: var(--accent); }}
.slide li.task {{ list-style: none; }}
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

        html.push_str(&slide_body(slide));

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

fn slide_body(slide: &Slide) -> String {
    let mut out = String::new();
    if !slide.title.is_empty() {
        let _ = writeln!(out, "<h1>{}</h1>", inline(&slide.title));
    }
    if !slide.subtitle.is_empty() {
        let _ = writeln!(
            out,
            "<div class=\"subtitle\">{}</div>",
            inline(&slide.subtitle)
        );
    }
    for block in &slide.blocks {
        match *block {
            Block::Paragraph(i) => {
                let _ = writeln!(out, "<p>{}</p>", inline(&slide.paragraphs[i]));
            }
            Block::Bullets(i) => {
                out.push_str(&list(&slide.bullets[slide.bullet_groups[i].clone()]))
            }
            Block::Code(i) => out.push_str(&code(&slide.code_blocks[i])),
            Block::Table(i) => out.push_str(&table(&slide.tables[i])),
            Block::Quote(i) => out.push_str(&quote(&slide.block_quotes[i])),
            Block::Diagram(i) => {
                let d = &slide.diagram_blocks[i];
                let graph = crate::diagram::parser::parse(&d.source);
                let c = crossterm::style::Color::Reset;
                let text: Vec<String> =
                    crate::diagram::render_adaptive(&graph, d.style, 200, c, c, c, "")
                        .iter()
                        .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
                        .collect();
                let _ = writeln!(
                    out,
                    "<pre class=\"diagram\">{}</pre>",
                    escape_html(&text.join("\n"))
                );
            }
            Block::Mermaid(i) => {
                let source = escape_html(&slide.mermaid_blocks[i].source);
                let _ = writeln!(out, "<pre class=\"mermaid\">{source}</pre>");
            }
            Block::Chart(i) => out.push_str(&chart(&slide.charts[i])),
            Block::Qr(i) => out.push_str(&qr_svg(&slide.qr_codes[i])),
            Block::Math(i) => out.push_str(&math(&slide.math[i])),
            Block::Image => {
                if let Some(img) = &slide.image {
                    out.push_str(&image(&img.path, &img.alt_text));
                }
            }
            Block::Columns => {
                let Some(cols) = &slide.columns else { continue };
                let template: Vec<String> = cols.ratios.iter().map(|r| format!("{r}fr")).collect();
                let _ = writeln!(
                    out,
                    "<div class=\"columns\" style=\"grid-template-columns: {}\">",
                    template.join(" ")
                );
                for content in &cols.contents {
                    let _ = writeln!(out, "<div>{}</div>", column(content));
                }
                out.push_str("</div>\n");
            }
        }
    }
    out
}

fn column(content: &ColumnContent) -> String {
    let mut out = String::new();
    let mut items = content.items.iter().peekable();
    while let Some(item) = items.next() {
        match *item {
            ColumnItem::Text(i) => {
                let _ = writeln!(out, "<p>{}</p>", inline(&content.text_lines[i]));
            }
            ColumnItem::Bullet(first) => {
                let mut last = first;
                while let Some(ColumnItem::Bullet(next)) = items.peek() {
                    last = *next;
                    items.next();
                }
                out.push_str(&list(&content.bullets[first..=last]));
            }
            ColumnItem::Code(i) => out.push_str(&code(&content.code_blocks[i])),
            ColumnItem::Table(i) => out.push_str(&table(&content.tables[i])),
            ColumnItem::Quote(i) => out.push_str(&quote(&content.quotes[i])),
            ColumnItem::Math(i) => out.push_str(&math(&content.math[i])),
            ColumnItem::Image => {
                if let Some(img) = &content.image {
                    out.push_str(&image(Path::new(&img.path), ""));
                }
            }
        }
    }
    out
}

fn quote(q: &BlockQuote) -> String {
    let mut out = String::new();
    match &q.callout {
        Some((kind, heading)) => {
            let kind = kind.name().to_lowercase();
            let _ = writeln!(out, "<div class=\"callout {kind}\">");
            let _ = writeln!(out, "<p class=\"heading\">{}</p>", escape_html(heading));
        }
        None => out.push_str("<blockquote>\n"),
    }
    for line in &q.lines {
        let _ = writeln!(out, "<p>{}</p>", inline(line));
    }
    out.push_str(if q.callout.is_some() {
        "</div>\n"
    } else {
        "</blockquote>\n"
    });
    out
}

fn list(items: &[Bullet]) -> String {
    let mut out = String::from("<ul>\n");
    for b in items {
        let mut class = format!("d{}", b.depth.min(2));
        let body = match b.task() {
            Some((done, text)) => {
                class.push_str(" task");
                let checked = if done { " checked" } else { "" };
                format!(
                    "<input type=\"checkbox\" disabled{checked}> {}",
                    inline(text)
                )
            }
            None => {
                if b.is_ordered() {
                    class.push_str(" ordered");
                }
                inline(&b.text)
            }
        };
        let _ = writeln!(out, "<li class=\"{class}\">{body}</li>");
    }
    out.push_str("</ul>\n");
    out
}

fn chart(chart: &Chart) -> String {
    let max = chart.bars.iter().map(|b| b.1).fold(0.0, f64::max);
    let mut out = String::from("<figure class=\"chart\">\n");
    if let Some(title) = &chart.title {
        let _ = writeln!(out, "<figcaption>{}</figcaption>", escape_html(title));
    }
    for (label, value, shown) in &chart.bars {
        let pct = if max > 0.0 { value / max * 100.0 } else { 0.0 };
        let _ = writeln!(
            out,
            "<div class=\"bar\"><span>{}</span><i style=\"width:{pct:.1}%\"></i><b>{}</b></div>",
            escape_html(label),
            escape_html(shown)
        );
    }
    out.push_str("</figure>\n");
    out
}

/// The QR code as an inline SVG, one square per dark module.
fn qr_svg(data: &str) -> String {
    let Ok(code) = qrcode::QrCode::new(data.as_bytes()) else {
        return format!("<p>{}</p>\n", escape_html(data));
    };
    let n = code.width();
    let mut path = String::new();
    for y in 0..n {
        for x in 0..n {
            if code[(x, y)] == qrcode::Color::Dark {
                let _ = write!(path, "M{} {}h1v1h-1z", x + 4, y + 4);
            }
        }
    }
    let size = n + 8;
    format!(
        "<svg class=\"qr\" viewBox=\"0 0 {size} {size}\" role=\"img\" aria-label=\"{}\"><rect width=\"{size}\" height=\"{size}\" fill=\"#fff\"/><path d=\"{path}\" fill=\"#000\"/></svg>\n",
        escape_html(data)
    )
}

fn code(cb: &crate::presentation::CodeBlock) -> String {
    format!(
        "<pre><code class=\"language-{}\">{}</code></pre>\n",
        escape_html(&cb.language),
        escape_html(&cb.code)
    )
}

fn table(t: &Table) -> String {
    let mut out = String::from("<table>\n<thead><tr>");
    for header in &t.headers {
        let _ = write!(out, "<th>{}</th>", inline(header));
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for row in &t.rows {
        out.push_str("<tr>");
        for cell in row {
            let _ = write!(out, "<td>{}</td>", inline(cell));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody></table>\n");
    out
}

fn image(path: &Path, alt: &str) -> String {
    image_data_uri(path)
        .map(|uri| format!("<img src=\"{uri}\" alt=\"{}\">\n", escape_html(alt)))
        .unwrap_or_default()
}

/// Inline markdown (bold, italic, code, strikethrough) as HTML.
fn inline(text: &str) -> String {
    let marker = crossterm::style::Color::AnsiValue(1);
    crate::markdown::parser::parse_inline_formatting(text, crossterm::style::Color::Reset, marker)
        .iter()
        .map(|span| {
            let is_code = span.bg == Some(marker);
            // The terminal pads code spans with a space on each side.
            let text = match is_code {
                true => span
                    .text
                    .strip_prefix(' ')
                    .and_then(|t| t.strip_suffix(' '))
                    .unwrap_or(&span.text),
                false => &span.text,
            };
            let mut html = escape_html(text);
            for (on, tag) in [
                (is_code, "code"),
                (span.strikethrough, "del"),
                (span.italic, "em"),
                (span.bold, "strong"),
            ] {
                if on {
                    html = format!("<{tag}>{html}</{tag}>");
                }
            }
            // Only web and mail links: a `javascript:` target must not become live.
            match span.link.as_deref().filter(|url| {
                ["http://", "https://", "mailto:"]
                    .iter()
                    .any(|scheme| url.starts_with(scheme))
            }) {
                Some(url) => format!("<a href=\"{}\">{html}</a>", escape_html(url)),
                None => html,
            }
        })
        .collect()
}

/// Inline an image file as a data URI. Anything that is not recognizably an
/// image is skipped: markdown can point `![](…)` at any path, and embedding
/// e.g. `~/.aws/credentials` would leak it into a file meant for sharing.
fn image_data_uri(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let is_svg = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
        && std::str::from_utf8(&data).is_ok_and(|text| text.contains("<svg"));
    let mime = if is_svg {
        "image/svg+xml"
    } else {
        image::guess_format(&data).ok()?.to_mime_type()
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
    Some(format!("data:{mime};base64,{encoded}"))
}

/// Display math as the terminal draws it; a `<pre>` keeps the rows aligned.
fn math(tex: &str) -> String {
    let rows = crate::math::display(tex).join("\n");
    format!("<pre class=\"math\">{}</pre>\n", escape_html(&rows))
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
        export_html(slides, &theme, "Deck", &path).unwrap();
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
            blocks: vec![Block::Image],
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
    fn bodies_follow_source_order_with_inline_markup() {
        let md = concat!(
            "# T\n\nIntro **bold** and `code`, [docs](https://x.dev), [x](javascript:alert(1))\n\n",
            "> quote\n\n> [!WARNING]\n> Careful\n\n- item ~~old~~\n- [x] shipped\n",
        );
        let (_, slides) = crate::markdown::parse_presentation(md, None).unwrap();
        let html = export(&slides);
        let at = |needle: &str| {
            html.find(needle)
                .unwrap_or_else(|| panic!("missing {needle}"))
        };
        assert!(at("<strong>bold</strong>") < at("<blockquote>"));
        assert!(at("<blockquote>") < at("<del>old</del>"));
        assert!(html.contains("<code>code</code>") && html.contains("<title>Deck</title>"));
        assert!(html.contains("<a href=\"https://x.dev\">docs</a>"));
        assert!(
            !html.contains("javascript:alert(1)\""),
            "script link made live"
        );
        assert!(at("<div class=\"callout warning\">") < at("Careful"));
        assert!(html.contains("<input type=\"checkbox\" disabled checked> shipped"));
    }

    #[test]
    fn embeds_images_but_not_arbitrary_files() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("pixel.png");
        image::RgbaImage::new(1, 1).save(&png).unwrap();
        let secret = dir.path().join("credentials");
        std::fs::write(&secret, "aws_secret_access_key=hunter2").unwrap();
        let renamed = dir.path().join("credentials.svg");
        std::fs::write(&renamed, "aws_secret_access_key=hunter2").unwrap();

        let content = export(&[
            slide_with_image(&png),
            slide_with_image(&secret),
            slide_with_image(&renamed),
        ]);
        assert_eq!(content.matches("<img ").count(), 1);
        assert!(content.contains("<img src=\"data:image/png;base64,"));
    }
}
