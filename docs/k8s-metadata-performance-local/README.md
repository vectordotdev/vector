# Local Kubernetes metadata performance investigation

These artifacts are intentionally uncommitted. The production change is committed
on `opencode/k8s-metadata-bulk-insert` and affects only pod labels and annotations.

## Throughput

Five alternating baseline/bulk pairs used 150k input EPS, a one-CPU collector,
21 labels, 10 annotations, one-second file discovery, and a blackhole sink.
Median paired throughput improvement: 33.33%; range: 31.17% to 37.29%.
Median paired CPU/event reduction: 24.97%. Bulk processed roughly 100k–105k EPS,
versus 75k–78k EPS baseline. Both drained backlog afterward. All bulk final counts
matched expectations; one baseline run had one extra event. Count agreement does
not establish per-event equivalence. See `comparison-bulk-150000eps/` for raw data.

The baseline and bulk images use pinned, unpatched VRL and matched Rust 1.96.1,
musl, jemalloc and release settings. See `build-metadata.json` for binary hashes.

## Comparison counts and CPU profiles

`k8s-bulk-count/` contains the controlled ordering-count fixture and its limitations.
At 21 labels plus 10 annotations: absent destinations use 284 versus 41 ordering
comparisons; existing destinations use 322 versus 215. These are fixture counts,
not live collector counts. The fixture references its isolated instrumented VRL
under the original temporary artifact directory; do not mistake it for a self-contained
reproduction or alter the sibling VRL checkout to satisfy that path.

`profiles-bulk-150000eps/` contains the completed four-run ABBA capture, including
raw perf data, stack reports and `comparison-cost-summary.json`. All four final
event counts matched expectations. Mean sampled memcmp CPU/event fell from
2.630 to 1.053 microseconds (59.9%); memcmp with an identifiable BTreeMap ancestor
fell from 1.933 to 0.681 microseconds (64.8%). Mean total collector CPU/event was
14.258 versus 10.121 microseconds. These are descriptive means from two runs per
variant, not independent confirmation of the five-pair throughput effect size.
Sampling estimates CPU cost, not invocation counts. Inclusive categories overlap;
inlining and unwind warnings limit attribution. A separate instrumented-fixture
compilation ran during this profiling session; assess overlap before interpreting
the profiles as uncontended performance measurements.

## Candidate 3: cached lexicographic key prefixes

Recommendation: investigate separately; do not change Vector's VRL dependency yet.
This also ultimately requires a VRL key-representation change, even though a
standalone prototype avoids complicating the current Vector image build.

`prefix-bench.rs` is a measured standalone prototype. It caches the first 8, 16 or 32 bytes
as big-endian integer words and falls back to full string ordering on ties. It
retains Borrow<str> lexical consistency, including short strings containing NULs.
Checks comparing ordering with String and borrowed map lookup passed, including
empty strings, embedded NULs, UTF-8 and deterministic generated strings.

The benchmark ran after live profiling exited, on host Rust 1.96.1 with `-O`,
five alternating-order measurements per case. See `prefix-results.csv`,
`prefix-summary.json`, `prefix-environment.json` and `prefix-checks.log`.
For 21 Kubernetes-style keys, cached prefixes increased complete map-lifetime
time by 9.9%, 13.0%, and 13.2% for repeated insertion, and 5.5%, 10.3%, and 24.7%
for sorted collection (8/16/32-byte prefixes respectively). Short or early-diverging
keys benefited in repeated insertion in this run, but not sorted collection.
Key size rose from 24 bytes to 32/40/56 bytes. These are BTreeMap model results,
not an implementation benchmark inside VRL or an end-to-end Vector measurement.
The Kubernetes-style fixture uses 21 common-prefix label names rather than the
exact throughput case's 20 labels plus differently named selector label.

Recommendation: do not pursue this simple cached-prefix representation for the
current workload. It regressed in both modeled insertion modes and enlarges keys.

Important limits:

- Borrowed string lookups bypass KeyString::cmp, so cached ordering cannot speed
  those comparisons without another API/representation change.
- The benchmark's `bench.example/keyNN` names share their first 16 bytes; an 8- or
  16-byte prefix cannot distinguish them and adds work before the fallback.
- Larger cached prefixes enlarge BTreeMap nodes and add construction/copy costs.
- Bulk insertion already eliminates most comparisons for absent destinations;
  remaining headroom must be measured on top of bulk insertion, not only baseline.
- A reduced memcmp sample percentage alone is insufficient: measure complete
  construction/lookup/drop lifetimes and absolute CPU/event.

The prototype is deliberately outside production source and has not been used in
either throughput image. Its benchmarks ran after live profiling finished
to avoid introducing additional host contention.
