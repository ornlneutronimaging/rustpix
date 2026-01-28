//! Out-of-core batching utilities for pulse-ordered TPX3 streams.

use crate::reader::{EventBatch, TimeOrderedEventStream, Tpx3FileReader};
use crate::{Error, Result};
use rustpix_core::soa::HitBatch;
use std::collections::VecDeque;
use std::mem::size_of;
use sysinfo::System;

const MEMORY_OVERHEAD_FACTOR: f64 = 1.2;

/// Configuration for out-of-core batching.
#[derive(Clone, Debug)]
pub struct OutOfCoreConfig {
    /// Fraction of available system memory to target (0.0 < fraction <= 1.0).
    pub memory_fraction: f64,
    /// Explicit memory budget override (bytes). If set, `memory_fraction` is ignored.
    pub memory_budget_bytes: Option<usize>,
}

impl Default for OutOfCoreConfig {
    fn default() -> Self {
        Self {
            memory_fraction: 0.5,
            memory_budget_bytes: None,
        }
    }
}

impl OutOfCoreConfig {
    #[must_use]
    pub fn with_memory_fraction(mut self, fraction: f64) -> Self {
        self.memory_fraction = fraction;
        self
    }

    #[must_use]
    pub fn with_memory_budget_bytes(mut self, bytes: usize) -> Self {
        self.memory_budget_bytes = Some(bytes);
        self
    }

    /// Resolve the target memory budget in bytes.
    ///
    /// # Errors
    /// Returns an error if the memory fraction is invalid or system memory cannot be queried.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    pub fn resolve_budget_bytes(&self) -> Result<usize> {
        if let Some(bytes) = self.memory_budget_bytes {
            return Ok(bytes);
        }
        if !(0.0 < self.memory_fraction && self.memory_fraction <= 1.0) {
            return Err(Error::InvalidFormat(
                "memory_fraction must be in (0.0, 1.0]".to_string(),
            ));
        }
        let mut system = System::new();
        system.refresh_memory();
        let available = system.available_memory();
        if available == 0 {
            return Err(Error::InvalidFormat(
                "available system memory reported as 0".to_string(),
            ));
        }
        let budget = (available as f64 * self.memory_fraction).floor() as u64;
        Ok(usize::try_from(budget).unwrap_or(usize::MAX))
    }
}

/// A pulse slice with an emission cutoff for overlap handling.
#[derive(Clone, Debug)]
pub struct PulseSlice {
    pub tdc_timestamp_25ns: u64,
    pub hits: HitBatch,
    /// Emit only clusters/neutrons with representative TOF <= cutoff.
    pub emit_cutoff_tof: u32,
}

impl PulseSlice {
    #[must_use]
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }
}

/// A bounded batch of pulse slices.
#[derive(Clone, Debug, Default)]
pub struct PulseBatchGroup {
    pub slices: Vec<PulseSlice>,
    pub estimated_bytes: usize,
}

impl PulseBatchGroup {
    #[must_use]
    pub fn len(&self) -> usize {
        self.slices.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slices.is_empty()
    }

    #[must_use]
    pub fn total_hits(&self) -> usize {
        self.slices.iter().map(PulseSlice::len).sum()
    }
}

/// Batcher that groups pulse slices into bounded-memory batches.
pub struct PulseBatcher<I>
where
    I: Iterator<Item = EventBatch>,
{
    source: I,
    queue: VecDeque<PulseSlice>,
    max_hits: usize,
    overlap_tof: u32,
    bytes_per_hit: usize,
}

impl<I> PulseBatcher<I>
where
    I: Iterator<Item = EventBatch>,
{
    /// Create a new batcher from a pulse-ordered event stream.
    ///
    /// `overlap_tof` is in 25ns ticks and is used only when a single pulse must
    /// be split to respect the memory budget.
    ///
    /// # Errors
    /// Returns an error if the memory budget cannot be resolved.
    pub fn new(source: I, config: &OutOfCoreConfig, overlap_tof: u32) -> Result<Self> {
        let bytes_per_hit = bytes_per_hit();
        let budget = config.resolve_budget_bytes()?;
        let max_hits = max_hits_for_budget(budget, bytes_per_hit);
        Ok(Self {
            source,
            queue: VecDeque::new(),
            max_hits,
            overlap_tof,
            bytes_per_hit,
        })
    }

    fn next_group(&mut self) -> Option<PulseBatchGroup> {
        let mut group = PulseBatchGroup::default();
        let mut group_hits = 0usize;

        loop {
            while let Some(slice) = self.queue.front() {
                let slice_hits = slice.len();
                if group_hits == 0 || group_hits.saturating_add(slice_hits) <= self.max_hits {
                    let slice = self.queue.pop_front().expect("queue not empty");
                    group_hits = group_hits.saturating_add(slice_hits);
                    group.slices.push(slice);
                } else {
                    break;
                }
            }

            if !group.is_empty() {
                group.estimated_bytes = estimate_batch_bytes(group_hits, self.bytes_per_hit);
                return Some(group);
            }

            let next = self.source.next()?;
            let slices = split_pulse_with_overlap(next, self.max_hits, self.overlap_tof);
            for slice in slices {
                self.queue.push_back(slice);
            }
        }
    }
}

impl<I> Iterator for PulseBatcher<I>
where
    I: Iterator<Item = EventBatch>,
{
    type Item = PulseBatchGroup;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_group()
    }
}

/// Convenience constructor for a reader-backed batcher.
///
/// # Errors
/// Returns an error if the underlying reader or memory budget fails.
pub fn pulse_batches(
    reader: &Tpx3FileReader,
    config: &OutOfCoreConfig,
    overlap_tof: u32,
) -> Result<PulseBatcher<TimeOrderedEventStream>> {
    let stream = reader.stream_time_ordered_events()?;
    PulseBatcher::new(stream, config, overlap_tof)
}

fn bytes_per_hit() -> usize {
    size_of::<u16>() * 2
        + size_of::<u32>() * 2
        + size_of::<u16>()
        + size_of::<u8>()
        + size_of::<i32>()
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn max_hits_for_budget(budget_bytes: usize, bytes_per_hit: usize) -> usize {
    let per_hit = (bytes_per_hit as f64 * MEMORY_OVERHEAD_FACTOR).ceil() as usize;
    let per_hit = per_hit.max(1);
    (budget_bytes / per_hit).max(1)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn estimate_batch_bytes(hit_count: usize, bytes_per_hit: usize) -> usize {
    let per_hit = bytes_per_hit as f64 * MEMORY_OVERHEAD_FACTOR;
    (hit_count as f64 * per_hit).ceil() as usize
}

fn split_pulse_with_overlap(
    batch: EventBatch,
    max_hits: usize,
    overlap_tof: u32,
) -> Vec<PulseSlice> {
    let hits = batch.hits;
    let total = hits.len();
    if total == 0 {
        return Vec::new();
    }

    if total <= max_hits {
        let cutoff = *hits.tof.last().unwrap_or(&0);
        return vec![PulseSlice {
            tdc_timestamp_25ns: batch.tdc_timestamp_25ns,
            hits,
            emit_cutoff_tof: cutoff,
        }];
    }

    let mut slices = Vec::new();
    let mut start = 0usize;
    while start < total {
        let mut end = (start + max_hits).min(total);
        if end == start {
            end = (start + 1).min(total);
        }

        let cutoff_tof = hits.tof[end - 1];
        while end < total && hits.tof[end] == cutoff_tof {
            end += 1;
        }

        let mut overlap_end = end;
        if overlap_tof > 0 {
            let overlap_limit = cutoff_tof.saturating_add(overlap_tof);
            while overlap_end < total && hits.tof[overlap_end] <= overlap_limit {
                overlap_end += 1;
            }
        }

        let slice = slice_hits(&hits, start, overlap_end);
        slices.push(PulseSlice {
            tdc_timestamp_25ns: batch.tdc_timestamp_25ns,
            hits: slice,
            emit_cutoff_tof: cutoff_tof,
        });

        start = end;
    }

    slices
}

fn slice_hits(batch: &HitBatch, start: usize, end: usize) -> HitBatch {
    let len = end.saturating_sub(start);
    let mut sliced = HitBatch::with_capacity(len);
    sliced.x.extend_from_slice(&batch.x[start..end]);
    sliced.y.extend_from_slice(&batch.y[start..end]);
    sliced.tof.extend_from_slice(&batch.tof[start..end]);
    sliced.tot.extend_from_slice(&batch.tot[start..end]);
    sliced
        .timestamp
        .extend_from_slice(&batch.timestamp[start..end]);
    sliced.chip_id.extend_from_slice(&batch.chip_id[start..end]);
    sliced
        .cluster_id
        .extend_from_slice(&batch.cluster_id[start..end]);
    sliced
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hit_batch(tofs: &[u32]) -> HitBatch {
        let mut batch = HitBatch::with_capacity(tofs.len());
        for (i, &tof) in tofs.iter().enumerate() {
            let x = u16::try_from(i % 512).unwrap_or(0);
            let y = u16::try_from(i / 512).unwrap_or(0);
            batch.push((x, y, tof, 0, tof, 0));
        }
        batch
    }

    #[test]
    fn split_pulse_with_overlap_keeps_order() {
        let tofs: Vec<u32> = (0..10).collect();
        let event = EventBatch {
            tdc_timestamp_25ns: 0,
            hits: make_hit_batch(&tofs),
        };
        let slices = split_pulse_with_overlap(event, 4, 1);
        assert_eq!(slices.len(), 3);
        assert_eq!(slices[0].hits.tof, vec![0, 1, 2, 3, 4]);
        assert_eq!(slices[0].emit_cutoff_tof, 3);
        assert_eq!(slices[1].hits.tof, vec![4, 5, 6, 7, 8]);
        assert_eq!(slices[1].emit_cutoff_tof, 7);
        assert_eq!(slices[2].hits.tof, vec![8, 9]);
        assert_eq!(slices[2].emit_cutoff_tof, 9);
    }

    #[test]
    fn batcher_groups_pulses_under_budget() {
        let bytes = bytes_per_hit() * 4;
        let config = OutOfCoreConfig::default().with_memory_budget_bytes(bytes);
        let overlap_tof = 0;

        let pulses = vec![
            EventBatch {
                tdc_timestamp_25ns: 0,
                hits: make_hit_batch(&[0, 1]),
            },
            EventBatch {
                tdc_timestamp_25ns: 1,
                hits: make_hit_batch(&[0, 1]),
            },
            EventBatch {
                tdc_timestamp_25ns: 2,
                hits: make_hit_batch(&[0, 1]),
            },
        ];

        let mut batcher = PulseBatcher::new(pulses.into_iter(), &config, overlap_tof).unwrap();
        let max_hits = max_hits_for_budget(bytes, bytes_per_hit());
        let mut total_hits = 0usize;
        let mut batch_count = 0usize;
        for batch in &mut batcher {
            assert!(batch.total_hits() <= max_hits);
            total_hits += batch.total_hits();
            batch_count += 1;
        }

        assert_eq!(total_hits, 6);
        assert!(batch_count >= 2);
    }
}
