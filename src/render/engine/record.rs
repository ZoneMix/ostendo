//! `--record`: an asciicast v2 file (asciinema's format) of everything drawn,
//! for replaying the talk or embedding it on a web page.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

pub(crate) struct Recorder {
    file: BufWriter<File>,
    start: Instant,
}

impl Recorder {
    pub fn create(path: &Path, width: u16, height: u16, title: &str) -> Result<Self> {
        let file =
            File::create(path).with_context(|| format!("cannot write {}", path.display()))?;
        let mut file = BufWriter::new(file);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let header = serde_json::json!({
            "version": 2,
            "width": width,
            "height": height,
            "timestamp": timestamp,
            "title": title,
            "env": { "TERM": "xterm-256color" },
        });
        writeln!(file, "{header}")?;
        Ok(Self {
            file,
            start: Instant::now(),
        })
    }

    /// Appends terminal output. Write errors are ignored: a full disk must
    /// not stop the talk.
    pub fn output(&mut self, bytes: &[u8]) {
        self.event("o", &String::from_utf8_lossy(bytes));
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.event("r", &format!("{width}x{height}"));
    }

    fn event(&mut self, kind: &str, data: &str) {
        let time = self.start.elapsed().as_secs_f64();
        let line = serde_json::json!([(time * 1e6).round() / 1e6, kind, data]);
        let _ = writeln!(self.file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_an_asciicast_v2_header_and_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("talk.cast");
        let mut rec = Recorder::create(&path, 100, 30, "Deck").unwrap();
        rec.output(b"\x1b[1;1Hhi \xff");
        rec.resize(80, 24);
        drop(rec);

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines[0]["version"], 2);
        assert_eq!(
            (lines[0]["width"].clone(), lines[0]["height"].clone()),
            (100.into(), 30.into())
        );
        assert_eq!(lines[1][1], "o");
        assert_eq!(lines[1][2], "\u{1b}[1;1Hhi \u{fffd}");
        assert_eq!(
            (lines[2][1].clone(), lines[2][2].clone()),
            ("r".into(), "80x24".into())
        );
        assert!(lines[1][0].as_f64().unwrap() <= lines[2][0].as_f64().unwrap());
    }
}
