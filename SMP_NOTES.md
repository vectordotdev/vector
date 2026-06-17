# Local Regression Benchmarking Notes

## What we're testing

We're measuring the performance impact of several VRL (Vector Remap Language) optimizations on Vector's throughput and memory usage. The optimizations are:

1. **Flat object map** — Replace BTreeMap with a flat array-backed map for storing event fields. Better cache locality, avoids tree traversal.
2. **Copy elimination** — Remove unnecessary `String` allocations when constructing map keys in VRL.
3. **KeyString as EcoString** — Change the map key type to `EcoString`, a reference-counted immutable string with cheap cloning.
4. **KeyString as CompactString** — Change the map key type to `CompactString`, an inline small-string-optimized type that avoids heap allocation for short strings.

The object map implementation is selected at runtime via the `VRL_OBJECT_MAP` env var (`btree`, `vec`, or the default `flat`). The KeyString and copy elimination changes are compile-time (different VRL builds).

## Tooling

### `bench.sh` — Build and run benchmarks

```bash
./bench.sh build <tag>                              # Build image from current source + ../vrl
./bench.sh derive <base-tag> <new-tag> KEY=VAL ...  # Layer env vars on an existing image (instant)
./bench.sh run <case> <baseline> <comparison> ...   # Run smp local comparison
./bench.sh cases                                    # List regression cases
```

The script auto-detects Colima and sets `DOCKER_HOST` and `TMPDIR` accordingly.

### `Dockerfile.bench` — Local build Dockerfile

Two-stage build using `rust:1.92-bookworm` (for native ARM64 on Apple Silicon) with BuildKit named context `--build-context vrl=../vrl` to include the local VRL checkout. Uses cache mounts for fast incremental rebuilds.

**Important**: The VRL repo at `../vrl` needs a `.dockerignore` excluding `target/` — without it, Docker sends ~25GB of build artifacts as context.

### `bench-all.sh` — Batch runner

Runs all regression cases across multiple variants. Skips completed runs (checks for `Δ mean` in output logs) and skips `file_to_blackhole`/`file_100_to_blackhole` (need FUSE, unavailable in Colima). Tolerates individual failures without aborting.

### `smp local run`

The actual comparison tool. Runs 3 replicates of each variant with 270 samples at 1Hz. Each case takes ~15 min (except `splunk_hec_route_s3` at ~32 min). Raw capture data (parquet) goes to `comparative-captures/<case>/`.

## Docker image inventory

All images use the same Vector source; they differ only in VRL version and the `VRL_OBJECT_MAP` env var.

| Image | VRL version | Map type | Description |
|---|---|---|---|
| `vector:btree` | baseline (no opts) | btree | **The baseline for all comparisons** |
| `vector:flat` | baseline | flat | Flat map only |
| `vector:btree-ks` | + EcoString | btree | Old keystring on btree |
| `vector:flat-ks` | + EcoString | flat | Old keystring on flat |
| `vector:btree-co` | + copy elim | btree | Copy elimination only |
| `vector:flat-co` | + copy elim | flat | Copy elimination only |
| `vector:btree-co-ks` | + copy elim + EcoString | btree | Copy elim + EcoString |
| `vector:flat-co-ks` | + copy elim + EcoString | flat | Copy elim + EcoString |
| `vector:btree-co-ksc` | + copy elim + CompactString | btree | Copy elim + CompactString |
| `vector:flat-co-ksc` | + copy elim + CompactString | flat | Copy elim + CompactString |

To rebuild: the VRL repo (`../vrl`) has copy elimination committed on `main`. KeyString variants are managed via `git stash`:
- **EcoString**: `git stash pop` the older stash
- **CompactString**: dirty working tree state (or stash pop the newer stash)
- **No keystring**: `git stash` everything

## Results

### Complete data: flat map across all cases

Throughput vs btree baseline, from `bench-results/full/`:

| Tier | Cases | Flat map Δ |
|---|---|---|
| High (heavy VRL) | datadog_agent_remap_blackhole | +35.5% |
| | datadog_agent_remap_blackhole_acks | +30.1% |
| | syslog_humio_logs | +25.8% |
| | syslog_log2metric_splunk_hec_metrics | +19.8% |
| | syslog_splunk_hec_logs | +18.4% |
| | syslog_loki | +18.3% |
| | datadog_agent_remap_datadog_logs_acks | +18.5% |
| | syslog_log2metric_humio_metrics | +17.1% |
| | syslog_log2metric_tag_cardinality_limit_blackhole | +16.9% |
| | datadog_agent_remap_datadog_logs | +16.2% |
| | syslog_regex_logs2metric_ddmetrics | +15.0% |
| Medium | http_text_to_http_json | +13.8% |
| | statsd_to_datadog_metrics | +1.6% |
| Low (passthrough) | all http_to_http, socket, splunk_hec, fluent, otlp | ~0% (no regression) |

Memory (RSS) is 3–8% lower with flat map on VRL-heavy workloads.

### Optimization interaction study (datadog_agent_remap_blackhole)

All measurements vs old btree baseline:

| Configuration | Throughput Δ |
|---|---|
| flat map only | +29.2% |
| btree + copy elim | -0.8% |
| btree + copy elim + EcoString | +7.4% |
| btree + copy elim + CompactString | +2.7% |
| flat + copy elim + EcoString | +22.8% |
| flat + copy elim + CompactString | +29.2% |

Key findings:
- **EcoString helps btree (+7.4%) but hurts flat (-9.6% vs flat alone)**. The reference-counting indirection undermines flat map cache locality.
- **CompactString is neutral on flat**. Inline small-string storage preserves cache behavior. Full +29.2% recovered.
- **Copy elimination is near-zero on its own** but is a prerequisite for the KeyString changes.

### Partially complete: full matrix

`bench-results/full/` has results for `flat` (all 25 working cases) and `flat-co-ks` (15 of 25 cases). The remaining variants (`flat-co-ksc`, `btree-co-ks`, `btree-co-ksc`) have not been run across all cases yet.

## Resuming the batch run

```bash
# The script skips completed runs automatically
caffeinate -d -i -s bash -c './bench-all.sh'
```

Colima must be running with 10 CPUs and sufficient disk:

```bash
colima start --cpu 10 --memory 16 --disk 60
```

## Infrastructure notes

- **Colima on Apple Silicon**: The `timberio/vector-dev` image is amd64-only. We use `rust:1.92-bookworm` instead (has native arm64).
- **Docker buildx**: Required for `--build-context`. Install: `brew install docker-buildx`, then `mkdir -p ~/.docker/cli-plugins && ln -sfn $(brew --prefix docker-buildx)/bin/docker-buildx ~/.docker/cli-plugins/docker-buildx`.
- **smp + Colima**: smp needs `DOCKER_HOST` set to the Colima socket, and `TMPDIR` under `$HOME` (Colima doesn't mount `/var/folders`). The `bench.sh` script handles this automatically.
- **FUSE cases**: `file_to_blackhole` and `file_100_to_blackhole` need FUSE, unavailable in Colima VMs. Skipped.
- **Disk space**: The Colima VM disk fills up from Docker build cache. Run `docker builder prune --all -f` before large rebuilds.
- **VRL .dockerignore**: Must contain `target` to avoid sending 25GB of build artifacts as Docker context.

## File locations

- `bench.sh` — Main benchmarking script
- `Dockerfile.bench` — Build Dockerfile
- `bench-all.sh` — Batch runner for full matrix
- `bench-results/` — Ad-hoc comparison logs from early exploration
- `bench-results/full/` — Systematic matrix results (`<case>--<variant>.log`)
- `comparative-captures/` — Raw smp parquet data (overwritten by each run; only latest is available)
- `../vrl/.dockerignore` — Must exist with `target` entry
