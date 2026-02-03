//! File loading worker and helper functions.
//!
//! This module handles TPX3 file loading in a background thread,
//! including section scanning, TDC state tracking, and hit processing.

use std::collections::BinaryHeap;
use std::fmt::Write;
use std::path::Path;
use std::sync::mpsc::{sync_channel, Sender};
use std::time::{Duration, Instant};

use rustpix_core::soa::HitBatch;
use rustpix_io::scanner::PacketScanner;
use rustpix_tpx::ordering::{PulseBatch, PulseReader};
use rustpix_tpx::section::{scan_section_tdc, Tpx3Section};
use rustpix_tpx::{ChipTransform, DetectorConfig};

use crate::histogram::Hyperstack3D;
use crate::message::AppMessage;
use crate::util::usize_to_f32;

/// Main entry point for file loading in a background thread.
///
/// Opens a TPX3 file, memory-maps it, scans sections, processes hits,
/// and sends progress/completion messages via the provided channel.
pub fn load_file_worker(
    path: &Path,
    tx: &Sender<AppMessage>,
    detector_config: DetectorConfig,
    n_tof_bins: usize,
    cache_hits: bool,
    cancel_flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let start = Instant::now();
    if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(AppMessage::LoadError(e.to_string()));
            return;
        }
    };

    // SAFETY: The file is opened read-only and we assume it is not modified concurrently.
    #[allow(unsafe_code)]
    let mmap = unsafe {
        match memmap2::Mmap::map(&file) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(AppMessage::LoadError(e.to_string()));
                return;
            }
        }
    };

    let _ = tx.send(AppMessage::LoadProgress(
        0.1,
        "Scanning sections...".to_string(),
    ));

    let io_sections = scan_sections_with_progress(&mmap, tx, cancel_flag.as_ref());
    if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let total_sections = io_sections.len();
    let _ = tx.send(AppMessage::LoadProgress(
        0.15,
        format!("Found {total_sections} sections. Prescanning TDCs..."),
    ));

    let tpx_sections = build_tpx_sections(&mmap, io_sections);

    let det_config = detector_config;
    let tdc_correction = det_config.tdc_correction_25ns();
    let debug_str = build_debug_info(&mmap, &tpx_sections, tdc_correction);

    let _ = tx.send(AppMessage::LoadProgress(
        0.25,
        "Processing hits...".to_string(),
    ));

    let (detector_width, detector_height) = det_config.detector_dimensions();
    let mut hyperstack = Hyperstack3D::new(
        n_tof_bins.max(1),
        detector_width,
        detector_height,
        tdc_correction,
    );
    let (full_batch, pulse_bounds, hit_count) = process_sections_to_batch(
        &mmap,
        &tpx_sections,
        &det_config,
        tx,
        cancel_flag.as_ref(),
        &mut hyperstack,
        cache_hits,
    );
    if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    let _ = tx.send(AppMessage::LoadComplete(
        hit_count,
        full_batch.map(Box::new),
        Box::new(hyperstack),
        start.elapsed(),
        debug_str,
        pulse_bounds,
    ));
}

/// Scan sections in chunks with progress reporting.
///
/// Processes the memory-mapped file in 50MB chunks, scanning for
/// TPX3 section boundaries and reporting progress.
fn scan_sections_with_progress(
    mmap: &memmap2::Mmap,
    tx: &Sender<AppMessage>,
    cancel_flag: &std::sync::atomic::AtomicBool,
) -> Vec<rustpix_io::scanner::Section> {
    let mut io_sections = Vec::new();
    let mut offset = 0;
    let chunk_size = 50 * 1024 * 1024; // 50MB chunks
    let total_bytes = mmap.len().max(1);

    while offset < total_bytes {
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let end = (offset + chunk_size).min(total_bytes);
        let is_eof = end == total_bytes;
        let data = &mmap[offset..end];

        let (sections, consumed) = PacketScanner::scan_sections(data, is_eof);
        for mut section in sections {
            section.start_offset += offset;
            section.end_offset += offset;
            io_sections.push(section);
        }

        offset = offset.saturating_add(consumed);

        let ratio = usize_to_f32(offset) / usize_to_f32(total_bytes);
        let _ = tx.send(AppMessage::LoadProgress(
            0.15 * ratio,
            format!("Scanning sections... {:.0}%", ratio * 100.0),
        ));

        if consumed == 0 && !is_eof {
            // Section may span chunk boundary - advance minimally to find next header
            // rather than skipping the entire chunk which could drop sections
            offset = offset.saturating_add(8); // One TPX3 packet size
        }
    }

    io_sections
}

/// Build TPX3 sections with TDC state tracking.
///
/// Converts I/O sections to TPX3 sections, tracking TDC state per chip
/// to handle timestamp rollover correctly.
fn build_tpx_sections(
    mmap: &memmap2::Mmap,
    io_sections: Vec<rustpix_io::scanner::Section>,
) -> Vec<Tpx3Section> {
    let mut tpx_sections = Vec::with_capacity(io_sections.len());
    let mut chip_tdc_state = [None; 256];

    for section in io_sections {
        let initial = chip_tdc_state[usize::from(section.chip_id)];
        let mut rules = Tpx3Section {
            start_offset: section.start_offset,
            end_offset: section.end_offset,
            chip_id: section.chip_id,
            initial_tdc: initial,
            final_tdc: None,
        };

        if let Some(final_t) = scan_section_tdc(mmap, &rules) {
            rules.final_tdc = Some(final_t);
            chip_tdc_state[usize::from(section.chip_id)] = Some(final_t);
        }

        tpx_sections.push(rules);
    }

    tpx_sections
}

/// Build debug information string for diagnostics.
///
/// Generates a debug string with TDC correction info and sample hit data
/// for diagnostic purposes.
fn build_debug_info(mmap: &memmap2::Mmap, sections: &[Tpx3Section], tdc_correction: u32) -> String {
    let mut debug_str = String::new();
    let _ = writeln!(debug_str, "TDC Correction (25ns): {tdc_correction}");

    if let Some(sec) = sections.iter().find(|s| s.initial_tdc.is_some()) {
        if let Some(tdc) = sec.initial_tdc {
            let _ = writeln!(debug_str, "Sec TDC Ref: {tdc}");
            let sdata = &mmap[sec.start_offset..sec.end_offset];
            let mut found = false;
            for ch in sdata.chunks_exact(8) {
                let raw = u64::from_le_bytes(ch.try_into().unwrap());
                let packet = rustpix_tpx::Tpx3Packet::new(raw);
                if packet.is_hit() {
                    let raw_ts = packet.timestamp_coarse();
                    let ts = rustpix_tpx::correct_timestamp_rollover(raw_ts, tdc);
                    let raw_tof = ts.wrapping_sub(tdc);
                    let tof = rustpix_tpx::calculate_tof(ts, tdc, tdc_correction);
                    let _ = writeln!(
                        debug_str,
                        "Sample Hit:\n  RawTS: {raw_ts}\n  CorrTS: {ts}\n  RawDelta: {raw_tof}\n  CalcTOF: {tof}"
                    );
                    found = true;
                    break;
                }
            }
            if !found {
                let _ = writeln!(debug_str, "Section has no hits.");
            }
        }
    } else {
        let _ = writeln!(debug_str, "No sections with valid Initial TDC found.");
    }

    debug_str
}

/// Process sections into a time-ordered hit batch.
///
/// Uses parallel processing per chip with synchronized merging
/// to produce a globally time-ordered `HitBatch`.
fn process_sections_to_batch(
    mmap: &memmap2::Mmap,
    sections: &[Tpx3Section],
    det_config: &DetectorConfig,
    tx: &Sender<AppMessage>,
    cancel_flag: &std::sync::atomic::AtomicBool,
    hyperstack: &mut Hyperstack3D,
    cache_hits: bool,
) -> (
    Option<HitBatch>,
    Option<Vec<crate::message::PulseBounds>>,
    usize,
) {
    let num_packets: usize = sections.iter().map(Tpx3Section::packet_count).sum();
    let mut full_batch = cache_hits.then(|| HitBatch::with_capacity(num_packets));
    let mut pulse_bounds = cache_hits.then(Vec::new);
    let tdc_correction = det_config.tdc_correction_25ns();

    let max_chip = sections.iter().map(|s| s.chip_id).max().unwrap_or(0) as usize;
    let mut sections_by_chip = vec![Vec::new(); max_chip + 1];
    for section in sections {
        sections_by_chip[section.chip_id as usize].push(section.clone());
    }

    let total_hits = num_packets.max(1);
    let mut processed_hits = 0usize;
    let mut last_update = Instant::now();
    let mut receivers: Vec<Option<std::sync::mpsc::Receiver<PulseBatch>>> =
        Vec::with_capacity(max_chip + 1);
    receivers.resize_with(max_chip + 1, || None);
    let mut heap = BinaryHeap::new();

    std::thread::scope(|scope| {
        for (chip_id, chip_sections) in sections_by_chip.iter().enumerate() {
            if chip_sections.is_empty() {
                continue;
            }

            let (tx_batch, rx_batch) = sync_channel::<PulseBatch>(2);
            receivers[chip_id] = Some(rx_batch);

            let chip_sections = chip_sections.clone();
            let transform = det_config
                .chip_transforms
                .get(chip_id)
                .cloned()
                .unwrap_or_else(ChipTransform::identity);
            scope.spawn(move || {
                let transform_closure = move |_cid, x, y| transform.apply(x, y);
                let mut reader =
                    PulseReader::new(mmap, &chip_sections, tdc_correction, transform_closure);
                while let Some(batch) = reader.next_pulse() {
                    if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    if tx_batch.send(batch).is_err() {
                        break;
                    }
                }
            });
        }

        if !prime_heap(&receivers, &mut heap, cancel_flag) {
            return;
        }

        while let Some(head) = heap.peek() {
            if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let min_tdc = head.extended_tdc();
            let mut merged = HitBatch::default();

            while let Some(batch) = heap.peek() {
                if batch.extended_tdc() != min_tdc {
                    break;
                }
                let batch = heap.pop().expect("heap not empty");

                if let Some(rx) = receivers
                    .get(batch.chip_id as usize)
                    .and_then(|opt| opt.as_ref())
                {
                    if let Some(next) = recv_batch_with_cancel(rx, cancel_flag) {
                        heap.push(next);
                    }
                }

                merged.append(&batch.hits);
            }

            if merged.is_empty() {
                continue;
            }
            if cache_hits {
                merged.sort_by_tof();
                if let Some(full_batch) = full_batch.as_mut() {
                    let start = full_batch.len();
                    full_batch.append(&merged);
                    if let Some(bounds) = pulse_bounds.as_mut() {
                        bounds.push(crate::message::PulseBounds {
                            tdc_timestamp_25ns: min_tdc,
                            start,
                            len: merged.len(),
                        });
                    }
                }
            }
            processed_hits = processed_hits.saturating_add(merged.len());
            hyperstack.accumulate_hits(&merged);

            if last_update.elapsed() > Duration::from_millis(200) {
                let progress =
                    0.25 + 0.75 * (usize_to_f32(processed_hits) / usize_to_f32(total_hits));
                let _ = tx.send(AppMessage::LoadProgress(
                    progress.min(0.99),
                    format!("Processed {processed_hits}/{num_packets} hits..."),
                ));
                last_update = Instant::now();
            }
        }
    });

    (full_batch, pulse_bounds, processed_hits)
}

fn recv_batch_with_cancel(
    rx: &std::sync::mpsc::Receiver<PulseBatch>,
    cancel_flag: &std::sync::atomic::AtomicBool,
) -> Option<PulseBatch> {
    loop {
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(batch) => return Some(batch),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn prime_heap(
    receivers: &[Option<std::sync::mpsc::Receiver<PulseBatch>>],
    heap: &mut BinaryHeap<PulseBatch>,
    cancel_flag: &std::sync::atomic::AtomicBool,
) -> bool {
    for rx_opt in receivers.iter().flatten() {
        match recv_batch_with_cancel(rx_opt, cancel_flag) {
            Some(batch) => heap.push(batch),
            None => {
                if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    return false;
                }
            }
        }
    }
    true
}
