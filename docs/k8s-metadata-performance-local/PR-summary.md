## Summary
<!-- Please provide a brief summary about what this PR does.
This should help the reviewers give feedback faster and with higher quality. -->

Batch pod labels and annotations instead of repeatedly traversing their shared metadata paths. Build missing objects once; extend existing objects while preserving unrelated keys and overwriting collisions.

Preserves both log namespaces, disabled/custom paths, literal keys, and empty-input behavior. No VRL dependency or configuration changes.

This **supersedes [VRL #2044](https://github.com/vectordotdev/vrl/pull/2044) for this Kubernetes-enrichment optimization**, without replacing its general-purpose CRUD improvements or depending on its patch.

### References
<!-- Closes: <#issue/#PR or full link> -->
<!-- Related: <#issue/#PR or full link> -->

Related: #23546 and https://github.com/vectordotdev/vrl/issues/1470.

## Vector configuration
<!-- Include Vector configuration(s) you used to test and debug your changes. -->

<details>
<summary>Benchmark configuration</summary>

```yaml
data_dir: /vector-data
sources:
  logs:
    type: kubernetes_logs
    self_node_name: ${VECTOR_SELF_NODE_NAME}
    extra_label_selector: benchmark=vrl-kind-ab
    max_read_bytes: 20480
    glob_minimum_cooldown_ms: 1000
  vector_metrics:
    type: internal_metrics
    scrape_interval_secs: 1
sinks:
  discard:
    type: blackhole
    inputs: [logs]
  metrics:
    type: prometheus_exporter
    inputs: [vector_metrics]
    address: 0.0.0.0:9598
```

</details>

## How did you test this PR?
<!-- Please describe how you tested your changes. Also include any information about your setup. -->

**Checks passed:** eight pod-annotator tests, focused workspace/all-target Clippy, and formatting. Differential tests compare batching against per-key insertion across namespaces, custom/root/array paths, literal keys, collisions, scalar replacement, and empty input. Full-feature checks were not run.

```sh
make test FEATURES="sources-kubernetes_logs" SCOPE="--package vector sources::kubernetes_logs::pod_metadata_annotator"
FEATURES="sources-kubernetes_logs" make check-clippy
make check-fmt
```

**Performance:** five alternating baseline/bulk pairs on local arm64 kind, with 150k input EPS, a one-CPU/512-MiB collector, 21 labels, and 10 annotations. Both images used the same Vector revision, unpatched pinned VRL, Rust 1.96.1, musl/jemalloc, minimal features, and release settings; only the pod annotator differed.

| Measurement | Result |
| --- | --- |
| Throughput | 75–78k → 100–105k EPS |
| Median paired throughput change | **+33.3%** (range +31.2% to +37.3%) |
| Median paired CPU/event change | **−25.0%** |
| Sampled `memcmp` CPU/event | 2.630 → 1.053 µs (**−60%**) |

Both variants drained backlog afterward. All bulk final counts matched expectations; one baseline run had one extra event. Aggregate counts do not prove per-event correctness, and these local results do not establish a production-wide speedup or resolve the original report's 4x gap.

The `memcmp` result comes from separate ABBA profiles (60 seconds at 99 Hz). It estimates CPU cost, not comparison counts; incomplete unwinding, inlining, and possible compilation contention limit precision. Raw artifacts remain local and uncommitted.

Implementation generated with GPT-6.1 Sol; separate manual code review found no actionable issues.

<details>
<summary>Reproduction method</summary>

**Build matched baseline and bulk-insert images**

Use Vector `ae18932a175eb6b710d5c49a775ad31d437f31c9` for the baseline and apply only this PR’s pod-annotator change for the second image. Keep VRL pinned to `165d5b37b8bbae4967ca1529ac8da4157a2a0cc6`; do not include VRL #2044.

Build both with Rust 1.96.1 for `aarch64-unknown-linux-musl`, using identical lockfiles and settings:

```sh
export CARGO_BUILD_JOBS=1
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_RELEASE_LTO=thin
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
export JEMALLOC_SYS_WITH_LG_PAGE=16
export CFLAGS="-g0 -O3"

cargo build --locked --release \
  --target aarch64-unknown-linux-musl \
  --no-default-features \
  --features sinks-blackhole,sinks-prometheus,sources-file,sources-internal_metrics,sources-kubernetes_logs,tikv-jemallocator
```

Use separate source/target directories and the same container packaging. Verify the source difference is limited to the pod annotator and the resulting binary hashes differ.

**Run the workload**

1. Load both images into an isolated arm64 kind cluster. Use the configuration above and one collector at a time, limited to one CPU and 512 MiB.
2. Start two stdout generators, each producing approximately 75k EPS for 120 seconds. Each generator pod has 20 custom labels, one selector label, and 10 annotations. Each JSON log contains the pod name, a sequence number, and a 128-character payload.
3. After 30 seconds of logging warm-up, measure blackhole sent-event counters and collector cgroup CPU usage over approximately 89 seconds.
4. Run five pairs in baseline/bulk, bulk/baseline, baseline/bulk, bulk/baseline, baseline/bulk order. Let each run drain before starting the next.
5. Calculate each pair’s throughput and CPU/event changes. Report their medians separately from final event counts.

For CPU attribution, run a separate baseline/bulk/bulk/baseline sequence with the same workload:

```sh
perf record -e cpu-clock:u -F 99 \
  --call-graph dwarf,8192 \
  -p <collector-pid> -o perf.data -- sleep 60
```

Normalize sampled costs by events processed during the profile window. Sampling measures CPU cost—not exact comparison counts. The original profiles had incomplete unwinding and possible compilation contention.

</details>

<details>
<summary>Per-pair throughput results</summary>

Each row is one matched pair. CPU/event changes use cgroup CPU consumption divided by processed events.

| Pair | Order | Baseline EPS | Bulk EPS | Throughput change | CPU/event change |
|---|---|---:|---:|---:|---:|
| 1 | Baseline → bulk | 76,136 | 100,147 | +31.54% | −24.04% |
| 2 | Bulk → baseline | 75,142 | 100,186 | +33.33% | −24.97% |
| 3 | Baseline → bulk | 76,438 | 104,941 | +37.29% | −27.00% |
| 4 | Bulk → baseline | 77,585 | 104,943 | +35.26% | −25.89% |
| 5 | Baseline → bulk | 77,970 | 102,272 | +31.17% | −23.78% |
| Median paired change | | | | **+33.33%** | **−24.97%** |

Every bulk run finished with the expected **18,000,202 events**. Four baseline runs matched; pair 4’s baseline had one extra event. These totals do not establish per-event equivalence.

</details>

## Does this PR include user facing changes?
<!-- If this PR alters Vector behavior in any way, for example, it adds a new config field or changes internal metrics it is considered a user facing change.
Changes to CI, website, playground and similar are generally not considered user facing -->

- [ ] Yes. Please add a changelog fragment based on our [guidelines](https://github.com/vectordotdev/vector/blob/master/changelog.d/README.md).
  For a new deprecation announcement, follow the [deprecation policy](https://github.com/vectordotdev/vector/blob/master/docs/DEPRECATION_POLICY.md) and add a `deprecation.d/` notice instead.
- [x] No. A maintainer will apply the `no-changelog` label to this PR.

Internal optimization; intended event behavior is unchanged.

## Contributor Guidelines

- Please read our [Vector contributor resources](https://github.com/vectordotdev/vector/tree/master/docs#getting-started).
- Do not hesitate to use `@vectordotdev/vector` to reach out to us regarding this PR.
- Before pushing, follow our [pre-push guidance](https://github.com/vectordotdev/vector/blob/master/CONTRIBUTING.md#pre-push).
- After a review is requested, please avoid force pushes to help us review incrementally.
  - Feel free to push as many commits as you want. They will be squashed into one before merging.
  - For example, you can run `git merge origin master` and `git push`.
