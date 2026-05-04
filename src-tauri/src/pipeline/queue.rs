use crate::fs::scanner::ScannedFile;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// An item in the processing queue.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub file: ScannedFile,
    pub attempt: u32,
}

/// Thread-safe job queue for distributing work to workers.
///
/// Supports live-append: callers may push more items into a running queue
/// (drag-and-drop while processing). Workers wait on an empty-but-open queue;
/// they exit only after `close()` is called and the queue is drained.
pub struct JobQueue {
    items: Mutex<VecDeque<QueueItem>>,
    total: AtomicUsize,
    closed: AtomicBool,
    last_activity: Mutex<Instant>,
}

impl JobQueue {
    /// Create a new queue from scanned files (already sorted by size desc).
    pub fn new(files: Vec<ScannedFile>) -> Self {
        let total = files.len();
        let items: VecDeque<QueueItem> = files
            .into_iter()
            .map(|file| QueueItem { file, attempt: 1 })
            .collect();
        Self {
            items: Mutex::new(items),
            total: AtomicUsize::new(total),
            closed: AtomicBool::new(false),
            last_activity: Mutex::new(Instant::now()),
        }
    }

    /// Dequeue the next item for processing. Returns None if queue is empty.
    pub fn dequeue(&self) -> Option<QueueItem> {
        self.items
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
    }

    /// Re-add an item for retry with incremented attempt count.
    pub fn requeue_for_retry(&self, item: QueueItem, next_attempt: u32) {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        items.push_back(QueueItem {
            file: item.file,
            attempt: next_attempt,
        });
    }

    /// Append new files to the queue (e.g. from a mid-run drag-and-drop).
    /// Returns the number of items added. No-op (returns 0) if the queue is closed.
    pub fn push_files(&self, files: Vec<ScannedFile>) -> usize {
        if self.closed.load(Ordering::Acquire) {
            return 0;
        }
        let added = files.len();
        if added == 0 {
            return 0;
        }
        {
            let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
            for file in files {
                items.push_back(QueueItem { file, attempt: 1 });
            }
        }
        self.total.fetch_add(added, Ordering::Relaxed);
        self.touch();
        added
    }

    /// Mark the queue as closed: workers will exit once it drains.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// Whether the queue has been closed to new items.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Update the last-activity timestamp. Called on push and on job completion.
    pub fn touch(&self) {
        *self.last_activity.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    /// Time since the last activity (push or job completion).
    pub fn idle_since(&self) -> std::time::Duration {
        self.last_activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .elapsed()
    }

    /// Number of items remaining in the queue.
    pub fn remaining(&self) -> usize {
        self.items.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.items
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    /// Total number of files ever queued (grows as items are pushed).
    pub fn total(&self) -> usize {
        self.total.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_file(name: &str, size: u64) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(format!("/music/{name}")),
            name: name.to_string(),
            size,
        }
    }

    #[test]
    fn test_queue_ordering() {
        let files = vec![make_file("a.flac", 100), make_file("b.flac", 50)];
        let queue = JobQueue::new(files);
        let first = queue.dequeue().unwrap();
        assert_eq!(first.file.name, "a.flac");
        let second = queue.dequeue().unwrap();
        assert_eq!(second.file.name, "b.flac");
    }

    #[test]
    fn test_queue_empty() {
        let queue = JobQueue::new(vec![]);
        assert!(queue.is_empty());
        assert!(queue.dequeue().is_none());
    }

    #[test]
    fn test_requeue_for_retry() {
        let files = vec![make_file("a.flac", 100)];
        let queue = JobQueue::new(files);
        let item = queue.dequeue().unwrap();
        assert_eq!(item.attempt, 1);
        queue.requeue_for_retry(item, 2);
        let retried = queue.dequeue().unwrap();
        assert_eq!(retried.attempt, 2);
    }

    #[test]
    fn test_queue_remaining() {
        let files = vec![
            make_file("a.flac", 100),
            make_file("b.flac", 50),
            make_file("c.flac", 25),
        ];
        let queue = JobQueue::new(files);
        assert_eq!(queue.remaining(), 3);
        assert_eq!(queue.total(), 3);
        let _ = queue.dequeue();
        assert_eq!(queue.remaining(), 2);
    }

    #[test]
    fn test_push_files_grows_queue_and_total() {
        let queue = JobQueue::new(vec![make_file("a.flac", 100)]);
        assert_eq!(queue.total(), 1);
        let added = queue.push_files(vec![make_file("b.flac", 50), make_file("c.flac", 25)]);
        assert_eq!(added, 2);
        assert_eq!(queue.total(), 3);
        assert_eq!(queue.remaining(), 3);
    }

    #[test]
    fn test_push_files_appends_to_back() {
        let queue = JobQueue::new(vec![make_file("a.flac", 100)]);
        queue.push_files(vec![make_file("b.flac", 50)]);
        let first = queue.dequeue().unwrap();
        assert_eq!(first.file.name, "a.flac");
        let second = queue.dequeue().unwrap();
        assert_eq!(second.file.name, "b.flac");
    }

    #[test]
    fn test_close_rejects_pushes() {
        let queue = JobQueue::new(vec![]);
        assert!(!queue.is_closed());
        queue.close();
        assert!(queue.is_closed());
        let added = queue.push_files(vec![make_file("a.flac", 100)]);
        assert_eq!(added, 0);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_idle_since_increases() {
        let queue = JobQueue::new(vec![]);
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(queue.idle_since() >= std::time::Duration::from_millis(10));
        queue.touch();
        assert!(queue.idle_since() < std::time::Duration::from_millis(10));
    }
}
