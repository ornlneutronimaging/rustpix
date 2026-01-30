//! Processing state for background operations.

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
}

impl Default for ProcessingState {
    fn default() -> Self {
        Self {
            is_loading: false,
            is_processing: false,
            progress: 0.0,
            status_text: "Ready".to_string(),
        }
    }
}
