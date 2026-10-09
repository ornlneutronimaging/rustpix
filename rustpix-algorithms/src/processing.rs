//! High-level processing helpers that combine clustering and extraction.

use crate::{AbsClustering, AbsConfig, AbsState, DbscanClustering, DbscanConfig, DbscanState};
use crate::{GridClustering, GridConfig, GridState};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::error::{ProcessingError, Result};
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
    /// DBSCAN minimum hits, including the hit itself, to seed a cluster.
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

/// Cluster one pulse of TOF-ordered hits, then extract neutrons.
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
    let num_clusters = cluster(batch, algorithm, clustering, params)?;
    let mut extractor = SimpleCentroidExtraction::new();
    extractor.configure(extraction.clone());
    extractor
        .extract_soa(batch, num_clusters)
        .map_err(Into::into)
}

/// Cluster one pulse of TOF-ordered hits, then extract a `NeutronBatch`.
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
    let num_clusters = cluster(batch, algorithm, clustering, params)?;
    let mut extractor = SimpleCentroidExtraction::new();
    extractor.configure(extraction.clone());
    extractor
        .extract_soa_batch(batch, num_clusters)
        .map_err(Into::into)
}

fn cluster(
    batch: &mut HitBatch,
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    params: &AlgorithmParams,
) -> Result<usize> {
    let num_clusters = match algorithm {
        ClusteringAlgorithm::Abs => AbsClustering::new(AbsConfig {
            radius: clustering.radius,
            neutron_correlation_window_ns: clustering.temporal_window_ns,
            min_cluster_size: clustering.min_cluster_size,
            scan_interval: params.abs_scan_interval,
        })
        .cluster(batch, &mut AbsState::default())?,
        ClusteringAlgorithm::Dbscan => DbscanClustering::new(DbscanConfig {
            epsilon: clustering.radius,
            temporal_window_ns: clustering.temporal_window_ns,
            min_points: params.dbscan_min_points,
            min_cluster_size: clustering.min_cluster_size,
        })
        .cluster(batch, &mut DbscanState::default())?,
        ClusteringAlgorithm::Grid => GridClustering::new(GridConfig {
            radius: clustering.radius,
            temporal_window_ns: clustering.temporal_window_ns,
            min_cluster_size: clustering.min_cluster_size,
            cell_size: params.grid_cell_size,
            max_cluster_size: clustering.max_cluster_size.map(usize::from),
        })
        .cluster(batch, &mut GridState::default())?,
    };
    Ok(match clustering.max_cluster_size {
        Some(max) => drop_oversized(batch, num_clusters, usize::from(max)),
        None => num_clusters,
    })
}

/// Relabels clusters with more than `max` hits as `-1` and renumbers the rest from 0.
fn drop_oversized(batch: &mut HitBatch, num_clusters: usize, max: usize) -> usize {
    let mut sizes = vec![0usize; num_clusters];
    for &label in &batch.cluster_id {
        if let Some(size) = usize::try_from(label).ok().and_then(|l| sizes.get_mut(l)) {
            *size += 1;
        }
    }
    let mut kept = 0i32;
    let relabel: Vec<i32> = sizes
        .iter()
        .map(|&size| {
            if size > max {
                return -1;
            }
            kept += 1;
            kept - 1
        })
        .collect();
    for label in &mut batch.cluster_id {
        if let Ok(old) = usize::try_from(*label) {
            *label = relabel.get(old).copied().unwrap_or(-1);
        }
    }
    usize::try_from(kept).unwrap_or(0)
}

/// Cluster and extract each TOF-ordered pulse separately; `pulse_starts` must ascend.
///
/// # Errors
/// Returns an error for invalid `pulse_starts` or failed clustering.
pub fn cluster_and_extract_pulses(
    batch: &mut HitBatch,
    pulse_starts: &[usize],
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
) -> Result<NeutronBatch> {
    if pulse_starts.windows(2).any(|pair| pair[0] > pair[1])
        || pulse_starts
            .last()
            .is_some_and(|&start| start > batch.len())
    {
        return Err(ProcessingError::Config(format!(
            "pulse starts must be ascending and at most the batch length ({})",
            batch.len()
        ))
        .into());
    }

    let mut bounds = Vec::with_capacity(pulse_starts.len() + 2);
    bounds.push(0);
    bounds.extend_from_slice(pulse_starts);
    bounds.push(batch.len());

    let mut neutrons = NeutronBatch::default();
    let mut label_offset = 0i32;
    for pair in bounds.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start == end {
            continue;
        }

        let mut pulse = copy_hits(batch, start, end);
        let pulse_neutrons =
            cluster_and_extract_batch(&mut pulse, algorithm, clustering, extraction, params)?;

        let mut label_count = 0i32;
        for (dest, &label) in batch.cluster_id[start..end]
            .iter_mut()
            .zip(&pulse.cluster_id)
        {
            *dest = if label < 0 {
                -1
            } else {
                label_count = label_count.max(label.saturating_add(1));
                label.saturating_add(label_offset)
            };
        }
        label_offset = label_offset.saturating_add(label_count);
        neutrons.append(&pulse_neutrons);
    }

    Ok(neutrons)
}

fn copy_hits(batch: &HitBatch, start: usize, end: usize) -> HitBatch {
    HitBatch {
        x: batch.x[start..end].to_vec(),
        y: batch.y[start..end].to_vec(),
        tof: batch.tof[start..end].to_vec(),
        tot: batch.tot[start..end].to_vec(),
        timestamp: batch.timestamp[start..end].to_vec(),
        chip_id: batch.chip_id[start..end].to_vec(),
        cluster_id: vec![-1; end - start],
    }
}

/// Cluster each batch and collect all neutrons into one batch.
///
/// # Errors
/// Returns the first clustering or extraction error.
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
