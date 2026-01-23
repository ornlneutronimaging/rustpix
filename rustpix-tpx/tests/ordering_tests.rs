#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::uninlined_format_args,
    clippy::cast_precision_loss,
    clippy::unreadable_literal,
    clippy::ignore_without_reason
)]
use rustpix_core::soa::HitBatch;
use rustpix_tpx::ordering::TimeOrderedStream;
use rustpix_tpx::section::discover_sections;
use rustpix_tpx::DetectorConfig;
use rustpix_tpx::Tpx3Packet;

// Helper to create a TPX3 header packet
fn make_header(chip_id: u8) -> u64 {
    Tpx3Packet::TPX3_HEADER_MAGIC | ((chip_id as u64) << 32)
}

// Helper to create a TDC packet
fn make_tdc(timestamp: u32) -> u64 {
    0x6F00_0000_0000_0000 | ((timestamp as u64) << 12)
}

// Helper to create a Hit packet (ID 0xB)
// hit_ts = (spidr << 14) | toa
// We want to control hit_ts.
// packet.timestamp_coarse() = (spidr << 14) | toa
// toa is 14 bits. spidr is 16 bits.
fn make_hit(timestamp: u32, tot: u16, addr: u16) -> u64 {
    let toa = (timestamp & 0x3FFF) as u16;
    let spidr = (timestamp >> 14) as u16;

    0xB000_0000_0000_0000
        | ((toa as u64) << 30)
        | ((tot as u64) << 20)
        | ((addr as u64) << 44)
        | (spidr as u64)
}

fn collect_batches(stream: TimeOrderedStream<'_>) -> HitBatch {
    let mut batch = HitBatch::default();
    for pulse_batch in stream {
        batch.append(&pulse_batch);
    }
    batch
}

#[test]
fn test_interleaved_ordering() {
    let mut data = Vec::new();

    // Pulse 1: T=1000.  Pulse 2: T=2000.
    // Chip 0: Hit at P1+100, P2+100
    // Chip 1: Hit at P1+200, P2+50 (earlier in P2!)

    // File structure:
    // Section 1: Chip 0, Pulse 1 & 2
    // Section 2: Chip 1, Pulse 1 & 2
    // (This simulates simple case. Real case is interleaved chunks).

    // --- Chip 0 Section ---
    data.extend_from_slice(&make_header(0).to_le_bytes());
    data.extend_from_slice(&make_tdc(1000).to_le_bytes()); // Pulse 1 Start

    // Hit at T=1100 (Pulse 1 + 100)
    data.extend_from_slice(&make_hit(1100, 10, 0).to_le_bytes());

    data.extend_from_slice(&make_tdc(2000).to_le_bytes()); // Pulse 2 Start (Ends Pulse 1)

    // Hit at T=2100 (Pulse 2 + 100)
    data.extend_from_slice(&make_hit(2100, 10, 0).to_le_bytes());

    data.extend_from_slice(&make_tdc(3000).to_le_bytes()); // Ends Pulse 2

    // --- Chip 1 Section ---
    data.extend_from_slice(&make_header(1).to_le_bytes());
    data.extend_from_slice(&make_tdc(1000).to_le_bytes()); // Pulse 1 Start

    // Hit at T=1200 (Pulse 1 + 200)
    data.extend_from_slice(&make_hit(1200, 10, 0).to_le_bytes());

    data.extend_from_slice(&make_tdc(2000).to_le_bytes()); // Pulse 2 Start

    // Hit at T=2050 (Pulse 2 + 50)
    data.extend_from_slice(&make_hit(2050, 10, 0).to_le_bytes());

    data.extend_from_slice(&make_tdc(3000).to_le_bytes()); // Ends Pulse 2

    // Discover sections
    let sections = discover_sections(&data);
    assert_eq!(sections.len(), 2);

    // Config
    let config = DetectorConfig::default();

    // Create Stream
    let stream = TimeOrderedStream::new(&data, &sections, &config);

    let hits = collect_batches(stream);

    assert_eq!(hits.len(), 4);

    // Expected Order:
    // Pulse 1 (TDC=1000):
    //   Chip 0 Hit: TOF = 1100 - 1000 = 100
    //   Chip 1 Hit: TOF = 1200 - 1000 = 200
    // Pulse 2 (TDC=2000):
    //   Chip 1 Hit: TOF = 2050 - 2000 = 50
    //   Chip 0 Hit: TOF = 2100 - 2000 = 100

    // Verify TOF order
    assert_eq!(hits.tof[0], 100);
    assert_eq!(hits.chip_id[0], 0);

    assert_eq!(hits.tof[1], 200);
    assert_eq!(hits.chip_id[1], 1);

    // Note: hits[2] should be the one with TOF=50 from Pulse 2
    // Because Pulse 2 > Pulse 1, so all Pulse 1 hits come first.
    // Within Pulse 2, TOF=50 comes before TOF=100.

    assert_eq!(hits.tof[2], 50);
    assert_eq!(hits.chip_id[2], 1);

    assert_eq!(hits.tof[3], 100);
    assert_eq!(hits.chip_id[3], 0);
}

#[test]
fn test_independent_rollover() {
    let mut data = Vec::new();

    // Scenario:
    // TDC is just before rollover (e.g. 0x3FFF_F000)
    // Hit is just after rollover (e.g. 0x0000_1000)
    // Hit appears AFTER TDC in stream.
    // They belong to same pulse.

    let tdc_val = 0x3FFF_F000;
    let hit_val = 0x0001; // Rolled over (timestamp bits)

    data.extend_from_slice(&make_header(0).to_le_bytes());
    data.extend_from_slice(&make_tdc(tdc_val).to_le_bytes());

    // Make hit with raw timestamp 0x0001
    // spidr=0, toa=1
    let raw_hit = make_hit(hit_val, 10, 0);
    data.extend_from_slice(&raw_hit.to_le_bytes());

    // Close pulse (next TDC)
    data.extend_from_slice(&make_tdc(tdc_val + 10000).to_le_bytes());

    let sections = discover_sections(&data);
    let config = DetectorConfig::default();
    let stream = TimeOrderedStream::new(&data, &sections, &config);
    let hits = collect_batches(stream);

    assert_eq!(hits.len(), 1);

    // Verify TOF
    // Hit 0x0001 corresponds to ... 0x40000001 (extension)
    // TDC 0x3FFFF000
    // Diff: 0x40000001 - 0x3FFFF000 = 0x1001 = 4097
    assert_eq!(hits.tof[0], 4097);
}

#[test]
fn test_late_hit_boundary() {
    let mut data = Vec::new();

    // Scenario:
    // Pulse 0: TDC=1000.
    // Pulse 1: TDC=2000.
    // Hit arrives AFTER Pulse 1 TDC.
    // Hit timestamp corresponds to T=1950 (Pulse 0 + 950).
    // Pulse 1 starts at 2000.
    // Hit < Pulse 1, so it MUST belong to Pulse 0.

    data.extend_from_slice(&make_header(0).to_le_bytes());
    data.extend_from_slice(&make_tdc(1000).to_le_bytes()); // Pulse 0 start

    // Normal hit for Pulse 0
    data.extend_from_slice(&make_hit(1100, 10, 0).to_le_bytes());

    data.extend_from_slice(&make_tdc(2000).to_le_bytes()); // Pulse 1 start

    // Late hit! Timestamp 1950. Appears after TDC(2000).
    data.extend_from_slice(&make_hit(1950, 10, 1).to_le_bytes());

    // Another hit for Pulse 1
    data.extend_from_slice(&make_hit(2100, 10, 2).to_le_bytes());

    data.extend_from_slice(&make_tdc(3000).to_le_bytes()); // Pulse 2 start

    let sections = discover_sections(&data);
    let config = DetectorConfig::default();
    let stream = TimeOrderedStream::new(&data, &sections, &config);
    let hits = collect_batches(stream);

    assert_eq!(hits.len(), 3);

    // Expected order:
    // 1. Hit 1100 (Pulse 0). TOF=100.
    // 2. Hit 1950 (Pulse 0). TOF=950. <-- Crucial check
    // 3. Hit 2100 (Pulse 1). TOF=100.

    assert_eq!(hits.tof[0], 100);
    assert_eq!(hits.tof[1], 950);
    assert_eq!(hits.tof[2], 100);
}

#[test]
fn test_tdc_rollover_ordering() {
    let mut data = Vec::new();

    let tdc_pre_a = 0x3FFFF000;
    let tdc_pre_b = 0x3FFFF100;
    let tdc_post = 0x00001000;

    // --- Chip 0 Section (rollover) ---
    data.extend_from_slice(&make_header(0).to_le_bytes());
    data.extend_from_slice(&make_tdc(tdc_pre_a).to_le_bytes());
    data.extend_from_slice(&make_hit(tdc_pre_a + 10, 10, 0).to_le_bytes());
    data.extend_from_slice(&make_tdc(tdc_post).to_le_bytes());
    data.extend_from_slice(&make_hit(tdc_post + 30, 10, 1).to_le_bytes());

    // --- Chip 1 Section (pre-rollover only, slightly later) ---
    data.extend_from_slice(&make_header(1).to_le_bytes());
    data.extend_from_slice(&make_tdc(tdc_pre_b).to_le_bytes());
    data.extend_from_slice(&make_hit(tdc_pre_b + 20, 10, 2).to_le_bytes());

    let sections = discover_sections(&data);
    let config = DetectorConfig::default();
    let stream = TimeOrderedStream::new(&data, &sections, &config);
    let hits = collect_batches(stream);

    assert_eq!(hits.len(), 3);

    // Expected order: pre-rollover pulses (chip 0 then chip 1), then post-rollover.
    assert_eq!(hits.chip_id[0], 0);
    assert_eq!(hits.tof[0], 10);

    assert_eq!(hits.chip_id[1], 1);
    assert_eq!(hits.tof[1], 20);

    assert_eq!(hits.chip_id[2], 0);
    assert_eq!(hits.tof[2], 30);
}

#[test]
#[ignore] // Run with `cargo test -- --ignored` to benchmark
fn test_performance_synthetic() {
    // Generate 1 million hits across 4 chips
    let mut data = Vec::with_capacity(100 * 1024 * 1024);
    let num_pulses = 1000;
    let hits_per_pulse = 250;

    // Interleave chips: Pulse 0 for Chip 0..3, Pulse 1 for Chip 0..3, etc.
    // Actually sections are usually large (e.g. 1 second of data per section).
    // Let's make 4 large sections.

    for chip in 0..4 {
        data.extend_from_slice(&make_header(chip).to_le_bytes());
        for p in 0..num_pulses {
            data.extend_from_slice(&make_tdc(p * 10000).to_le_bytes());
            for i in 0..hits_per_pulse {
                // Random TOF within pulse
                let tof = (i * 10) as u32;
                let ts = (p * 10000) + tof;
                data.extend_from_slice(&make_hit(ts, 1, 0).to_le_bytes());
            }
        }
    }

    let sections = discover_sections(&data);
    let config = DetectorConfig::default();

    let start = std::time::Instant::now();
    let stream = TimeOrderedStream::new(&data, &sections, &config);
    let count: usize = stream.map(|batch| batch.len()).sum();
    let elapsed = start.elapsed();

    println!("Processed {} hits in {:.2?}", count, elapsed);
    assert_eq!(count, num_pulses as usize * hits_per_pulse as usize * 4);

    // Throughput
    let hits_per_sec = count as f64 / elapsed.as_secs_f64();
    println!("Throughput: {:.2} M hits/s", hits_per_sec / 1e6);
}
