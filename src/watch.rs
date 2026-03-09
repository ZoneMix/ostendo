use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Polling file watcher that checks for modifications at a fixed interval.
pub struct FileWatcher {
    modified: Arc<AtomicBool>,
    _handle: std::thread::JoinHandle<()>,
}

impl FileWatcher {
    /// Start watching a file for modifications (polls every 500ms).
    pub fn new(path: PathBuf) -> Self {
        let modified = Arc::new(AtomicBool::new(false));
        let flag = modified.clone();

        let handle = std::thread::spawn(move || {
            let mut last_mtime = Self::file_mtime(&path);
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let current = Self::file_mtime(&path);
                if current != last_mtime {
                    last_mtime = current;
                    flag.store(true, Ordering::Relaxed);
                }
            }
        });

        Self {
            modified,
            _handle: handle,
        }
    }

    /// Check if the file has been modified since last check. Resets the flag.
    pub fn check_modified(&self) -> bool {
        self.modified.swap(false, Ordering::Relaxed)
    }

    fn file_mtime(path: &PathBuf) -> Option<SystemTime> {
        std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
    }
}
