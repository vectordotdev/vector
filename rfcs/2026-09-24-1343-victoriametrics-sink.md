# RFC 1343 - 2026-09-24 - VictoriaMetrics sink

This RFC adds a `victoriametrics` sink that delivers metrics using the VictoriaMetrics remote write
protocol (Prometheus `WriteRequest` protobuf, zstd-compressed), with native VictoriaMetrics
multi-tenancy, `vmrange` histograms for distributions, and bounded memory for incremental
distributions. The sink targets single-node VictoriaMetrics, cluster `vminsert`, and `vmauth`.

## Context

- Feature request: [#1343](https://github.com/vectordotdev/vector/issues/1343).
- Existing users send to VictoriaMetrics through `prometheus_remote_write` and report problems:
  - [#13890](https://github.com/vectordotdev/vector/issues/13890): healthcheck fails against
    VictoriaMetrics (`400 Bad Request`), worked around with `healthcheck.uri`.
  - [#19041](https://github.com/vectordotdev/vector/issues/19041): CPU climbs to 100% with
    histograms.
  - [#23990](https://github.com/vectordotdev/vector/issues/23990): metrics lost during endpoint
    downtime (root cause not yet known, out of scope here).
- [#17334](https://github.com/vectordotdev/vector/pull/17334) added zstd to
  `prometheus_remote_write`.
- Prior art:
  - `vmagent` remote write client (`app/vmagent/remotewrite/client.go`): zstd with
    `X-VictoriaMetrics-Remote-Write-Version: 1`, 2xx is success, 400 and 409 are dropped,
    everything else is retried with backoff.
  - `VictoriaMetrics/metrics` `histogram.go`: the `vmrange` histogram layout.
  - Vector `datadog_*` and `splunk_hec` sinks: per-event credentials carried in event secrets
    (`datadog_api_key`, `splunk_hec_token`) that override a configured default.

## Scope

### In scope

- A new `victoriametrics` sink for metric events.
- VictoriaMetrics remote write protocol only (zstd).
- Tenancy for single-node, `vminsert` (path and label based), and `vmauth` (credential based).
- `vmrange` encoding of distributions, with sink-owned bounded state.
- A protobuf encoder that avoids the intermediate structures used by `prometheus_remote_write`.

### Out of scope

- Changes to `prometheus_remote_write`.
- The Prometheus remote write protocol (snappy) and fallback to it. VictoriaMetrics versions
  without zstd remote write support are not supported.
- VictoriaMetrics native import (`/api/v1/import/native`). Its format "may change in incompatible
  way between VictoriaMetrics releases".
- Text import formats (`/api/v1/import`, Influx line protocol, CSV).
- Log ingestion into VictoriaLogs.
- Investigating [#23990](https://github.com/vectordotdev/vector/issues/23990).

## Motivation

- `prometheus_remote_write` works with VictoriaMetrics but is not a good fit:
  - `tenant_id` sets `X-Scope-OrgID`, which VictoriaMetrics ignores. VictoriaMetrics tenants live
    in the URL path (`/insert/<accountID>:<projectID>/...`), in `vm_account_id`/`vm_project_id`
    labels, or behind `vmauth` credentials. Per-event tenant routing requires one sink per tenant.
  - Distributions are bucketed into a fixed `buckets` list (11 buckets by default). Precision is
    poor and does not line up with `vmrange` histograms emitted by applications instrumented with
    VictoriaMetrics client libraries.
  - `MetricValue::add` merges incremental distributions with `samples.extend_from_slice`, so
    `make_absolute` keeps every sample for the lifetime of the series and re-buckets all of them on
    every flush. Memory and CPU grow without bound. This matches the reports in #19041.
  - The healthcheck sends `GET` to the write path and requires `200`. VictoriaMetrics returns `400`
    for that request and `204` for successful writes.
  - Compression defaults to snappy. zstd has to be opted into and is not advertised with the
    VictoriaMetrics version header.
- VictoriaMetrics reports 2x-5x lower network bandwidth for its zstd protocol compared to snappy.
- Without this sink, VictoriaMetrics users keep hand-tuning a sink designed for Prometheus and
  Cortex, and multi-tenant setups need one sink per tenant.

## Proposal

### User Experience

A single-node VictoriaMetrics:

```yaml
sinks:
  vm:
    type: victoriametrics
    inputs: [statsd]
    endpoint: http://victoriametrics:8428
```

A cluster, one tenant per event, routed by URL path:

```yaml
sinks:
  vm:
    type: victoriametrics
    inputs: [app_metrics]
    endpoint: http://vminsert:8480
    tenant:
      mode: path
      id: "42:{{ tags.project_id }}"
```

`tenant.id` follows Vector's template confinement: a template with dynamic content needs a literal
prefix (here `42:`), unless `dangerously_allow_unconfined_template_resolution` is set. The rendered
value must also be a valid tenant (digits only), so it cannot change the rest of the URL path.

A cluster behind `vmauth`, where the credential decides the tenant:

```yaml
transforms:
  pick_token:
    type: remap
    inputs: [app_metrics]
    source: |
      set_secret("victoriametrics_token", get_env_var!("VM_TOKEN_" + upcase!(.tags.team)))

sinks:
  vm:
    type: victoriametrics
    inputs: [pick_token]
    endpoint: http://vmauth:8427
    auth:
      strategy: bearer
      token: "${VM_TOKEN_DEFAULT}"
```

Configuration reference:

| Option | Default | Description |
| ------ | ------- | ----------- |
| `endpoint` | required | Base URL. The sink appends the write path. |
| `tenant.mode` | `none` | `none`, `path`, or `labels`. |
| `tenant.id` | none | Template rendering `accountID[:projectID]`. Required for `path` and `labels`. Subject to template confinement. |
| `auth` | none | Shared HTTP auth: `basic`, `bearer`, `custom`, and `aws` where enabled. |
| `default_namespace` | none | Prefix for metrics without a namespace, joined with `_`. |
| `send_metadata` | `true` | Send metric type metadata, which VictoriaMetrics stores when `-enableMetadata` is set (the default), as `vmagent` does. |
| `compression_level` | 3 | zstd level. Negative levels trade bandwidth for CPU, as in `vmagent`. |
| `expire_metrics_secs` | none | TTL for incremental state. |
| `batch`, `request`, `tls`, `buffer`, `acknowledgements` | standard | Standard sink options. `request.headers` adds static headers. |

Write path per tenant mode:

| `tenant.mode` | Target | Path | Batches split by |
| ------------- | ------ | ---- | ---------------- |
| `none` | single-node, or `vmauth` | `/api/v1/write` | credential |
| `path` | `vminsert` | `/insert/<id>/prometheus/api/v1/write` | tenant, credential |
| `labels` | `vminsert` | `/insert/multitenant/prometheus/api/v1/write` | credential |

In `labels` mode, the rendered `id` is split into `vm_account_id` and `vm_project_id` labels on each
series. Both labels are always written, with the project defaulting to `0`, as `vmagent` does.
`vminsert` removes them before storage. Metric metadata carries the tenant in the `AccountID` (11)
and `ProjectID` (12) fields of `MetricMetadata`, a VictoriaMetrics extension that `vminsert` reads
for the multitenant endpoint. Tenant IDs are normalized (`042:07` becomes `42:7`) in paths and
labels.

Per-event credentials follow the `datadog_*` and `splunk_hec` precedent. If an event carries the
secret `victoriametrics_token`, the sink sends `Authorization: Bearer <secret>` for that event and
ignores the configured `auth`. Otherwise the configured `auth` applies. `vmauth` maps each
credential to its `url_prefix`, so Vector does not need to know the tenant.

Distribution metrics become `vmrange` histograms:

```text
request_duration_seconds_bucket{vmrange="1.000e-01...1.136e-01"} 42
request_duration_seconds_bucket{vmrange="1.136e-01...1.292e-01"} 7
request_duration_seconds_sum 5.1
request_duration_seconds_count 49
```

They work with `histogram_quantile()` and `prometheus_buckets()` in MetricsQL, and aggregate across
series and with application-emitted `vmrange` histograms.

This is a new component. It does not change existing behavior.

### Implementation

#### Module layout

```text
src/sinks/victoriametrics/
  mod.rs
  config.rs           # VictoriaMetricsConfig, TenantConfig, validation, build
  sink.rs             # stream pipeline: normalize -> partition -> batch -> request
  normalize.rs        # sink-owned incremental state, including HistogramSet
  histogram.rs        # vmrange bucket math and label formatting
  encoder.rs          # direct protobuf writer for WriteRequest
  service.rs          # HTTP request construction
  retry.rs            # RetryLogic matching vmagent
  tests.rs
  integration_tests.rs
tests/integration/victoriametrics/   # compose.yaml, test.yaml
website/cue/reference/components/sinks/victoriametrics.cue
```

Cargo feature: `sinks-victoriametrics`. It reuses `HttpClient`, TLS, `crate::http::Auth`, the
partitioned batcher, and `ConfinedTemplate` for `tenant.id`.

#### Requests

Every write is a `POST` with these headers:

```text
Content-Type: application/x-protobuf
Content-Encoding: zstd
X-VictoriaMetrics-Remote-Write-Version: 1
```

#### Partitioning

```rust
#[derive(Clone, Eq, Hash, PartialEq)]
struct PartitionKey {
    tenant: Option<String>,        // rendered tenant.id in `path` mode
    token: Option<Arc<str>>,       // `victoriametrics_token` event secret
}
```

The number of partitions is bounded by the number of distinct tenants and tokens, which the operator
controls through templates and VRL.

#### Retries

| Response | Action |
| -------- | ------ |
| 2xx (VictoriaMetrics returns 204) | success |
| 400, 409, 415 | drop, emit error event |
| any other status, timeout, connection error | retry with backoff |

This matches `sendBlockHTTP` in `vmagent`, which drops blocks only on 400, 409, and 415 and retries
everything else. In particular, 401 and 403 are retried because `vmauth` returns them during token
rotation or misconfiguration, and dropping would lose all data silently. `retry_strategy` overrides
these defaults. Vector's retry framework uses exponential backoff and does not read `Retry-After`,
unlike `vmagent`.

#### Healthcheck

`POST` an empty zstd-compressed `WriteRequest` to the write path with the configured `auth`,
expecting 2xx. This validates reachability, auth, and `vmauth` routing, and behaves the same for
single-node, `vminsert`, and `vmauth`. In `path` mode, the healthcheck targets the rendered path only
when `tenant.id` is static, and is skipped otherwise. Per-event tokens are not checked because they
are unknown at startup. `healthcheck.uri` overrides the target.

#### Normalization

Counters, gauges, and sets keep using `MetricSet::make_absolute`, as in `prometheus_remote_write`.
Distributions use a separate state map owned by the sink:

```rust
struct VmHistogram {
    // Sparse: slot 0 is the lower overflow bucket (below 1e-9, including 0), slots 1..=486 are
    // the regular buckets, slot 487 is the upper overflow bucket (1e18 and above).
    buckets: BTreeMap<u16, u64>,
    sum: f64,
    count: u64,
}

struct HistogramSet {
    series: HashMap<MetricSeries, HistogramEntry>, // entry: histogram and last-seen time
    ttl: Option<Duration>,                         // from expire_metrics_secs
}
```

- An incremental distribution is bucketed on arrival and added to the stored `VmHistogram`, so
  memory per series is bounded (at most 488 counters) and flush cost does not depend on history.
- An absolute distribution is bucketed directly and is not stored.
- Each sample counts `rate` times, which keeps statsd sample rates.
- `AggregatedHistogram` and `AggregatedSummary` pass through with their existing `le` and
  `quantile` labels.
- Distributions become `vmrange` histograms regardless of their statistic, so summary-style
  distributions (for example statsd timers) become aggregatable too.
- `Sketch` is encoded as a summary with the 0.5, 0.75, 0.9, 0.95, and 0.99 quantiles plus `_sum`,
  `_count`, `_min`, and `_max`, matching how VictoriaMetrics stores Datadog sketches
  (`lib/protoparser/datadogsketches`). A `vmrange` conversion may follow in a later phase.

#### `vmrange` bucket math

Ported from `VictoriaMetrics/metrics` `histogram.go`:

```rust
const E10_MIN: i32 = -9;
const E10_MAX: i32 = 18;
const BUCKETS_PER_DECIMAL: usize = 18;
const BUCKETS_COUNT: usize = (E10_MAX - E10_MIN) as usize * BUCKETS_PER_DECIMAL; // 486

fn bucket_index(v: f64) -> Bucket {
    // NaN and negative values are ignored.
    // idx = (log10(v) - E10_MIN) * BUCKETS_PER_DECIMAL
    // idx < 0 -> Lower; idx >= BUCKETS_COUNT -> Upper.
    // Exact powers of 10 go to the lower bucket, as in histogram.go.
}
```

Labels use Go's `%.3e` layout, which pads the exponent to two digits with an explicit sign
(`1.000e-09`). Rust's `{:.3e}` prints `1.000e-9`, so the sink uses its own formatter. The overflow
buckets are `0...1.000e-09` and `1.000e+18...+Inf`. Unit tests compare against golden labels
generated by the Go library, so that series from Vector and from VictoriaMetrics-instrumented
applications merge.

Go computes `math.Log10(v)` as `math.Log(v) * (1/Ln10)`, and the Go compiler fuses the following
subtraction into a single FMA on arm64 but not on amd64. The two architectures therefore disagree for
some exact powers of ten (for example `1e-6`). The sink follows amd64, where every power of ten falls
in the lower bucket as the Go source intends, and pins the Go values of `10^(1/18)` and `1/Ln10` so
that the bucket bounds are bit-identical.

#### Encoder

`prometheus_remote_write` clones tags into a `Vec<proto::Label>`, sorts it, hashes it into an
`IndexMap<Labels, Vec<Sample>>`, then builds and encodes prost structs. This sink writes
`WriteRequest` bytes directly into a reused `BytesMut`:

- One `TimeSeries` per sample, with no grouping map.
- Labels are written from borrowed `&str` and sorted by name: `MetricTags` iterates in key order,
  and the labels added by the sink (`__name__`, `le`, `quantile`, `vmrange`, `vm_account_id`,
  `vm_project_id`) are merged into that order. A label added by the sink replaces a tag with the
  same name.
- The request is encoded into a reused per-thread buffer and compressed with a single zstd call using
  a reused per-thread compression context, as `vmagent` does. The frame therefore records its content
  size, which lets VictoriaMetrics decompress into a buffer of the right size instead of falling back
  to streaming decompression.
- Metadata entries are written only when `send_metadata` is `true`.

A Criterion benchmark in `benches/` will compare this encoder with the `prometheus_remote_write`
encoder on counter-heavy and distribution-heavy batches.

#### Batching defaults

`max_events: 10000`, `max_bytes: 8388608`, and `timeout_secs: 1`, following the `vmagent` defaults
`-remoteWrite.maxRowsPerBlock`, `-remoteWrite.maxBlockSize`, and `-remoteWrite.flushInterval`.

`max_bytes` counts the estimated encoded size of each metric, not its in-memory size. A single
distribution can expand to 490 series that each repeat every tag, so in-memory size would let a
batch exceed the VictoriaMetrics `-maxInsertRequestSize` limit (32 MiB by default), which rejects the
whole request with 400. The estimate is an upper bound of the encoded size.

## Rationale

- Remote write with zstd is the protocol `vmagent` uses to talk to `vminsert`. It is the most
  compatible and the most efficient stable option.
- Supporting only zstd removes the protocol negotiation state machine and its ambiguous
  400-means-downgrade case.
- `vmrange` histograms give better precision without bucket configuration and interoperate with the
  VictoriaMetrics ecosystem.
- Keeping distribution state in the sink fixes unbounded growth without changing Vector's data model.
- Event secrets for per-event credentials reuse an existing Vector pattern. Credentials stay out of
  tags and templates, and VRL can map any event attribute to a token.
- Reusing `crate::http::Auth` matches the `http`, `loki`, `clickhouse`, `databend`, `doris`, and
  `prometheus_exporter` sinks.

## Alternatives

- **Document `prometheus_remote_write` for VictoriaMetrics.** Cheap, but leaves tenancy,
  distribution precision, and unbounded distribution state unresolved.
- **Extend `prometheus_remote_write` with VictoriaMetrics options.** Mixes two tenancy models and two
  histogram layouts in one sink, and the retry and healthcheck behaviors conflict.
- **Native import format.** Unstable across VictoriaMetrics releases.
- **Snappy fallback on 400/415 like `vmagent`.** Adds state and cannot tell "unsupported encoding"
  from "bad data". Rejected because older VictoriaMetrics versions are not a target.
- **A `vmrange` variant of `MetricValue`.** Cleaner for the data model but touches every component
  that matches on `MetricValue`. Rejected in favor of sink-owned state.
- **A token lookup table in the sink** (`tenant.tokens: {key: token}` selected by a template).
  Duplicates what VRL and event secrets already do.
- **`AccountID`/`ProjectID` headers.** Enabled by default only from `vminsert` v1.150.
- **Drop on 401/403 (Vector's default).** Loses data during token rotation.

Drawbacks:

- A new sink to maintain, overlapping with `prometheus_remote_write`.
- In `vmauth` setups, one failing credential applies backpressure to the whole sink, because all
  partitions share one service and concurrency limit.
- `vmrange` series are not readable by plain Prometheus. The sink is for VictoriaMetrics only.

## Outstanding Questions

- Confirm that VictoriaMetrics answers 2xx to an empty zstd `WriteRequest` on single-node,
  `vminsert`, and through `vmauth`. The source (`lib/protoparser/promremotewrite/stream`) decodes an
  empty frame into an empty request and returns 204; the integration tests run the healthcheck
  against all three to confirm it.
- Decide whether custom auth header names (`vmauth -httpAuthHeader`) need a dedicated secret-safe
  option, or whether `request.headers` is enough.

## Plan Of Attack

- [x] Sink skeleton: config, tenancy modes, event secret credentials, zstd transport, retry logic,
  healthcheck, counters, gauges, sets, aggregated histograms, and summaries. Integration tests with
  single-node, `vminsert`, and `vmauth`.
- [x] `vmrange` histograms and bounded distribution state, with golden tests against
  `VictoriaMetrics/metrics`.
- [x] Direct protobuf encoder.
- [ ] Criterion benchmarks against the `prometheus_remote_write` encoder.
- [ ] `Sketch` to `vmrange` conversion.

## Future Improvements

- Per-partition concurrency so one failing `vmauth` credential does not block other tenants.
- A `victorialogs` sink for logs.
