//! End-to-end tests that run the `rustpix` binary on synthetic TPX3 files.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

const CSV_HEADER: &str = "x,y,tof,tot,n_hits,chip_id";

fn rustpix(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rustpix"))
        .args(args)
        .output()
        .expect("run rustpix")
}

fn header(chip: u8) -> u64 {
    0x3358_5054 | (u64::from(chip) << 32)
}

fn tdc(timestamp: u32) -> u64 {
    0x6F00_0000_0000_0000 | (u64::from(timestamp) << 12)
}

/// Hit at pixel (0, 0) with `ToT` 30 and a 30-bit coarse timestamp.
fn hit(timestamp: u32) -> u64 {
    0xB000_0000_0000_0000
        | (u64::from(timestamp & 0x3FFF) << 30)
        | (30 << 20)
        | u64::from(timestamp >> 14)
}

fn write_packets(path: &Path, packets: &[u64]) {
    let bytes: Vec<u8> = packets.iter().flat_map(|p| p.to_le_bytes()).collect();
    fs::write(path, bytes).unwrap();
}

/// Two pulses on chip 3: neutrons at TOF 100 and 5000, then TOF 100.
fn write_sample(path: &Path) {
    write_packets(
        path,
        &[
            header(3),
            tdc(1000),
            hit(1100),
            hit(6000),
            tdc(700_000),
            hit(700_100),
        ],
    );
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn partial_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".partial"))
        .collect()
}

/// Counts column of a spectrum CSV.
fn spectrum_counts(path: &Path) -> Vec<u64> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| line.split(',').nth(1).unwrap().parse().unwrap())
        .collect()
}

#[test]
fn process_writes_csv_and_leaves_no_partial_file() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    let output = dir.path().join("out.csv");
    write_sample(&input);

    let result = rustpix(&["process", s(&input), "-o", s(&output)]);
    assert!(result.status.success(), "{result:?}");

    let text = fs::read_to_string(&output).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], CSV_HEADER);
    assert_eq!(lines.len(), 4);
    assert!(String::from_utf8_lossy(&result.stdout).contains("Total neutrons: 3"));
    assert_eq!(partial_files(dir.path()), Vec::<String>::new());
}

#[test]
fn in_memory_mode_matches_out_of_core() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    write_sample(&input);
    let streamed = dir.path().join("a.csv");
    let in_memory = dir.path().join("b.csv");

    assert!(rustpix(&["process", s(&input), "-o", s(&streamed)])
        .status
        .success());
    let result = rustpix(&[
        "process",
        s(&input),
        "-o",
        s(&in_memory),
        "--out-of-core",
        "false",
    ]);
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        fs::read_to_string(&streamed).unwrap(),
        fs::read_to_string(&in_memory).unwrap()
    );
}

#[test]
fn missing_input_leaves_existing_output_untouched() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.tpx3");
    for name in ["keep.csv", "keep.h5", "keep.nxs.h5", "keep.tif", "keep.bin"] {
        let output = dir.path().join(name);
        fs::write(&output, "precious").unwrap();

        let result = rustpix(&["process", s(&missing), "-o", s(&output)]);
        assert!(!result.status.success(), "{name}");
        assert_eq!(fs::read_to_string(&output).unwrap(), "precious", "{name}");
    }
    assert_eq!(partial_files(dir.path()), Vec::<String>::new());
}

#[test]
fn output_naming_the_input_is_refused() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("same.tpx3");
    write_sample(&input);
    let original = fs::read(&input).unwrap();

    let result = rustpix(&["process", s(&input), "-o", s(&input)]);
    assert!(!result.status.success());
    assert_eq!(fs::read(&input).unwrap(), original);

    #[cfg(unix)]
    {
        let link = dir.path().join("alias.csv");
        fs::hard_link(&input, &link).unwrap();
        let result = rustpix(&["process", s(&input), "-o", s(&link)]);
        assert!(!result.status.success());
        assert_eq!(fs::read(&input).unwrap(), original);

        let symlink = dir.path().join("symlink.csv");
        std::os::unix::fs::symlink(&input, &symlink).unwrap();
        let result = rustpix(&["process", s(&input), "-o", s(&symlink)]);
        assert!(!result.status.success());
        assert_eq!(fs::read(&input).unwrap(), original);
    }
}

#[test]
fn output_with_raw_data_extension_is_refused() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("b.tpx3");
    let other = dir.path().join("a2.TPX3");
    write_sample(&input);
    write_sample(&other);
    let original = fs::read(&other).unwrap();

    let result = rustpix(&["process", "-o", s(&other), s(&input)]);
    assert!(!result.status.success());
    assert_eq!(fs::read(&other).unwrap(), original);
}

#[test]
fn failure_midway_leaves_no_output() {
    let dir = TempDir::new().unwrap();
    let good = dir.path().join("good.tpx3");
    let truncated = dir.path().join("truncated.tpx3");
    write_sample(&good);
    let mut bytes = fs::read(&good).unwrap();
    bytes.extend_from_slice(&[0u8; 4]);
    fs::write(&truncated, bytes).unwrap();

    for name in ["out.csv", "out.h5", "out.nxs.h5", "out.tif"] {
        let output = dir.path().join(name);
        let result = rustpix(&["process", s(&good), s(&truncated), "-o", s(&output)]);
        assert!(!result.status.success(), "{name}");
        assert!(!output.exists(), "{name} was left behind");
    }
    assert!(!dir.path().join("out.csv").with_extension("tif").exists());
    assert_eq!(partial_files(dir.path()), Vec::<String>::new());
}

#[test]
fn successful_run_replaces_an_existing_output() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    let output = dir.path().join("out.csv");
    write_sample(&input);
    fs::write(&output, "old contents").unwrap();

    assert!(rustpix(&["process", s(&input), "-o", s(&output)])
        .status
        .success());
    assert!(fs::read_to_string(&output).unwrap().starts_with(CSV_HEADER));
}

#[test]
fn hdf5_outputs_are_written() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    write_sample(&input);
    for name in ["out.h5", "out.nxs.h5"] {
        let output = dir.path().join(name);
        let result = rustpix(&["process", s(&input), "-o", s(&output)]);
        assert!(result.status.success(), "{name}: {result:?}");
        assert!(fs::metadata(&output).unwrap().len() > 0, "{name}");
    }
    assert_eq!(partial_files(dir.path()), Vec::<String>::new());
}

#[test]
fn tiff_leaves_out_neutrons_above_tof_max() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    let output = dir.path().join("stack.tif");
    write_sample(&input);

    let result = rustpix(&[
        "process",
        s(&input),
        "-o",
        s(&output),
        "--tof-bins",
        "10",
        "--tof-max",
        "1000",
    ]);
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        spectrum_counts(&output.with_extension("csv")),
        vec![0, 2, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("1 neutron(s)"), "{stderr}");
}

#[test]
fn tiff_keeps_a_neutron_exactly_at_tof_max() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    write_sample(&input);

    for tof_max_args in [&["--tof-max", "5000"][..], &[][..]] {
        let output = dir.path().join("stack.tif");
        let mut args = vec!["process", s(&input), "-o", s(&output), "--tof-bins", "10"];
        args.extend_from_slice(tof_max_args);
        let result = rustpix(&args);
        assert!(result.status.success(), "{result:?}");
        assert_eq!(
            spectrum_counts(&output.with_extension("csv")),
            vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            "{tof_max_args:?}"
        );
    }
}

#[test]
fn tiff_spectrum_does_not_overwrite_an_unrelated_csv() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    let events = dir.path().join("ev.csv");
    let stack = dir.path().join("ev.tif");
    write_sample(&input);

    assert!(rustpix(&["process", s(&input), "-o", s(&events)])
        .status
        .success());
    let event_csv = fs::read_to_string(&events).unwrap();

    let result = rustpix(&["process", s(&input), "-o", s(&stack)]);
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(&events).unwrap(), event_csv);
    assert!(!stack.exists());
}

#[test]
fn tiff_rerun_replaces_its_own_spectrum() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    let stack = dir.path().join("stack.tif");
    write_sample(&input);

    for bins in ["10", "20"] {
        let result = rustpix(&["process", s(&input), "-o", s(&stack), "--tof-bins", bins]);
        assert!(result.status.success(), "{result:?}");
    }
    assert_eq!(spectrum_counts(&stack.with_extension("csv")).len(), 20);
}

#[test]
fn tiff_written_to_a_csv_path_skips_the_spectrum() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    let output = dir.path().join("stack.CSV");
    write_sample(&input);

    let result = rustpix(&["process", s(&input), "-o", s(&output), "-f", "tiff", "-v"]);
    assert!(result.status.success(), "{result:?}");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("skipping spectrum CSV"), "{stderr}");
    assert!(!stderr.contains("Wrote spectrum"), "{stderr}");
    assert_eq!(&fs::read(&output).unwrap()[..2], b"II");
}

#[test]
fn oversized_tof_bins_is_a_clean_error() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    let output = dir.path().join("stack.tif");
    write_sample(&input);

    for bins in ["100000000", "70000000000000"] {
        let result = rustpix(&["process", s(&input), "-o", s(&output), "--tof-bins", bins]);
        assert_eq!(result.status.code(), Some(1), "{bins}: {result:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("--tof-bins"));
        assert!(!output.exists());
    }
}

#[test]
fn inputs_without_tpx3_data_are_rejected() {
    let dir = TempDir::new().unwrap();
    let empty = dir.path().join("empty.tpx3");
    let garbage = dir.path().join("garbage.tpx3");
    fs::write(&empty, b"").unwrap();
    fs::write(&garbage, vec![0xAAu8; 8192]).unwrap();

    for input in [&empty, &garbage] {
        let output = dir.path().join("out.csv");
        let result = rustpix(&["process", s(input), "-o", s(&output)]);
        assert!(!result.status.success(), "{input:?}");
        assert!(!output.exists(), "{input:?}");
        assert!(!rustpix(&["info", s(input)]).status.success(), "{input:?}");
    }
}

#[test]
fn zero_iterations_is_rejected() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    write_sample(&input);

    for command in ["benchmark", "out-of-core-benchmark"] {
        let result = rustpix(&[command, s(&input), "--iterations", "0"]);
        assert!(!result.status.success(), "{command}");
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(!stdout.contains("NaN"), "{command}: {stdout}");
    }
}

#[test]
fn out_of_core_flags_require_out_of_core() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    let output = dir.path().join("out.csv");
    write_sample(&input);

    for flag in [
        ["--parallelism", "4"],
        ["--memory-fraction", "0.25"],
        ["--memory-budget-bytes", "1000000"],
        ["--queue-depth", "4"],
        ["--async-io", "true"],
    ] {
        let result = rustpix(&[
            "process",
            s(&input),
            "-o",
            s(&output),
            "--out-of-core",
            "false",
            flag[0],
            flag[1],
        ]);
        assert!(!result.status.success(), "{flag:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains(flag[0]));

        let result = rustpix(&["process", s(&input), "-o", s(&output), flag[0], flag[1]]);
        assert!(result.status.success(), "{flag:?}: {result:?}");
    }
}

#[test]
fn info_reports_hit_ranges() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    write_sample(&input);

    let result = rustpix(&["info", s(&input)]);
    assert!(result.status.success(), "{result:?}");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("Hits: 3"), "{stdout}");
    assert!(stdout.contains("TOF range: 100 - 5000"), "{stdout}");
    assert!(stdout.contains("X range: 0 - 0"), "{stdout}");
}

#[test]
fn benchmark_clusters_pulse_by_pulse() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("run.tpx3");
    write_sample(&input);

    let result = rustpix(&["benchmark", s(&input), "--iterations", "1"]);
    assert!(result.status.success(), "{result:?}");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("3 hits in 2 pulses"), "{stdout}");
}
