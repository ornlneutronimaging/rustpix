# rustpix-tools

Diagnostic and analysis tools for TPX3 detector data.

## Tools

### analyze-timestamps

Analyzes timestamp patterns in TPX3 files to understand TDC and hit behavior.

```bash
cargo run --bin analyze-timestamps -- <tpx3_file>
```

**Output includes:**
- TDC packet count and rollover detection
- Hit timestamp statistics and disorder rate
- Hits per TDC period (pulse) statistics
- Sample TDC timestamps and inter-pulse intervals

### analyze-rollover

Detects and reports timestamp rollover events in TPX3 files.

```bash
cargo run --bin analyze-rollover -- <tpx3_file>
```

**Output includes:**
- TDC rollover events with packet indices
- Hit timestamp rollover events
- Context around rollover points

## Use Cases

1. **Data validation**: Verify TPX3 file integrity and timestamp consistency
2. **Performance tuning**: Understand hit distribution across pulses
3. **Algorithm development**: Characterize timestamp patterns for ordering algorithms
4. **Debugging**: Identify rollover issues and timing anomalies

## Example Analysis

```bash
# Analyze a small test file
cargo run --bin analyze-timestamps -- tmp_test_data/test_data/tiny.tpx3

# Analyze a section of a large file
head -c 100000000 large_file.tpx3 > /tmp/sample.tpx3
cargo run --bin analyze-timestamps -- /tmp/sample.tpx3

# Find rollover points in a large file
cargo run --bin analyze-rollover -- /tmp/sample.tpx3
```

## Technical Notes

- Both tools parse TPX3 packets directly without using the rustpix-tpx library
- This makes them useful for debugging the library itself
- 30-bit timestamp counters overflow every ~26 seconds at 60Hz operation
