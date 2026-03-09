use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

/// Renderer for Mermaid diagrams, with a hash-based cache to avoid re-rendering.
#[allow(dead_code)]
pub struct MermaidRenderer {
    cache_dir: PathBuf,
    cache: HashMap<u64, PathBuf>,
}

#[allow(dead_code)]
impl MermaidRenderer {
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

    /// Render a Mermaid diagram to a PNG image.
    /// Returns the path to the rendered PNG file.
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

    /// Simple hash of source + width for cache keying.
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
