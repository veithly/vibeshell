//! Shared output replay buffer used by both SSH and local shell sessions.
//!
//! Keeps the most recent `OUTPUT_REPLAY_LIMIT_BYTES` of terminal output so that
//! a frontend terminal re-attaching to a session (e.g. after a pane is
//! re-created) can replay recent history instead of showing a blank screen.

use std::collections::VecDeque;

const OUTPUT_REPLAY_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Default)]
pub struct OutputReplayBuffer {
    chunks: VecDeque<Vec<u8>>,
    total_bytes: usize,
}

impl OutputReplayBuffer {
    pub fn push(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        let stored = if data.len() > OUTPUT_REPLAY_LIMIT_BYTES {
            data[data.len() - OUTPUT_REPLAY_LIMIT_BYTES..].to_vec()
        } else {
            data.to_vec()
        };

        self.total_bytes += stored.len();
        self.chunks.push_back(stored);

        while self.total_bytes > OUTPUT_REPLAY_LIMIT_BYTES {
            if let Some(removed) = self.chunks.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(removed.len());
            } else {
                self.total_bytes = 0;
                break;
            }
        }
    }

    pub fn snapshot(&self) -> Vec<Vec<u8>> {
        self.chunks.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_returns_pushed_chunks() {
        let mut buf = OutputReplayBuffer::default();
        buf.push(b"hello ".to_vec().as_slice());
        buf.push(b"world".to_vec().as_slice());
        let snap = buf.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0], b"hello ".to_vec());
        assert_eq!(snap[1], b"world".to_vec());
    }

    #[test]
    fn evicts_oldest_chunks_beyond_limit() {
        let mut buf = OutputReplayBuffer::default();
        // Push 3 x 32 KiB = 96 KiB, exceeding the 64 KiB limit.
        buf.push(&vec![0u8; 32 * 1024]);
        buf.push(&vec![1u8; 32 * 1024]);
        buf.push(&vec![2u8; 32 * 1024]);

        let snap = buf.snapshot();
        // Only the last two chunks (64 KiB) should remain.
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0], vec![1u8; 32 * 1024]);
        assert_eq!(snap[1], vec![2u8; 32 * 1024]);
    }

    #[test]
    fn truncates_oversized_single_chunk() {
        let mut buf = OutputReplayBuffer::default();
        let big = vec![7u8; 100 * 1024];
        buf.push(&big);
        let snap = buf.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].len(), 64 * 1024);
    }

    #[test]
    fn ignores_empty_push() {
        let mut buf = OutputReplayBuffer::default();
        buf.push(&[]);
        assert!(buf.snapshot().is_empty());
    }
}
