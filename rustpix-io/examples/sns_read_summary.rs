//! Read an SNS `NeXus` event file and print a summary — banks, run
//! metadata, and event statistics from a full sliced read of `bank100`.
//!
//! ```sh
//! cargo run -p rustpix-io --features hdf5 --example sns_read_summary -- VENUS_12345.nxs.h5
//! ```

use rustpix_io::hdf5_sns::{venus_bank100_config, SnsEventReader};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: sns_read_summary <file.nxs.h5>");
    let reader = SnsEventReader::open(&path).expect("open NeXus file");

    println!("banks:");
    for b in reader.banks() {
        println!(
            "  {}: {} events, {} pulses",
            b.name, b.event_count, b.pulse_count
        );
    }
    let md = reader.metadata();
    println!(
        "run {:?}  experiment {:?}  start {:?}  duration {:?} s  proton charge {:?} pC",
        md.run_number, md.experiment_identifier, md.start_time, md.duration_s, md.proton_charge_pc
    );

    // Read bank100 in slices, the way the GUI loader does.
    let cfg = venus_bank100_config();
    let mut offset = 0u64;
    let (mut min_tof, mut max_tof) = (u64::MAX, 0u64);
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (u16::MAX, 0u16, u16::MAX, 0u16);
    let mut pulse_groups = 0u64;
    let mut non_monotonic = 0u64;
    let mut last_pulse = None;
    loop {
        let chunk = reader
            .read_events(&cfg, offset, Some(2_000_000))
            .expect("read events");
        if chunk.is_empty() {
            break;
        }
        for i in 0..chunk.len() {
            min_tof = min_tof.min(chunk.tof_ns[i]);
            max_tof = max_tof.max(chunk.tof_ns[i]);
            min_x = min_x.min(chunk.x[i]);
            max_x = max_x.max(chunk.x[i]);
            min_y = min_y.min(chunk.y[i]);
            max_y = max_y.max(chunk.y[i]);
            let p = chunk.pulse_time_ns[i];
            if last_pulse != Some(p) {
                if last_pulse.is_some_and(|lp| p < lp) {
                    non_monotonic += 1;
                }
                pulse_groups += 1;
                last_pulse = Some(p);
            }
        }
        offset += chunk.len() as u64;
    }
    println!(
        "read {offset} events across {pulse_groups} pulse groups ({non_monotonic} non-monotonic)"
    );
    #[allow(clippy::cast_precision_loss)]
    {
        println!(
            "tof range: {:.3} - {:.3} ms  ({} ticks of 25 ns max)",
            min_tof as f64 / 1e6,
            max_tof as f64 / 1e6,
            max_tof / 25
        );
    }
    println!("x range: {min_x}-{max_x}, y range: {min_y}-{max_y}");
}
