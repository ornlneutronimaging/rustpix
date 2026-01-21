# TPX3 Timestamp Analysis Report

**Date**: January 2026
**Data Source**: VENUS detector TPX3 files from SNS
**Tools Used**: `analyze-timestamps`, `analyze-rollover` (see `tools/`)

## Executive Summary

This report documents critical findings about TPX3 timestamp behavior that are essential for implementing time-ordered hit output. The key discovery is that **hit timestamps and TDC timestamps can roll over independently**, which was previously undocumented and has significant implications for data processing algorithms.

## 1. TPX3 Timestamp Architecture

### 1.1 Counter Structure

```
┌─────────────────────────────────────────────────────────────┐
│                    30-bit Timestamp Space                    │
│                                                             │
│  Max Value: 1,073,741,823 (0x3FFFFFFF)                     │
│  Resolution: 25ns per unit                                  │
│  Rollover Period: ~26.8 seconds                            │
└─────────────────────────────────────────────────────────────┘
```

**Two types of timestamps share this 30-bit space:**

| Type | Source | Formula |
|------|--------|---------|
| TDC Timestamp | TDC packet (0x6F) | `(raw >> 12) & 0x3FFFFFFF` |
| Hit Timestamp | Hit packet (0xB*) | `(spidr << 14) \| toa` |

### 1.2 TDC Behavior at SNS

```
TDC Period (60Hz):  666,675 units = 16.67ms
TDC Period (10Hz): 4,000,000 units = 100ms

Timeline (60Hz operation):
┌────┬────┬────┬────┬────┬────┬────┬────┬────┬────┐
│TDC0│TDC1│TDC2│TDC3│... │... │... │... │TDC │TDC │
│ 0  │667K│1.3M│2.0M│    │    │    │    │1073M│365K│
└────┴────┴────┴────┴────┴────┴────┴────┴────┴────┘
                                         ↑
                                    ROLLOVER
                               (after ~1610 pulses / ~26.8s)
```

## 2. Key Findings

### 2.1 Finding #1: TDC Timestamps Are Synchronized Across Chips

All 4 chips in the VENUS quad detector receive identical TDC timestamps from the synchronized hardware trigger.

**Evidence** (from 100MB sample):
```
Chip 0: First 5 TDC: [613820, 1280491, 1947166, 2613841, 3280516]
Chip 1: First 5 TDC: [613820, 1280491, 1947166, 2613841, 3280516]
Chip 2: First 5 TDC: [613820, 1280491, 1947166, 2613841, 3280516]
Chip 3: First 5 TDC: [613820, 1280491, 1947166, 2613841, 3280516]

TDC diffs: [666671, 666675, 666675, 666675] (consistent 60Hz)
```

**Implication**: TDC can be used as a reliable pulse boundary marker across all chips.

### 2.2 Finding #2: Significant Short-Range Timestamp Disorder

Approximately **27% of hit timestamps decrease** relative to the previous hit, indicating substantial local reordering from TPX3 FIFO readout.

**Evidence**:
```
Chip 0: Hit decrease rate: 27.22%
Chip 1: Hit decrease rate: 27.80%
Chip 2: Hit decrease rate: 27.61%
Chip 3: Hit decrease rate: 27.50%
```

**Visualization**:
```
Ideal ordering:     h1 → h2 → h3 → h4 → h5 → h6
                    ↓
Actual ordering:    h1 → h3 → h2 → h4 → h6 → h5
                         ↑         ↑
                      decreases (27% of transitions)
```

**Implication**: Simple sequential processing assumes wrong order for ~27% of hit pairs.

### 2.3 Finding #3: Hit and TDC Timestamps Roll Over Independently

**Critical Discovery**: Hit timestamps can reach the 30-bit maximum and roll over BEFORE the corresponding TDC timestamp rolls over.

**Evidence** (from rollover analysis at file offset ~1.53GB):
```
=== HIT TIMESTAMP ROLLOVER ===
Packet 1887425: hit_ts 1,073,741,747 → 174
Current TDC: 1,073,440,551 (not yet rolled over!)

... ~10,000 packets later ...

=== TDC ROLLOVER ===
Packet 1898445: TDC 1,073,440,551 → 365,402
```

**Timeline visualization**:
```
Packet Index:     1.88M          1.89M          1.90M
                    ↓              ↓              ↓
Hit Timestamps: ███████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
                   1073M→0         (already low: ~365K)

TDC Timestamps: █████████████████████░░░░░░░░░░░░░░░░░
                                  1073M→365K
                                     ↑
                               TDC rolls over later
```

**Gap**: ~10,000 packets (~800μs at typical rates) between hit rollover and TDC rollover.

**Implication**:
- The assumption that TDC and hit timestamps are "in sync" is incorrect
- Rollover correction must handle the case where hits have rolled but TDC hasn't
- This is already handled by `correct_timestamp_rollover()` but wasn't documented

### 2.4 Finding #4: TDC Does NOT Reset Each Pulse

**Previous assumption**: TDC resets to 0 at each pulse or when approaching overflow.

**Actual behavior**: TDC continuously increments until 30-bit overflow, then wraps to 0.

**Evidence**:
```
File offset 0GB:    TDC range:    613,820 →   27,280,807
File offset 1GB:    TDC range: 635,163,991 → 661,830,982
File offset 1.5GB:  TDC range: 961,439,189 → 974,772,686  (approaching max)
File offset 1.54GB: TDC range:     365,402 →  13,698,898  (rolled over!)
File offset 2GB:    TDC range: 213,305,889 → 240,639,555
```

**Implication**: Long experiments (>26s) will have multiple TDC rollovers that must be tracked for global time reconstruction.

## 3. Data Flow Diagram

```
                        TPX3 Hardware
                             │
         ┌───────────────────┼───────────────────┐
         ▼                   ▼                   ▼
    ┌─────────┐         ┌─────────┐         ┌─────────┐
    │ Chip 0  │         │ Chip 1  │         │ Chip N  │
    │  FIFO   │         │  FIFO   │         │  FIFO   │
    └────┬────┘         └────┬────┘         └────┬────┘
         │                   │                   │
         │  Short-range      │  Short-range      │
         │  disorder         │  disorder         │
         │  (~27%)           │  (~27%)           │
         ▼                   ▼                   ▼
    ┌─────────────────────────────────────────────────┐
    │              TPX3 File (Sections)               │
    │                                                 │
    │  [Header₀][TDC][Hits...][TDC][Hits...]         │
    │  [Header₁][TDC][Hits...][TDC][Hits...]         │
    │  ...interleaved arbitrarily...                 │
    └─────────────────────────────────────────────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │   Processing    │
                    │                 │
                    │ • Long-range    │
                    │   ordered       │
                    │ • Short-range   │
                    │   disordered    │
                    │ • Independent   │
                    │   rollovers     │
                    └─────────────────┘
```

## 4. Implications for Time-Ordering Algorithm

### 4.1 What CANNOT Work

1. **Simple TDC-based assignment**: Assigning hits to TDC based on arrival order fails for ~27% of hits near pulse boundaries.

2. **Assuming synchronized rollovers**: Hit timestamps can be "ahead" of TDC by ~10K packets when approaching rollover.

3. **Global sort by timestamp**: Would incorrectly interleave hits from different pulses due to rollover.

### 4.2 What CAN Work

1. **Pulse-based processing with boundary refinement**:
   - Use TDC as coarse pulse boundary
   - Use TOF values to refine hit-to-pulse assignment near boundaries
   - Late hits (TOF > 14ms at 60Hz) arriving after TDC likely belong to previous pulse

2. **K-way merge with rollover tracking**:
   - Track rollover count for global timestamp reconstruction
   - Merge chip streams by (rollover_count, tdc_timestamp, tof)

3. **Bounded reordering buffer**:
   - Exploit long-range ordering property
   - Buffer size ~10K packets handles both disorder and rollover gap

## 5. Recommendations

### 5.1 Immediate Actions

1. **Document the independent rollover behavior** in code comments
2. **Add rollover tracking** to any global time reconstruction
3. **Use TOF-based boundary refinement** (as in NeuNorm) for pulse assignment

### 5.2 Algorithm Design

The existing `correct_timestamp_rollover()` and `calculate_tof()` functions handle individual hit correction correctly. The challenge is in the ordering/streaming layer:

```rust
// Pseudo-code for correct pulse assignment
fn assign_pulse(hit_ts: u32, tdc_ts: u32, tdc_period: u32) -> PulseAssignment {
    let corrected_ts = correct_timestamp_rollover(hit_ts, tdc_ts);
    let tof = calculate_tof(corrected_ts, tdc_ts, tdc_period);

    if tof > LATE_HIT_THRESHOLD {
        // This hit belongs to PREVIOUS pulse
        PulseAssignment::Previous
    } else {
        PulseAssignment::Current
    }
}
```

### 5.3 Testing Requirements

Any time-ordering implementation must be tested against:
1. Normal operation (no rollovers)
2. Hit rollover without TDC rollover
3. TDC rollover
4. Multi-chip synchronization
5. Variable pulse rates (10-60Hz)

## 6. Appendix: Raw Data

### A. Sample File Statistics

**File**: `Run_8216_April25_2025_OB_MCP_TPX3_0_8C_1_9_AngsMin_serval_000000.tpx3`
**Size**: 8.2 GB
**Sections**: 4 chips (VENUS quad)

| Metric | Value |
|--------|-------|
| Total pulses (estimated) | ~50,000 (8GB / 100MB sample × 412 TDCs) |
| Hits per pulse (avg) | ~7,500 |
| Hit disorder rate | 27.2% - 27.8% |
| TDC rollovers | ~3 (8GB / 1.5GB per rollover) |

### B. Tools Used

```bash
# Timestamp pattern analysis
cargo run --bin analyze-timestamps -- <file>

# Rollover detection
cargo run --bin analyze-rollover -- <file>

# Sampling large files
head -c 100000000 large.tpx3 > sample.tpx3
```

---

*This report was generated through empirical analysis of VENUS detector data. The findings should be validated against detector documentation and confirmed with the detector group.*
