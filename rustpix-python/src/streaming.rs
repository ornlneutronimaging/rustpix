//! Streaming processing of TPX3 files.
#![allow(
    unsafe_code,
    clippy::map_unwrap_or,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::redundant_closure_for_method_calls,
    clippy::cast_sign_loss,
    clippy::similar_names
)]

use numpy::PyArray1;
use pyo3::prelude::*;
use rustpix_algorithms::{GridClustering, GridConfig, GridState};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::{ExtractionConfig, NeutronExtraction, SimpleCentroidExtraction};
// use rustpix_core::hit::GenericHit; // Removed
use rustpix_core::soa::HitBatch;
use rustpix_io::{MappedFileReader, PacketScanner};
use rustpix_tpx::section::{process_section_into_batch, scan_section_tdc, Tpx3Section};
use rustpix_tpx::DetectorConfig;

use crate::{PyClusteringConfig, PyDetectorConfig};

struct UnsafeSlice(*const u8, usize);
unsafe impl Send for UnsafeSlice {}
unsafe impl Sync for UnsafeSlice {}
impl UnsafeSlice {
    unsafe fn as_slice<'a>(&self) -> &'a [u8] {
        std::slice::from_raw_parts(self.0, self.1)
    }
}

/// Iterator that processes a TPX3 file in chunks.
#[pyclass]
pub struct MeasurementStream {
    reader: MappedFileReader,
    offset: usize,
    chunk_size: usize,
    detector_config: DetectorConfig,
    clustering_config: ClusteringConfig,
    algorithm: String,
    super_resolution: f64,
    tot_weighted: bool,
    tdc_state: Vec<Option<u32>>,
}

#[pymethods]
impl MeasurementStream {
    #[new]
    #[pyo3(signature = (path, chunk_size=100_000_000, algorithm="grid", clustering_config=None, detector_config=None, super_resolution=8.0, tot_weighted=true))]
    fn new(
        path: &str,
        chunk_size: usize,
        algorithm: &str,
        clustering_config: Option<PyClusteringConfig>,
        detector_config: Option<PyDetectorConfig>,
        super_resolution: f64,
        tot_weighted: bool,
    ) -> PyResult<Self> {
        let reader = MappedFileReader::open(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

        let det_conf = detector_config
            .map(|c| c.inner.clone())
            .unwrap_or_else(DetectorConfig::venus_defaults);
        let clust_conf = clustering_config
            .map(|c| c.inner.clone())
            .unwrap_or_default();

        Ok(Self {
            reader,
            offset: 0,
            chunk_size,
            detector_config: det_conf,
            clustering_config: clust_conf,
            algorithm: algorithm.to_string(),
            super_resolution,
            tot_weighted,
            tdc_state: vec![None; 256],
        })
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__<'py>(
        mut slf: PyRefMut<'py, Self>,
        py: Python<'py>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        if slf.offset >= slf.reader.len() {
            return Ok(None);
        }

        // Prepare data for allow_threads
        let offset = slf.offset;
        let chunk_size = slf.chunk_size;
        let total_len = slf.reader.len();

        let detector_config = slf.detector_config.clone();
        let clustering_config = slf.clustering_config.clone();
        let algorithm = slf.algorithm.clone();
        let super_resolution = slf.super_resolution;
        let tot_weighted = slf.tot_weighted;
        let mut tdc_state = slf.tdc_state.clone();

        let reader_ptr = slf.reader.as_bytes().as_ptr();
        // SAFETY: The reader is owned by the MeasurementStream, which is pinned by the Python
        // object reference (slf). We hold PyRefMut, so no other thread can mutate it.
        // We only read from the memory map, which is stable.
        let reader_slice = UnsafeSlice(reader_ptr, total_len);

        // Release GIL
        let (combined_batch, neutrons, consumed, new_tdc_state) = py
            .allow_threads(move || {
                let data = unsafe { reader_slice.as_slice() };

                let start = offset;
                let end = (start + chunk_size).min(total_len);
                let is_eof_chunk = end == total_len;
                let chunk_data = &data[start..end];

                // Scan for sections (offsets relative to chunk_data start)
                let (io_sections, consumed) =
                    PacketScanner::scan_sections(chunk_data, is_eof_chunk);

                if consumed == 0 && !is_eof_chunk {
                    return Err("Chunk size too small to contain a single section header or section is huge.".to_string());
                }

                // 1. Prepare Tpx3Sections and propagate TDC logic SEQUENTIALLY
                let mut tpx_sections = Vec::with_capacity(io_sections.len());

                for io_sec in io_sections {
                    let chip = io_sec.chip_id;
                    let initial = tdc_state[chip as usize];

                    let mut tpx_sec = Tpx3Section {
                        start_offset: io_sec.start_offset,
                        end_offset: io_sec.end_offset,
                        chip_id: chip,
                        initial_tdc: initial,
                        final_tdc: None,
                    };

                    // Scan for final TDC to update state
                    let final_tdc = scan_section_tdc(chunk_data, &tpx_sec);
                    tpx_sec.final_tdc = final_tdc;

                    if let Some(tdc) = final_tdc {
                        tdc_state[chip as usize] = Some(tdc);
                    }

                    tpx_sections.push(tpx_sec);
                }

                // 2. Process sections into a single HitBatch (avoid per-section merge copies)
                let tdc_corr = detector_config.tdc_correction_25ns();
                let det_config = &detector_config;
                let total_capacity: usize =
                    tpx_sections.iter().map(|section| section.packet_count()).sum();
                let mut combined_batch = HitBatch::with_capacity(total_capacity);

                for section in &tpx_sections {
                    let _ = process_section_into_batch(
                        chunk_data,
                        section,
                        tdc_corr,
                        |c, x, y| det_config.map_chip_to_global(c, x, y),
                        &mut combined_batch,
                    );
                }

                // 3. Clustering (only if hits exist)
                let n = combined_batch.len();
                if n > 0 {
                    match algorithm.as_str() {
                        "grid" => {
                            let algo = GridClustering::new(GridConfig {
                                cell_size: 32,
                                radius: clustering_config.radius,
                                temporal_window_ns: clustering_config.temporal_window_ns,
                                min_cluster_size: clustering_config.min_cluster_size,
                                max_cluster_size: clustering_config
                                    .max_cluster_size
                                    .map(|s| s as usize),
                            });
                            let mut state = GridState::default();
                            algo.cluster(&mut combined_batch, &mut state)
                                .map_err(|e| e.to_string())?;
                        }
                        _ => {
                            return Err(
                                "Only 'grid' algorithm supported for streaming currently."
                                    .to_string(),
                            );
                        }
                    }
                }

                // 4. Extract Neutrons
                // We no longer need to construct Vec<GenericHit>, saving a massive allocation and copy.
                // We used to do:
                // let mut hit_data = Vec::with_capacity(n); ...

                // Find num clusters
                let max_label = combined_batch
                    .cluster_id
                    .iter()
                    .max()
                    .copied()
                    .unwrap_or(-1);
                let num_clusters = if max_label < 0 {
                    0
                } else {
                    (max_label + 1) as usize
                };

                let mut extractor = SimpleCentroidExtraction::new();
                extractor.configure(
                    ExtractionConfig::default()
                        .with_super_resolution(super_resolution)
                        .with_weighted_by_tot(tot_weighted),
                );

                let neutrons = extractor
                    .extract_soa(&combined_batch, num_clusters)
                    .map_err(|e| e.to_string())?;

                Ok((combined_batch, neutrons, consumed, tdc_state))
            })
            .map_err(pyo3::exceptions::PyValueError::new_err)?;

        // Update state
        slf.offset += consumed;
        slf.tdc_state = new_tdc_state;

        // 5. Return Data Info
        // Prepare PyDict output
        let dict = pyo3::types::PyDict::new(py);

        // Hits Dict
        let hits_dict = pyo3::types::PyDict::new(py);
        hits_dict.set_item("x", PyArray1::from_vec(py, combined_batch.x))?;
        hits_dict.set_item("y", PyArray1::from_vec(py, combined_batch.y))?;
        hits_dict.set_item("tof", PyArray1::from_vec(py, combined_batch.tof))?;
        hits_dict.set_item("tot", PyArray1::from_vec(py, combined_batch.tot))?;
        hits_dict.set_item(
            "timestamp",
            PyArray1::from_vec(py, combined_batch.timestamp),
        )?;
        hits_dict.set_item("chip_id", PyArray1::from_vec(py, combined_batch.chip_id))?;
        hits_dict.set_item(
            "cluster_id",
            PyArray1::from_vec(py, combined_batch.cluster_id),
        )?;

        dict.set_item("hits", hits_dict)?;

        // Neutrons Dict
        let neutrons_dict = pyo3::types::PyDict::new(py);
        let nx: Vec<f64> = neutrons.iter().map(|n| n.x).collect();
        let ny: Vec<f64> = neutrons.iter().map(|n| n.y).collect();
        let ntof: Vec<u32> = neutrons.iter().map(|n| n.tof).collect();
        let ntot: Vec<u16> = neutrons.iter().map(|n| n.tot).collect();
        let nn: Vec<u16> = neutrons.iter().map(|n| n.n_hits).collect();

        neutrons_dict.set_item("x", PyArray1::from_vec(py, nx))?;
        neutrons_dict.set_item("y", PyArray1::from_vec(py, ny))?;
        neutrons_dict.set_item("tof", PyArray1::from_vec(py, ntof))?;
        neutrons_dict.set_item("tot", PyArray1::from_vec(py, ntot))?;
        neutrons_dict.set_item("n_hits", PyArray1::from_vec(py, nn))?;

        dict.set_item("neutrons", neutrons_dict)?;

        Ok(Some(dict.into_any()))
    }
}
