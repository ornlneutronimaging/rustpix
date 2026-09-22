//! Processing state for background operations.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How much progress history the ETA estimate looks back over. Long enough
/// to smooth the bursty per-chunk progress messages the workers send, short
/// enough to adapt when a load moves between phases with different speeds
/// (e.g. TPX3 section scanning vs. hit processing).
const ETA_WINDOW: Duration = Duration::from_secs(10);

/// Minimum history span before an estimate is shown. Below this the rate is
/// dominated by startup noise and the estimate would jump around.
const ETA_MIN_SPAN: Duration = Duration::from_secs(1);

/// Estimates time remaining from (time, progress) samples.
///
/// The rate is measured across a sliding window of recent samples, so the
/// estimate tracks the current phase of the operation instead of averaging
/// over everything since the start.
#[derive(Default)]
pub struct EtaEstimator {
    samples: VecDeque<(Instant, f32)>,
}

impl EtaEstimator {
    /// Record a progress sample (0.0 to 1.0) observed at `now`.
    pub fn update(&mut self, now: Instant, progress: f32) {
        // A progress reset (new phase reporting a lower value) invalidates
        // the accumulated rate.
        if self
            .samples
            .back()
            .is_some_and(|&(_, p)| progress < p - 0.01)
        {
            self.samples.clear();
        }
        self.samples.push_back((now, progress));
        while self
            .samples
            .front()
            .is_some_and(|&(t, _)| now.duration_since(t) > ETA_WINDOW)
            && self.samples.len() > 2
        {
            self.samples.pop_front();
        }
    }

    /// Drop all samples (call when an operation starts, finishes, or fails).
    pub fn reset(&mut self) {
        self.samples.clear();
    }

    /// Estimated time remaining as of `now`, or `None` while there is not
    /// yet enough signal for a stable estimate.
    #[must_use]
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        let &(t_old, p_old) = self.samples.front()?;
        let &(t_new, p_new) = self.samples.back()?;
        let span = t_new.duration_since(t_old);
        let dp = f64::from(p_new - p_old);
        if span < ETA_MIN_SPAN || dp <= 1e-4 {
            return None;
        }
        let rate = dp / span.as_secs_f64();
        let left = f64::from((1.0 - p_new).max(0.0)) / rate;
        // Count down between progress messages instead of freezing.
        let left = left - now.duration_since(t_new).as_secs_f64();
        if !left.is_finite() || left > 99.0 * 3600.0 {
            return None;
        }
        Some(Duration::from_secs_f64(left.max(0.0)))
    }
}

/// Format a time-remaining estimate for the progress bar ("~12s left").
///
/// Estimates are rounded hard on purpose — per-second precision on a
/// minutes-long load would suggest an accuracy the estimate does not have.
#[must_use]
pub fn format_eta(remaining: Duration) -> String {
    let secs = remaining.as_secs();
    if secs < 60 {
        // Round up so the display never shows "~0s left" while still busy.
        format!("~{}s left", secs.max(1))
    } else if secs < 3600 {
        let (m, s) = (secs / 60, secs % 60);
        // 10-second granularity keeps the countdown calm.
        format!("~{m}m {:02}s left", s / 10 * 10)
    } else {
        format!("~{}h {:02}m left", secs / 3600, (secs % 3600) / 60)
    }
}

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
    /// Time-remaining estimate for the current operation.
    pub eta: EtaEstimator,
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
            eta: EtaEstimator::default(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ProcessingState {
    /// Request cancellation of the current operation.
    pub fn request_cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    /// Reset the cancellation flag for a new operation.
    pub fn reset_cancel(&self) {
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    /// Get a clone of the cancel flag for passing to workers.
    #[must_use]
    pub fn cancel_flag_clone(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_flag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, secs: f64) -> Instant {
        base + Duration::from_secs_f64(secs)
    }

    #[test]
    fn no_estimate_without_enough_history() {
        let base = Instant::now();
        let mut eta = EtaEstimator::default();
        assert!(eta.remaining(base).is_none());
        eta.update(base, 0.1);
        assert!(eta.remaining(base).is_none(), "single sample: no rate");
        eta.update(at(base, 0.5), 0.2);
        assert!(
            eta.remaining(at(base, 0.5)).is_none(),
            "less than ETA_MIN_SPAN of history"
        );
    }

    #[test]
    fn steady_rate_extrapolates_linearly() {
        let base = Instant::now();
        let mut eta = EtaEstimator::default();
        // 10%/s: at t=2 s we are at 20%, so 80% ≈ 8 s remain.
        eta.update(base, 0.0);
        eta.update(at(base, 1.0), 0.1);
        eta.update(at(base, 2.0), 0.2);
        let left = eta.remaining(at(base, 2.0)).expect("estimate available");
        assert!((left.as_secs_f64() - 8.0).abs() < 0.1, "got {left:?}");
    }

    #[test]
    fn counts_down_between_messages() {
        let base = Instant::now();
        let mut eta = EtaEstimator::default();
        eta.update(base, 0.0);
        eta.update(at(base, 2.0), 0.2);
        let at_msg = eta.remaining(at(base, 2.0)).unwrap();
        let later = eta.remaining(at(base, 5.0)).unwrap();
        assert!(
            (at_msg.as_secs_f64() - later.as_secs_f64() - 3.0).abs() < 0.1,
            "estimate should fall by the elapsed 3 s: {at_msg:?} -> {later:?}"
        );
    }

    #[test]
    fn stalled_progress_gives_no_estimate() {
        let base = Instant::now();
        let mut eta = EtaEstimator::default();
        eta.update(base, 0.5);
        eta.update(at(base, 5.0), 0.5);
        assert!(eta.remaining(at(base, 5.0)).is_none());
    }

    #[test]
    fn window_tracks_current_phase_not_total_average() {
        let base = Instant::now();
        let mut eta = EtaEstimator::default();
        // Slow first phase: 1%/s for 20 s — but only the windowed samples
        // (last 10 s) should matter once the fast phase starts.
        for i in 0u8..=20 {
            eta.update(at(base, f64::from(i)), 0.01 * f32::from(i));
        }
        // Fast phase: 5%/s for 10 s.
        for i in 1u8..=10 {
            let t = 20.0 + f64::from(i);
            eta.update(at(base, t), 0.2 + 0.05 * f32::from(i));
        }
        // At t=30 s progress is 0.7 and the window only saw the 5%/s phase,
        // so remaining ≈ 0.3 / 0.05 = 6 s (not the ~13 s a whole-run
        // average would give).
        let left = eta.remaining(at(base, 30.0)).unwrap().as_secs_f64();
        assert!((left - 6.0).abs() < 0.5, "got {left}");
    }

    #[test]
    fn progress_reset_clears_stale_rate() {
        let base = Instant::now();
        let mut eta = EtaEstimator::default();
        eta.update(base, 0.8);
        eta.update(at(base, 2.0), 0.9);
        // New operation phase restarts at a lower value.
        eta.update(at(base, 3.0), 0.1);
        assert!(
            eta.remaining(at(base, 3.0)).is_none(),
            "history from before the reset must not leak into the estimate"
        );
    }

    #[test]
    fn eta_formatting() {
        assert_eq!(format_eta(Duration::from_secs(0)), "~1s left");
        assert_eq!(format_eta(Duration::from_secs(45)), "~45s left");
        assert_eq!(format_eta(Duration::from_secs(135)), "~2m 10s left");
        assert_eq!(format_eta(Duration::from_secs(3700)), "~1h 01m left");
    }
}
