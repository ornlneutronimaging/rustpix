//! Out-of-core processing pipeline for pulse-bounded streams.

use crate::out_of_core::{
    pulse_batches, OutOfCoreConfig, PulseBatchGroup, PulseBatcher, PulseSlice,
};
use crate::reader::{TimeOrderedEventStream, Tpx3FileReader};
use crate::{Error, Result};
use rustpix_algorithms::{cluster_and_extract_batch, AlgorithmParams, ClusteringAlgorithm};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;
use std::collections::VecDeque;

/// Neutron output for a single pulse.
#[derive(Clone, Debug)]
pub struct PulseNeutronBatch {
    pub tdc_timestamp_25ns: u64,
    pub hits_processed: usize,
    pub neutrons: NeutronBatch,
}

/// Iterator over out-of-core pulse batches.
pub struct OutOfCoreNeutronStream<I>
where
    I: Iterator<Item = PulseBatchGroup>,
{
    batches: I,
    slices: VecDeque<PulseSlice>,
    algorithm: ClusteringAlgorithm,
    clustering: ClusteringConfig,
    extraction: ExtractionConfig,
    params: AlgorithmParams,
    current_tdc: Option<u64>,
    current_neutrons: NeutronBatch,
    current_hits: usize,
    current_cutoff_tof: Option<u32>,
    pending: VecDeque<PulseNeutronBatch>,
    finished: bool,
}

impl<I> OutOfCoreNeutronStream<I>
where
    I: Iterator<Item = PulseBatchGroup>,
{
    #[must_use]
    pub fn new(
        batches: I,
        algorithm: ClusteringAlgorithm,
        clustering: ClusteringConfig,
        extraction: ExtractionConfig,
        params: AlgorithmParams,
    ) -> Self {
        Self {
            batches,
            slices: VecDeque::new(),
            algorithm,
            clustering,
            extraction,
            params,
            current_tdc: None,
            current_neutrons: NeutronBatch::default(),
            current_hits: 0,
            current_cutoff_tof: None,
            pending: VecDeque::new(),
            finished: false,
        }
    }

    fn next_slice(&mut self) -> Option<PulseSlice> {
        if let Some(slice) = self.slices.pop_front() {
            return Some(slice);
        }

        let group = self.batches.next()?;
        self.slices.extend(group.slices);
        self.slices.pop_front()
    }

    fn process_slice(&mut self, slice: PulseSlice) -> Result<()> {
        let mut hits = slice.hits;
        let mut neutrons = cluster_and_extract_batch(
            &mut hits,
            self.algorithm,
            &self.clustering,
            &self.extraction,
            &self.params,
        )
        .map_err(Error::CoreError)?;

        let emitted_hits =
            count_emitted_hits(&hits, self.current_cutoff_tof, slice.emit_cutoff_tof);
        if slice.emit_cutoff_tof != u32::MAX {
            neutrons = filter_neutrons_by_tof(&neutrons, slice.emit_cutoff_tof);
        }

        self.append_neutrons(
            slice.tdc_timestamp_25ns,
            &neutrons,
            emitted_hits,
            slice.emit_cutoff_tof,
        );
        Ok(())
    }

    fn append_neutrons(
        &mut self,
        tdc_timestamp_25ns: u64,
        neutrons: &NeutronBatch,
        emitted_hits: usize,
        cutoff_tof: u32,
    ) {
        let current = self.current_tdc.unwrap_or(tdc_timestamp_25ns);
        if current != tdc_timestamp_25ns {
            if self.current_hits > 0 || !self.current_neutrons.is_empty() {
                self.pending.push_back(PulseNeutronBatch {
                    tdc_timestamp_25ns: current,
                    hits_processed: self.current_hits,
                    neutrons: std::mem::take(&mut self.current_neutrons),
                });
            }
            self.current_tdc = Some(tdc_timestamp_25ns);
            self.current_hits = 0;
            self.current_cutoff_tof = None;
        } else if self.current_tdc.is_none() {
            self.current_tdc = Some(tdc_timestamp_25ns);
        }

        self.current_neutrons.append(neutrons);
        self.current_hits = self.current_hits.saturating_add(emitted_hits);
        self.current_cutoff_tof = Some(cutoff_tof);
    }

    fn flush_current(&mut self) {
        if let Some(tdc) = self.current_tdc.take() {
            if self.current_hits > 0 || !self.current_neutrons.is_empty() {
                self.pending.push_back(PulseNeutronBatch {
                    tdc_timestamp_25ns: tdc,
                    hits_processed: self.current_hits,
                    neutrons: std::mem::take(&mut self.current_neutrons),
                });
            }
        }
        self.current_hits = 0;
        self.current_cutoff_tof = None;
    }
}

impl<I> Iterator for OutOfCoreNeutronStream<I>
where
    I: Iterator<Item = PulseBatchGroup>,
{
    type Item = Result<PulseNeutronBatch>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(batch) = self.pending.pop_front() {
                return Some(Ok(batch));
            }

            if self.finished {
                return None;
            }

            if let Some(slice) = self.next_slice() {
                if let Err(err) = self.process_slice(slice) {
                    return Some(Err(err));
                }
                continue;
            }

            self.flush_current();
            if self.pending.is_empty() {
                self.finished = true;
                return None;
            }
        }
    }
}

/// Build an out-of-core neutron stream from a TPX3 reader.
///
/// # Errors
/// Returns an error if the reader fails or the memory budget is invalid.
pub fn out_of_core_neutron_stream(
    reader: &Tpx3FileReader,
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    memory: &OutOfCoreConfig,
) -> Result<OutOfCoreNeutronStream<PulseBatcher<TimeOrderedEventStream>>> {
    let overlap_tof = clustering.window_tof();
    let batcher = pulse_batches(reader, memory, overlap_tof)?;
    Ok(OutOfCoreNeutronStream::new(
        batcher,
        algorithm,
        clustering.clone(),
        extraction.clone(),
        params.clone(),
    ))
}

fn filter_neutrons_by_tof(neutrons: &NeutronBatch, cutoff_tof: u32) -> NeutronBatch {
    let mut filtered = NeutronBatch::with_capacity(neutrons.len());
    for i in 0..neutrons.len() {
        if neutrons.tof[i] <= cutoff_tof {
            push_neutron(&mut filtered, neutrons, i);
        }
    }
    filtered
}

fn push_neutron(dest: &mut NeutronBatch, src: &NeutronBatch, idx: usize) {
    dest.x.push(src.x[idx]);
    dest.y.push(src.y[idx]);
    dest.tof.push(src.tof[idx]);
    dest.tot.push(src.tot[idx]);
    dest.n_hits.push(src.n_hits[idx]);
    dest.chip_id.push(src.chip_id[idx]);
}

fn count_emitted_hits(hits: &HitBatch, previous_cutoff: Option<u32>, cutoff: u32) -> usize {
    if hits.is_empty() {
        return 0;
    }
    let end = hits.tof.partition_point(|&tof| tof <= cutoff);
    if let Some(prev) = previous_cutoff {
        let start = hits.tof.partition_point(|&tof| tof <= prev);
        end.saturating_sub(start)
    } else {
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::EventBatch;
    use rustpix_core::soa::HitRecord;

    fn make_event_batch(tdc: u64, hits: &[HitRecord]) -> EventBatch {
        let mut batch = HitBatch::with_capacity(hits.len());
        for &hit in hits {
            batch.push(hit);
        }
        batch.sort_by_tof();
        EventBatch {
            tdc_timestamp_25ns: tdc,
            hits: batch,
        }
    }

    fn collect_neutrons<I>(iter: I) -> Vec<u32>
    where
        I: Iterator<Item = Result<PulseNeutronBatch>>,
    {
        let mut tofs = Vec::new();
        for batch in iter {
            let batch = batch.unwrap();
            for tof in batch.neutrons.tof {
                tofs.push(tof);
            }
        }
        tofs.sort_unstable();
        tofs
    }

    #[test]
    fn out_of_core_matches_pulse_processing() {
        let pulses_for_stream = vec![
            make_event_batch(1, &[(1, 1, 10, 1, 10, 0), (100, 100, 20, 1, 20, 0)]),
            make_event_batch(2, &[(50, 50, 30, 1, 30, 0)]),
        ];
        let pulses_for_expected = vec![
            make_event_batch(1, &[(1, 1, 10, 1, 10, 0), (100, 100, 20, 1, 20, 0)]),
            make_event_batch(2, &[(50, 50, 30, 1, 30, 0)]),
        ];

        let config = OutOfCoreConfig::default().with_memory_budget_bytes(10_000);
        let batcher =
            crate::out_of_core::PulseBatcher::new(pulses_for_stream.into_iter(), &config, 0)
                .unwrap();

        let clustering = ClusteringConfig {
            radius: 1.0,
            temporal_window_ns: 25.0,
            min_cluster_size: 1,
            max_cluster_size: None,
        };
        let extraction = ExtractionConfig::default();
        let params = AlgorithmParams::default();

        let stream = OutOfCoreNeutronStream::new(
            batcher,
            ClusteringAlgorithm::Grid,
            clustering.clone(),
            extraction.clone(),
            params.clone(),
        );
        let ooc_tofs = collect_neutrons(stream);

        let mut expected = Vec::new();
        for mut pulse in pulses_for_expected {
            let neutrons = cluster_and_extract_batch(
                &mut pulse.hits,
                ClusteringAlgorithm::Grid,
                &clustering,
                &extraction,
                &params,
            )
            .unwrap();
            expected.extend(neutrons.tof);
        }
        expected.sort_unstable();

        assert_eq!(ooc_tofs, expected);
    }

    #[test]
    fn out_of_core_split_pulse_preserves_hits() {
        let hits = [
            (1, 1, 1, 1, 1, 0),
            (50, 50, 2, 1, 2, 0),
            (100, 100, 3, 1, 3, 0),
            (150, 150, 4, 1, 4, 0),
            (200, 200, 5, 1, 5, 0),
        ];
        let pulse_for_stream = make_event_batch(7, &hits);
        let mut pulse_for_expected = make_event_batch(7, &hits);

        let config = OutOfCoreConfig::default().with_memory_budget_bytes(32);
        let batcher =
            crate::out_of_core::PulseBatcher::new(vec![pulse_for_stream].into_iter(), &config, 1)
                .unwrap();

        let clustering = ClusteringConfig {
            radius: 1.0,
            temporal_window_ns: 25.0,
            min_cluster_size: 1,
            max_cluster_size: None,
        };
        let extraction = ExtractionConfig::default();
        let params = AlgorithmParams::default();

        let stream = OutOfCoreNeutronStream::new(
            batcher,
            ClusteringAlgorithm::Grid,
            clustering.clone(),
            extraction.clone(),
            params.clone(),
        );
        let ooc_tofs = collect_neutrons(stream);

        let mut expected = Vec::new();
        let neutrons = cluster_and_extract_batch(
            &mut pulse_for_expected.hits,
            ClusteringAlgorithm::Grid,
            &clustering,
            &extraction,
            &params,
        )
        .unwrap();
        expected.extend(neutrons.tof);
        expected.sort_unstable();

        assert_eq!(ooc_tofs, expected);
    }

    #[test]
    fn out_of_core_counts_hits_without_double_counting() {
        let hits = [
            (1, 1, 1, 1, 1, 0),
            (50, 50, 2, 1, 2, 0),
            (100, 100, 3, 1, 3, 0),
            (150, 150, 4, 1, 4, 0),
            (200, 200, 5, 1, 5, 0),
        ];
        let pulse = make_event_batch(7, &hits);

        let config = OutOfCoreConfig::default().with_memory_budget_bytes(32);
        let batcher =
            crate::out_of_core::PulseBatcher::new(vec![pulse].into_iter(), &config, 1).unwrap();

        let clustering = ClusteringConfig {
            radius: 1.0,
            temporal_window_ns: 25.0,
            min_cluster_size: 1,
            max_cluster_size: None,
        };
        let extraction = ExtractionConfig::default();
        let params = AlgorithmParams::default();

        let stream = OutOfCoreNeutronStream::new(
            batcher,
            ClusteringAlgorithm::Grid,
            clustering,
            extraction,
            params,
        );

        let total_hits: usize = stream.map(|batch| batch.unwrap().hits_processed).sum();
        assert_eq!(total_hits, hits.len());
    }
}
