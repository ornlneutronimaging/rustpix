//! Out-of-core processing pipeline for pulse-bounded streams.

use crate::out_of_core::{
    pulse_batches, OutOfCoreConfig, PulseBatchGroup, PulseBatcher, PulseSlice,
};
use crate::reader::{TimeOrderedEventStream, Tpx3FileReader};
use crate::{Error, Result};
use rayon::prelude::*;
use rustpix_algorithms::{cluster_and_extract_batch, AlgorithmParams, ClusteringAlgorithm};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

/// Neutron output for a single pulse.
#[derive(Clone, Debug)]
pub struct PulseNeutronBatch {
    /// Pulse TDC timestamp (25ns ticks).
    pub tdc_timestamp_25ns: u64,
    /// Number of hits processed for this pulse.
    pub hits_processed: usize,
    /// Neutrons extracted from this pulse.
    pub neutrons: NeutronBatch,
}

struct SliceOutput {
    tdc_timestamp_25ns: u64,
    hits_processed: usize,
    neutrons: NeutronBatch,
}

/// Stream handle that can be single-threaded or threaded.
pub enum OutOfCoreNeutronStreamHandle {
    /// Single-threaded out-of-core stream.
    Single(Box<OutOfCoreNeutronStream<PulseBatcher<TimeOrderedEventStream>>>),
    /// Threaded out-of-core stream.
    Threaded(ThreadedOutOfCoreNeutronStream),
}

impl Iterator for OutOfCoreNeutronStreamHandle {
    type Item = Result<PulseNeutronBatch>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Single(stream) => stream.next(),
            Self::Threaded(stream) => stream.next(),
        }
    }
}

/// Threaded out-of-core stream with bounded queues.
///
/// Dropping the stream signals cancellation and joins worker threads; if a
/// batch is already being processed, shutdown waits for that batch to finish.
pub struct ThreadedOutOfCoreNeutronStream {
    /// Receives pulse outputs from the worker thread.
    receiver: mpsc::Receiver<Result<PulseNeutronBatch>>,
    /// Join handles for reader/worker threads.
    handles: Vec<thread::JoinHandle<()>>,
    /// Cancellation flag used to stop worker threads.
    cancel: Arc<AtomicBool>,
}

impl Iterator for ThreadedOutOfCoreNeutronStream {
    type Item = Result<PulseNeutronBatch>;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}

impl Drop for ThreadedOutOfCoreNeutronStream {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

/// Iterator over out-of-core pulse batches.
pub struct OutOfCoreNeutronStream<I>
where
    I: Iterator<Item = PulseBatchGroup>,
{
    /// Pulse batch source.
    batches: I,
    /// Pending slices from the current batch group.
    slices: VecDeque<PulseSlice>,
    /// Selected clustering algorithm.
    algorithm: ClusteringAlgorithm,
    /// Clustering configuration.
    clustering: ClusteringConfig,
    /// Extraction configuration.
    extraction: ExtractionConfig,
    /// Algorithm tuning parameters.
    params: AlgorithmParams,
    /// Current pulse TDC timestamp.
    current_tdc: Option<u64>,
    /// Accumulated neutrons for the current pulse.
    current_neutrons: NeutronBatch,
    /// Count of emitted hits for the current pulse.
    current_hits: usize,
    /// Completed pulse outputs waiting to be returned.
    pending: VecDeque<PulseNeutronBatch>,
    /// Whether the stream has been fully drained.
    finished: bool,
}

impl<I> OutOfCoreNeutronStream<I>
where
    I: Iterator<Item = PulseBatchGroup>,
{
    /// Create a new out-of-core stream from pulse batch groups.
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

        let emitted_hits = count_emitted_hits(&hits, slice.emit_cutoff_tof);
        if slice.emit_cutoff_tof != u32::MAX {
            neutrons = filter_neutrons_by_tof(&neutrons, slice.emit_cutoff_tof);
        }

        self.append_neutrons(slice.tdc_timestamp_25ns, &neutrons, emitted_hits);
        Ok(())
    }

    fn append_neutrons(
        &mut self,
        tdc_timestamp_25ns: u64,
        neutrons: &NeutronBatch,
        emitted_hits: usize,
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
        } else if self.current_tdc.is_none() {
            self.current_tdc = Some(tdc_timestamp_25ns);
        }

        self.current_neutrons.append(neutrons);
        self.current_hits = self.current_hits.saturating_add(emitted_hits);
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
) -> Result<Box<dyn Iterator<Item = Result<PulseNeutronBatch>>>> {
    let handle = out_of_core_neutron_stream_handle(
        reader, algorithm, clustering, extraction, params, memory,
    )?;

    Ok(Box::new(handle))
}

/// Build an out-of-core neutron stream handle from a TPX3 reader.
///
/// This exposes the underlying handle type, while [`out_of_core_neutron_stream`]
/// returns a boxed iterator for compatibility.
///
/// # Errors
/// Returns an error if the reader fails or the memory budget is invalid.
pub fn out_of_core_neutron_stream_handle(
    reader: &Tpx3FileReader,
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    memory: &OutOfCoreConfig,
) -> Result<OutOfCoreNeutronStreamHandle> {
    let overlap_tof = clustering.window_tof();
    let batcher = pulse_batches(reader, memory, overlap_tof)?;
    if memory.use_threaded_pipeline() {
        Ok(OutOfCoreNeutronStreamHandle::Threaded(
            build_threaded_stream(
                batcher,
                algorithm,
                clustering.clone(),
                extraction.clone(),
                params.clone(),
                memory.effective_parallelism(),
                memory.effective_queue_depth(),
            ),
        ))
    } else {
        Ok(OutOfCoreNeutronStreamHandle::Single(Box::new(
            OutOfCoreNeutronStream::new(
                batcher,
                algorithm,
                clustering.clone(),
                extraction.clone(),
                params.clone(),
            ),
        )))
    }
}

fn build_threaded_stream<I>(
    batcher: PulseBatcher<I>,
    algorithm: ClusteringAlgorithm,
    clustering: ClusteringConfig,
    extraction: ExtractionConfig,
    params: AlgorithmParams,
    parallelism: usize,
    queue_depth: usize,
) -> ThreadedOutOfCoreNeutronStream
where
    I: Iterator<Item = crate::reader::EventBatch> + Send + 'static,
{
    let (group_tx, group_rx) = mpsc::sync_channel::<PulseBatchGroup>(queue_depth);
    let (out_tx, out_rx) = mpsc::sync_channel::<Result<PulseNeutronBatch>>(queue_depth);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_reader = Arc::clone(&cancel);
    let cancel_worker = Arc::clone(&cancel);

    let reader_handle = thread::spawn(move || {
        for group in batcher {
            if cancel_reader.load(Ordering::Relaxed) {
                break;
            }
            let mut pending = group;
            loop {
                if cancel_reader.load(Ordering::Relaxed) {
                    return;
                }
                match group_tx.try_send(pending) {
                    Ok(()) => break,
                    Err(mpsc::TrySendError::Disconnected(_)) => return,
                    Err(mpsc::TrySendError::Full(group)) => {
                        pending = group;
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            }
        }
    });

    let worker_handle = thread::spawn(move || {
        let pool = if parallelism > 1 {
            rayon::ThreadPoolBuilder::new()
                .num_threads(parallelism)
                .build()
                .ok()
        } else {
            None
        };

        loop {
            if cancel_worker.load(Ordering::Relaxed) {
                break;
            }
            let group = match group_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(group) => group,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };

            if cancel_worker.load(Ordering::Relaxed) {
                break;
            }

            let result = if let Some(pool) = &pool {
                pool.install(|| {
                    process_group(group, algorithm, &clustering, &extraction, &params, true)
                })
            } else {
                process_group(group, algorithm, &clustering, &extraction, &params, false)
            };

            match result {
                Ok(group_batches) => {
                    for batch in group_batches {
                        if cancel_worker.load(Ordering::Relaxed) {
                            return;
                        }
                        if out_tx.send(Ok(batch)).is_err() {
                            return;
                        }
                    }
                }
                Err(err) => {
                    let _ = out_tx.send(Err(err));
                    return;
                }
            }
        }
    });

    ThreadedOutOfCoreNeutronStream {
        receiver: out_rx,
        handles: vec![reader_handle, worker_handle],
        cancel,
    }
}

fn process_group(
    group: PulseBatchGroup,
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    parallel: bool,
) -> Result<Vec<PulseNeutronBatch>> {
    let slice_results: Vec<Result<SliceOutput>> = if parallel {
        group
            .slices
            .into_par_iter()
            .map(|slice| process_slice_output(slice, algorithm, clustering, extraction, params))
            .collect()
    } else {
        group
            .slices
            .into_iter()
            .map(|slice| process_slice_output(slice, algorithm, clustering, extraction, params))
            .collect()
    };

    let mut outputs = Vec::with_capacity(slice_results.len());
    for result in slice_results {
        outputs.push(result?);
    }

    let mut batches = Vec::new();
    let mut current_tdc: Option<u64> = None;
    let mut current_batch = PulseNeutronBatch {
        tdc_timestamp_25ns: 0,
        hits_processed: 0,
        neutrons: NeutronBatch::default(),
    };

    for output in outputs {
        if current_tdc != Some(output.tdc_timestamp_25ns) {
            if current_tdc.is_some()
                && (current_batch.hits_processed > 0 || !current_batch.neutrons.is_empty())
            {
                batches.push(current_batch);
            }
            current_batch = PulseNeutronBatch {
                tdc_timestamp_25ns: output.tdc_timestamp_25ns,
                hits_processed: 0,
                neutrons: NeutronBatch::default(),
            };
            current_tdc = Some(output.tdc_timestamp_25ns);
        }

        current_batch.neutrons.append(&output.neutrons);
        current_batch.hits_processed = current_batch
            .hits_processed
            .saturating_add(output.hits_processed);
    }

    if current_tdc.is_some()
        && (current_batch.hits_processed > 0 || !current_batch.neutrons.is_empty())
    {
        batches.push(current_batch);
    }

    Ok(batches)
}

fn process_slice_output(
    slice: PulseSlice,
    algorithm: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
) -> Result<SliceOutput> {
    let mut hits = slice.hits;
    let mut neutrons =
        cluster_and_extract_batch(&mut hits, algorithm, clustering, extraction, params)
            .map_err(Error::CoreError)?;

    let emitted_hits = count_emitted_hits(&hits, slice.emit_cutoff_tof);
    if slice.emit_cutoff_tof != u32::MAX {
        neutrons = filter_neutrons_by_tof(&neutrons, slice.emit_cutoff_tof);
    }

    Ok(SliceOutput {
        tdc_timestamp_25ns: slice.tdc_timestamp_25ns,
        hits_processed: emitted_hits,
        neutrons,
    })
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

fn count_emitted_hits(hits: &HitBatch, cutoff: u32) -> usize {
    if hits.is_empty() {
        return 0;
    }
    hits.tof.partition_point(|&tof| tof <= cutoff)
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

    #[test]
    fn out_of_core_threaded_matches_single() {
        let config = OutOfCoreConfig::default().with_memory_budget_bytes(10_000);
        let batcher = crate::out_of_core::PulseBatcher::new(
            vec![
                make_event_batch(1, &[(1, 1, 10, 1, 10, 0), (100, 100, 20, 1, 20, 0)]),
                make_event_batch(2, &[(50, 50, 30, 1, 30, 0)]),
            ]
            .into_iter(),
            &config,
            0,
        )
        .unwrap();

        let clustering = ClusteringConfig {
            radius: 1.0,
            temporal_window_ns: 25.0,
            min_cluster_size: 1,
            max_cluster_size: None,
        };
        let extraction = ExtractionConfig::default();
        let params = AlgorithmParams::default();

        let threaded = build_threaded_stream(
            batcher,
            ClusteringAlgorithm::Grid,
            clustering.clone(),
            extraction.clone(),
            params.clone(),
            2,
            1,
        );
        let threaded_tofs = collect_neutrons(threaded);

        let mut expected = Vec::new();
        for mut pulse in [
            make_event_batch(1, &[(1, 1, 10, 1, 10, 0), (100, 100, 20, 1, 20, 0)]),
            make_event_batch(2, &[(50, 50, 30, 1, 30, 0)]),
        ] {
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

        assert_eq!(threaded_tofs, expected);
    }
}
