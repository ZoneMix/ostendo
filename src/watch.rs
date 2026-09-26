//! Hot reload: a background thread polls the presentation's modification time.
//!
//! Polling (every 500 ms) keeps dependencies minimal and behaves the same on
//! every platform; editors that replace the file on save are handled too.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub struct FileWatcher {
    modified: Arc<AtomicBool>,
}

impl FileWatcher {
    pub fn new(path: PathBuf) -> Self {
        let modified = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&modified);
        let mtime = move || std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        std::thread::spawn(move || {
            let mut last: Option<SystemTime> = mtime();
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let current = mtime();
                if current != last {
                    last = current;
                    flag.store(true, Ordering::Relaxed);
                }
            }
        });
        Self { modified }
    }

    /// Whether the file changed since the last call.
    pub fn check_modified(&self) -> bool {
        self.modified.swap(false, Ordering::Relaxed)
    }
}
