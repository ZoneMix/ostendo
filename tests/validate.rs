//! `--validate --size` lays slides out on a headless presenter; nothing it does
//! may reach the terminal, even where font sizing and Kitty images are available.

use std::process::Command;

#[test]
fn validate_with_a_size_prints_only_its_report() {
    let out = Command::new(env!("CARGO_BIN_EXE_ostendo"))
        .args(["--validate", "--size", "100x30"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/presentations/examples/test_presentation.md"
        ))
        .env("KITTY_WINDOW_ID", "1")
        .env("TERM", "xterm-kitty")
        .env_remove("TMUX")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    for stream in [&out.stdout, &out.stderr] {
        assert!(
            !stream.contains(&0x1b),
            "escape sequence in output: {:?}",
            String::from_utf8_lossy(stream)
        );
    }
    assert!(String::from_utf8_lossy(&out.stdout).ends_with("OK\n"));
}
