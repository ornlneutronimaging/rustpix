//! Efficient time-ordering of hits using a K-way merge strategy.
//!
//! # Problem
//! TPX3 data comes in "sections" (chunks) per chip. Packets within a chip are
//! roughly ordered by time (TDC packets delineate pulses, pixels are between TDCs).
//! However, sections from different chips can be interleaved arbitrarily.
//!
//! # Solution
//! We use a "Pulse-Based K-Way Merge":
//! 1. Create a `PulseReader` for each chip that reads section-by-section but
//!    yields "Pulse Batches" (all hits belonging to one TDC period).
//! 2. `PulseReader` sorts hits *within* the pulse (small, fast sort).
//!    It uses a 1-pulse lookahead buffer to correctly attribute "late hits"
//!    (hits arriving after TDC boundary but belonging to previous pulse).
//! 3. `TimeOrderedStream` uses a Min-Heap to merge these pulse batches based on
//!    their TDC timestamp.

use crate::hit::{calculate_tof, correct_timestamp_rollover};
use crate::packet::Tpx3Packet;
use crate::section::Tpx3Section;
use crate::DetectorConfig;
use rustpix_core::soa::HitBatch;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::collections::VecDeque;
use std::sync::Arc;

/// A batch of hits belonging to a single pulse (TDC period) from one chip.
#[derive(Debug, Clone)]
pub struct PulseBatch {
    /// Chip identifier for this pulse batch.
    pub chip_id: u8,
    /// Raw TDC timestamp for the pulse (25ns ticks).
    pub tdc_timestamp: u32,
    /// TDC rollover epoch for the pulse.
    pub tdc_epoch: u64,
    /// Hit batch belonging to this pulse.
    pub hits: HitBatch,
}

impl PulseBatch {
    /// Extended TDC timestamp that includes the rollover epoch.
    #[inline]
    #[must_use]
    pub fn extended_tdc(&self) -> u64 {
        (self.tdc_epoch << 30) | u64::from(self.tdc_timestamp)
    }
}

/// A merged pulse batch across chips with the same TDC timestamp.
#[derive(Debug, Clone)]
pub struct MergedPulseBatch {
    /// Extended TDC timestamp shared by merged chips.
    pub tdc_timestamp: u64,
    /// Merged hits from all chips for this pulse.
    pub hits: HitBatch,
}

// Order by TDC timestamp (reverse for Min-Heap)
impl PartialEq for PulseBatch {
    fn eq(&self, other: &Self) -> bool {
        self.extended_tdc() == other.extended_tdc() && self.chip_id == other.chip_id
    }
}

impl Eq for PulseBatch {}

impl PartialOrd for PulseBatch {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PulseBatch {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for Min-Heap (smallest timestamp first)
        other
            .extended_tdc()
            .cmp(&self.extended_tdc())
            .then_with(|| other.chip_id.cmp(&self.chip_id))
    }
}

/// Reads a stream of sections for a single chip and yields sorted `PulseBatch` values.
///
/// Implements a 1-pulse lookahead to handle "late hits" and independent timestamp rollovers.
pub struct PulseReader<D>
where
    D: AsRef<[u8]> + Clone,
{
    data: D,
    sections: Vec<Tpx3Section>,
    section_idx: usize,
    packet_idx: usize,

    // State for lookahead buffering
    prev_batch: Option<PulseBatch>,
    curr_tdc: Option<u32>,
    curr_batch: HitBatch,
    tdc_epoch: u64,
    last_tdc: Option<u32>,

    // Ready batches to be yielded
    ready_queue: VecDeque<PulseBatch>,

    tdc_correction: u32,
    chip_transform: Arc<dyn Fn(u8, u16, u16) -> (u16, u16) + Send + Sync + 'static>,
}

impl<D> PulseReader<D>
where
    D: AsRef<[u8]> + Clone,
{
    /// Create a pulse reader for a single chip's section stream.
    pub fn new(
        data: D,
        sections: &[Tpx3Section],
        tdc_correction: u32,
        chip_transform: impl Fn(u8, u16, u16) -> (u16, u16) + Send + Sync + 'static,
    ) -> Self {
        let owned_sections = sections.to_vec();

        // Initialize state
        let initial_tdc = owned_sections.first().and_then(|s| s.initial_tdc);

        Self {
            data,
            sections: owned_sections,
            section_idx: 0,
            packet_idx: 0,
            prev_batch: None,
            curr_tdc: initial_tdc,
            curr_batch: HitBatch::with_capacity(4096),
            ready_queue: VecDeque::new(),
            tdc_epoch: 0,
            last_tdc: initial_tdc,
            tdc_correction,
            chip_transform: Arc::new(chip_transform),
        }
    }

    /// Return the next pulse batch from this chip, if available.
    pub fn next_pulse(&mut self) -> Option<PulseBatch> {
        const PACKET_SIZE: usize = 8;

        // If we have ready batches from a previous parsing step, return them.
        if let Some(batch) = self.ready_queue.pop_front() {
            return Some(batch);
        }

        let data = self.data.as_ref();
        while self.section_idx < self.sections.len() {
            let section = &self.sections[self.section_idx];
            let section_data = &data[section.start_offset..section.end_offset];
            let num_packets = section_data.len() / PACKET_SIZE;

            while self.packet_idx < num_packets {
                let offset = self.packet_idx * PACKET_SIZE;
                let Some(packet_bytes) = section_data.get(offset..offset + PACKET_SIZE) else {
                    break;
                };
                let mut bytes = [0u8; PACKET_SIZE];
                bytes.copy_from_slice(packet_bytes);
                let raw = u64::from_le_bytes(bytes);
                let packet = Tpx3Packet::new(raw);
                self.packet_idx += 1;

                if packet.is_tdc() {
                    let new_tdc = packet.tdc_timestamp();
                    let rollover = self.last_tdc.is_some_and(|last| new_tdc < last);

                    // TDC marks the start of a new pulse (or end of previous).

                    if let Some(old_tdc) = self.curr_tdc {
                        // We are finishing `curr_tdc` (Pulse N) and starting `new_tdc` (Pulse N+1).
                        // Move `curr` to `prev`.
                        // If we already had `prev` (Pulse N-1), it is strictly sealed now.

                        // 1. Seal and emit `prev_batch` if exists
                        if let Some(mut prev) = self.prev_batch.take() {
                            prev.hits.sort_by_tof();
                            self.ready_queue.push_back(prev);
                        }

                        // 2. Promote `curr` to `prev`
                        let batch = PulseBatch {
                            chip_id: section.chip_id, // Approximation: assumes pulse doesn't cross chips differently
                            tdc_timestamp: old_tdc,
                            tdc_epoch: self.tdc_epoch,
                            hits: std::mem::take(&mut self.curr_batch),
                        };
                        self.prev_batch = Some(batch);
                    }

                    // 3. Start new `curr`
                    if rollover {
                        self.tdc_epoch += 1;
                    }
                    self.curr_tdc = Some(new_tdc);
                    self.last_tdc = Some(new_tdc);

                    // If we have items in ready_queue, return immediately.
                    // This pauses parsing, preserving state.
                    if !self.ready_queue.is_empty() {
                        return self.ready_queue.pop_front();
                    }
                } else if packet.is_hit() {
                    // Decide where this hit belongs.
                    // Candidates:
                    // 1. `curr_tdc` (the active pulse we just saw start)
                    // 2. `prev_batch` (the pulse before that, for late hits)

                    let (local_x, local_y) = packet.pixel_coordinates();
                    let (gx, gy) = (self.chip_transform)(section.chip_id, local_x, local_y);
                    let raw_ts = packet.timestamp_coarse();
                    let tot = packet.tot();
                    let chip = section.chip_id;

                    // Logic to assign hit
                    let mut assigned_to_prev = false;

                    if let Some(ref mut prev) = self.prev_batch {
                        // Check fit with prev
                        let prev_tdc = prev.tdc_timestamp;
                        let ts_prev = correct_timestamp_rollover(raw_ts, prev_tdc);

                        // If we assume a pulse is ~16ms (666666 units).
                        // Late margin ~14ms.
                        // If `ts_prev - prev_tdc` is reasonable (< 1.5 * period?),
                        // AND it fits better than current?

                        // Simple check from empirical findings:
                        // "A hit arriving AFTER TDC1 with TOF > 14ms ... likely belongs to TDC0".
                        // Note: The hit arrives after TDC1 (which started `curr`).
                        // So checking against `curr` would yield a very small TOF (or wrapped large if hit < tdc).
                        // Checking against `prev` yields a large TOF (~16ms).

                        // Wait, if hit < curr_tdc, it DEFINITELY belongs to prev (or earlier).
                        // If hit >= curr_tdc, but only by a little?
                        // Causality: Hit cannot happen before it is read out.
                        // But timestamps reflect physical event time.

                        // Rule:
                        // 1. Calculate TS relative to Prev.
                        let _tof_prev = ts_prev.wrapping_sub(prev_tdc);

                        // 2. If valid late hit:
                        //    valid if tof_prev < tdc_correction + margin?
                        //    Actually, tdc_correction is "period".
                        //    Let's allow late hits up to 2 * period for safety?
                        //    Or just check if it is "before" current pulse start?

                        if let Some(curr_tdc) = self.curr_tdc {
                            let ts_curr = correct_timestamp_rollover(raw_ts, curr_tdc);
                            // If timestamp is strictly before current pulse start
                            // (allow for some jitter/rollover logic)
                            if ts_curr < curr_tdc {
                                // Definitely prev
                                // Calculate correct TOF relative to prev
                                let tof = calculate_tof(ts_prev, prev_tdc, self.tdc_correction);
                                prev.hits.push((gx, gy, tof, tot, ts_prev, chip));
                                assigned_to_prev = true;
                            } else {
                                // It is >= curr_tdc.
                                // Does it belong to prev anyway? (Very late hit?)
                                // "TOF > 14ms" means event happened 14ms after Prev TDC.
                                // Current TDC is at 16.6ms.
                                // So hit is at Prev + 14ms.
                                // Current is at Prev + 16.6ms.
                                // Hit is BEFORE Current.
                                // So `ts_curr < curr_tdc` covers this!

                                // Is there a case where Hit > Curr but belongs to Prev? NO.
                                // That would mean Hit happened AFTER Curr start, meaning it belongs to Curr (or Next).

                                // So the simple check `ts_curr < curr_tdc` correctly identifies late hits
                                // provided `correct_timestamp_rollover` works.
                            }
                        }
                    }

                    if !assigned_to_prev {
                        // Assign to current
                        if let Some(curr_tdc) = self.curr_tdc {
                            let ts_curr = correct_timestamp_rollover(raw_ts, curr_tdc);
                            let tof = calculate_tof(ts_curr, curr_tdc, self.tdc_correction);
                            self.curr_batch.push((gx, gy, tof, tot, ts_curr, chip));
                        }
                    }
                }
            }

            self.section_idx += 1;
            self.packet_idx = 0;
        }

        // End of stream. Flush.
        if let Some(mut prev) = self.prev_batch.take() {
            prev.hits.sort_by_tof();
            self.ready_queue.push_back(prev);
        }

        let last_chip = self.sections.last().map_or(0, |s| s.chip_id);

        if let Some(curr_tdc) = self.curr_tdc.take() {
            if !self.curr_batch.is_empty() {
                self.curr_batch.sort_by_tof();
                self.ready_queue.push_back(PulseBatch {
                    chip_id: last_chip,
                    tdc_timestamp: curr_tdc,
                    tdc_epoch: self.tdc_epoch,
                    hits: std::mem::take(&mut self.curr_batch),
                });
            }
        }

        self.ready_queue.pop_front()
    }
}

/// Iterator that yields time-ordered hits from multiple chips.
pub struct TimeOrderedStream<D>
where
    D: AsRef<[u8]> + Clone,
{
    readers: Vec<PulseReader<D>>,
    heap: BinaryHeap<PulseBatch>,
}

impl<D> TimeOrderedStream<D>
where
    D: AsRef<[u8]> + Clone,
{
    /// Construct a time-ordered stream from per-chip sections.
    pub fn new(data: D, sections: &[Tpx3Section], config: &DetectorConfig) -> Self {
        // Group sections by chip
        let max_chip = sections.iter().map(|s| s.chip_id).max().unwrap_or(0);
        let mut sections_by_chip: Vec<Vec<Tpx3Section>> = vec![Vec::new(); (max_chip + 1) as usize];

        for section in sections {
            sections_by_chip[section.chip_id as usize].push(section.clone());
        }

        let tdc_correction = config.tdc_correction_25ns();
        let mut readers = Vec::new();
        let mut heap = BinaryHeap::new();

        for (chip_id, chip_sections) in sections_by_chip.into_iter().enumerate() {
            if chip_sections.is_empty() {
                continue;
            }

            let transform = config
                .chip_transforms
                .get(chip_id)
                .cloned()
                .unwrap_or_else(crate::ChipTransform::identity);

            let transform_closure = move |_cid, x, y| transform.apply(x, y);

            let mut reader = PulseReader::new(
                data.clone(),
                &chip_sections,
                tdc_correction,
                transform_closure,
            );

            if let Some(batch) = reader.next_pulse() {
                heap.push(batch);
            }

            readers.push(reader);
        }

        Self { readers, heap }
    }

    /// Returns the next merged pulse batch with its TDC timestamp.
    pub fn next_pulse_batch(&mut self) -> Option<MergedPulseBatch> {
        loop {
            if let Some(head) = self.heap.peek() {
                let min_tdc = head.extended_tdc();
                let mut merged_batch = HitBatch::default();

                while let Some(batch) = self.heap.peek() {
                    if batch.extended_tdc() == min_tdc {
                        let Some(batch) = self.heap.pop() else {
                            break;
                        };

                        // Replenish from the corresponding reader
                        if let Some(reader) = self
                            .readers
                            .iter_mut()
                            .find(|r| reader_chip_id(r) == batch.chip_id)
                        {
                            if let Some(next) = reader.next_pulse() {
                                self.heap.push(next);
                            }
                        }

                        merged_batch.append(&batch.hits);
                    } else {
                        break;
                    }
                }

                if merged_batch.is_empty() {
                    continue;
                }

                merged_batch.sort_by_tof();
                return Some(MergedPulseBatch {
                    tdc_timestamp: min_tdc,
                    hits: merged_batch,
                });
            }
            return None;
        }
    }
}

impl<D> Iterator for TimeOrderedStream<D>
where
    D: AsRef<[u8]> + Clone,
{
    type Item = HitBatch;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_pulse_batch().map(|batch| batch.hits)
    }
}

fn reader_chip_id<D>(reader: &PulseReader<D>) -> u8
where
    D: AsRef<[u8]> + Clone,
{
    reader.sections.first().map_or(0, |s| s.chip_id)
}
