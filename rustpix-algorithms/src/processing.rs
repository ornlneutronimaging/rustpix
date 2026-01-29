//! High-level processing helpers that combine clustering and extraction.

use crate::{AbsClustering, AbsConfig, AbsState, DbscanClustering, DbscanConfig, DbscanState};
use crate::{GridClustering, GridConfig, GridState};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::error::Result;
use rustpix_core::extraction::{ExtractionConfig, NeutronExtraction, SimpleCentroidExtraction};
use rustpix_core::neutron::{Neutron, NeutronBatch};
use rustpix_core::soa::HitBatch;

/// Supported clustering algorithms.
#[derive(Clone, Copy, Debug)]
pub enum ClusteringAlgorithm {
    /// Age-Based Spatial clustering.
    Abs,
    /// DBSCAN clustering.
    Dbscan,
    /// Grid-based clustering.
    Grid,
}

/// Algorithm-specific tuning parameters.
#[derive(Clone, Debug)]
pub struct AlgorithmParams {
    /// ABS scan interval (hits between aging scans).
    pub abs_scan_interval: usize,
    /// DBSCAN minimum points for a seed cluster.
    pub dbscan_min_points: usize,
    /// Grid cell size (pixels).
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

/// Iterator that clusters and extracts each incoming batch.
pub struct ClusterAndExtractStream<I>
where
    I: Iterator<Item = HitBatch>,
{
    batches: I,
    algorithm: ClusteringAlgorithm,
    clustering: ClusteringConfig,
    extraction: ExtractionConfig,
    params: AlgorithmParams,
}

impl<I> Iterator for ClusterAndExtractStream<I>
where
    I: Iterator<Item = HitBatch>,
{
    type Item = Result<NeutronBatch>;

    fn next(&mut self) -> Option<Self::Item> {
        self.batches.next().map(|mut batch| {
            cluster_and_extract_batch(
                &mut batch,
                self.algorithm,
                &self.clustering,
                &self.extraction,
                &self.params,
            )
        })
    }
}

/// Create a streaming cluster-and-extract iterator.
pub fn cluster_and_extract_stream_iter<I>(
    batches: I,
    algorithm: ClusteringAlgorithm,
    clustering: ClusteringConfig,
    extraction: ExtractionConfig,
    params: AlgorithmParams,
) -> ClusterAndExtractStream<I::IntoIter>
where
    I: IntoIterator<Item = HitBatch>,
{
    ClusterAndExtractStream {
        batches: batches.into_iter(),
        algorithm,
        clustering,
        extraction,
        params,
    }
}

/// Cluster hits in-place, then extract neutrons using the configured algorithm.
///
/// # Errors
/// Returns an error if clustering or extraction fails.
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
    extractor
        .extract_soa(batch, num_clusters)
        .map_err(Into::into)
}

/// Cluster hits in-place, then extract neutrons into a `NeutronBatch`.
///
/// # Errors
/// Returns an error if clustering or extraction fails.
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

/// Cluster hits in batches, then extract and append neutrons into a single batch.
///
/// # Errors
/// Returns an error if clustering or extraction fails for any batch.
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
    let iter = cluster_and_extract_stream_iter(
        batches,
        algorithm,
        clustering.clone(),
        extraction.clone(),
        params.clone(),
    );
    for neutrons in iter {
        let neutrons = neutrons?;
        all_neutrons.append(&neutrons);
    }
    Ok(all_neutrons)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_iter_matches_batch_results() {
        let mut batch1 = HitBatch::with_capacity(2);
        batch1.push((10, 10, 100, 5, 1_000, 0));
        batch1.push((11, 10, 102, 6, 1_002, 0));

        let mut batch2 = HitBatch::with_capacity(2);
        batch2.push((20, 20, 200, 7, 2_000, 1));
        batch2.push((21, 20, 202, 8, 2_002, 1));

        let algorithm = ClusteringAlgorithm::Abs;
        let clustering = ClusteringConfig::default();
        let extraction = ExtractionConfig::default();
        let params = AlgorithmParams::default();

        let mut expected1 = batch1.clone();
        let expected1 =
            cluster_and_extract_batch(&mut expected1, algorithm, &clustering, &extraction, &params)
                .unwrap();

        let mut expected2 = batch2.clone();
        let expected2 =
            cluster_and_extract_batch(&mut expected2, algorithm, &clustering, &extraction, &params)
                .unwrap();

        let mut iter = cluster_and_extract_stream_iter(
            vec![batch1, batch2],
            algorithm,
            clustering,
            extraction,
            params,
        );

        let batch_out1 = iter.next().unwrap().unwrap();
        assert_eq!(batch_out1.x, expected1.x);
        assert_eq!(batch_out1.y, expected1.y);
        assert_eq!(batch_out1.tof, expected1.tof);
        assert_eq!(batch_out1.tot, expected1.tot);
        assert_eq!(batch_out1.n_hits, expected1.n_hits);
        assert_eq!(batch_out1.chip_id, expected1.chip_id);

        let batch_out2 = iter.next().unwrap().unwrap();
        assert_eq!(batch_out2.x, expected2.x);
        assert_eq!(batch_out2.y, expected2.y);
        assert_eq!(batch_out2.tof, expected2.tof);
        assert_eq!(batch_out2.tot, expected2.tot);
        assert_eq!(batch_out2.n_hits, expected2.n_hits);
        assert_eq!(batch_out2.chip_id, expected2.chip_id);

        assert!(iter.next().is_none());
    }
}
