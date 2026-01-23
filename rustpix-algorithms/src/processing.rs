//! High-level processing helpers that combine clustering and extraction.

use crate::{AbsClustering, AbsConfig, AbsState, DbscanClustering, DbscanConfig, DbscanState};
use crate::{GridClustering, GridConfig, GridState};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::error::Result;
use rustpix_core::extraction::{ExtractionConfig, NeutronExtraction, SimpleCentroidExtraction};
use rustpix_core::neutron::{Neutron, NeutronBatch};
use rustpix_core::soa::HitBatch;

#[derive(Clone, Copy, Debug)]
pub enum ClusteringAlgorithm {
    Abs,
    Dbscan,
    Grid,
}

#[derive(Clone, Debug)]
pub struct AlgorithmParams {
    pub abs_scan_interval: usize,
    pub dbscan_min_points: usize,
    pub grid_cell_size: usize,
}

impl Default for AlgorithmParams {
    fn default() -> Self {
        Self {
            abs_scan_interval: 100,
            dbscan_min_points: 2,
            grid_cell_size: 32,
        }
    }
}

/// Cluster hits in-place, then extract neutrons using the configured algorithm.
pub fn cluster_and_extract(
    batch: &mut HitBatch,
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
) -> Result<Vec<Neutron>> {
    let num_clusters = match algorithm {
        ClusteringAlgorithm::Abs => {
            let algo = AbsClustering::new(AbsConfig {
                radius: clustering.radius,
                neutron_correlation_window_ns: clustering.temporal_window_ns,
                min_cluster_size: clustering.min_cluster_size,
                scan_interval: params.abs_scan_interval,
            });
            let mut state = AbsState::default();
            algo.cluster(batch, &mut state)?
        }
        ClusteringAlgorithm::Dbscan => {
            let algo = DbscanClustering::new(DbscanConfig {
                epsilon: clustering.radius,
                temporal_window_ns: clustering.temporal_window_ns,
                min_points: params.dbscan_min_points,
                min_cluster_size: clustering.min_cluster_size,
            });
            let mut state = DbscanState::default();
            algo.cluster(batch, &mut state)?
        }
        ClusteringAlgorithm::Grid => {
            let algo = GridClustering::new(GridConfig {
                radius: clustering.radius,
                temporal_window_ns: clustering.temporal_window_ns,
                min_cluster_size: clustering.min_cluster_size,
                cell_size: params.grid_cell_size,
                max_cluster_size: clustering.max_cluster_size.map(|value| value as usize),
            });
            let mut state = GridState::default();
            algo.cluster(batch, &mut state)?
        }
    };

    let mut extractor = SimpleCentroidExtraction::new();
    extractor.configure(extraction.clone());
    extractor.extract_soa(batch, num_clusters).map_err(Into::into)
}

pub fn cluster_and_extract_batch(
    batch: &mut HitBatch,
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
) -> Result<NeutronBatch> {
    let num_clusters = match algorithm {
        ClusteringAlgorithm::Abs => {
            let algo = AbsClustering::new(AbsConfig {
                radius: clustering.radius,
                neutron_correlation_window_ns: clustering.temporal_window_ns,
                min_cluster_size: clustering.min_cluster_size,
                scan_interval: params.abs_scan_interval,
            });
            let mut state = AbsState::default();
            algo.cluster(batch, &mut state)?
        }
        ClusteringAlgorithm::Dbscan => {
            let algo = DbscanClustering::new(DbscanConfig {
                epsilon: clustering.radius,
                temporal_window_ns: clustering.temporal_window_ns,
                min_points: params.dbscan_min_points,
                min_cluster_size: clustering.min_cluster_size,
            });
            let mut state = DbscanState::default();
            algo.cluster(batch, &mut state)?
        }
        ClusteringAlgorithm::Grid => {
            let algo = GridClustering::new(GridConfig {
                radius: clustering.radius,
                temporal_window_ns: clustering.temporal_window_ns,
                min_cluster_size: clustering.min_cluster_size,
                cell_size: params.grid_cell_size,
                max_cluster_size: clustering.max_cluster_size.map(|value| value as usize),
            });
            let mut state = GridState::default();
            algo.cluster(batch, &mut state)?
        }
    };

    let mut extractor = SimpleCentroidExtraction::new();
    extractor.configure(extraction.clone());
    extractor
        .extract_soa_batch(batch, num_clusters)
        .map_err(Into::into)
}

pub fn cluster_and_extract_stream<I>(
    batches: I,
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
) -> Result<NeutronBatch>
where
    I: IntoIterator<Item = HitBatch>,
{
    let mut all_neutrons = NeutronBatch::default();
    for mut batch in batches {
        let neutrons =
            cluster_and_extract_batch(&mut batch, algorithm, clustering, extraction, params)?;
        all_neutrons.append(&neutrons);
    }
    Ok(all_neutrons)
}
