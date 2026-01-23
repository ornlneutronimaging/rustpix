# Research: Out-of-core processing strategy for large TPX3 datasets

## 1. Goal

Process TPX3 files that are larger than RAM while preserving correct time ordering and clustering
behavior. The pipeline should remain SoA-native and avoid whole-file materialization.

## 2. Observations and constraints

- TPX3 data is structured into per-chip sections. Sections are not globally ordered across chips.
- TDC packets provide pulse boundaries and are synchronized across chips.
- Hit timestamps and TDC timestamps can roll over independently.
- Global sorting by raw hit timestamp is incorrect; ordering should be based on
  (tdc_epoch, tdc_timestamp, tof) with late-hit handling. See:
  - docs/research/tpx3-timestamp-analysis.md
  - docs/research/time-ordered-hits.md
- Some clustering algorithms rely on time ordering (Grid assumes TOF-sorted input).
- Memory must remain bounded by a function of hits-per-pulse and temporal windows,
  not by total file size.

## 3. Proposed out-of-core architecture

### 3.1 Reader and ordering stage

Use chunked IO with the existing packet scanner and time-ordered merge:

1) Chunk the mmap region (or use a sliding window). For each chunk:
   - Use PacketScanner::scan_sections to find sections and consumed bytes.
   - Build Tpx3Section entries per chip, carrying initial_tdc from the previous chunk.
   - Scan each section for final_tdc to update per-chip TDC state.

2) Time-order hits with TimeOrderedStream:
   - Group sections by chip, buffer one pulse per chip.
   - Sort hits within a pulse and merge pulses by (tdc_epoch, tdc_timestamp).
   - Track TDC rollover per chip and carry the epoch across chunks.

Output of this stage is a time-ordered stream of HitBatch values per pulse.

### 3.2 Clustering stage (streaming)

Clustering can be made out-of-core by processing a bounded time window.
Two viable strategies:

Option A: Pulse window with overlap
- Maintain a rolling buffer of hits across K pulses, where K is large enough to cover
  the maximum temporal window (temporal_window_ns / 25ns).
- When a new pulse arrives, add it to the buffer and cluster the window.
- Emit clusters for hits that are older than the window minus overlap.
- Keep residual hits in the overlap so clusters that span boundaries remain correct.

Option B: Algorithm-specific streaming
- Abs clustering already maintains internal state and can be truly streaming.
- Grid clustering can be adapted to finalize clusters when hits fall outside
  the temporal window; this still requires time-ordered input.
- DBSCAN is not incremental; for out-of-core it should use Option A with overlap
  or be limited to in-memory batches.

### 3.3 Extraction stage (streaming)

Extraction can be done once cluster labels are final for a region:

- Maintain per-cluster accumulators (sum_x, sum_y, sum_tot, max_tot hit) for the
  active window.
- Finalize and emit neutrons when a cluster is sealed (no future hits can be assigned
  given the time window).
- The accumulation logic mirrors SimpleCentroidExtraction but operates on a
  rolling window instead of a full batch.

### 3.4 Outputs

- Write neutrons or histograms incrementally to disk (CSV, binary, HDF5).
- Support an optional "emit hits" mode for debugging, but default to neutrons
  or histograms for memory efficiency.

## 4. Memory profile

Let:
- P = hits per pulse per chip
- C = number of chips
- K = pulses in the overlap window

Then memory is O(P * C * K) for hits plus clustering state. This is bounded and
independent of file size.

## 5. Implementation plan

1) Introduce a core streaming pipeline that yields time-ordered pulse batches
   using TimeOrderedStream with rollover tracking.
2) Add a streaming clustering interface:
   - Provide a windowed wrapper for Grid/DBSCAN.
   - Use AbsState for true streaming.
3) Add streaming extraction using per-cluster accumulators.
4) Expose a CLI and Python binding entry-point for out-of-core processing.
5) Add tests:
   - Cross-chunk correctness (clusters spanning boundary).
   - TDC rollover ordering across long scans.
   - Regression tests for clustering equivalence to in-memory results.

## 6. Open questions

- What maximum temporal window should define the overlap size in pulses?
- Should DBSCAN be supported out-of-core or only for in-memory runs?
- Do we need a stable global cluster id across windows, or is per-window fine
  for downstream analysis?

## 7. Risks and mitigations

- Risk: incorrect cluster finalization if overlap is too small.
  Mitigation: enforce overlap >= max temporal window and add tests.

- Risk: performance regression if overlap is too large.
  Mitigation: tune overlap based on detector config and algorithm requirements.

- Risk: divergence from in-memory results.
  Mitigation: compare streaming outputs to in-memory for small datasets.
