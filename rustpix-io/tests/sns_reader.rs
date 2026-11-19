//! SNS event reader on hand-built files, including malformed ones.

use hdf5::types::{VarLenAscii, VarLenUnicode};
use hdf5::{Dataset, File, Group};
use rustpix_io::hdf5_sns::{venus_bank100_config, SnsEventReader};
use std::path::Path;
use std::str::FromStr;
use tempfile::TempDir;

const PIXEL_OFFSET: u32 = 1_000_000;

struct Bank<'a> {
    tof_us: &'a [f32],
    pulse_s: &'a [f64],
    event_index: &'a [u64],
}

/// Writes `entry/bank100_events` and returns the group for further edits.
fn write_bank(file: &File, bank: &Bank<'_>) -> Group {
    let entry = file.create_group("entry").unwrap();
    let group = entry.create_group("bank100_events").unwrap();
    let count = u32::try_from(bank.tof_us.len()).unwrap();
    let ids: Vec<u32> = (0..count).map(|i| PIXEL_OFFSET + i).collect();
    group
        .new_dataset_builder()
        .with_data(&ids)
        .create("event_id")
        .unwrap();
    group
        .new_dataset_builder()
        .with_data(bank.tof_us)
        .create("event_time_offset")
        .unwrap();
    group
        .new_dataset_builder()
        .with_data(bank.pulse_s)
        .create("event_time_zero")
        .unwrap();
    group
        .new_dataset_builder()
        .with_data(bank.event_index)
        .create("event_index")
        .unwrap();
    group
}

fn set_units_array(dataset: &Dataset, units: &str) {
    dataset
        .new_attr::<VarLenUnicode>()
        .shape(1)
        .create("units")
        .unwrap()
        .write(&[VarLenUnicode::from_str(units).unwrap()])
        .unwrap();
}

fn reader(path: &Path) -> SnsEventReader {
    SnsEventReader::open(path).unwrap()
}

fn single_event_file(path: &Path, pulse_s: f64) -> (File, Group) {
    let file = File::create(path).unwrap();
    let group = write_bank(
        &file,
        &Bank {
            tof_us: &[1.0],
            pulse_s: &[pulse_s],
            event_index: &[0],
        },
    );
    (file, group)
}

#[test]
fn slices_get_the_same_pulse_times_as_a_full_read() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let file = File::create(&path).unwrap();
    // Pulses 0 and 2 are empty.
    write_bank(
        &file,
        &Bank {
            tof_us: &[1.0, 2.0, 3.0, 4.0, 5.0],
            pulse_s: &[0.0, 0.1, 0.2, 0.3, 0.4],
            event_index: &[0, 0, 2, 2, 4],
        },
    );
    drop(file);

    let reader = reader(&path);
    let config = venus_bank100_config();
    let full = reader.read_events(&config, 0, None).unwrap();
    assert_eq!(
        full.pulse_time_ns,
        vec![
            100_000_000,
            100_000_000,
            300_000_000,
            300_000_000,
            400_000_000
        ]
    );
    for start in 0..5usize {
        for count in 1..=5 - start {
            let part = reader
                .read_events(&config, start as u64, Some(count as u64))
                .unwrap();
            assert_eq!(
                part.pulse_time_ns,
                full.pulse_time_ns[start..start + count],
                "start {start}, count {count}"
            );
        }
    }
}

#[test]
fn units_stored_as_one_element_array_are_honoured() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let file = File::create(&path).unwrap();
    let group = write_bank(
        &file,
        &Bank {
            tof_us: &[100.0],
            pulse_s: &[0.0],
            event_index: &[0],
        },
    );
    set_units_array(&group.dataset("event_time_offset").unwrap(), "nanosecond");
    drop(file);

    let events = reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .unwrap();
    assert_eq!(events.tof_ns, vec![100]);
}

#[test]
fn epics_offset_stored_as_one_element_array_is_honoured() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let (file, group) = single_event_file(&path, 0.5);
    group
        .dataset("event_time_zero")
        .unwrap()
        .new_attr::<u32>()
        .shape(1)
        .create("offset_seconds")
        .unwrap()
        .write(&[10u32])
        .unwrap();
    drop(file);

    let events = reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .unwrap();
    // The EPICS epoch (1990-01-01) is 631152000 s after the Unix epoch.
    assert_eq!(
        events.pulse_time_ns,
        vec![(631_152_000 + 10) * 1_000_000_000 + 500_000_000]
    );
}

#[test]
fn huge_declared_pulse_arrays_are_not_read_whole() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let file = File::create(&path).unwrap();
    let group = file
        .create_group("entry")
        .unwrap()
        .create_group("bank100_events")
        .unwrap();
    group
        .new_dataset_builder()
        .with_data(&[PIXEL_OFFSET])
        .create("event_id")
        .unwrap();
    group
        .new_dataset_builder()
        .with_data(&[1.0f32])
        .create("event_time_offset")
        .unwrap();
    // Declared but never written: reading either array whole would need 256 TiB.
    group
        .new_dataset::<f64>()
        .shape(1usize << 45)
        .chunk(1024)
        .create("event_time_zero")
        .unwrap();
    group
        .new_dataset::<u64>()
        .shape(1usize << 45)
        .chunk(1024)
        .create("event_index")
        .unwrap();
    drop(file);

    let events = reader(&path)
        .read_events(&venus_bank100_config(), 0, Some(1))
        .unwrap();
    assert_eq!(events.pulse_time_ns.len(), 1);
}

#[test]
fn mismatched_pulse_arrays_are_an_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let file = File::create(&path).unwrap();
    write_bank(
        &file,
        &Bank {
            tof_us: &[1.0],
            pulse_s: &[0.0, 0.1],
            event_index: &[0],
        },
    );
    drop(file);

    assert!(reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .is_err());
}

#[test]
fn unsorted_event_index_is_an_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let file = File::create(&path).unwrap();
    write_bank(
        &file,
        &Bank {
            tof_us: &[1.0, 2.0, 3.0],
            pulse_s: &[0.0, 0.1, 0.2],
            event_index: &[0, 2, 1],
        },
    );
    drop(file);

    assert!(reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .is_err());
}

#[test]
fn non_finite_tof_is_an_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let file = File::create(&path).unwrap();
    write_bank(
        &file,
        &Bank {
            tof_us: &[1.0, f32::NAN],
            pulse_s: &[0.0],
            event_index: &[0],
        },
    );
    drop(file);

    assert!(reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .is_err());
}

#[test]
fn out_of_range_pulse_times_are_an_error() {
    let dir = TempDir::new().unwrap();

    let path = dir.path().join("huge_value.nxs.h5");
    let (file, _) = single_event_file(&path, 1e30);
    drop(file);
    assert!(reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .is_err());

    let path = dir.path().join("far_future.nxs.h5");
    let (file, group) = single_event_file(&path, 0.0);
    group
        .dataset("event_time_zero")
        .unwrap()
        .new_attr::<VarLenUnicode>()
        .shape(())
        .create("offset")
        .unwrap()
        .write_scalar(&VarLenUnicode::from_str("9999-01-01T00:00:00Z").unwrap())
        .unwrap();
    drop(file);
    assert!(reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .is_err());
}

#[test]
#[allow(unsafe_code)]
fn invalid_utf8_metadata_is_not_returned() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let (file, _) = single_event_file(&path, 0.0);
    // SAFETY: stores non-ASCII bytes on purpose to model a corrupt file.
    let bad = unsafe { VarLenAscii::from_ascii_unchecked(&[0x6d, 0xff, 0xfe, 0x73][..]) };
    file.group("entry")
        .unwrap()
        .new_dataset::<VarLenAscii>()
        .shape(())
        .create("title")
        .unwrap()
        .write_scalar(&bad)
        .unwrap();
    drop(file);

    assert_eq!(reader(&path).metadata().title, None);
}

#[test]
fn malformed_offset_without_fallback_is_an_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let (file, group) = single_event_file(&path, 0.0);
    group
        .dataset("event_time_zero")
        .unwrap()
        .new_attr::<VarLenUnicode>()
        .shape(())
        .create("offset")
        .unwrap()
        .write_scalar(&VarLenUnicode::from_str("2025-01-01T00:00:00+0530").unwrap())
        .unwrap();
    drop(file);
    assert!(reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .is_err());
}

#[test]
fn many_element_units_attribute_is_ignored() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run.nxs.h5");
    let file = File::create(&path).unwrap();
    let group = write_bank(
        &file,
        &Bank {
            tof_us: &[100.0],
            pulse_s: &[0.0],
            event_index: &[0],
        },
    );
    let units = vec![VarLenUnicode::from_str("nanosecond").unwrap(); 3];
    group
        .dataset("event_time_offset")
        .unwrap()
        .new_attr::<VarLenUnicode>()
        .shape(3)
        .create("units")
        .unwrap()
        .write(&units)
        .unwrap();
    drop(file);
    // Not one value, so the default (microseconds) applies.
    let events = reader(&path)
        .read_events(&venus_bank100_config(), 0, None)
        .unwrap();
    assert_eq!(events.tof_ns, vec![100_000]);
}
