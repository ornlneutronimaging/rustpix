//! Analyze timestamp patterns in TPX3 data to understand rollover behavior.
//!
//! Run with: cargo run --bin analyze_timestamps -- <tpx3_file>

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <tpx3_file>", args[0]);
        std::process::exit(1);
    }

    let mut file = File::open(&args[1])?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    println!(
        "File size: {} bytes ({} packets)",
        data.len(),
        data.len() / 8
    );
    println!();

    // Track per-chip statistics
    let mut current_chip: u8 = 0;
    let mut chip_stats: HashMap<u8, ChipStats> = HashMap::new();

    let packet_count = data.len() / 8;
    for i in 0..packet_count {
        let offset = i * 8;
        let raw = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());

        // Check packet type
        let packet_type = (raw >> 56) & 0xFF;

        // Header packet - extract chip ID
        if (raw & 0xFFFFFFFF) == 0x33585054 {
            current_chip = ((raw >> 32) & 0xFF) as u8;
            chip_stats
                .entry(current_chip)
                .or_insert_with(ChipStats::new);
            continue;
        }

        let stats = chip_stats
            .entry(current_chip)
            .or_insert_with(ChipStats::new);

        // TDC packet (0x6F)
        if packet_type == 0x6F {
            let tdc_ts = ((raw >> 12) & 0x3FFFFFFF) as u32;
            stats.record_tdc(tdc_ts);
        }
        // Hit packet (0xB*)
        else if (packet_type >> 4) == 0xB {
            let spidr = (raw & 0xFFFF) as u32;
            let toa = ((raw >> 30) & 0x3FFF) as u32;
            let hit_ts = (spidr << 14) | toa;
            stats.record_hit(hit_ts);
        }
    }

    // Print results
    for (chip_id, stats) in chip_stats.iter() {
        println!("=== Chip {} ===", chip_id);
        stats.print_summary();
        println!();
    }

    Ok(())
}

struct ChipStats {
    tdc_count: usize,
    tdc_timestamps: Vec<u32>,
    tdc_decreases: usize,
    tdc_last: Option<u32>,

    hit_count: usize,
    hit_timestamps: Vec<u32>,
    hit_decreases: usize,
    hit_last: Option<u32>,

    // Track hits relative to TDC
    hits_since_tdc: usize,
    hits_per_tdc: Vec<usize>,
}

impl ChipStats {
    fn new() -> Self {
        Self {
            tdc_count: 0,
            tdc_timestamps: Vec::new(),
            tdc_decreases: 0,
            tdc_last: None,

            hit_count: 0,
            hit_timestamps: Vec::new(),
            hit_decreases: 0,
            hit_last: None,

            hits_since_tdc: 0,
            hits_per_tdc: Vec::new(),
        }
    }

    fn record_tdc(&mut self, ts: u32) {
        self.tdc_count += 1;

        // Record hits per TDC period
        if self.tdc_count > 1 {
            self.hits_per_tdc.push(self.hits_since_tdc);
        }
        self.hits_since_tdc = 0;

        // Check for decrease (rollover)
        if let Some(last) = self.tdc_last {
            if ts < last {
                self.tdc_decreases += 1;
            }
        }

        // Sample timestamps (first 100 and last 100)
        if self.tdc_count <= 100 || self.tdc_count % 1000 == 0 {
            self.tdc_timestamps.push(ts);
        }

        self.tdc_last = Some(ts);
    }

    fn record_hit(&mut self, ts: u32) {
        self.hit_count += 1;
        self.hits_since_tdc += 1;

        // Check for decrease
        if let Some(last) = self.hit_last {
            if ts < last {
                self.hit_decreases += 1;
            }
        }

        // Sample timestamps
        if self.hit_count <= 100 || self.hit_count % 10000 == 0 {
            self.hit_timestamps.push(ts);
        }

        self.hit_last = Some(ts);
    }

    fn print_summary(&self) {
        println!("TDC packets: {}", self.tdc_count);
        println!("TDC decreases (rollovers): {}", self.tdc_decreases);
        if !self.tdc_timestamps.is_empty() {
            println!(
                "TDC range: {} - {} (diff: {})",
                self.tdc_timestamps.first().unwrap(),
                self.tdc_last.unwrap_or(0),
                self.tdc_last.unwrap_or(0) as i64 - *self.tdc_timestamps.first().unwrap() as i64
            );
        }

        println!();
        println!("Hit packets: {}", self.hit_count);
        println!("Hit timestamp decreases: {}", self.hit_decreases);
        println!(
            "Hit decrease rate: {:.4}%",
            100.0 * self.hit_decreases as f64 / self.hit_count.max(1) as f64
        );

        if !self.hits_per_tdc.is_empty() {
            let avg_hits: f64 =
                self.hits_per_tdc.iter().sum::<usize>() as f64 / self.hits_per_tdc.len() as f64;
            let min_hits = self.hits_per_tdc.iter().min().unwrap_or(&0);
            let max_hits = self.hits_per_tdc.iter().max().unwrap_or(&0);
            println!();
            println!("Hits per TDC period:");
            println!(
                "  Avg: {:.1}, Min: {}, Max: {}",
                avg_hits, min_hits, max_hits
            );
        }

        // Print first few TDC timestamps to see pattern
        if self.tdc_timestamps.len() >= 5 {
            println!();
            println!("First 5 TDC timestamps: {:?}", &self.tdc_timestamps[..5]);
            if self.tdc_timestamps.len() > 5 {
                let diffs: Vec<i64> = self.tdc_timestamps[..5]
                    .windows(2)
                    .map(|w| w[1] as i64 - w[0] as i64)
                    .collect();
                println!("TDC diffs (first 4): {:?}", diffs);
            }
        }
    }
}
