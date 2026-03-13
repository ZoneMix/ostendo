//! File watcher for hot-reload.
//!
//! Polls the presentation markdown file every 500ms for changes by comparing
//! the file's modification timestamp. When a change is detected, it sets an
//! atomic flag that the main render loop checks on each tick. The render loop
//! then calls `try_reload()`, which re-parses the markdown while preserving
//! the current slide position — giving the presenter a live-editing experience.
//!
//! This uses polling instead of OS-level file watching (e.g., `inotify` or
//! `FSEvents`) to keep dependencies minimal and behavior consistent across
//! platforms.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Polling file watcher that checks for modifications at a fixed interval.
///
/// Spawns a background thread that runs for the lifetime of the `FileWatcher`.
/// The thread is a detached daemon — it runs until the process exits. The
/// `_handle` field holds the `JoinHandle` to keep the thread referenced, but
/// it is never joined (hence the `_` prefix).
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

    /// Read the file's last-modified timestamp from the filesystem.
    ///
    /// Returns `None` if the file does not exist or the metadata cannot be read.
    /// This is used by the polling loop to detect changes.
    fn file_mtime(path: &PathBuf) -> Option<SystemTime> {
        std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
    }
}
