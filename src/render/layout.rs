//! Terminal dimension handling.
//!
//! Provides [`WindowSize`], which captures both the character-grid size
//! (columns x rows) and the pixel dimensions of the terminal window.  Pixel
//! dimensions are essential for rendering protocol images (Kitty, iTerm2, Sixel)
//! at the correct resolution -- without them the engine would have to guess
//! how many pixels each character cell occupies.
//!
//! On terminals that do not report pixel sizes, heuristic defaults of 8 px/column
//! and 16 px/row are used (the most common cell dimensions for a 10-12 pt monospace
//! font).

/// Terminal window size with pixel dimensions for accurate image scaling.
///
/// Both character-grid dimensions (`columns`, `rows`) and pixel dimensions
/// (`pixel_width`, `pixel_height`) are stored.  The pixel values are used by
/// the image rendering pipeline to scale images to exact pixel counts before
/// converting back to terminal cell counts.
#[derive(Debug, Clone, Copy)]
pub struct WindowSize {
    /// Number of character columns (e.g. 80, 120, 200).
    pub columns: u16,
    /// Number of character rows (e.g. 24, 40, 60).
    pub rows: u16,
    /// Total window width in pixels (0 if the terminal does not report it).
    pub pixel_width: u16,
    /// Total window height in pixels (0 if the terminal does not report it).
    pub pixel_height: u16,
}

impl WindowSize {
    /// Query the terminal for its current size including pixel dimensions.
    pub fn query() -> Self {
        // Try crossterm's window_size() which returns pixel dimensions on supported terminals
        if let Ok(size) = crossterm::terminal::window_size() {
            if size.width > 0 && size.height > 0 {
                return Self {
                    columns: size.columns,
                    rows: size.rows,
                    pixel_width: size.width,
                    pixel_height: size.height,
                };
            }
        }
        // Fallback: get character dimensions, estimate pixels
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        Self {
            columns: cols,
            rows,
            pixel_width: cols.saturating_mul(8), // heuristic: 8px per column
            pixel_height: rows.saturating_mul(16), // heuristic: 16px per row
        }
    }

    /// Pixels per terminal column.
    pub fn pixels_per_column(&self) -> f64 {
        if self.columns == 0 {
            return 8.0;
        }
        self.pixel_width as f64 / self.columns as f64
    }

    /// Pixels per terminal row.
    pub fn pixels_per_row(&self) -> f64 {
        if self.rows == 0 {
            return 16.0;
        }
        self.pixel_height as f64 / self.rows as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_size_zero_safe() {
        let ws = WindowSize {
            columns: 0,
            rows: 0,
            pixel_width: 0,
            pixel_height: 0,
        };
        assert_eq!(ws.pixels_per_column(), 8.0);
        assert_eq!(ws.pixels_per_row(), 16.0);
    }
}
