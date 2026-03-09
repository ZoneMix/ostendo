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
