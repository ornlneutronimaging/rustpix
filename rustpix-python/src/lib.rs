//! rustpix-python: PyO3 Python bindings for rustpix.
#![allow(
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::uninlined_format_args,
    clippy::elidable_lifetime_names,
    clippy::similar_names
)]
//!
//! This crate provides Python bindings using PyO3 and numpy
//! for efficient data exchange with Python.

use numpy::{PyArray1, PyReadonlyArray1};
use pyo3::prelude::*;
use rustpix_algorithms::{
    AbsClustering, AbsState, DbscanClustering, DbscanState, GridClustering, GridState,
};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::{ExtractionConfig, NeutronExtraction, SimpleCentroidExtraction};
use rustpix_core::neutron::Neutron;
use rustpix_core::soa::HitBatch;
use rustpix_io::Tpx3FileReader;
pub mod streaming;

fn io_error(context: &str, err: impl std::fmt::Display) -> PyErr {
    pyo3::exceptions::PyIOError::new_err(format!("{context}: {err}"))
}

fn value_error(context: &str, err: impl std::fmt::Display) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!("{context}: {err}"))
}

/// Python wrapper for Neutron.
#[pyclass(name = "Neutron")]
#[derive(Clone)]
pub struct PyNeutron {
    inner: Neutron,
}

#[pymethods]
impl PyNeutron {
    #[new]
    fn new(x: f64, y: f64, tof: u32, tot: u16, n_hits: u16, chip_id: u8) -> Self {
        Self {
            inner: Neutron::new(x, y, tof, tot, n_hits, chip_id),
        }
    }

    #[getter]
    fn x(&self) -> f64 {
        self.inner.x
    }

    #[getter]
    fn y(&self) -> f64 {
        self.inner.y
    }

    #[getter]
    fn tof(&self) -> u32 {
        self.inner.tof
    }

    #[getter]
    fn tof_ns(&self) -> f64 {
        self.inner.tof_ns()
    }

    #[getter]
    fn tot(&self) -> u16 {
        self.inner.tot
    }

    #[getter]
    fn n_hits(&self) -> u16 {
        self.inner.n_hits
    }

    #[getter]
    fn chip_id(&self) -> u8 {
        self.inner.chip_id
    }

    fn __repr__(&self) -> String {
        format!(
            "Neutron(x={:.2}, y={:.2}, tof={}, tot={}, n_hits={}, chip={})",
            self.inner.x,
            self.inner.y,
            self.inner.tof,
            self.inner.tot,
            self.inner.n_hits,
            self.inner.chip_id
        )
    }
}

/// Python wrapper for ClusteringConfig.
#[pyclass(name = "ClusteringConfig")]
#[derive(Clone)]
pub struct PyClusteringConfig {
    inner: ClusteringConfig,
}

#[pymethods]
impl PyClusteringConfig {
    #[new]
    #[pyo3(signature = (radius=5.0, temporal_window_ns=75.0, min_cluster_size=1, max_cluster_size=None))]
    fn new(
        radius: f64,
        temporal_window_ns: f64,
        min_cluster_size: u16,
        max_cluster_size: Option<usize>,
    ) -> Self {
        Self {
            inner: ClusteringConfig {
                radius,
                temporal_window_ns,
                min_cluster_size,
                max_cluster_size: max_cluster_size.map(|s| s as u16),
            },
        }
    }

    #[staticmethod]
    fn default() -> Self {
        Self {
            inner: ClusteringConfig::default(),
        }
    }

    #[getter]
    fn radius(&self) -> f64 {
        self.inner.radius
    }

    #[getter]
    fn temporal_window_ns(&self) -> f64 {
        self.inner.temporal_window_ns
    }

    #[getter]
    fn min_cluster_size(&self) -> u16 {
        self.inner.min_cluster_size
    }

    #[getter]
    fn max_cluster_size(&self) -> Option<usize> {
        self.inner.max_cluster_size.map(|s| s as usize)
    }
}

/// Python wrapper for ChipTransform.
#[pyclass(name = "ChipTransform")]
#[derive(Clone)]
pub struct PyChipTransform {
    inner: rustpix_tpx::ChipTransform,
}

#[pymethods]
impl PyChipTransform {
    #[new]
    #[pyo3(signature = (a=1, b=0, c=0, d=1, tx=0, ty=0))]
    fn new(a: i32, b: i32, c: i32, d: i32, tx: i32, ty: i32) -> Self {
        Self {
            inner: rustpix_tpx::ChipTransform { a, b, c, d, tx, ty },
        }
    }

    #[staticmethod]
    fn identity() -> Self {
        Self {
            inner: rustpix_tpx::ChipTransform::identity(),
        }
    }

    #[getter]
    fn a(&self) -> i32 {
        self.inner.a
    }

    #[getter]
    fn b(&self) -> i32 {
        self.inner.b
    }

    #[getter]
    fn c(&self) -> i32 {
        self.inner.c
    }

    #[getter]
    fn d(&self) -> i32 {
        self.inner.d
    }

    #[getter]
    fn tx(&self) -> i32 {
        self.inner.tx
    }

    #[getter]
    fn ty(&self) -> i32 {
        self.inner.ty
    }
}

/// Python wrapper for DetectorConfig.
#[pyclass(name = "DetectorConfig")]
#[derive(Clone)]
pub struct PyDetectorConfig {
    inner: rustpix_tpx::DetectorConfig,
}

#[pymethods]
impl PyDetectorConfig {
    #[new]
    #[pyo3(signature = (tdc_frequency_hz=60.0, enable_missing_tdc_correction=true, chip_size_x=256, chip_size_y=256, chip_transforms=None))]
    fn new(
        tdc_frequency_hz: f64,
        enable_missing_tdc_correction: bool,
        chip_size_x: u16,
        chip_size_y: u16,
        chip_transforms: Option<Vec<PyChipTransform>>,
    ) -> PyResult<Self> {
        let transforms = chip_transforms
            .map(|v| v.into_iter().map(|t| t.inner).collect())
            .unwrap_or_default();

        let config = Self {
            inner: rustpix_tpx::DetectorConfig {
                tdc_frequency_hz,
                enable_missing_tdc_correction,
                chip_size_x,
                chip_size_y,
                chip_transforms: transforms,
            },
        };

        config
            .inner
            .validate_transforms()
            .map_err(|e| value_error("DetectorConfig.validate_transforms", e))?;

        Ok(config)
    }

    #[staticmethod]
    fn venus_defaults() -> Self {
        Self {
            inner: rustpix_tpx::DetectorConfig::venus_defaults(),
        }
    }

    #[staticmethod]
    fn from_file(path: &str) -> PyResult<Self> {
        rustpix_tpx::DetectorConfig::from_file(path)
            .map(|config| Self { inner: config })
            .map_err(|e| io_error(&format!("DetectorConfig.from_file({path})"), e))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        rustpix_tpx::DetectorConfig::from_json(json)
            .map(|config| Self { inner: config })
            .map_err(|e| value_error("DetectorConfig.from_json", e))
    }
}

/// Read hits from a TPX3 file and return as numpy structured arrays.
#[pyfunction]
#[pyo3(signature = (path, detector_config=None))]
fn read_tpx3_file_numpy<'py>(
    py: Python<'py>,
    path: &str,
    detector_config: Option<PyDetectorConfig>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut reader = Tpx3FileReader::open(path)
        .map_err(|e| io_error(&format!("read_tpx3_file_numpy: open {path}"), e))?;

    if let Some(config) = detector_config {
        reader = reader.with_config(config.inner);
    }

    let batch = reader
        .read_batch()
        .map_err(|e| value_error("read_tpx3_file_numpy: read_batch", e))?;

    let dict = hits_dict_from_batch(py, batch)?;
    Ok(dict.into_any())
}

fn hits_dict_from_batch<'py>(
    py: Python<'py>,
    batch: HitBatch,
) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
    let HitBatch {
        x,
        y,
        tof,
        tot,
        timestamp,
        chip_id,
        cluster_id,
    } = batch;

    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("x", PyArray1::from_vec(py, x))?;
    dict.set_item("y", PyArray1::from_vec(py, y))?;
    dict.set_item("tof", PyArray1::from_vec(py, tof))?;
    dict.set_item("tot", PyArray1::from_vec(py, tot))?;
    dict.set_item("timestamp", PyArray1::from_vec(py, timestamp))?;
    dict.set_item("chip_id", PyArray1::from_vec(py, chip_id))?;
    if !cluster_id.is_empty() {
        dict.set_item("cluster_id", PyArray1::from_vec(py, cluster_id))?;
    }

    Ok(dict)
}

fn arrow_table_from_dict<'py>(
    py: Python<'py>,
    dict: &Bound<'py, pyo3::types::PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    let pyarrow = py
        .import("pyarrow")
        .map_err(|_| pyo3::exceptions::PyImportError::new_err("pyarrow is required"))?;
    pyarrow.call_method("table", (dict,), None)
}

/// Read hits from a TPX3 file and return as a PyArrow table.
#[pyfunction]
#[pyo3(signature = (path, detector_config=None))]
fn read_tpx3_file_arrow<'py>(
    py: Python<'py>,
    path: &str,
    detector_config: Option<PyDetectorConfig>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut reader = Tpx3FileReader::open(path)
        .map_err(|e| io_error(&format!("read_tpx3_file_arrow: open {path}"), e))?;

    if let Some(config) = detector_config {
        reader = reader.with_config(config.inner);
    }

    let batch = reader
        .read_batch()
        .map_err(|e| value_error("read_tpx3_file_arrow: read_batch", e))?;
    let dict = hits_dict_from_batch(py, batch)?;
    arrow_table_from_dict(py, &dict)
}

fn batch_from_numpy(
    x: PyReadonlyArray1<u16>,
    y: PyReadonlyArray1<u16>,
    tof: PyReadonlyArray1<u32>,
    tot: PyReadonlyArray1<u16>,
    timestamp: Option<PyReadonlyArray1<u32>>,
    chip_id: Option<PyReadonlyArray1<u8>>,
) -> PyResult<HitBatch> {
    let len = x.len()?;
    let y_len = y.len()?;
    if y_len != len {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Length mismatch: y={}, x={}",
            y_len, len
        )));
    }
    let tof_len = tof.len()?;
    if tof_len != len {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Length mismatch: tof={}, x={}",
            tof_len, len
        )));
    }
    let tot_len = tot.len()?;
    if tot_len != len {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Length mismatch: tot={}, x={}",
            tot_len, len
        )));
    }

    if let Some(ref ts) = timestamp {
        let ts_len = ts.len()?;
        if ts_len != len {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Length mismatch: timestamp={}, x={}",
                ts_len, len
            )));
        }
    }
    if let Some(ref chip) = chip_id {
        let chip_len = chip.len()?;
        if chip_len != len {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Length mismatch: chip_id={}, x={}",
                chip_len, len
            )));
        }
    }

    let x = x.as_slice()?;
    let y = y.as_slice()?;
    let tof = tof.as_slice()?;
    let tot = tot.as_slice()?;
    let timestamp = timestamp.as_ref().map(|ts| ts.as_slice()).transpose()?;
    let chip_id = chip_id.as_ref().map(|chip| chip.as_slice()).transpose()?;

    let mut batch = HitBatch::with_capacity(len);
    for i in 0..len {
        let ts = timestamp.map_or(0, |ts| ts[i]);
        let chip = chip_id.map_or(0, |chip| chip[i]);
        batch.push(x[i], y[i], tof[i], tot[i], ts, chip);
    }

    Ok(batch)
}

/// Cluster hits using SoA numpy arrays.
#[pyfunction]
#[pyo3(signature = (x, y, tof, tot, timestamp=None, chip_id=None, config=None, algorithm="abs"))]
#[allow(clippy::too_many_arguments)]
fn cluster_hits_numpy<'py>(
    py: Python<'py>,
    x: PyReadonlyArray1<u16>,
    y: PyReadonlyArray1<u16>,
    tof: PyReadonlyArray1<u32>,
    tot: PyReadonlyArray1<u16>,
    timestamp: Option<PyReadonlyArray1<u32>>,
    chip_id: Option<PyReadonlyArray1<u8>>,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
) -> PyResult<(Bound<'py, PyArray1<i32>>, usize)> {
    let mut batch = batch_from_numpy(x, y, tof, tot, timestamp, chip_id)?;
    let num_clusters = cluster_from_batch(&mut batch, config, algorithm)?;
    let labels = PyArray1::from_vec(py, batch.cluster_id);
    Ok((labels, num_clusters))
}

/// Extract neutrons from SoA numpy arrays and labels.
#[pyfunction]
#[pyo3(signature = (x, y, tof, tot, labels, num_clusters, super_resolution=8.0, tot_weighted=true, timestamp=None, chip_id=None))]
#[allow(clippy::too_many_arguments)]
fn extract_neutrons_numpy<'py>(
    py: Python<'py>,
    x: PyReadonlyArray1<u16>,
    y: PyReadonlyArray1<u16>,
    tof: PyReadonlyArray1<u32>,
    tot: PyReadonlyArray1<u16>,
    labels: PyReadonlyArray1<i32>,
    num_clusters: usize,
    super_resolution: f64,
    tot_weighted: bool,
    timestamp: Option<PyReadonlyArray1<u32>>,
    chip_id: Option<PyReadonlyArray1<u8>>,
) -> PyResult<Bound<'py, PyAny>> {
    let labels = labels.as_slice()?;
    let mut batch = batch_from_numpy(x, y, tof, tot, timestamp, chip_id)?;
    if labels.len() != batch.len() {
        return Err(value_error(
            "extract_neutrons_numpy",
            format!(
                "Length mismatch: labels={}, hits={}",
                labels.len(),
                batch.len()
            ),
        ));
    }
    for (i, label) in labels.iter().enumerate() {
        batch.cluster_id[i] = *label;
    }

    let mut extractor = SimpleCentroidExtraction::new();
    extractor.configure(
        ExtractionConfig::default()
            .with_super_resolution(super_resolution)
            .with_weighted_by_tot(tot_weighted),
    );

    let neutrons = extractor
        .extract_soa(&batch, num_clusters)
        .map_err(|e| value_error("extract_neutrons_numpy: extract_soa", e))?;

    let x: Vec<f64> = neutrons.iter().map(|n| n.x).collect();
    let y: Vec<f64> = neutrons.iter().map(|n| n.y).collect();
    let tof: Vec<u32> = neutrons.iter().map(|n| n.tof).collect();
    let tot: Vec<u16> = neutrons.iter().map(|n| n.tot).collect();
    let n_hits: Vec<u16> = neutrons.iter().map(|n| n.n_hits).collect();
    let chip_id: Vec<u8> = neutrons.iter().map(|n| n.chip_id).collect();

    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("x", PyArray1::from_vec(py, x))?;
    dict.set_item("y", PyArray1::from_vec(py, y))?;
    dict.set_item("tof", PyArray1::from_vec(py, tof))?;
    dict.set_item("tot", PyArray1::from_vec(py, tot))?;
    dict.set_item("n_hits", PyArray1::from_vec(py, n_hits))?;
    dict.set_item("chip_id", PyArray1::from_vec(py, chip_id))?;

    Ok(dict.into_any())
}

fn cluster_from_batch(
    batch: &mut HitBatch,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
) -> PyResult<usize> {
    let config = config.unwrap_or_else(|| PyClusteringConfig::new(5.0, 75.0, 1, None));

    let algorithm = algorithm.to_lowercase();
    match algorithm.as_str() {
        "abs" => {
            let algo_config = rustpix_algorithms::AbsConfig {
                radius: config.inner.radius,
                neutron_correlation_window_ns: config.inner.temporal_window_ns,
                min_cluster_size: config.inner.min_cluster_size,
                scan_interval: 100, // default
            };
            let algo = AbsClustering::new(algo_config);
            let mut state = AbsState::default();
            algo.cluster(batch, &mut state)
                .map_err(|e| value_error(&format!("cluster_from_batch({algorithm})"), e))
        }
        "dbscan" => {
            let algo_config = rustpix_algorithms::DbscanConfig {
                epsilon: config.inner.radius, // Map radius to epsilon
                temporal_window_ns: config.inner.temporal_window_ns,
                min_points: 2, // Default or hardcoded?
                min_cluster_size: config.inner.min_cluster_size,
            };
            let algo = DbscanClustering::new(algo_config);
            let mut state = DbscanState::default();
            algo.cluster(batch, &mut state)
                .map_err(|e| value_error(&format!("cluster_from_batch({algorithm})"), e))
        }
        "grid" => {
            let algo_config = rustpix_algorithms::GridConfig {
                radius: config.inner.radius,
                temporal_window_ns: config.inner.temporal_window_ns,
                min_cluster_size: config.inner.min_cluster_size,
                cell_size: 32, // default
                max_cluster_size: config.inner.max_cluster_size.map(|s| s as usize),
            };
            let algo = GridClustering::new(algo_config);
            let mut state = GridState::default();
            algo.cluster(batch, &mut state)
                .map_err(|e| value_error(&format!("cluster_from_batch({algorithm})"), e))
        }
        "graph" => Err(pyo3::exceptions::PyValueError::new_err(
            "Algorithm 'graph' is deprecated/removed. Use 'abs', 'dbscan', or 'grid'",
        )),
        _ => Err(value_error(
            "cluster_from_batch",
            format!("Unknown algorithm: {algorithm}. Use 'abs', 'dbscan', or 'grid'"),
        )),
    }
}

/// Helper to read hits directly into batch from file (internal usage).
fn read_file_to_batch(path: &str, detector_config: Option<PyDetectorConfig>) -> PyResult<HitBatch> {
    let mut reader = Tpx3FileReader::open(path)
        .map_err(|e| io_error(&format!("read_file_to_batch: open {path}"), e))?;

    if let Some(config) = detector_config {
        reader = reader.with_config(config.inner);
    }

    reader
        .read_batch()
        .map_err(|e| value_error("read_file_to_batch: read_batch", e))
}

/// Process a TPX3 file: read, cluster, and extract neutrons.
#[pyfunction]
#[pyo3(signature = (path, config=None, algorithm="abs", super_resolution=8.0, tot_weighted=true, detector_config=None))]
fn process_tpx3_file(
    path: &str,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
    super_resolution: f64,
    tot_weighted: bool,
    detector_config: Option<PyDetectorConfig>,
) -> PyResult<Vec<PyNeutron>> {
    let mut batch = read_file_to_batch(path, detector_config)?;
    let num_clusters = cluster_from_batch(&mut batch, config, algorithm)?;

    let mut extractor = SimpleCentroidExtraction::new();
    extractor.configure(
        ExtractionConfig::default()
            .with_super_resolution(super_resolution)
            .with_weighted_by_tot(tot_weighted),
    );

    let neutrons = extractor
        .extract_soa(&batch, num_clusters)
        .map_err(|e| value_error("process_tpx3_file: extract_soa", e))?;

    Ok(neutrons
        .into_iter()
        .map(|n| PyNeutron { inner: n })
        .collect())
}

/// Process a TPX3 file and return neutrons as numpy arrays.
#[pyfunction]
#[pyo3(signature = (path, config=None, algorithm="abs", super_resolution=8.0, tot_weighted=true, detector_config=None))]
fn process_tpx3_file_numpy<'py>(
    py: Python<'py>,
    path: &str,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
    super_resolution: f64,
    tot_weighted: bool,
    detector_config: Option<PyDetectorConfig>,
) -> PyResult<Bound<'py, PyAny>> {
    // Reuse the process_tpx3_file logic but return dict
    // Actually, calling process_tpx3_file creates Vec<PyNeutron>.
    // Then we act on it.
    let neutrons_vec = process_tpx3_file(
        path,
        config,
        algorithm,
        super_resolution,
        tot_weighted,
        detector_config,
    )?;

    let x: Vec<f64> = neutrons_vec.iter().map(|n| n.inner.x).collect();
    let y: Vec<f64> = neutrons_vec.iter().map(|n| n.inner.y).collect();
    let tof: Vec<u32> = neutrons_vec.iter().map(|n| n.inner.tof).collect();
    let tot: Vec<u16> = neutrons_vec.iter().map(|n| n.inner.tot).collect();
    let n_hits: Vec<u16> = neutrons_vec.iter().map(|n| n.inner.n_hits).collect();
    let chip_id: Vec<u8> = neutrons_vec.iter().map(|n| n.inner.chip_id).collect();

    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("x", PyArray1::from_vec(py, x))?;
    dict.set_item("y", PyArray1::from_vec(py, y))?;
    dict.set_item("tof", PyArray1::from_vec(py, tof))?;
    dict.set_item("tot", PyArray1::from_vec(py, tot))?;
    dict.set_item("n_hits", PyArray1::from_vec(py, n_hits))?;
    dict.set_item("chip_id", PyArray1::from_vec(py, chip_id))?;

    Ok(dict.into_any())
}

/// Process a TPX3 file and return neutrons as a PyArrow table.
#[pyfunction]
#[pyo3(signature = (path, config=None, algorithm="abs", super_resolution=8.0, tot_weighted=true, detector_config=None))]
fn process_tpx3_file_arrow<'py>(
    py: Python<'py>,
    path: &str,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
    super_resolution: f64,
    tot_weighted: bool,
    detector_config: Option<PyDetectorConfig>,
) -> PyResult<Bound<'py, PyAny>> {
    let dict_any = process_tpx3_file_numpy(
        py,
        path,
        config,
        algorithm,
        super_resolution,
        tot_weighted,
        detector_config,
    )?;
    let dict = dict_any.downcast::<pyo3::types::PyDict>()?;
    arrow_table_from_dict(py, dict)
}

/// Python module for rustpix.
#[pymodule]
fn rustpix(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNeutron>()?;
    m.add_class::<PyClusteringConfig>()?;
    m.add_class::<PyChipTransform>()?;
    m.add_class::<PyDetectorConfig>()?;
    m.add_class::<streaming::MeasurementStream>()?;
    m.add_function(wrap_pyfunction!(read_tpx3_file_numpy, m)?)?;
    m.add_function(wrap_pyfunction!(read_tpx3_file_arrow, m)?)?;
    m.add_function(wrap_pyfunction!(cluster_hits_numpy, m)?)?;
    m.add_function(wrap_pyfunction!(extract_neutrons_numpy, m)?)?;
    m.add_function(wrap_pyfunction!(process_tpx3_file, m)?)?;
    m.add_function(wrap_pyfunction!(process_tpx3_file_numpy, m)?)?;
    m.add_function(wrap_pyfunction!(process_tpx3_file_arrow, m)?)?;
    Ok(())
}
