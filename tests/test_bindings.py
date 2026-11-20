"""Tests for the rustpix Python bindings, on a synthetic TPX3 file."""

import struct
import sys
import warnings

import pytest

import rustpix


def _header(chip):
    return 0x3358_5054 | (chip << 32)


def _tdc(timestamp):
    return 0x6F00_0000_0000_0000 | (timestamp << 12)


def _hit(timestamp, x=0):
    """Hit packet at local pixel (x, 0) with ToT 30 and a 30-bit coarse timestamp."""
    addr = (x & 0xFE) << 8 | (x & 1) << 2
    return (
        0xB000_0000_0000_0000
        | (timestamp & 0x3FFF) << 30
        | 30 << 20
        | addr << 44
        | timestamp >> 14
    )


@pytest.fixture
def tpx3_file(tmp_path):
    """Two pulses on chip 3: hits at TOF 100 and 5000, then TOF 100."""
    packets = [_header(3), _tdc(1000), _hit(1100), _hit(6000, x=20), _tdc(700_000), _hit(700_100)]
    path = tmp_path / "run.tpx3"
    path.write_bytes(b"".join(struct.pack("<Q", p) for p in packets))
    return str(path)


def test_hit_batch_to_arrow_returns_a_table(tpx3_file):
    pyarrow = pytest.importorskip("pyarrow")
    table = rustpix.read_tpx3_hits(tpx3_file).to_arrow()
    assert isinstance(table, pyarrow.Table)
    assert table.num_rows == 3


def test_neutron_batch_to_arrow_returns_a_table(tpx3_file):
    pyarrow = pytest.importorskip("pyarrow")
    table = rustpix.process_tpx3_neutrons(tpx3_file, collect=True).to_arrow()
    assert isinstance(table, pyarrow.Table)
    assert table.num_rows == 3
    assert table.column_names == ["x", "y", "tof", "tot", "n_hits", "chip_id"]


def test_to_arrow_without_pyarrow_keeps_the_batch(tpx3_file, monkeypatch):
    monkeypatch.setitem(sys.modules, "pyarrow", None)
    hits = rustpix.read_tpx3_hits(tpx3_file)
    with pytest.raises(ImportError):
        hits.to_arrow()
    assert hits.len() == 3


@pytest.mark.parametrize(
    "call",
    [
        lambda path: rustpix.process_tpx3_neutrons(path, collect=True, radius=1.0),
        lambda path: rustpix.process_tpx3_neutrons(path, collect=True, algoritm="dbscan"),
        lambda path: rustpix.stream_tpx3_neutrons(path, time_ordered=False),
        lambda path: rustpix.cluster_hits(rustpix.read_tpx3_hits(path), memory_fraction=0.5),
    ],
)
def test_unknown_keyword_arguments_warn(tpx3_file, call):
    with pytest.warns(FutureWarning, match="unexpected keyword argument"):
        call(tpx3_file)


def test_known_keyword_arguments_do_not_warn(tpx3_file):
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        rustpix.process_tpx3_neutrons(
            tpx3_file, collect=True, algorithm="abs", abs_scan_interval=10, time_ordered=True
        )
        rustpix.cluster_hits(rustpix.read_tpx3_hits(tpx3_file), algorithm="grid")


def test_cluster_hits_refuses_sns_output_before_clustering(tpx3_file, tmp_path):
    hits = rustpix.read_tpx3_hits(tpx3_file)
    with pytest.raises(ValueError, match="nxs.h5"):
        rustpix.cluster_hits(hits, output_path=str(tmp_path / "out.nxs.h5"))
    assert list(hits.to_numpy()["cluster_id"]) == [-1, -1, -1]


def test_missing_tdc_correction_setting_is_deprecated():
    with pytest.warns(DeprecationWarning, match="enable_missing_tdc_correction"):
        rustpix.DetectorConfig(enable_missing_tdc_correction=True)
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        rustpix.DetectorConfig(tdc_frequency_hz=60.0)
