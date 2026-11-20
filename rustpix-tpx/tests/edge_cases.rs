//! Parser edge cases: chip IDs, TDC rollover, missed TDCs, and config validation.

use rustpix_core::soa::HitBatch;
use rustpix_tpx::ordering::{MergedPulseBatch, TimeOrderedStream};
use rustpix_tpx::section::{discover_sections, process_section_into_batch};
use rustpix_tpx::{DetectorConfig, Tpx3Packet, MIN_TDC_FREQUENCY_HZ};

const PERIOD: u32 = 666_667;

fn header(chip: u8) -> u64 {
    Tpx3Packet::TPX3_HEADER_MAGIC | (u64::from(chip) << 32)
}

fn tdc(timestamp: u32) -> u64 {
    0x6F00_0000_0000_0000 | (u64::from(timestamp) << 12)
}

/// Hit at pixel (0, 0) with a 30-bit coarse timestamp.
fn hit(timestamp: u32) -> u64 {
    0xB000_0000_0000_0000
        | (u64::from(timestamp & 0x3FFF) << 30)
        | (10 << 20)
        | u64::from(timestamp >> 14)
}

fn bytes(packets: &[u64]) -> Vec<u8> {
    packets.iter().flat_map(|p| p.to_le_bytes()).collect()
}

fn pulses(packets: &[u64], config: &DetectorConfig) -> Vec<MergedPulseBatch> {
    let data = bytes(packets);
    let sections = discover_sections(&data);
    let mut stream = TimeOrderedStream::new(data, &sections, config);
    std::iter::from_fn(|| stream.next_pulse_batch()).collect()
}

/// Identity transforms, so hits keep their local coordinates.
fn identity_config() -> DetectorConfig {
    DetectorConfig::from_json(r#"{"detector": {"chip_transformations": []}}"#).unwrap()
}

#[test]
fn chip_255_in_a_header_is_parsed() {
    let found = pulses(
        &[header(255), tdc(1000), hit(1100), tdc(2000)],
        &identity_config(),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].hits.tof, vec![100]);
    assert_eq!(found[0].hits.chip_id, vec![255]);
}

#[test]
fn chip_255_in_a_config_is_accepted() {
    let json =
        r#"{"detector":{"chip_transformations":[{"chip_id":255,"matrix":[[1,0,3],[0,1,0]]}]}}"#;
    let config = DetectorConfig::from_json(json).unwrap();
    assert_eq!(config.chip_transforms.len(), 256);
    assert_eq!(config.map_chip_to_global(255, 1, 1), (4, 1));
    assert_eq!(config.map_chip_to_global(0, 1, 1), (1, 1));
}

#[test]
fn late_hit_across_the_counter_rollover_stays_in_its_pulse() {
    let tdc_a = 0x3FFF_0000;
    let tdc_b = (tdc_a + PERIOD) & 0x3FFF_FFFF;
    let late = 0x3FFF_FF00; // after TDC A, before the wrap, read out after TDC B
    let found = pulses(
        &[
            header(0),
            tdc(tdc_a),
            hit(tdc_a + 100),
            tdc(tdc_b),
            hit(late),
            hit(tdc_b + 100),
            tdc(tdc_b + PERIOD),
        ],
        &identity_config(),
    );
    let summary: Vec<(u64, Vec<u32>)> = found
        .iter()
        .map(|p| (p.tdc_timestamp, p.hits.tof.clone()))
        .collect();
    // TDC B follows the wrap, so its extended timestamp is one counter range later.
    assert_eq!(
        summary,
        vec![
            (u64::from(tdc_a), vec![100, late - tdc_a]),
            (u64::from(tdc_b) + (1 << 30), vec![100]),
        ]
    );
}

#[test]
fn hit_stamped_before_the_first_tdc_is_dropped() {
    let found = pulses(
        &[header(0), tdc(1000), hit(999), hit(1100), tdc(2000)],
        &identity_config(),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].hits.tof, vec![100]);

    let data = bytes(&[tdc(1000), hit(999), hit(1100)]);
    let section = rustpix_tpx::section::Tpx3Section {
        start_offset: 0,
        end_offset: data.len(),
        chip_id: 0,
        initial_tdc: None,
        final_tdc: None,
    };
    let mut batch = HitBatch::default();
    process_section_into_batch(&data, &section, PERIOD, |_, x, y| (x, y), &mut batch);
    assert_eq!(batch.tof, vec![100]);
}

#[test]
fn missing_tdc_correction_follows_its_flag() {
    // One TDC is missing, so the hit lands 700000 ticks after the last one.
    let packets = [header(0), tdc(1000), hit(701_000), tdc(1_400_000)];
    let mut config = identity_config();

    config.enable_missing_tdc_correction = true;
    assert_eq!(
        pulses(&packets, &config)[0].hits.tof,
        vec![700_000 - PERIOD]
    );

    config.enable_missing_tdc_correction = false;
    assert_eq!(pulses(&packets, &config)[0].hits.tof, vec![700_000]);
}

#[test]
fn late_hit_keeps_its_pulse_in_the_section_parser() {
    let data = bytes(&[tdc(1000), tdc(2000), hit(1999), hit(2100)]);
    let section = rustpix_tpx::section::Tpx3Section {
        start_offset: 0,
        end_offset: data.len(),
        chip_id: 0,
        initial_tdc: None,
        final_tdc: None,
    };
    let mut batch = HitBatch::default();
    process_section_into_batch(&data, &section, PERIOD, |_, x, y| (x, y), &mut batch);
    assert_eq!(batch.tof, vec![999, 100]);
}

#[test]
fn lowest_frequency_keeps_long_and_missed_tdc_tofs() {
    let mut config = identity_config();
    config.tdc_frequency_hz = MIN_TDC_FREQUENCY_HZ;
    config.validate().unwrap();
    let period = config.tdc_correction_25ns();
    // The second hit follows a missed TDC, one period plus 5 ticks late.
    let found = pulses(
        &[
            header(0),
            tdc(1000),
            hit(1000 + period - 1),
            hit(1000 + period + 5),
            tdc(1000 + 2 * period + 10),
        ],
        &config,
    );
    let mut tofs = found[0].hits.tof.clone();
    tofs.sort_unstable();
    assert_eq!(tofs, vec![5, period - 1]);
}

#[test]
fn late_hit_near_two_periods_after_a_missed_tdc() {
    let mut config = identity_config();
    config.tdc_frequency_hz = MIN_TDC_FREQUENCY_HZ;
    let period = config.tdc_correction_25ns();
    let a = 1000;
    // TDC A + P is missed; the hit is stamped just before A + 2P and read out after it.
    let packets = [
        header(0),
        tdc(a),
        tdc(a + 2 * period),
        hit(a + 2 * period - 1),
        tdc(a + 3 * period),
    ];
    let first = |config: &DetectorConfig| {
        let found = pulses(&packets, config);
        (found[0].tdc_timestamp, found[0].hits.tof.clone())
    };

    config.enable_missing_tdc_correction = true;
    assert_eq!(first(&config), (u64::from(a), vec![period - 1]));
    config.enable_missing_tdc_correction = false;
    assert_eq!(first(&config), (u64::from(a), vec![2 * period - 1]));
}

#[test]
fn late_hit_at_a_section_start_is_dropped_by_the_section_parser() {
    // The hit belongs to TDC 1000 but is read out in the next section.
    let packets = [
        header(0),
        tdc(1000),
        tdc(1000 + PERIOD),
        header(0),
        hit(900 + PERIOD),
        tdc(1000 + 2 * PERIOD),
    ];
    let data = bytes(&packets);
    let sections = discover_sections(&data);
    assert_eq!(sections.len(), 2);
    for threshold in [PERIOD, u32::MAX] {
        let mut batch = HitBatch::default();
        for section in &sections {
            process_section_into_batch(&data, section, threshold, |_, x, y| (x, y), &mut batch);
        }
        assert_eq!(batch.tof, Vec::<u32>::new(), "threshold {threshold}");
    }

    let found = pulses(&packets, &identity_config());
    let found = found.iter().find(|p| !p.hits.tof.is_empty()).unwrap();
    assert_eq!(found.tdc_timestamp, 1000);
    assert_eq!(found.hits.tof, vec![PERIOD - 100]);
}

#[test]
fn frequency_below_the_minimum_is_rejected() {
    let json = r#"{"detector":{"timing":{"tdc_frequency_hz":0.05}}}"#;
    assert!(DetectorConfig::from_json(json).is_err());
    let mut config = DetectorConfig::venus_defaults();
    config.tdc_frequency_hz = 0.14;
    assert!(config.validate().is_err());
}

#[test]
fn section_parser_orders_hits_with_correction_off() {
    let section_for = |data: &[u8]| rustpix_tpx::section::Tpx3Section {
        start_offset: 0,
        end_offset: data.len(),
        chip_id: 0,
        initial_tdc: None,
        final_tdc: None,
    };
    let off = DetectorConfig::venus_defaults();
    let threshold = DetectorConfig {
        enable_missing_tdc_correction: false,
        ..off
    }
    .missing_tdc_threshold_25ns();

    let early = bytes(&[tdc(1000), hit(998)]);
    let mut batch = HitBatch::default();
    process_section_into_batch(
        &early,
        &section_for(&early),
        threshold,
        |_, x, y| (x, y),
        &mut batch,
    );
    assert_eq!(batch.tof, Vec::<u32>::new());

    let late = bytes(&[tdc(1000), tdc(2000), hit(1900)]);
    let mut batch = HitBatch::default();
    process_section_into_batch(
        &late,
        &section_for(&late),
        threshold,
        |_, x, y| (x, y),
        &mut batch,
    );
    assert_eq!(batch.tof, vec![900]);
}

#[test]
fn invalid_frequency_is_rejected() {
    for json in [
        r#"{"detector":{"timing":{"tdc_frequency_hz":0.0}}}"#,
        r#"{"detector":{"timing":{"tdc_frequency_hz":-60.0}}}"#,
    ] {
        assert!(DetectorConfig::from_json(json).is_err(), "{json}");
    }
    let mut config = DetectorConfig::venus_defaults();
    config.tdc_frequency_hz = f64::NAN;
    assert!(config.validate().is_err());
    assert!(DetectorConfig::venus_defaults().validate().is_ok());
}

#[test]
fn overflowing_transform_is_rejected_without_panicking() {
    let json = r#"{"detector":{"chip_transformations":[{"chip_id":0,"matrix":[[16843009,0,1],[0,1,0]]}]}}"#;
    assert!(DetectorConfig::from_json(json).is_err());
}
