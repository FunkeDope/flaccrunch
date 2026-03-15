use crate::fs::scanner::ScannedFile;
use std::collections::VecDeque;
use std::sync::Mutex;

/// An item in the processing queue.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub file: ScannedFile,
    pub attempt: u32,
}

/// Thread-safe job queue for distributing work to workers.
pub struct JobQueue {
    items: Mutex<VecDeque<QueueItem>>,
    total: usize,
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
            total,
        }
    }

    /// Dequeue the next item for processing. Returns None if queue is empty.
    pub fn dequeue(&self) -> Option<QueueItem> {
        self.items.lock().unwrap().pop_front()
    }

    /// Re-add an item for retry with incremented attempt count.
    pub fn requeue_for_retry(&self, item: QueueItem, next_attempt: u32) {
        let mut items = self.items.lock().unwrap();
        items.push_back(QueueItem {
            file: item.file,
            attempt: next_attempt,
        });
    }

    /// Number of items remaining in the queue.
    pub fn remaining(&self) -> usize {
        self.items.lock().unwrap().len()
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.items.lock().unwrap().is_empty()
    }

    /// Total number of files initially queued.
    pub fn total(&self) -> usize {
        self.total
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
}
