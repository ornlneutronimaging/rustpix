//! rustpix-python: PyO3 Python bindings for rustpix.
//!
//! This crate provides Python bindings using PyO3 and numpy
//! for efficient data exchange with Python.

use numpy::ndarray::Array1;
use numpy::PyArray1;
use pyo3::prelude::*;
use rustpix_algorithms::{AbsClustering, DbscanClustering, GraphClustering, GridClustering};
use rustpix_core::{
    Centroid, CentroidExtractor, Cluster, ClusteringAlgorithm, ClusteringConfig, ExtractionConfig,
    HitData, WeightedCentroidExtractor,
};
use rustpix_io::Tpx3FileReader;
use rustpix_tpx::Tpx3Hit;

/// Python wrapper for HitData.
#[pyclass(name = "Hit")]
#[derive(Clone)]
pub struct PyHit {
    inner: HitData,
}

#[pymethods]
impl PyHit {
    #[new]
    fn new(x: u16, y: u16, toa: u64, tot: u16) -> Self {
        Self {
            inner: HitData::new(x, y, toa, tot),
        }
    }

    #[getter]
    fn x(&self) -> u16 {
        self.inner.coord.x
    }

    #[getter]
    fn y(&self) -> u16 {
        self.inner.coord.y
    }

    #[getter]
    fn toa(&self) -> u64 {
        self.inner.toa.as_u64()
    }

    #[getter]
    fn tot(&self) -> u16 {
        self.inner.tot
    }

    fn __repr__(&self) -> String {
        format!(
            "Hit(x={}, y={}, toa={}, tot={})",
            self.inner.coord.x,
            self.inner.coord.y,
            self.inner.toa.as_u64(),
            self.inner.tot
        )
    }
}

/// Python wrapper for Centroid.
#[pyclass(name = "Centroid")]
#[derive(Clone)]
pub struct PyCentroid {
    inner: Centroid,
}

#[pymethods]
impl PyCentroid {
    #[new]
    fn new(x: f64, y: f64, toa: u64, tot_sum: u32, cluster_size: u16) -> Self {
        Self {
            inner: Centroid::new(x, y, toa, tot_sum, cluster_size),
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
    fn toa(&self) -> u64 {
        self.inner.toa.as_u64()
    }

    #[getter]
    fn tot_sum(&self) -> u32 {
        self.inner.tot_sum
    }

    #[getter]
    fn cluster_size(&self) -> u16 {
        self.inner.cluster_size
    }

    fn __repr__(&self) -> String {
        format!(
            "Centroid(x={:.2}, y={:.2}, toa={}, tot_sum={}, cluster_size={})",
            self.inner.x,
            self.inner.y,
            self.inner.toa.as_u64(),
            self.inner.tot_sum,
            self.inner.cluster_size
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
    #[pyo3(signature = (spatial_epsilon=1.5, temporal_epsilon=1000, min_cluster_size=1, max_cluster_size=None))]
    fn new(
        spatial_epsilon: f64,
        temporal_epsilon: u64,
        min_cluster_size: usize,
        max_cluster_size: Option<usize>,
    ) -> Self {
        Self {
            inner: ClusteringConfig {
                spatial_epsilon,
                temporal_epsilon,
                min_cluster_size,
                max_cluster_size,
            },
        }
    }

    #[getter]
    fn spatial_epsilon(&self) -> f64 {
        self.inner.spatial_epsilon
    }

    #[getter]
    fn temporal_epsilon(&self) -> u64 {
        self.inner.temporal_epsilon
    }

    #[getter]
    fn min_cluster_size(&self) -> usize {
        self.inner.min_cluster_size
    }

    #[getter]
    fn max_cluster_size(&self) -> Option<usize> {
        self.inner.max_cluster_size
    }
}

/// Read hits from a TPX3 file.
#[pyfunction]
fn read_tpx3_file(path: &str) -> PyResult<Vec<PyHit>> {
    let reader = Tpx3FileReader::open(path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

    let hits = reader
        .read_hits()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    Ok(hits
        .into_iter()
        .map(|h| PyHit { inner: h.into() })
        .collect())
}

/// Read hits from a TPX3 file and return as numpy structured arrays.
#[pyfunction]
fn read_tpx3_file_numpy<'py>(py: Python<'py>, path: &str) -> PyResult<Bound<'py, PyAny>> {
    let reader = Tpx3FileReader::open(path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

    let hits: Vec<Tpx3Hit> = reader
        .read_hits()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    // Create separate arrays for each field
    let x: Array1<u16> = hits.iter().map(|h| h.x).collect();
    let y: Array1<u16> = hits.iter().map(|h| h.y).collect();
    let toa: Array1<u64> = hits.iter().map(|h| h.toa).collect();
    let tot: Array1<u16> = hits.iter().map(|h| h.tot).collect();

    let x_arr = PyArray1::from_array(py, &x);
    let y_arr = PyArray1::from_array(py, &y);
    let toa_arr = PyArray1::from_array(py, &toa);
    let tot_arr = PyArray1::from_array(py, &tot);

    // Return as a dictionary
    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("x", x_arr)?;
    dict.set_item("y", y_arr)?;
    dict.set_item("toa", toa_arr)?;
    dict.set_item("tot", tot_arr)?;

    Ok(dict.into_any())
}

/// Cluster hits using the specified algorithm.
#[pyfunction]
#[pyo3(signature = (hits, config=None, algorithm="abs"))]
fn cluster_hits(
    hits: Vec<PyHit>,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
) -> PyResult<Vec<Vec<PyHit>>> {
    let config = config.unwrap_or_else(|| PyClusteringConfig::new(1.5, 1000, 1, None));
    let hit_data: Vec<HitData> = hits.iter().map(|h| h.inner).collect();

    let clusters: Vec<Cluster<HitData>> = match algorithm.to_lowercase().as_str() {
        "abs" => AbsClustering::new()
            .cluster(&hit_data, &config.inner)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        "dbscan" => DbscanClustering::new()
            .cluster(&hit_data, &config.inner)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        "graph" => GraphClustering::new()
            .cluster(&hit_data, &config.inner)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        "grid" => GridClustering::new()
            .cluster(&hit_data, &config.inner)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Unknown algorithm: {}. Use 'abs', 'dbscan', 'graph', or 'grid'",
                algorithm
            )))
        }
    };

    Ok(clusters
        .into_iter()
        .map(|c| c.hits.into_iter().map(|h| PyHit { inner: h }).collect())
        .collect())
}

/// Extract centroids from clusters.
#[pyfunction]
#[pyo3(signature = (clusters, tot_weighted=true))]
fn extract_centroids(clusters: Vec<Vec<PyHit>>, tot_weighted: bool) -> PyResult<Vec<PyCentroid>> {
    let extractor = WeightedCentroidExtractor::new();
    let config = ExtractionConfig::new().with_tot_weighted(tot_weighted);

    let mut centroids = Vec::with_capacity(clusters.len());

    for cluster_hits in clusters {
        let hit_data: Vec<HitData> = cluster_hits.iter().map(|h| h.inner).collect();
        let cluster: Cluster<HitData> = hit_data.into_iter().collect();

        let centroid = extractor
            .extract(&cluster, &config)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        centroids.push(PyCentroid { inner: centroid });
    }

    Ok(centroids)
}

/// Extract centroids and return as numpy arrays.
#[pyfunction]
#[pyo3(signature = (clusters, tot_weighted=true))]
fn extract_centroids_numpy<'py>(
    py: Python<'py>,
    clusters: Vec<Vec<PyHit>>,
    tot_weighted: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let centroids = extract_centroids(clusters, tot_weighted)?;

    let x: Array1<f64> = centroids.iter().map(|c| c.inner.x).collect();
    let y: Array1<f64> = centroids.iter().map(|c| c.inner.y).collect();
    let toa: Array1<u64> = centroids.iter().map(|c| c.inner.toa.as_u64()).collect();
    let tot_sum: Array1<u32> = centroids.iter().map(|c| c.inner.tot_sum).collect();
    let cluster_size: Array1<u16> = centroids.iter().map(|c| c.inner.cluster_size).collect();

    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("x", PyArray1::from_array(py, &x))?;
    dict.set_item("y", PyArray1::from_array(py, &y))?;
    dict.set_item("toa", PyArray1::from_array(py, &toa))?;
    dict.set_item("tot_sum", PyArray1::from_array(py, &tot_sum))?;
    dict.set_item("cluster_size", PyArray1::from_array(py, &cluster_size))?;

    Ok(dict.into_any())
}

/// Process a TPX3 file: read, cluster, and extract centroids.
#[pyfunction]
#[pyo3(signature = (path, config=None, algorithm="abs", tot_weighted=true))]
fn process_tpx3_file(
    path: &str,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
    tot_weighted: bool,
) -> PyResult<Vec<PyCentroid>> {
    let hits = read_tpx3_file(path)?;
    let clusters = cluster_hits(hits, config, algorithm)?;
    extract_centroids(clusters, tot_weighted)
}

/// Process a TPX3 file and return centroids as numpy arrays.
#[pyfunction]
#[pyo3(signature = (path, config=None, algorithm="abs", tot_weighted=true))]
fn process_tpx3_file_numpy<'py>(
    py: Python<'py>,
    path: &str,
    config: Option<PyClusteringConfig>,
    algorithm: &str,
    tot_weighted: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let hits = read_tpx3_file(path)?;
    let clusters = cluster_hits(hits, config, algorithm)?;
    extract_centroids_numpy(py, clusters, tot_weighted)
}

/// Python module for rustpix.
#[pymodule]
fn rustpix(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyHit>()?;
    m.add_class::<PyCentroid>()?;
    m.add_class::<PyClusteringConfig>()?;
    m.add_function(wrap_pyfunction!(read_tpx3_file, m)?)?;
    m.add_function(wrap_pyfunction!(read_tpx3_file_numpy, m)?)?;
    m.add_function(wrap_pyfunction!(cluster_hits, m)?)?;
    m.add_function(wrap_pyfunction!(extract_centroids, m)?)?;
    m.add_function(wrap_pyfunction!(extract_centroids_numpy, m)?)?;
    m.add_function(wrap_pyfunction!(process_tpx3_file, m)?)?;
    m.add_function(wrap_pyfunction!(process_tpx3_file_numpy, m)?)?;
    Ok(())
}
