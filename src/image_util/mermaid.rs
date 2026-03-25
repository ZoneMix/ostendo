//! Mermaid diagram rendering via the external `mmdc` CLI tool.
//!
//! Converts Mermaid diagram syntax (flowcharts, sequence diagrams, Gantt charts,
//! etc.) into PNG images with transparent backgrounds.  The rendered PNGs are
//! then displayed using the normal image rendering pipeline.
//!
//! # External dependency
//!
//! Requires the `mmdc` command (Mermaid CLI) to be installed and available on
//! `$PATH`.  Install it with `npm install -g @mermaid-js/mermaid-cli`.
//! Use [`MermaidRenderer::is_available`] to check before attempting to render.
//!
//! # Caching
//!
//! Rendered diagrams are cached by a hash of `(source_text, width)`.  If the
//! same diagram source and width are requested again, the cached PNG is returned
//! immediately without re-invoking `mmdc`.  Cache files are stored in the OS
//! temp directory under `ostendo-mermaid-cache/`.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

/// Stateful renderer that invokes `mmdc` and caches the output PNG files.
///
/// Create one instance with [`MermaidRenderer::new`] and reuse it across slides
/// to benefit from the content-hash cache.
pub struct MermaidRenderer {
    /// Directory where cached `.mmd` source and `.png` output files are stored.
    cache_dir: PathBuf,
    /// In-memory map from content hash to the path of the rendered PNG.
    cache: HashMap<u64, PathBuf>,
}

impl MermaidRenderer {
    /// Create a new renderer, initializing the cache directory in the OS temp folder.
    pub fn new() -> Self {
        let cache_dir = std::env::temp_dir().join("ostendo-mermaid-cache");
        let _ = std::fs::create_dir_all(&cache_dir);
        Self {
            cache_dir,
            cache: HashMap::new(),
        }
    }

    /// Check if the mmdc CLI is available.
    pub fn is_available() -> bool {
        Command::new("mmdc")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Render a Mermaid diagram to a PNG image and return the file path.
    ///
    /// # Parameters
    ///
    /// - `source` -- the Mermaid diagram source text (e.g. `"graph LR; A-->B"`).
    /// - `width` -- the desired output width in pixels, passed to `mmdc -w`.
    ///
    /// # Returns
    ///
    /// The path to the rendered PNG file (inside the cache directory).
    /// On cache hit the file is returned immediately without invoking `mmdc`.
    pub fn render(&mut self, source: &str, width: usize) -> Result<PathBuf> {
        let hash = self.hash_source(source, width);

        // Check cache
        if let Some(path) = self.cache.get(&hash) {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        let input_path = self.cache_dir.join(format!("{}.mmd", hash));
        let output_path = self.cache_dir.join(format!("{}.png", hash));

        std::fs::write(&input_path, source)?;

        let status = Command::new("mmdc")
            .arg("-i")
            .arg(&input_path)
            .arg("-o")
            .arg(&output_path)
            .arg("-w")
            .arg(width.to_string())
            .arg("--backgroundColor")
            .arg("transparent")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()?;

        if !status.success() {
            anyhow::bail!("mmdc failed to render Mermaid diagram");
        }

        self.cache.insert(hash, output_path.clone());
        Ok(output_path)
    }

    /// Compute a deterministic hash of the diagram source and width for cache keying.
    ///
    /// Uses Rust's default `DefaultHasher` (SipHash).  Collisions are extremely
    /// unlikely for the small number of diagrams in a typical presentation.
    fn hash_source(&self, source: &str, width: usize) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        width.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for MermaidRenderer {
    fn default() -> Self {
        Self::new()
    }
}
