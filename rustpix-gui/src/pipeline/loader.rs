//! File loading worker and helper functions.
//!
//! This module handles file loading in a background thread. TPX3 files are
//! memory-mapped and processed via section scanning and TDC state tracking;
//! SNS `NeXus` event files (`*.nxs.h5`) are read in event slices through
//! [`SnsEventReader`].

use std::collections::BinaryHeap;
use std::fmt::Write;
use std::path::Path;
use std::sync::mpsc::{sync_channel, Sender};
use std::time::{Duration, Instant};

use rustpix_core::soa::HitBatch;
use rustpix_io::hdf5_sns::{venus_bank100_config, SnsEventReader, SnsFileMetadata};
use rustpix_io::scanner::PacketScanner;
use rustpix_tpx::ordering::{PulseBatch, PulseReader};
use rustpix_tpx::section::{scan_section_tdc, Tpx3Section};
use rustpix_tpx::{ChipTransform, DetectorConfig};

use crate::histogram::Hyperstack3D;
use crate::message::{AppMessage, PulseBounds};
use crate::util::usize_to_f32;

/// Returns true when the path looks like an SNS `NeXus` HDF5 file
/// (`.h5`, `.hdf5`, or `.nxs` — which covers facility `*.nxs.h5` names).
pub fn is_sns_nexus_path(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        e.eq_ignore_ascii_case("h5")
            || e.eq_ignore_ascii_case("hdf5")
            || e.eq_ignore_ascii_case("nxs")
    })
}

/// Main entry point for file loading in a background thread.
///
/// Dispatches on file type: SNS `NeXus` files are read via `SnsEventReader`;
/// everything else is treated as TPX3 — memory-mapped, section-scanned, and
/// hit-processed. Progress/completion messages are sent via the channel.
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
    if is_sns_nexus_path(path) {
        load_sns_file_worker(
            path,
            tx,
            &detector_config,
            n_tof_bins,
            cache_hits,
            cancel_flag,
            SNS_CHUNK_EVENTS,
        );
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

/// Number of events to read from a `NeXus` bank per slice. At ~28 bytes per
/// decoded event this keeps the transient slice under ~60 MB while still
/// amortizing HDF5 read overhead.
const SNS_CHUNK_EVENTS: u64 = 2_000_000;

/// Load an SNS `NeXus` event file (`*.nxs.h5`) in a background thread.
///
/// Reads VENUS `bank100` in slices, converts events to 25 ns TOF ticks on
/// the gap-removed 512×512 grid, groups them per pulse (sorted by TOF within
/// each pulse, matching the TPX3 path), and sends the same
/// progress/completion messages the TPX3 loader does. `NeXus` events carry no
/// time-over-threshold, so `tot` is 0 for every hit.
///
/// `chunk_events` is the slice size ([`SNS_CHUNK_EVENTS`] in production;
/// tests shrink it to exercise pulses spanning slice boundaries).
#[allow(clippy::too_many_arguments)]
fn load_sns_file_worker(
    path: &Path,
    tx: &Sender<AppMessage>,
    detector_config: &DetectorConfig,
    n_tof_bins: usize,
    cache_hits: bool,
    cancel_flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    chunk_events: u64,
) {
    let start = Instant::now();
    let _ = tx.send(AppMessage::LoadProgress(
        0.01,
        "Opening NeXus file...".to_string(),
    ));

    let reader = match SnsEventReader::open(path) {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AppMessage::LoadError(format!("NeXus open failed: {e}")));
            return;
        }
    };

    let Some((total_events, pulse_count)) = select_sns_bank(&reader, tx) else {
        return;
    };
    let bank_config = venus_bank100_config();
    let metadata = reader.metadata();
    let debug_str = build_sns_debug_info(&metadata, &bank_config.name, total_events, pulse_count);

    let tdc_correction = detector_config.tdc_correction_25ns();
    let width = bank_config.width as usize;
    let height = bank_config.height as usize;
    let mut hyperstack = Hyperstack3D::new(n_tof_bins.max(1), width, height, tdc_correction);

    let capacity = usize::try_from(total_events).unwrap_or(usize::MAX);
    let mut full_batch = cache_hits.then(|| HitBatch::with_capacity(capacity));
    let mut pulse_bounds = cache_hits.then(Vec::new);

    // Events belonging to the pulse currently being assembled. A pulse can
    // span a slice boundary, so this persists across read iterations.
    let mut pulse_hits = HitBatch::default();
    let mut current_pulse_ns: Option<u64> = None;

    let mut offset = 0u64;
    let mut last_update = Instant::now();
    while offset < total_events {
        if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let chunk = match reader.read_events(&bank_config, offset, Some(chunk_events)) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppMessage::LoadError(format!("NeXus read failed: {e}")));
                return;
            }
        };
        if chunk.is_empty() {
            break;
        }

        for i in 0..chunk.len() {
            let pulse_ns = chunk.pulse_time_ns[i];
            if current_pulse_ns != Some(pulse_ns) {
                flush_sns_pulse(
                    &mut pulse_hits,
                    current_pulse_ns,
                    &mut hyperstack,
                    full_batch.as_mut(),
                    pulse_bounds.as_mut(),
                );
                current_pulse_ns = Some(pulse_ns);
            }
            let tof_25 = u32::try_from(chunk.tof_ns[i] / 25).unwrap_or(u32::MAX);
            #[allow(clippy::cast_possible_truncation)]
            let timestamp_25 = ((pulse_ns / 25).wrapping_add(u64::from(tof_25))) as u32;
            pulse_hits.push((chunk.x[i], chunk.y[i], tof_25, 0, timestamp_25, 0));
        }

        offset += chunk.len() as u64;
        if last_update.elapsed() > Duration::from_millis(200) {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let ratio = (offset as f64 / total_events.max(1) as f64) as f32;
            let _ = tx.send(AppMessage::LoadProgress(
                (0.05 + 0.94 * ratio).min(0.99),
                format!("Reading NeXus events... {offset} / {total_events}"),
            ));
            last_update = Instant::now();
        }
    }
    flush_sns_pulse(
        &mut pulse_hits,
        current_pulse_ns,
        &mut hyperstack,
        full_batch.as_mut(),
        pulse_bounds.as_mut(),
    );

    let _ = tx.send(AppMessage::LoadComplete(
        usize::try_from(offset).unwrap_or(usize::MAX),
        full_batch.map(Box::new),
        Box::new(hyperstack),
        start.elapsed(),
        debug_str,
        pulse_bounds,
    ));
}

/// Find `bank100` in the file and return its `(event_count, pulse_count)`.
///
/// Sends a `LoadError` and returns `None` when the bank is missing or empty
/// (VENUS facility ADARA files have empty event banks — the Timepix data
/// lives in `.tpx3` files).
fn select_sns_bank(reader: &SnsEventReader, tx: &Sender<AppMessage>) -> Option<(u64, u64)> {
    let Some(bank) = reader.banks().iter().find(|b| b.name == "bank100") else {
        let found = reader
            .banks()
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = tx.send(AppMessage::LoadError(format!(
            "No bank100 in NeXus file (banks found: [{found}]). \
             Only VENUS bank100 files are supported for now."
        )));
        return None;
    };
    if bank.event_count == 0 {
        let _ = tx.send(AppMessage::LoadError(
            "bank100 is empty. VENUS facility .nxs.h5 files do not contain Timepix \
             events — load the run's .tpx3 file or a rustpix NXsnsevent export instead."
                .to_string(),
        ));
        return None;
    }
    Some((bank.event_count, bank.pulse_count))
}

/// Finish the pulse being assembled: sort its hits by TOF, accumulate them
/// into the hyperstack, and (when caching) append them to the full batch
/// with a matching pulse-bounds entry.
fn flush_sns_pulse(
    pulse_hits: &mut HitBatch,
    pulse_ns: Option<u64>,
    hyperstack: &mut Hyperstack3D,
    full_batch: Option<&mut HitBatch>,
    pulse_bounds: Option<&mut Vec<PulseBounds>>,
) {
    if pulse_hits.is_empty() {
        return;
    }
    pulse_hits.sort_by_tof();
    hyperstack.accumulate_hits(pulse_hits);
    if let Some(batch) = full_batch {
        let start = batch.len();
        let len = pulse_hits.len();
        batch.append(pulse_hits);
        if let (Some(bounds), Some(pulse_ns)) = (pulse_bounds, pulse_ns) {
            bounds.push(PulseBounds {
                tdc_timestamp_25ns: pulse_ns / 25,
                start,
                len,
            });
        }
    }
    pulse_hits.clear();
}

/// Debug summary for a loaded SNS `NeXus` file (shown in the debug panel).
fn build_sns_debug_info(
    metadata: &SnsFileMetadata,
    bank: &str,
    events: u64,
    pulses: u64,
) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "SNS `NeXus` file, bank {bank}: {events} events, {pulses} pulses"
    );
    if let Some(run) = &metadata.run_number {
        let _ = writeln!(s, "Run: {run}");
    }
    if let Some(ipts) = &metadata.experiment_identifier {
        let _ = writeln!(s, "Experiment: {ipts}");
    }
    if let Some(t) = &metadata.start_time {
        let _ = writeln!(s, "Start: {t}");
    }
    if let Some(t) = &metadata.end_time {
        let _ = writeln!(s, "End: {t}");
    }
    if let Some(d) = metadata.duration_s {
        let _ = writeln!(s, "Duration: {d:.1} s");
    }
    if let Some(pc) = metadata.proton_charge_pc {
        let _ = writeln!(s, "Proton charge: {pc:.3e} pC");
    }
    let _ = writeln!(s, "Note: NeXus events have no ToT (stored as 0).");
    s
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
            for packet_bytes in sdata.as_chunks::<8>().0 {
                let raw = u64::from_le_bytes(*packet_bytes);
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
    let total_packets: usize = sections.iter().map(Tpx3Section::packet_count).sum();
    let mut full_batch = cache_hits.then(|| HitBatch::with_capacity(total_packets));
    let mut pulse_bounds = cache_hits.then(Vec::new);
    let tdc_correction = det_config.tdc_correction_25ns();

    let max_chip = sections.iter().map(|s| s.chip_id).max().unwrap_or(0) as usize;
    let mut sections_by_chip = vec![Vec::new(); max_chip + 1];
    for section in sections {
        sections_by_chip[section.chip_id as usize].push(section.clone());
    }

    let progress_denominator = total_packets.max(1);
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
                let progress = 0.25
                    + 0.75 * (usize_to_f32(processed_hits) / usize_to_f32(progress_denominator));
                let _ = tx.send(AppMessage::LoadProgress(
                    progress.min(0.99),
                    format!("Processed {processed_hits} hits..."),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::channel;
    use std::sync::Arc;

    use rustpix_io::hdf5_sns::{SnsEventSink, SnsRunMetadata, SnsWriteOptions};
    use rustpix_io::EventBatch;

    #[test]
    fn sns_nexus_path_detection() {
        assert!(is_sns_nexus_path(&PathBuf::from("VENUS_15159.nxs.h5")));
        assert!(is_sns_nexus_path(&PathBuf::from("export.H5")));
        assert!(is_sns_nexus_path(&PathBuf::from("run.hdf5")));
        assert!(is_sns_nexus_path(&PathBuf::from("run.nxs")));
        assert!(!is_sns_nexus_path(&PathBuf::from("run.tpx3")));
        assert!(!is_sns_nexus_path(&PathBuf::from("no_extension")));
    }

    fn write_sns_test_file(path: &Path) {
        let run = SnsRunMetadata {
            run_number: 99999,
            experiment_identifier: "IPTS-00001".to_string(),
            start_time: "2025-01-01T00:00:00-05:00".to_string(),
            end_time: None,
            duration: None,
            proton_charge: Some(100.0),
            title: Some("gui loader test".to_string()),
        };
        let mut sink = SnsEventSink::create(path, SnsWriteOptions::venus_defaults(run)).unwrap();
        // Two pulses (TDC 1000 and 2000 in 25 ns ticks). Source column 256
        // is a chip-gap pixel and is dropped on write; 300 shifts to 298.
        let make = |tdc: u64, xs: &[u16], ys: &[u16], tofs: &[u32]| {
            let n = xs.len();
            let mut hits = HitBatch::with_capacity(n);
            hits.x.extend_from_slice(xs);
            hits.y.extend_from_slice(ys);
            hits.tof.extend_from_slice(tofs);
            hits.tot.extend_from_slice(&vec![10u16; n]);
            hits.timestamp.extend_from_slice(&vec![0u32; n]);
            hits.chip_id.extend_from_slice(&vec![0u8; n]);
            hits.cluster_id.extend_from_slice(&vec![-1i32; n]);
            EventBatch {
                tdc_timestamp_25ns: tdc,
                hits,
            }
        };
        sink.write_hits(
            0,
            &make(1000, &[5, 300, 256], &[10, 20, 30], &[40, 80, 120]),
        )
        .unwrap();
        sink.write_hits(0, &make(2000, &[513], &[513], &[200]))
            .unwrap();
        sink.finalize().unwrap();
    }

    /// Export a small SNS file, load it back through the worker, and check
    /// that hits, pulse bounds, and the hyperstack all round-trip.
    #[test]
    fn load_sns_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui_roundtrip.nxs.h5");
        write_sns_test_file(&path);

        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        load_file_worker(&path, &tx, DetectorConfig::default(), 10, true, &cancel);
        drop(tx);

        let mut complete = None;
        while let Ok(msg) = rx.recv() {
            match msg {
                AppMessage::LoadError(e) => panic!("load failed: {e}"),
                AppMessage::LoadComplete(count, batch, hyperstack, _, debug, bounds) => {
                    complete = Some((count, batch, hyperstack, debug, bounds));
                }
                _ => {}
            }
        }
        let (count, batch, hyperstack, debug, bounds) = complete.expect("LoadComplete sent");

        // The gap hit at source x=256 is dropped on write, leaving 3 events.
        assert_eq!(count, 3);
        let batch = *batch.expect("hits cached");
        assert_eq!(batch.x, vec![5, 298, 511]);
        assert_eq!(batch.y, vec![10, 20, 511]);
        // TOF round-trips through nanoseconds back to 25 ns ticks.
        assert_eq!(batch.tof, vec![40, 80, 200]);
        // NeXus events carry no time-over-threshold.
        assert_eq!(batch.tot, vec![0, 0, 0]);

        // One bounds entry per pulse; rustpix exports store pulse times
        // relative to the first pulse (0 ns and 25 ticks * 1000 * 25 ns).
        let bounds = bounds.expect("pulse bounds cached");
        assert_eq!(bounds.len(), 2);
        assert_eq!((bounds[0].start, bounds[0].len), (0, 2));
        assert_eq!((bounds[1].start, bounds[1].len), (2, 1));
        assert_eq!(bounds[0].tdc_timestamp_25ns, 0);
        assert_eq!(bounds[1].tdc_timestamp_25ns, 1000);

        // Every event landed in the hyperstack on the 512x512 bank grid.
        assert_eq!(hyperstack.width(), 512);
        assert_eq!(hyperstack.height(), 512);
        assert_eq!(hyperstack.full_spectrum().iter().sum::<u64>(), 3);

        assert!(
            debug.contains("bank100"),
            "debug info names the bank: {debug}"
        );
    }

    /// A pulse spanning a read-slice boundary must not be split into two
    /// bounds entries: chunk size 2 puts the first pulse's two events in
    /// slice one and the second pulse's event in slice two.
    #[test]
    fn load_sns_file_pulse_spans_chunk_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui_chunked.nxs.h5");
        write_sns_test_file(&path);

        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        load_sns_file_worker(&path, &tx, &DetectorConfig::default(), 10, true, &cancel, 2);
        drop(tx);

        let mut complete = None;
        while let Ok(msg) = rx.recv() {
            match msg {
                AppMessage::LoadError(e) => panic!("load failed: {e}"),
                AppMessage::LoadComplete(count, batch, _, _, _, bounds) => {
                    complete = Some((count, batch, bounds));
                }
                _ => {}
            }
        }
        let (count, batch, bounds) = complete.expect("LoadComplete sent");
        assert_eq!(count, 3);
        let batch = *batch.expect("hits cached");
        assert_eq!(batch.tof, vec![40, 80, 200]);
        let bounds = bounds.expect("pulse bounds cached");
        assert_eq!(bounds.len(), 2, "pulses must not split at chunk boundaries");
        assert_eq!((bounds[0].start, bounds[0].len), (0, 2));
        assert_eq!((bounds[1].start, bounds[1].len), (2, 1));
    }

    /// A `NeXus` file whose bank100 has no events (like VENUS facility ADARA
    /// files, where Timepix data lives in .tpx3 files) reports a clear error.
    #[test]
    fn load_sns_file_empty_bank_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui_empty.nxs.h5");
        let run = SnsRunMetadata {
            run_number: 1,
            experiment_identifier: "IPTS-00001".to_string(),
            start_time: "2025-01-01T00:00:00-05:00".to_string(),
            end_time: None,
            duration: None,
            proton_charge: None,
            title: None,
        };
        let mut sink = SnsEventSink::create(&path, SnsWriteOptions::venus_defaults(run)).unwrap();
        sink.finalize().unwrap();

        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        load_file_worker(&path, &tx, DetectorConfig::default(), 10, true, &cancel);
        drop(tx);

        let mut error = None;
        while let Ok(msg) = rx.recv() {
            match msg {
                AppMessage::LoadError(e) => error = Some(e),
                AppMessage::LoadComplete(..) => panic!("empty bank must not complete"),
                _ => {}
            }
        }
        let error = error.expect("LoadError sent");
        assert!(
            error.contains(".tpx3"),
            "error should point at .tpx3: {error}"
        );
    }

    /// Streaming mode (no hit cache) still fills the hyperstack.
    #[test]
    fn load_sns_file_streaming() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gui_streaming.nxs.h5");
        write_sns_test_file(&path);

        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        load_file_worker(&path, &tx, DetectorConfig::default(), 10, false, &cancel);
        drop(tx);

        let mut complete = None;
        while let Ok(msg) = rx.recv() {
            match msg {
                AppMessage::LoadError(e) => panic!("load failed: {e}"),
                AppMessage::LoadComplete(count, batch, hyperstack, _, _, bounds) => {
                    complete = Some((count, batch, hyperstack, bounds));
                }
                _ => {}
            }
        }
        let (count, batch, hyperstack, bounds) = complete.expect("LoadComplete sent");
        assert_eq!(count, 3);
        assert!(batch.is_none());
        assert!(bounds.is_none());
        assert_eq!(hyperstack.full_spectrum().iter().sum::<u64>(), 3);
    }
}
