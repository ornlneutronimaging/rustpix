//! Analyze behavior around TDC rollover point.
//!
//! Run with: cargo run --bin `analyze_rollover` -- <`tpx3_file`>

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

    let packet_count = data.len() / 8;
    let mut last_tdc: Option<u32> = None;
    let mut last_hit_ts: Option<u32> = None;
    let mut current_chip: u8 = 0;

    let mut tdc_events: Vec<(usize, u8, u32)> = Vec::new(); // (packet_idx, chip, tdc_ts)
    let mut hit_events: Vec<(usize, u8, u32)> = Vec::new(); // (packet_idx, chip, hit_ts)

    for i in 0..packet_count {
        let offset = i * 8;
        let raw = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
        let packet_type = (raw >> 56) & 0xFF;

        // Header
        if (raw & 0xFFFF_FFFF) == 0x3358_5054 {
            current_chip = ((raw >> 32) & 0xFF) as u8;
            continue;
        }

        // TDC
        if packet_type == 0x6F {
            let tdc_ts = ((raw >> 12) & 0x3FFF_FFFF) as u32;

            // Check for rollover
            if let Some(last) = last_tdc {
                if tdc_ts < last && (last - tdc_ts) > 500_000_000 {
                    println!("=== TDC ROLLOVER DETECTED ===");
                    println!("Packet {i}: TDC {last} -> {tdc_ts}");
                    println!("Chip: {current_chip}");

                    // Print surrounding TDCs
                    println!("\nRecent TDCs before rollover:");
                    for (idx, chip, ts) in tdc_events.iter().rev().take(5).rev() {
                        println!("  Packet {idx}, Chip {chip}: TDC {ts}");
                    }

                    // Print recent hits
                    println!("\nRecent hits before rollover:");
                    for (idx, chip, ts) in hit_events.iter().rev().take(10).rev() {
                        println!("  Packet {idx}, Chip {chip}: hit_ts {ts}");
                    }
                    println!();
                }
            }

            tdc_events.push((i, current_chip, tdc_ts));
            if tdc_events.len() > 100 {
                tdc_events.remove(0);
            }
            last_tdc = Some(tdc_ts);
        }
        // Hit
        else if (packet_type >> 4) == 0xB {
            let spidr = (raw & 0xFFFF) as u32;
            let toa = ((raw >> 30) & 0x3FFF) as u32;
            let hit_ts = (spidr << 14) | toa;

            // Check for large jump after TDC rollover
            if let Some(last) = last_hit_ts {
                if hit_ts < last && (last - hit_ts) > 500_000_000 {
                    println!("=== HIT TIMESTAMP ROLLOVER ===");
                    println!("Packet {i}: hit_ts {last} -> {hit_ts}");
                    println!("Chip: {current_chip}");
                    println!("Current TDC: {last_tdc:?}");
                }
            }

            hit_events.push((i, current_chip, hit_ts));
            if hit_events.len() > 100 {
                hit_events.remove(0);
            }
            last_hit_ts = Some(hit_ts);
        }
    }

    println!("Analysis complete. {packet_count} packets processed.");
    Ok(())
}
