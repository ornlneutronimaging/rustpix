//! Thin Python bindings for rustpix.

use numpy::PyArray1;
use pyo3::exceptions::{PyImportError, PyNotImplementedError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use rustpix_algorithms::{
    cluster_and_extract_batch, cluster_and_extract_stream, cluster_and_extract_stream_iter,
    AlgorithmParams, ClusteringAlgorithm,
};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;
use rustpix_io::{
    out_of_core_neutron_stream, OutOfCoreConfig, TimeOrderedHitStream, Tpx3FileReader,
};
use rustpix_tpx::{ChipTransform, DetectorConfig};

type ChipTransformTuple = (i32, i32, i32, i32, i32, i32);
type NeutronStreamItem = std::result::Result<NeutronBatch, String>;
type NeutronStream = Box<dyn Iterator<Item = NeutronStreamItem>>;

#[derive(Clone)]
struct BatchMetadata {
    detector: DetectorConfig,
    clustering: Option<ClusteringConfig>,
    extraction: Option<ExtractionConfig>,
    algorithm: Option<String>,
    source_path: Option<String>,
    time_ordered: bool,
}

impl BatchMetadata {
    fn to_pydict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item(
            "detector_config",
            detector_config_to_dict(py, &self.detector)?,
        )?;
        if let Some(ref clustering) = self.clustering {
            dict.set_item(
                "clustering_config",
                clustering_config_to_dict(py, clustering)?,
            )?;
        }
        if let Some(ref extraction) = self.extraction {
            dict.set_item(
                "extraction_config",
                extraction_config_to_dict(py, extraction)?,
            )?;
        }
        if let Some(ref algorithm) = self.algorithm {
            dict.set_item("algorithm", algorithm)?;
        }
        if let Some(ref source_path) = self.source_path {
            dict.set_item("source_path", source_path)?;
        }
        dict.set_item("time_ordered", self.time_ordered)?;
        Ok(dict.into_any().unbind())
    }
}

#[pyclass(name = "DetectorConfig")]
#[derive(Clone)]
struct PyDetectorConfig {
    inner: DetectorConfig,
}

#[pymethods]
impl PyDetectorConfig {
    #[new]
    #[pyo3(signature = (
        tdc_frequency_hz=None,
        enable_missing_tdc_correction=None,
        chip_size_x=None,
        chip_size_y=None,
        chip_transforms=None
    ))]
    fn new(
        tdc_frequency_hz: Option<f64>,
        enable_missing_tdc_correction: Option<bool>,
        chip_size_x: Option<u16>,
        chip_size_y: Option<u16>,
        chip_transforms: Option<Vec<ChipTransformTuple>>,
    ) -> Self {
        let mut config = DetectorConfig::default();
        if let Some(value) = tdc_frequency_hz {
            config.tdc_frequency_hz = value;
        }
        if let Some(value) = enable_missing_tdc_correction {
            config.enable_missing_tdc_correction = value;
        }
        if let Some(value) = chip_size_x {
            config.chip_size_x = value;
        }
        if let Some(value) = chip_size_y {
            config.chip_size_y = value;
        }
        if let Some(transforms) = chip_transforms {
            config.chip_transforms = transforms
                .into_iter()
                .map(|(a, b, c, d, tx, ty)| ChipTransform { a, b, c, d, tx, ty })
                .collect();
        }
        Self { inner: config }
    }

    #[staticmethod]
    fn venus_defaults() -> Self {
        Self {
            inner: DetectorConfig::venus_defaults(),
        }
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        DetectorConfig::from_json(json)
            .map(|config| Self { inner: config })
            .map_err(|err| PyValueError::new_err(err.to_string()))
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        detector_config_to_dict(py, &self.inner)
    }
}

#[pyclass(name = "ClusteringConfig")]
#[derive(Clone)]
struct PyClusteringConfig {
    inner: ClusteringConfig,
}

#[pymethods]
impl PyClusteringConfig {
    #[new]
    #[pyo3(signature = (radius=None, temporal_window_ns=None, min_cluster_size=None, max_cluster_size=None))]
    fn new(
        radius: Option<f64>,
        temporal_window_ns: Option<f64>,
        min_cluster_size: Option<u16>,
        max_cluster_size: Option<u16>,
    ) -> Self {
        let mut config = ClusteringConfig::default();
        if let Some(value) = radius {
            config.radius = value;
        }
        if let Some(value) = temporal_window_ns {
            config.temporal_window_ns = value;
        }
        if let Some(value) = min_cluster_size {
            config.min_cluster_size = value;
        }
        if let Some(value) = max_cluster_size {
            config.max_cluster_size = Some(value);
        }
        Self { inner: config }
    }

    #[staticmethod]
    fn venus_defaults() -> Self {
        Self {
            inner: ClusteringConfig::venus_defaults(),
        }
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        clustering_config_to_dict(py, &self.inner)
    }
}

#[pyclass(name = "ExtractionConfig")]
#[derive(Clone)]
struct PyExtractionConfig {
    inner: ExtractionConfig,
}

#[pymethods]
impl PyExtractionConfig {
    #[new]
    #[pyo3(signature = (super_resolution_factor=None, weighted_by_tot=None, min_tot_threshold=None))]
    fn new(
        super_resolution_factor: Option<f64>,
        weighted_by_tot: Option<bool>,
        min_tot_threshold: Option<u16>,
    ) -> Self {
        let mut config = ExtractionConfig::default();
        if let Some(value) = super_resolution_factor {
            config.super_resolution_factor = value;
        }
        if let Some(value) = weighted_by_tot {
            config.weighted_by_tot = value;
        }
        if let Some(value) = min_tot_threshold {
            config.min_tot_threshold = value;
        }
        Self { inner: config }
    }

    #[staticmethod]
    fn venus_defaults() -> Self {
        Self {
            inner: ExtractionConfig::venus_defaults(),
        }
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        extraction_config_to_dict(py, &self.inner)
    }
}

#[pyclass(name = "HitBatch")]
struct PyHitBatch {
    batch: Option<HitBatch>,
    metadata: BatchMetadata,
}

#[pymethods]
impl PyHitBatch {
    fn len(&self) -> usize {
        self.batch.as_ref().map_or(0, HitBatch::len)
    }

    fn is_empty(&self) -> bool {
        self.batch.as_ref().is_none_or(HitBatch::is_empty)
    }

    fn metadata(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.metadata.to_pydict(py)
    }

    /// Convert the batch to NumPy arrays (moves the underlying buffers).
    #[pyo3(name = "to_numpy")]
    fn take_numpy(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let batch = self
            .batch
            .take()
            .ok_or_else(|| PyValueError::new_err("HitBatch data has already been moved"))?;

        let HitBatch {
            x,
            y,
            tof,
            tot,
            timestamp,
            chip_id,
            cluster_id,
        } = batch;

        let dict = PyDict::new(py);
        dict.set_item("x", PyArray1::from_vec(py, x))?;
        dict.set_item("y", PyArray1::from_vec(py, y))?;
        dict.set_item("tof", PyArray1::from_vec(py, tof))?;
        dict.set_item("tot", PyArray1::from_vec(py, tot))?;
        dict.set_item("timestamp", PyArray1::from_vec(py, timestamp))?;
        dict.set_item("chip_id", PyArray1::from_vec(py, chip_id))?;
        dict.set_item("cluster_id", PyArray1::from_vec(py, cluster_id))?;
        Ok(dict.into_any().unbind())
    }

    /// Convert the batch to a PyArrow Table (moves the underlying buffers).
    #[pyo3(name = "to_arrow")]
    fn take_arrow(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let batch = self
            .batch
            .take()
            .ok_or_else(|| PyValueError::new_err("HitBatch data has already been moved"))?;

        let HitBatch {
            x,
            y,
            tof,
            tot,
            timestamp,
            chip_id,
            cluster_id,
        } = batch;

        let arrays = vec![
            PyArray1::from_vec(py, x).into_any().unbind(),
            PyArray1::from_vec(py, y).into_any().unbind(),
            PyArray1::from_vec(py, tof).into_any().unbind(),
            PyArray1::from_vec(py, tot).into_any().unbind(),
            PyArray1::from_vec(py, timestamp).into_any().unbind(),
            PyArray1::from_vec(py, chip_id).into_any().unbind(),
            PyArray1::from_vec(py, cluster_id).into_any().unbind(),
        ];

        pyarrow_table_from_numpy(
            py,
            &arrays,
            &["x", "y", "tof", "tot", "timestamp", "chip_id", "cluster_id"],
        )
    }

    fn __repr__(&self) -> String {
        format!("HitBatch(len={})", self.len())
    }
}

#[pyclass(name = "NeutronBatch")]
struct PyNeutronBatch {
    batch: Option<NeutronBatch>,
    metadata: BatchMetadata,
}

#[pymethods]
impl PyNeutronBatch {
    fn len(&self) -> usize {
        self.batch.as_ref().map_or(0, NeutronBatch::len)
    }

    fn is_empty(&self) -> bool {
        self.batch.as_ref().is_none_or(NeutronBatch::is_empty)
    }

    fn metadata(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.metadata.to_pydict(py)
    }

    /// Convert neutrons to NumPy arrays (SoA layout).
    #[pyo3(name = "to_numpy")]
    fn take_numpy(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let batch = self
            .batch
            .take()
            .ok_or_else(|| PyValueError::new_err("NeutronBatch data has already been moved"))?;

        let NeutronBatch {
            x,
            y,
            tof,
            tot,
            n_hits,
            chip_id,
        } = batch;

        let dict = PyDict::new(py);
        dict.set_item("x", PyArray1::from_vec(py, x))?;
        dict.set_item("y", PyArray1::from_vec(py, y))?;
        dict.set_item("tof", PyArray1::from_vec(py, tof))?;
        dict.set_item("tot", PyArray1::from_vec(py, tot))?;
        dict.set_item("n_hits", PyArray1::from_vec(py, n_hits))?;
        dict.set_item("chip_id", PyArray1::from_vec(py, chip_id))?;
        Ok(dict.into_any().unbind())
    }

    /// Convert neutrons to a PyArrow Table (moves the underlying buffers).
    #[pyo3(name = "to_arrow")]
    fn take_arrow(&mut self, py: Python<'_>) -> PyResult<PyObject> {
        let batch = self
            .batch
            .take()
            .ok_or_else(|| PyValueError::new_err("NeutronBatch data has already been moved"))?;

        let NeutronBatch {
            x,
            y,
            tof,
            tot,
            n_hits,
            chip_id,
        } = batch;

        let arrays = vec![
            PyArray1::from_vec(py, x).into_any().unbind(),
            PyArray1::from_vec(py, y).into_any().unbind(),
            PyArray1::from_vec(py, tof).into_any().unbind(),
            PyArray1::from_vec(py, tot).into_any().unbind(),
            PyArray1::from_vec(py, n_hits).into_any().unbind(),
            PyArray1::from_vec(py, chip_id).into_any().unbind(),
        ];

        pyarrow_table_from_numpy(py, &arrays, &["x", "y", "tof", "tot", "n_hits", "chip_id"])
    }

    fn __repr__(&self) -> String {
        format!("NeutronBatch(len={})", self.len())
    }
}

#[pyclass(name = "HitBatchStream", unsendable)]
struct PyHitBatchStream {
    stream: TimeOrderedHitStream,
    metadata: BatchMetadata,
}

#[pymethods]
impl PyHitBatchStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> Option<PyHitBatch> {
        self.stream.next().map(|batch| PyHitBatch {
            batch: Some(batch),
            metadata: self.metadata.clone(),
        })
    }
}

#[pyclass(name = "NeutronBatchStream", unsendable)]
struct PyNeutronBatchStream {
    stream: NeutronStream,
    metadata: BatchMetadata,
}

#[pymethods]
impl PyNeutronBatchStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> PyResult<Option<PyNeutronBatch>> {
        match self.stream.next() {
            None => Ok(None),
            Some(Ok(batch)) => Ok(Some(PyNeutronBatch {
                batch: Some(batch),
                metadata: self.metadata.clone(),
            })),
            Some(Err(err)) => Err(PyRuntimeError::new_err(err)),
        }
    }
}

#[pyfunction]
#[pyo3(signature = (path, detector_config=None, output_path=None))]
/// Read TPX3 hits as a single batch (always time-ordered).
fn read_tpx3_hits(
    path: &str,
    detector_config: Option<PyRef<'_, PyDetectorConfig>>,
    output_path: Option<&str>,
) -> PyResult<PyHitBatch> {
    ensure_hdf5_disabled(output_path)?;
    let config = detector_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();

    let reader = Tpx3FileReader::open(path)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        .with_config(config.clone());

    let batch = reader
        .read_batch()
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;

    Ok(PyHitBatch {
        batch: Some(batch),
        metadata: BatchMetadata {
            detector: config,
            clustering: None,
            extraction: None,
            algorithm: None,
            source_path: Some(path.to_string()),
            time_ordered: true,
        },
    })
}

/// Process a TPX3 file into neutrons.
///
/// By default this returns a streaming iterator (`NeutronBatchStream`) that yields
/// pulse-bounded batches to keep memory usage bounded. Use `collect=True` to return
/// a single `NeutronBatch` for small files.
///
/// Additional kwargs:
/// - out_of_core (bool): enable the out-of-core pipeline (default: True for streaming).
/// - memory_fraction (float): fraction of available RAM to target (default: 0.5).
/// - memory_budget_bytes (int): explicit memory budget override.
#[pyfunction]
#[pyo3(signature = (path, detector_config=None, clustering_config=None, extraction_config=None, collect=false, **kwargs))]
fn process_tpx3_neutrons(
    py: Python<'_>,
    path: &str,
    detector_config: Option<PyRef<'_, PyDetectorConfig>>,
    clustering_config: Option<PyRef<'_, PyClusteringConfig>>,
    extraction_config: Option<PyRef<'_, PyExtractionConfig>>,
    collect: bool,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyObject> {
    let processing = parse_processing_kwargs(kwargs)?;
    ensure_hdf5_disabled(processing.output_path.as_deref())?;

    let detector = detector_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();
    let clustering = clustering_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();
    let extraction = extraction_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();

    let params = processing.selection.params;
    let algo = processing.selection.algorithm;

    let reader = Tpx3FileReader::open(path)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        .with_config(detector.clone());

    if !collect && !processing.time_ordered {
        return Err(PyValueError::new_err(
            "Streaming mode (collect=False) requires time_ordered=True; set collect=True to return a full batch with time_ordered=False",
        ));
    }

    if collect
        && (processing.out_of_core.enabled == Some(true) || processing.out_of_core.has_overrides())
    {
        return Err(PyValueError::new_err(
            "out_of_core is only supported when collect=False",
        ));
    }

    if collect {
        let neutrons = if processing.time_ordered {
            let stream = reader
                .stream_time_ordered()
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            cluster_and_extract_stream(stream, algo, &clustering, &extraction, &params)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        } else {
            let mut batch = reader
                .read_batch()
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            cluster_and_extract_batch(&mut batch, algo, &clustering, &extraction, &params)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        };

        let batch = PyNeutronBatch {
            batch: Some(neutrons),
            metadata: BatchMetadata {
                detector,
                clustering: Some(clustering),
                extraction: Some(extraction),
                algorithm: Some(processing.selection.name),
                source_path: Some(path.to_string()),
                time_ordered: processing.time_ordered,
            },
        };
        Ok(Py::new(py, batch)?.into_any())
    } else {
        let use_out_of_core = processing.out_of_core.enabled.unwrap_or(true);
        if !use_out_of_core && processing.out_of_core.has_overrides() {
            return Err(PyValueError::new_err(
                "memory_fraction/memory_budget_bytes require out_of_core=True",
            ));
        }

        let stream: NeutronStream = if use_out_of_core {
            let mut memory = OutOfCoreConfig::default();
            if let Some(fraction) = processing.out_of_core.memory_fraction {
                memory = memory.with_memory_fraction(fraction);
            }
            if let Some(bytes) = processing.out_of_core.memory_budget_bytes {
                memory = memory.with_memory_budget_bytes(bytes);
            }

            let stream = out_of_core_neutron_stream(
                &reader,
                algo,
                &clustering,
                &extraction,
                &params,
                &memory,
            )
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Box::new(stream.map(|result| {
                result
                    .map(|batch| batch.neutrons)
                    .map_err(|err| err.to_string())
            }))
        } else {
            let stream = reader
                .stream_time_ordered()
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            let stream = cluster_and_extract_stream_iter(
                stream,
                algo,
                clustering.clone(),
                extraction.clone(),
                params,
            );
            Box::new(stream.map(|result| result.map_err(|err| err.to_string())))
        };

        let stream = PyNeutronBatchStream {
            stream,
            metadata: BatchMetadata {
                detector,
                clustering: Some(clustering),
                extraction: Some(extraction),
                algorithm: Some(processing.selection.name),
                source_path: Some(path.to_string()),
                time_ordered: true,
            },
        };
        Ok(Py::new(py, stream)?.into_any())
    }
}

#[pyfunction]
#[pyo3(signature = (batch, clustering_config=None, extraction_config=None, **kwargs))]
fn cluster_hits(
    mut batch: PyRefMut<'_, PyHitBatch>,
    clustering_config: Option<PyRef<'_, PyClusteringConfig>>,
    extraction_config: Option<PyRef<'_, PyExtractionConfig>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyNeutronBatch> {
    let selection = parse_algorithm_kwargs(kwargs)?;
    let output_path = parse_output_path(kwargs)?;
    ensure_hdf5_disabled(output_path.as_deref())?;

    let clustering = clustering_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();
    let extraction = extraction_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();

    let params = selection.params;
    let algo = selection.algorithm;

    let batch_ref = batch
        .batch
        .as_mut()
        .ok_or_else(|| PyValueError::new_err("HitBatch data has already been moved"))?;

    let neutrons = cluster_and_extract_batch(batch_ref, algo, &clustering, &extraction, &params)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;

    Ok(PyNeutronBatch {
        batch: Some(neutrons),
        metadata: BatchMetadata {
            detector: batch.metadata.detector.clone(),
            clustering: Some(clustering),
            extraction: Some(extraction),
            algorithm: Some(selection.name),
            source_path: batch.metadata.source_path.clone(),
            time_ordered: batch.metadata.time_ordered,
        },
    })
}

#[pyfunction]
#[pyo3(signature = (path, detector_config=None, clustering_config=None, extraction_config=None, **kwargs))]
fn stream_tpx3_neutrons(
    path: &str,
    detector_config: Option<PyRef<'_, PyDetectorConfig>>,
    clustering_config: Option<PyRef<'_, PyClusteringConfig>>,
    extraction_config: Option<PyRef<'_, PyExtractionConfig>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyNeutronBatchStream> {
    let selection = parse_algorithm_kwargs(kwargs)?;
    let output_path = parse_output_path(kwargs)?;
    let out_of_core = parse_out_of_core_kwargs(kwargs)?;
    ensure_hdf5_disabled(output_path.as_deref())?;

    let detector = detector_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();
    let clustering = clustering_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();
    let extraction = extraction_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();

    let params = selection.params;
    let algo = selection.algorithm;

    let reader = Tpx3FileReader::open(path)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        .with_config(detector.clone());

    let use_out_of_core = out_of_core.enabled.unwrap_or(true);
    if !use_out_of_core && out_of_core.has_overrides() {
        return Err(PyValueError::new_err(
            "memory_fraction/memory_budget_bytes require out_of_core=True",
        ));
    }

    let stream: NeutronStream = if use_out_of_core {
        let mut memory = OutOfCoreConfig::default();
        if let Some(fraction) = out_of_core.memory_fraction {
            memory = memory.with_memory_fraction(fraction);
        }
        if let Some(bytes) = out_of_core.memory_budget_bytes {
            memory = memory.with_memory_budget_bytes(bytes);
        }

        let stream =
            out_of_core_neutron_stream(&reader, algo, &clustering, &extraction, &params, &memory)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
        Box::new(stream.map(|result| {
            result
                .map(|batch| batch.neutrons)
                .map_err(|err| err.to_string())
        }))
    } else {
        let stream = reader
            .stream_time_ordered()
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
        let stream = cluster_and_extract_stream_iter(
            stream,
            algo,
            clustering.clone(),
            extraction.clone(),
            params,
        );
        Box::new(stream.map(|result| result.map_err(|err| err.to_string())))
    };

    Ok(PyNeutronBatchStream {
        stream,
        metadata: BatchMetadata {
            detector,
            clustering: Some(clustering),
            extraction: Some(extraction),
            algorithm: Some(selection.name),
            source_path: Some(path.to_string()),
            time_ordered: true,
        },
    })
}

#[pyfunction]
#[pyo3(signature = (path, detector_config=None))]
fn stream_tpx3_hits(
    path: &str,
    detector_config: Option<PyRef<'_, PyDetectorConfig>>,
) -> PyResult<PyHitBatchStream> {
    let detector = detector_config
        .as_ref()
        .map(|cfg| cfg.inner.clone())
        .unwrap_or_default();

    let reader = Tpx3FileReader::open(path)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        .with_config(detector.clone());

    let stream = reader
        .stream_time_ordered()
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;

    Ok(PyHitBatchStream {
        stream,
        metadata: BatchMetadata {
            detector,
            clustering: None,
            extraction: None,
            algorithm: None,
            source_path: Some(path.to_string()),
            time_ordered: true,
        },
    })
}

#[pymodule]
fn rustpix(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDetectorConfig>()?;
    m.add_class::<PyClusteringConfig>()?;
    m.add_class::<PyExtractionConfig>()?;
    m.add_class::<PyHitBatch>()?;
    m.add_class::<PyNeutronBatch>()?;
    m.add_class::<PyHitBatchStream>()?;
    m.add_class::<PyNeutronBatchStream>()?;

    m.add_function(wrap_pyfunction!(read_tpx3_hits, m)?)?;
    m.add_function(wrap_pyfunction!(process_tpx3_neutrons, m)?)?;
    m.add_function(wrap_pyfunction!(cluster_hits, m)?)?;
    m.add_function(wrap_pyfunction!(stream_tpx3_neutrons, m)?)?;
    m.add_function(wrap_pyfunction!(stream_tpx3_hits, m)?)?;
    Ok(())
}

fn ensure_hdf5_disabled(output_path: Option<&str>) -> PyResult<()> {
    if output_path.is_some() {
        return Err(PyNotImplementedError::new_err(
            "HDF5 output is not implemented yet",
        ));
    }
    Ok(())
}

struct AlgorithmSelection {
    name: String,
    algorithm: ClusteringAlgorithm,
    params: AlgorithmParams,
}

struct ProcessingKwargs {
    selection: AlgorithmSelection,
    time_ordered: bool,
    output_path: Option<String>,
    out_of_core: OutOfCoreKwargs,
}

struct OutOfCoreKwargs {
    enabled: Option<bool>,
    memory_fraction: Option<f64>,
    memory_budget_bytes: Option<usize>,
}

impl OutOfCoreKwargs {
    fn has_overrides(&self) -> bool {
        self.memory_fraction.is_some() || self.memory_budget_bytes.is_some()
    }
}

fn extract_kwarg<'py, T: FromPyObject<'py>>(
    kwargs: &Bound<'py, PyDict>,
    key: &str,
) -> PyResult<Option<T>> {
    match kwargs.get_item(key)? {
        Some(value) => Ok(Some(value.extract()?)),
        None => Ok(None),
    }
}

fn parse_algorithm_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<AlgorithmSelection> {
    let mut params = AlgorithmParams::default();
    let mut name = "abs".to_string();

    if let Some(kwargs) = kwargs {
        if let Some(value) = extract_kwarg::<String>(kwargs, "algorithm")? {
            name = value;
        }
        if let Some(value) = extract_kwarg::<usize>(kwargs, "abs_scan_interval")? {
            params.abs_scan_interval = value;
        }
        if let Some(value) = extract_kwarg::<usize>(kwargs, "dbscan_min_points")? {
            params.dbscan_min_points = value;
        }
        if let Some(value) = extract_kwarg::<usize>(kwargs, "grid_cell_size")? {
            params.grid_cell_size = value;
        }
    }

    let normalized = name.to_ascii_lowercase();
    let algorithm = parse_algorithm(&normalized)?;

    Ok(AlgorithmSelection {
        name: normalized,
        algorithm,
        params,
    })
}

fn parse_output_path(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Option<String>> {
    if let Some(kwargs) = kwargs {
        extract_kwarg::<String>(kwargs, "output_path")
    } else {
        Ok(None)
    }
}

fn parse_out_of_core_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<OutOfCoreKwargs> {
    let mut enabled = None;
    let mut memory_fraction = None;
    let mut memory_budget_bytes = None;

    if let Some(kwargs) = kwargs {
        if let Some(value) = extract_kwarg::<bool>(kwargs, "out_of_core")? {
            enabled = Some(value);
        }
        if let Some(value) = extract_kwarg::<f64>(kwargs, "memory_fraction")? {
            memory_fraction = Some(value);
        }
        if let Some(value) = extract_kwarg::<usize>(kwargs, "memory_budget_bytes")? {
            memory_budget_bytes = Some(value);
        }
    }

    Ok(OutOfCoreKwargs {
        enabled,
        memory_fraction,
        memory_budget_bytes,
    })
}

fn parse_processing_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<ProcessingKwargs> {
    let selection = parse_algorithm_kwargs(kwargs)?;
    let output_path = parse_output_path(kwargs)?;
    let out_of_core = parse_out_of_core_kwargs(kwargs)?;

    let mut time_ordered = true;
    if let Some(kwargs) = kwargs {
        if let Some(value) = extract_kwarg::<bool>(kwargs, "time_ordered")? {
            time_ordered = value;
        }
    }

    Ok(ProcessingKwargs {
        selection,
        time_ordered,
        output_path,
        out_of_core,
    })
}

fn parse_algorithm(name: &str) -> PyResult<ClusteringAlgorithm> {
    match name.to_ascii_lowercase().as_str() {
        "abs" => Ok(ClusteringAlgorithm::Abs),
        "dbscan" => Ok(ClusteringAlgorithm::Dbscan),
        "grid" => Ok(ClusteringAlgorithm::Grid),
        _ => Err(PyValueError::new_err(format!(
            "Unknown algorithm '{}'. Expected one of: abs, dbscan, grid",
            name
        ))),
    }
}

fn detector_config_to_dict(py: Python<'_>, config: &DetectorConfig) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("tdc_frequency_hz", config.tdc_frequency_hz)?;
    dict.set_item(
        "enable_missing_tdc_correction",
        config.enable_missing_tdc_correction,
    )?;
    dict.set_item("chip_size_x", config.chip_size_x)?;
    dict.set_item("chip_size_y", config.chip_size_y)?;

    let transforms: Vec<(i32, i32, i32, i32, i32, i32)> = config
        .chip_transforms
        .iter()
        .map(|t| (t.a, t.b, t.c, t.d, t.tx, t.ty))
        .collect();
    dict.set_item("chip_transforms", transforms)?;
    Ok(dict.into_any().unbind())
}

fn clustering_config_to_dict(py: Python<'_>, config: &ClusteringConfig) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("radius", config.radius)?;
    dict.set_item("temporal_window_ns", config.temporal_window_ns)?;
    dict.set_item("min_cluster_size", config.min_cluster_size)?;
    dict.set_item("max_cluster_size", config.max_cluster_size)?;
    Ok(dict.into_any().unbind())
}

fn extraction_config_to_dict(py: Python<'_>, config: &ExtractionConfig) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("super_resolution_factor", config.super_resolution_factor)?;
    dict.set_item("weighted_by_tot", config.weighted_by_tot)?;
    dict.set_item("min_tot_threshold", config.min_tot_threshold)?;
    Ok(dict.into_any().unbind())
}

fn pyarrow_table_from_numpy(
    py: Python<'_>,
    arrays: &[PyObject],
    names: &[&str],
) -> PyResult<PyObject> {
    let pyarrow = PyModule::import(py, "pyarrow").map_err(|err| {
        PyImportError::new_err(format!(
            "pyarrow is required for to_arrow (import failed: {err})"
        ))
    })?;
    let array_fn = pyarrow.getattr("array")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("copy", false)?;

    let mut arrow_arrays = Vec::with_capacity(arrays.len());
    for array in arrays {
        arrow_arrays.push(array_fn.call((array,), Some(&kwargs))?);
    }

    let arrays_list = PyList::new(py, arrow_arrays)?;
    let names_list = PyList::new(py, names)?;
    let table = pyarrow
        .getattr("Table")?
        .getattr("from_arrays")?
        .call((arrays_list, names_list), None)?;
    Ok(table.into_any().unbind())
}
