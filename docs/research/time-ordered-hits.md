
# Research: Efficient Time-Ordering of Hits During TPX3 Processing

## 1. Research Questions & Findings

### 1.1 How is TOF currently tracked per chip?

- **Mechanism**: TOF is calculated relative to the most recent TDC reference timestamp.
- **Data Flow**: `process_section` iterates through packets. When a TDC packet (ID 0x6F) is encountered, `current_tdc` is updated.
- **Calculation**: For each hit packet:
    1. A coarse timestamp (30-bit) is constructed from SPIDR time (16-bit) and ToA (14-bit).
    2. `correct_timestamp_rollover` adjusts this timestamp relative to the current `tdc_timestamp` to handle the 30-bit wrap-around near the boundary.
    3. `calculate_tof` computes the difference: `hit_timestamp - tdc_timestamp`.
- **Scope**: This tracking is strictly local to the current "section" (chip data chunk) and resets/inherits based on section boundaries logic in `discover_sections`.

### 1.2 How can pulse boundaries be detected?

- **Definition**: A "pulse" in the data stream is delineated by TDC packets.
- **Detection**: Monitoring the packet stream for ID 0x6F (TDC).
- **Reliability**: TDC packets are generally periodic (10-60Hz). Hits occurring between TDC $T_N$ and TDC $T_{N+1}$ belong to the pulse starting at $T_N$.

### 1.3 What ordering guarantees exist?

- **Within a Chip**:
  - **Long-range ordered**: TDCs appear in strict chronological order.
  - **Short-range disordered**: Pixel hits between two TDCs are *mostly* ordered but can be disordered due to pixel readout latency and bus arbitration.
- **Across Chips**:
  - **No guarantees**: The file format consists of chunks (sections) where each chunk belongs to a specific chip. These chunks might be interleaved arbitrarily (e.g., sequentially or round-robin), meaning Chip 0's Pulse 1 data could appear far apart from Chip 1's Pulse 1 data in the raw file.

### 1.4 What approach can provide time-ordered output efficiently?

- **Challenge**: We need to produce a single stream of hits sorted by global time (or Pulse ID + TOF) from $N$ potentially disordered chip streams.
- **Constraint**: Performance degradation must be minimal (<10%). Full sorting of the dataset is $O(N \log N)$ and too slow/memory-intensive.
- **Opportunity**: The data is already "globally sorted" at the pulse level. Disorder is local (within a pulse or chip).

## 2. Proposed Approach: Pulse-Based K-Way Merge

We propose a **Pulse-Based K-Way Merge** strategy that exploits the natural structure of the data.

### 2.1 Algorithm

1. **Stream Separation**: Logically view the file as $K$ independent streams (one per chip). This is achieved by grouping `Tpx3Section`s by `chip_id`.
2. **Pulse Buffering (Per Stream)**:
    - Read from each stream until a TDC packet (or end of stream) is encountered.
    - Collect all hits for this "Pulse Frame".
    - **Sort** the hits within this single frame. Since the frame size is small (hits per 16ms pulse), sorting is extremely fast ($O(M \log M)$ where $M$ is small).
3. **Global Merge**:
    - Maintain a Min-Heap (Priority Queue) containing the next available "Pulse Batch" from each of the $K$ chips.
    - The Heap is ordered by the Batch's Reference TDC Timestamp.
    - Extract the Batch with the smallest TDC Timestamp.
    - If multiple chips have batches with the *same* TDC Timestamp (synchronous pulse), extract all of them.
    - Perform a merge sort of these concurrent batches (or simply concatenate if they are spatially distinct and strict time ordering between concurrent hits on different chips isn't critical, though true time-ordering requires merging).
    - Emit the sorted hits.
4. **64-bit Extension (Optional but Recommended)**:
    - Track TDC rollovers to construct a 64-bit global timestamp to ensure ordering across long experiments (>26s).

### 2.2 Trade-offs

- **Pros**:
  - **Memory Efficient**: Only buffers one pulse per chip at a time.
  - **High Performance**: Avoids sorting the massive global dataset. Sorts only small local buffers.
  - **Robust**: Handles short-range disorder naturally via the pulse buffer sort.
- **Cons**:
  - **Complexity**: Requires managing state for $K$ parallel iterators.
  - **Latency**: Output is delayed by one pulse duration (must wait for end-of-pulse TDC to sort). This is negligible for offline processing.

## 3. Implementation Plan

1. **Refactor**: No major refactoring needed. New functionality will be additive.
2. **New Module**: `rustpix-tpx/src/ordering.rs`
    - `PulseReader`: A struct that consumes a chip's sections and yields `SortedPulse` objects.
    - `TimeOrderedStream`: The main iterator that manages `Vec<PulseReader>` and performs the merge.
3. **Verification**:
    - Unit tests with synthetic interleaved data.
    - Benchmark against trivial "collect all and sort" baseline.
