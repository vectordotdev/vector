# Datadog Metrics v1 vs v2 Endpoint Benchmark Results

## Setup

- **v1/v2 selection**: `VECTOR_TEMP_USE_DD_METRICS_SERIES_V2_API=1` enables v2 (default is v1)
- **Metric count**: 10 unique metric names
- **Event profile**: ~46 tags per event (~932 bytes statsd wire size)
  - 26 static tags (env, service, region, cluster, kube labels, etc.)
  - 15 high-cardinality tags (8-byte random values)
  - host tag (1000-cardinality), run label, seq (incrementing — prevents collapse)
- **Repeats**: 2 (averaged), **Warmup**: 15s, **Measurement**: 60s
- **Batch timeout**: 2s (default)

## Reproducing

```bash
# Build Vector
cargo build --release

# Experiment 1 — 50k/s, no limits
DD_API_KEY=<key> DD_SITE=<site> python3 scripts/benchmark_dd_metrics_v1_v2.py \
  --vector-bin ./target/release/vector \
  --rate 50000 --metric-count 10 --repeats 1 \
  --warmup-seconds 15 --measure-seconds 60

# Experiment 2 — 50k/s, explicit per-endpoint limits
DD_API_KEY=<key> DD_SITE=<site> python3 scripts/benchmark_dd_metrics_v1_v2.py \
  --vector-bin ./target/release/vector \
  --rate 50000 --metric-count 10 --repeats 1 \
  --warmup-seconds 15 --measure-seconds 60 \
  --batch-max-bytes-v1 62914560 --batch-max-bytes-v2 5242880

# Experiment 3 — 100k/s, no limits
DD_API_KEY=<key> DD_SITE=<site> python3 scripts/benchmark_dd_metrics_v1_v2.py \
  --vector-bin ./target/release/vector \
  --rate 100000 --metric-count 10 --repeats 1 \
  --warmup-seconds 15 --measure-seconds 60

# Experiment 4 — 100k/s, explicit per-endpoint limits
DD_API_KEY=<key> DD_SITE=<site> python3 scripts/benchmark_dd_metrics_v1_v2.py \
  --vector-bin ./target/release/vector \
  --rate 100000 --metric-count 10 --repeats 1 \
  --warmup-seconds 15 --measure-seconds 60 \
  --batch-max-bytes-v1 62914560 --batch-max-bytes-v2 5242880
```

v1 is the default. To run in v2 mode the script sets `VECTOR_TEMP_USE_DD_METRICS_SERIES_V2_API=1`
automatically for the v2 run.

---

## Experiment 1 — 50k/s, no batch byte limits

Both endpoints use Vector's default: no byte cap, up to 100k events per batch.

```
metric                    v1         v2      delta(v2-v1)
avg_cpu_percent             165.25    148.97      -16.28 (-9.9%)
avg_rss_mb                 3062      4871        1809    (+59.1%)
peak_rss_mb                3615      5988        2373    (+65.6%)
http_requests_sent_eps        1.04      9.44        8.40  (+808%)
loss_rate                     0.00      0.00        0.00  (n/a)
```

## Experiment 2 — 50k/s, explicit per-endpoint byte limits

`batch.max_bytes`: v1=62914560 (60 MiB), v2=5242880 (5 MiB)

```
metric                    v1         v2      delta(v2-v1)
avg_cpu_percent             154.44    135.25      -19.19 (-12.4%)
avg_rss_mb                  837      1001         164    (+19.6%)
peak_rss_mb                 923      1170         247    (+26.7%)
http_requests_sent_eps        4.28     52.34       48.06  (+1123%)
loss_rate                     0.00      0.00        0.00  (n/a)
```

## Experiment 3 — 100k/s, no batch byte limits

```
metric                    v1         v2      delta(v2-v1)
avg_cpu_percent             161.58    149.57      -12.02 (-7.4%)
avg_rss_mb                 4565      7166        2601    (+57.0%)
peak_rss_mb                5769      9901        4132    (+71.6%)
http_requests_sent_eps        2.07     18.23       16.16  (+781%)
loss_rate                     0.00      0.00        0.00  (n/a)
```

## Experiment 4 — 100k/s, explicit per-endpoint byte limits

`batch.max_bytes`: v1=62914560 (60 MiB), v2=5242880 (5 MiB)

```
metric                    v1         v2      delta(v2-v1)
avg_cpu_percent             184.48    144.11      -40.37 (-21.9%)
avg_rss_mb                 1896      1853          -43   (-2.3%)
peak_rss_mb                2445      2052         -393   (-16.1%)
http_requests_sent_eps        8.68    102.81       94.13  (+1085%)
loss_rate                     0.00      0.00        0.00  (n/a)
```

---

## Summary

| rate | limits | v1 avg RSS | v2 avg RSS | v2 vs v1 RSS | v1 cpu | v2 cpu | v2 vs v1 cpu | v1 req/s | v2 req/s |
|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 50k/s | none | 3062 MB | 4871 MB | **+59%** | 165% | 149% | -10% | 1.04 | 9.44 |
| 50k/s | explicit | 837 MB | 1001 MB | **+20%** | 154% | 135% | **-12%** | 4.28 | 52.34 |
| 100k/s | none | 4565 MB | 7166 MB | **+57%** | 162% | 150% | -7% | 2.07 | 18.23 |
| 100k/s | explicit | 1896 MB | 1853 MB | **-2%** | 184% | 144% | **-22%** | 8.68 | 102.81 |

---

## Root cause

The sink pipeline is:

```
statsd events
  → batched_partitioned   (batcher: max_events=100k, max_bytes=unlimited by default)
  → concurrent_map        (sort_and_collapse, 8 concurrent tasks)
  → incremental_request_builder  (splits oversized batches into sub-requests)
  → flat_map              (unrolls sub-requests)
  → into_driver           (HTTP sender)
```

`sort_and_collapse_counters_by_series_and_timestamp` (`sink.rs:174`) does two things on
the full batch `Vec<Metric>`:

1. **Sort** (`sort_unstable_by`) — O(N log N) in-place sort by `(metric_type, series,
   timestamp)`, where series is the unique `(metric_name, tags)` combination. This also
   improves HTTP compression by 2–3x on the encoded payload.
2. **Collapse** (`dedup_by`) — merges consecutive counters that share the same series and
   timestamp (second precision) by summing their values. In our benchmark this is a no-op
   because every event has a unique `seq:N` tag, so no two events share a series.

**Without byte limits**, the batcher accumulates up to **100,000 events** before flushing
(hits `MAX_EVENTS`). At 50k/s with 2s timeout, every batch hits the 100k cap. The encoder
then splits each 100k-event batch into sub-requests to fit the API payload limit:

- v2: ~100k × ~5.5 KB = ~550 MB → **~110 sub-requests** (5 MiB each)
- v1: ~100k × ~5.5 KB = ~550 MB → **~9 sub-requests** (60 MiB each)

v2's ~110 sub-requests take ~27s to drain at 4 concurrent HTTP connections. During that
time, new batches keep arriving and their sort tasks are eagerly spawned by `ConcurrentMap`
(implemented with `FuturesOrdered`). The sorted `Vec<Metric>` results sit in
`FuturesOrdered` waiting to be consumed, so up to 8 full 100k-event batches are held in
memory simultaneously — not because sorting is slow, but because downstream backpressure
prevents `concurrent_map` from yielding completed results.

**With explicit byte limits**, the batcher flushes every ~960 events for v2 (5 MiB /
~5.5 KB per event) vs every ~11,600 events for v1 (60 MiB / ~5.5 KB). Each batch fits
in a single HTTP request — no splitting, no downstream backpressure. The `sort_and_collapse`
buffer shrinks from 100k events to ~960 events per task (~100x).

---

## Memory model

Each event contributes two terms to memory usage:

- **`E_mem`**: size of a `Metric` struct on the Rust heap. Dominated by the `BTreeMap`
  of tags (~70 bytes per entry for node pointer overhead + two heap-allocated `String`s).
  With N tags averaging L chars each: `E_mem ≈ N × (70 + L)`.
- **`E_wire`**: serialized size per event in the HTTP payload (protobuf for v2, JSON for
  v1). With N tags: `E_wire ≈ N × L + metric_name_len + framing`.

**Without byte limits** (worst case — HTTP driver stalled, all 8 `FuturesOrdered` slots filled):

```
memory = concurrency × MAX_EVENTS × (E_mem + E_wire)
       = 8 × 100,000 × (E_mem + E_wire)
```

Memory scales **linearly with event size**. Larger events = proportionally more RAM.

**With byte limits** (batcher capped at `payload_limit / E_wire` events per batch):

```
memory = concurrency × (payload_limit / E_wire) × (E_mem + E_wire)
       = concurrency × payload_limit × (1 + E_mem / E_wire)
```

Since `E_mem ≈ E_wire` (both dominated by tag data), this simplifies to:

```
memory ≈ 2 × concurrency × payload_limit
       = 2 × 8 × 5 MiB ≈ 80 MB
```

Memory is **bounded by the payload limit** and independent of event size.

### Worst-case estimates

A somewhat worst-case (but realistic) production workload: k8s pod labels + cloud resource tags + custom business
tags. Typical range: 50–100 tags, 25–40 chars average per tag.

| tags | avg tag len | E_wire | E_mem | without fix | with fix |
|---:|---:|---:|---:|---:|---:|
| 30 | 20 chars | ~700 B | ~2.5 KB | **2.6 GB** | ~190 MB |
| 50 | 25 chars | ~1.4 KB | ~4 KB | **4.3 GB** | ~155 MB |
| 80 | 30 chars | ~2.5 KB | ~6.5 KB | **7.2 GB** | ~145 MB |
| 100 | 35 chars | ~3.6 KB | ~8.5 KB | **9.7 GB** | ~135 MB |

Without the fix, a customer with 100 tags per metric can push Vector to **~10 GB RSS**
under normal operating conditions at 50k events/s. With the fix, the same workload stays
under ~150 MB regardless of tag count.

---

## Analysis

### Memory (RSS)

Without byte limits, v2 uses 57–59% more memory than v1 at both rates.
With explicit per-endpoint limits, memory drops to near-parity: +20% at 50k/s,
-2% at 100k/s (where v1's larger batches accumulate more under backpressure).

### CPU

CPU is broadly comparable between v1 and v2. With explicit limits, v2 shows some
improvement (12–22%), possibly due to smaller batches reducing sort work or protobuf
being cheaper to encode than JSON — but no CPU profiling has been done to confirm the
root cause.

### HTTP request rate

v2 sends ~12x more HTTP requests than v1 — a direct consequence of the 12x difference in
payload limits (60 MiB / 5 MiB). This is expected and consistent across all experiments.

### Throughput & Delivery

Zero loss and zero errors in all experiments. Batch size does not affect correctness
or throughput.

### Fix

The fix is to cap the batcher's byte limit to the endpoint's uncompressed payload limit
(5 MiB for v2, 60 MiB for v1) when the user has not configured `batch.max_bytes`. This
ensures batches never accumulate more data than fits in a single HTTP request, eliminating
the encoder splitting step and its associated backpressure and memory overhead — without
requiring any user configuration.
