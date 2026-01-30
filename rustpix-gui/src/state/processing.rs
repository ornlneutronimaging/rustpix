//! Processing state for background operations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Tracks the state of background loading and processing operations.
pub struct ProcessingState {
    /// Whether a file is currently being loaded.
    pub is_loading: bool,
    /// Whether clustering is currently in progress.
    pub is_processing: bool,
    /// Progress value from 0.0 to 1.0.
    pub progress: f32,
    /// User-facing status message.
    pub status_text: String,
    /// Shared cancellation flag for background workers.
    pub cancel_flag: Arc<AtomicBool>,
}

impl Default for ProcessingState {
    fn default() -> Self {
        Self {
            is_loading: false,
            is_processing: false,
            progress: 0.0,
            status_text: "Ready".to_string(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ProcessingState {
    /// Request cancellation of the current operation.
    pub fn request_cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    /// Check if cancellation was requested.
    #[must_use]
    #[allow(dead_code)] // Will be used by loader worker
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }

    /// Reset the cancellation flag for a new operation.
    pub fn reset_cancel(&self) {
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    /// Get a clone of the cancel flag for passing to workers.
    #[must_use]
    #[allow(dead_code)] // Will be used when passing to loader worker
    pub fn cancel_flag_clone(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_flag)
    }
}
