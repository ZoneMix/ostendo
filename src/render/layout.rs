/// Terminal window size with pixel dimensions for accurate image scaling.
#[derive(Debug, Clone, Copy)]
pub struct WindowSize {
    pub columns: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

#[allow(dead_code)]
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
            pixel_width: cols * 8,   // heuristic: 8px per column
            pixel_height: rows * 16, // heuristic: 16px per row
        }
    }

    /// Pixels per terminal column.
    pub fn pixels_per_column(&self) -> f64 {
        if self.columns == 0 { return 8.0; }
        self.pixel_width as f64 / self.columns as f64
    }

    /// Pixels per terminal row.
    pub fn pixels_per_row(&self) -> f64 {
        if self.rows == 0 { return 16.0; }
        self.pixel_height as f64 / self.rows as f64
    }

    /// Aspect ratio correction factor: (pixels_per_row / pixels_per_column).
    /// Used to convert image aspect ratio to terminal cell aspect ratio.
    pub fn aspect_ratio(&self) -> f64 {
        self.pixels_per_column() / self.pixels_per_row()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_size_aspect_ratio() {
        let ws = WindowSize {
            columns: 80,
            rows: 24,
            pixel_width: 640,
            pixel_height: 384,
        };
        assert_eq!(ws.pixels_per_column(), 8.0);
        assert_eq!(ws.pixels_per_row(), 16.0);
        assert!((ws.aspect_ratio() - 0.5).abs() < 0.01);
    }

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
