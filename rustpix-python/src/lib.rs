//! rustpix-python: PyO3 Python bindings for rustpix.
//!
//! This crate provides Python bindings using PyO3 and numpy
//! for efficient data exchange with Python.

use numpy::ndarray::Array1;
use numpy::PyArray1;
use pyo3::prelude::*;
use rustpix_algorithms::{
    AbsClustering, AbsState, DbscanClustering, DbscanState, GridClustering, GridState,
};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::{ExtractionConfig, NeutronExtraction, SimpleCentroidExtraction};
use rustpix_core::hit::GenericHit;
use rustpix_core::neutron::Neutron;
use rustpix_core::soa::HitBatch;
use rustpix_io::Tpx3FileReader;
use rustpix_tpx::Tpx3Hit;
pub mod streaming;

/// Python wrapper for GenericHit.
#[pyclass(name = "Hit")]
#[derive(Clone)]
pub struct PyHit {
    inner: GenericHit,
}

#[pymethods]
impl PyHit {
    #[new]
    fn new(x: u16, y: u16, tof: u32, tot: u16, timestamp: u32, chip_id: u8) -> Self {
        Self {
            inner: GenericHit {
                x,
                y,
                tof,
                tot,
                timestamp,
                chip_id,
                _padding: 0,
                cluster_id: -1,
            },
        }
    }

    #[getter]
    fn x(&self) -> u16 {
        self.inner.x
    }

    #[getter]
    fn y(&self) -> u16 {
        self.inner.y
    }

    #[getter]
    fn tof(&self) -> u32 {
        self.inner.tof
    }

    #[getter]
    fn tot(&self) -> u16 {
        self.inner.tot
    }

    #[getter]
    fn timestamp(&self) -> u32 {
        self.inner.timestamp
    }

    #[getter]
    fn chip_id(&self) -> u8 {
        self.inner.chip_id
    }

    fn __repr__(&self) -> String {
        format!(
            "Hit(x={}, y={}, tof={}, tot={}, chip={})",
            self.inner.x, self.inner.y, self.inner.tof, self.inner.tot, self.inner.chip_id
        )
    }
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
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

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
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        rustpix_tpx::DetectorConfig::from_json(json)
            .map(|config| Self { inner: config })
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

/// Read hits from a TPX3 file.
#[pyfunction]
#[pyo3(signature = (path, detector_config=None))]
fn read_tpx3_file(path: &str, detector_config: Option<PyDetectorConfig>) -> PyResult<Vec<PyHit>> {
    let mut reader = Tpx3FileReader::open(path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

    if let Some(config) = detector_config {
        reader = reader.with_config(config.inner);
    }

    let hits = reader
        .read_hits()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    Ok(hits
        .into_iter()
        .map(|h| PyHit {
            inner: GenericHit {
                x: h.x,
                y: h.y,
                tof: h.tof,
                tot: h.tot,
                timestamp: h.timestamp,
                chip_id: h.chip_id,
                _padding: 0,
                cluster_id: -1,
            },
        })
        .collect())
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
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

    if let Some(config) = detector_config {
        reader = reader.with_config(config.inner);
    }

    let hits: Vec<Tpx3Hit> = reader
        .read_hits()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    // Create separate arrays for each field
    let x: Array1<u16> = hits.iter().map(|h| h.x).collect();
    let y: Array1<u16> = hits.iter().map(|h| h.y).collect();
    let tof: Array1<u32> = hits.iter().map(|h| h.tof).collect();
    let tot: Array1<u16> = hits.iter().map(|h| h.tot).collect();
    let chip_id: Array1<u8> = hits.iter().map(|h| h.chip_id).collect();

    let x_arr = PyArray1::from_array(py, &x);
    let y_arr = PyArray1::from_array(py, &y);
    let tof_arr = PyArray1::from_array(py, &tof);
    let tot_arr = PyArray1::from_array(py, &tot);
    let chip_arr = PyArray1::from_array(py, &chip_id);

    // Return as a dictionary
    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("x", x_arr)?;
    dict.set_item("y", y_arr)?;
    dict.set_item("tof", tof_arr)?;
    dict.set_item("tot", tot_arr)?;
    dict.set_item("chip_id", chip_arr)?;

    Ok(dict.into_any())
}

/// Cluster hits using the specified algorithm.
#[pyfunction]
#[pyo3(signature = (hits, config=None, algorithm="abs"))]
fn cluster_hits(
    hits: Vec<PyHit>,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
) -> PyResult<(Vec<i32>, usize)> {
    cluster_hits_impl(&hits, config, algorithm)
}

fn cluster_hits_impl(
    hits: &[PyHit],
    config: Option<PyClusteringConfig>,
    algorithm: &str,
) -> PyResult<(Vec<i32>, usize)> {
    let config = config.unwrap_or_else(|| PyClusteringConfig::new(5.0, 75.0, 1, None));

    // Convert to SoA Batch
    let mut batch = HitBatch::with_capacity(hits.len());
    for hit in hits {
        batch.push(
            hit.inner.x,
            hit.inner.y,
            hit.inner.tof,
            hit.inner.tot,
            hit.inner.timestamp,
            hit.inner.chip_id,
        );
    }

    let num_clusters = match algorithm.to_lowercase().as_str() {
        "abs" => {
            let algo_config = rustpix_algorithms::AbsConfig {
                radius: config.inner.radius,
                neutron_correlation_window_ns: config.inner.temporal_window_ns,
                min_cluster_size: config.inner.min_cluster_size,
                scan_interval: 100, // default
            };
            let algo = AbsClustering::new(algo_config);
            let mut state = AbsState::default();
            algo.cluster(&mut batch, &mut state)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
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
            algo.cluster(&mut batch, &mut state)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
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
            algo.cluster(&mut batch, &mut state)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
        }
        "graph" => {
            // Fallback or Error? Plan said remove.
            // But if user passes 'graph', we should probably error or fallback to abs/grid?
            // "Acceptance Criteria: ... Graph algorithm - Either create SoA version OR remove it entirely"
            // "API Changes: ... CLI algorithm argument will no longer accept graph."
            // For Python, if I remove it, I should throw error.
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Algorithm 'graph' is deprecated/removed. Use 'abs', 'dbscan', or 'grid'",
            ));
        }
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Unknown algorithm: {}. Use 'abs', 'dbscan', or 'grid'",
                algorithm
            )))
        }
    };

    // Extract labels
    // HitBatch stores cluster_id as i32, so we can return it directly.
    let labels = batch.cluster_id.clone();

    Ok((labels, num_clusters))
}

/// Extract neutrons from clustered hits.
#[pyfunction]
#[pyo3(signature = (hits, labels, num_clusters, super_resolution=8.0, tot_weighted=true))]
fn extract_neutrons(
    hits: Vec<PyHit>,
    labels: Vec<i32>,
    num_clusters: usize,
    super_resolution: f64,
    tot_weighted: bool,
) -> PyResult<Vec<PyNeutron>> {
    let mut extractor = SimpleCentroidExtraction::new();
    extractor.configure(
        ExtractionConfig::default()
            .with_super_resolution(super_resolution)
            .with_weighted_by_tot(tot_weighted),
    );

    let hit_data: Vec<GenericHit> = hits.iter().map(|h| h.inner).collect();

    let neutrons = extractor
        .extract(&hit_data, &labels, num_clusters)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    Ok(neutrons
        .into_iter()
        .map(|n| PyNeutron { inner: n })
        .collect())
}

/// Extract neutrons and return as numpy arrays.
#[pyfunction]
#[pyo3(signature = (hits, labels, num_clusters, super_resolution=8.0, tot_weighted=true))]
fn extract_neutrons_numpy<'py>(
    py: Python<'py>,
    hits: Vec<PyHit>,
    labels: Vec<i32>,
    num_clusters: usize,
    super_resolution: f64,
    tot_weighted: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let neutrons = extract_neutrons(hits, labels, num_clusters, super_resolution, tot_weighted)?;

    let x: Array1<f64> = neutrons.iter().map(|n| n.inner.x).collect();
    let y: Array1<f64> = neutrons.iter().map(|n| n.inner.y).collect();
    let tof: Array1<u32> = neutrons.iter().map(|n| n.inner.tof).collect();
    let tot: Array1<u16> = neutrons.iter().map(|n| n.inner.tot).collect();
    let n_hits: Array1<u16> = neutrons.iter().map(|n| n.inner.n_hits).collect();
    let chip_id: Array1<u8> = neutrons.iter().map(|n| n.inner.chip_id).collect();

    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("x", PyArray1::from_array(py, &x))?;
    dict.set_item("y", PyArray1::from_array(py, &y))?;
    dict.set_item("tof", PyArray1::from_array(py, &tof))?;
    dict.set_item("tot", PyArray1::from_array(py, &tot))?;
    dict.set_item("n_hits", PyArray1::from_array(py, &n_hits))?;
    dict.set_item("chip_id", PyArray1::from_array(py, &chip_id))?;

    Ok(dict.into_any())
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
    let hits = read_tpx3_file(path, detector_config)?;
    let (labels, num_clusters) = cluster_hits_impl(&hits, config, algorithm)?;
    extract_neutrons(hits, labels, num_clusters, super_resolution, tot_weighted)
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
    let hits = read_tpx3_file(path, detector_config)?;
    let (labels, num_clusters) = cluster_hits_impl(&hits, config, algorithm)?;
    extract_neutrons_numpy(
        py,
        hits,
        labels,
        num_clusters,
        super_resolution,
        tot_weighted,
    )
}

/// Python module for rustpix.
#[pymodule]
fn rustpix(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyHit>()?;
    m.add_class::<PyNeutron>()?;
    m.add_class::<PyClusteringConfig>()?;
    m.add_class::<PyChipTransform>()?;
    m.add_class::<PyDetectorConfig>()?;
    m.add_class::<streaming::MeasurementStream>()?;
    m.add_function(wrap_pyfunction!(read_tpx3_file, m)?)?;
    m.add_function(wrap_pyfunction!(read_tpx3_file_numpy, m)?)?;
    m.add_function(wrap_pyfunction!(cluster_hits, m)?)?;
    m.add_function(wrap_pyfunction!(extract_neutrons, m)?)?;
    m.add_function(wrap_pyfunction!(extract_neutrons_numpy, m)?)?;
    m.add_function(wrap_pyfunction!(process_tpx3_file, m)?)?;
    m.add_function(wrap_pyfunction!(process_tpx3_file_numpy, m)?)?;
    Ok(())
}
