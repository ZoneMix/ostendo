//! Progress bar rendering for the status bar.
//!
//! Produces a simple Unicode block-character progress bar (e.g. `[███░░░░]`)
//! that shows the presenter how far through the slide deck they are.  The bar
//! is rendered into a `String` and embedded in the status bar line by the
//! rendering engine.

/// Render a text-based progress bar showing `current` out of `total` progress.
///
/// # Parameters
///
/// - `current` -- the current position (e.g. slide index, 0-based is fine).
/// - `total` -- the total number of items (e.g. total slides).
/// - `width` -- how many characters wide the bar should be (excluding brackets).
///
/// # Returns
///
/// A string like `[████░░░░░░]`.  Returns an empty string if `total` is zero
/// to avoid a division-by-zero panic.
pub fn render_progress_bar(current: usize, total: usize, width: usize) -> String {
    if total == 0 {
        return String::new();
    }
    let filled = (current * width) / total;
    let empty = width - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_total() {
        assert_eq!(render_progress_bar(1, 0, 10), "");
    }

    #[test]
    fn test_full_progress() {
        let bar = render_progress_bar(10, 10, 10);
        assert!(bar.contains("██████████"));
        assert!(!bar.contains('░'));
    }

    #[test]
    fn test_zero_progress() {
        let bar = render_progress_bar(0, 10, 10);
        assert!(!bar.contains('█'));
        assert!(bar.contains("░░░░░░░░░░"));
    }

    #[test]
    fn test_half_progress() {
        let bar = render_progress_bar(5, 10, 10);
        assert!(bar.starts_with('['));
        assert!(bar.ends_with(']'));
        assert!(bar.contains('█'));
        assert!(bar.contains('░'));
    }
}
