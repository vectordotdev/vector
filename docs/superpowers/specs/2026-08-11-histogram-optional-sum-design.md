# Optional histogram sum

## Problem

`MetricValue::AggregatedHistogram` stores `sum: f64`, which cannot express "this histogram
reported no sum". Sources that receive a sum-less histogram are forced to substitute `0.0`, and
every downstream consumer then treats that fabricated zero as a real measurement.

This became visible while fixing the `AggregatedHistogram` to DDSketch conversion (PR #25752). That
fix takes `sum`/`avg` from the histogram's own exact totals instead of reconstructing them from
bucket-boundary interpolation. For a histogram whose sum was fabricated as `0.0`, the "exact"
totals are a lie, and the fix propagates that lie in place of an estimate that would at least have
been roughly right.

Two upstream formats genuinely omit the histogram sum:

- **OTLP.** `HistogramDataPoint.sum` is `optional double`. The comment in `metrics.proto` states
  that sum "should not be filled out" when negative events are recorded, specifically to stay
  compatible with OpenMetrics. Vector currently collapses this to `0.0` at
  `lib/opentelemetry-proto/src/metrics.rs:335` (histogram) and `:391` (exponential histogram).
- **Prometheus / OpenMetrics.** The OpenMetrics specification makes the `_sum` series
  RECOMMENDED, not required — "A Histogram MetricPoint MUST contain at least one bucket, and
  SHOULD contain Sum, and Created values" — and outright forbids it for histograms with negative
  bucket thresholds: "Negative threshold buckets MAY be used, but then the Histogram MetricPoint
  MUST NOT contain a sum value as it would no longer be a counter semantically." The OpenTelemetry
  Collector's `prometheusremotewrite` translator and the Prometheus server's own OTLP write
  receiver both gate the series on `if pt.HasSum()`, so a Prometheus 3.x server stores sum-less
  histograms and anything scraping or federating that data back out observes them.

## Solution

Make the histogram sum an `Option<f64>` end to end. Absence propagates rather than being
flattened, and each consumer either omits its sum output or falls back to estimation, but never
fabricates a zero.

### Scope decision: histograms only

`AggregatedSummary.sum` stays `f64`. This is not an oversight — the formats treat summaries
differently:

- OTLP's `SummaryDataPoint.sum` is a plain `double`, not optional.
- The OpenTelemetry Prometheus compatibility specification is explicit about the asymmetry: for
  histograms, "If `_sum` is not present, the histogram's sum MUST be unset"; for summaries, "If
  `_sum` is not present, the summary's sum MUST be set to zero."

So for summaries, defaulting a missing `_sum` to zero is the specified behavior. Likewise
`lib/prometheus-parser`'s `SummaryMetric.sum` stays `f64`.

## 1. Core type

`lib/vector-core/src/event/metric/value.rs:51`:

```rust
AggregatedHistogram {
    /// The buckets within this histogram.
    buckets: Vec<Bucket>,

    /// The total number of observations contained within this histogram.
    count: u64,

    /// The sum of all observations contained within this histogram.
    ///
    /// `None` when the source did not report a sum. OTLP makes the histogram sum optional, and
    /// OpenMetrics only recommends the `_sum` series (and forbids it for histograms with negative
    /// bucket thresholds), so a sum-less histogram is a legitimate input rather than an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sum: Option<f64>,
},
```

`skip_serializing_if` matches the convention already used in the sibling metric modules
(`series.rs:16`, `series.rs:98`, `data.rs:32`, `data.rs:39`). It also keeps the change invisible to
the checked-in native-encoding fixtures: no fixture has a `None` sum, so all 1024 JSON files
continue to serialize byte-identically.

## 2. Mutator semantics

`MetricValue` has four sum-touching operations. Each is specified below together with who actually
calls it, because the right `None` behavior depends on that.

### `zero()` — `value.rs:170`

Sets `*sum = sum.map(|_| 0.0)`, preserving absence.

**Callers: none in production.** The only references are the delegation chain
`Metric::zero()` (`mod.rs:365`) to `MetricData::zero()` (`data.rs:153`) to `MetricValue::zero()`,
plus unit tests. Nothing under `src/` calls it. It remains part of `vector-core`'s public surface,
so it still needs defined behavior, but no shipping code path observes the choice.

Preserving absence is chosen for internal consistency: zeroing the bucket counts is a statement
about observations, not a discovery of a sum that was never reported. A histogram that never had a
sum should not acquire one by being zeroed.

### `add()` — `value.rs:211`

`*sum = (*sum).zip(*sum2).map(|(a, b)| a + b)` — the result has a sum only if both operands do.

**Callers: live.** `MetricSet::incremental_to_absolute` (`src/sinks/util/buffer/metrics/normalize.rs:683`)
accumulates incremental metrics into a running absolute value, and the `aggregate` transform
merges metrics through `MetricData::update`.

Absence poisons the result because the sum of a known and an unknown quantity is unknown. Treating
the missing side as zero would silently under-report the total.

### `subtract()` — `value.rs:277`

`*sum = (*sum).zip(*sum2).map(|(a, b)| a - b)`, same rule.

**Callers: live, and directly on the path this change exists to fix.**
`MetricSet::absolute_to_incremental` converts absolute histograms to deltas, which is how the
`datadog_metrics` normalizer (`src/sinks/datadog/metrics/normalizer.rs:26`) prepares a metric
before `AgentDDSketch::transform_to_sketch` consumes it. An absolute OTLP histogram with no sum
passes through here first, so if `subtract` invented a number the DDSketch fix would never see the
`None` it needs to act on.

### `PartialEq` — `value.rs:409`

Compares the two `Option`s: `None` equals `None`, and `None` never equals `Some`. Requires a new
`pub(crate) fn opt_float_eq(l: Option<f64>, r: Option<f64>) -> bool` beside `float_eq` in
`lib/vector-core/src/lib.rs:69`, since `float_eq` takes bare `f64`s. It delegates to `float_eq` for
the `Some`/`Some` case so the existing NaN and 1-ULP tolerance behavior is unchanged.

### `Display` — `value.rs:457`

Emits `count={count} sum={sum}` today, with a trailing space. When `sum` is `None`, the `sum=` token
is dropped entirely, leaving `count={count}` and its trailing space. This is the only cosmetic output
change in the design. Rendering
`sum=None` was rejected as user-facing debug noise; omission reads as "not reported", which is what
it means.

## 3. Defaults

Two distinct notions of "default" arise, and they resolve differently.

**Serde default: must be `None`.** Combined with `skip_serializing_if = "Option::is_none"`, any
other default breaks round-tripping — `None` would serialize to an absent key and deserialize back
as `Some(0.0)`. The native-codec fixture tests
(`roundtrip_current_native_json_fixtures`, `roundtrip_current_native_proto_fixtures` in
`lib/codecs/tests/native.rs`) assert byte-exact round-trips, so this is a correctness requirement,
not a preference.

A visible consequence: `sum` becomes optional in user-authored config and unit-test fixtures, since
`MetricValue` is exposed as configuration through `src/sources/static_metrics.rs:69`. That is
intended — it lets a user express a sum-less histogram. The existing documented example at
`website/content/en/docs/reference/configuration/unit-tests.md:398` passes `sum: 0` explicitly and
stays valid.

**`zero()`'s notion of zero: preserves `None`.** See section 2. Unlike the serde default, nothing
forces this choice, and nothing in production observes it.

There is no `impl Default for MetricValue`, so no third case exists.

`lib/prometheus-parser`'s `HistogramMetric` does derive `Default`, and that derive is the mechanism
that produces the right answer for free: `sum` is only assigned when a `_sum` line is parsed
(`lib/prometheus-parser/src/lib.rs:194`), so once the field is `Option<f64>` its default `None`
already means "no `_sum` series was seen".

## 4. Producers

Sites that construct an `AggregatedHistogram` and can now yield `None`:

| Site | Change |
| --- | --- |
| `lib/opentelemetry-proto/src/metrics.rs:335` | Drop `.unwrap_or(0.0)`; pass `self.point.sum` through. |
| `lib/opentelemetry-proto/src/metrics.rs:391` | Same, for the exponential-histogram conversion. |
| `lib/prometheus-parser/src/lib.rs:102` | `HistogramMetric.sum` becomes `Option<f64>`; the `_sum` arm at `:194` assigns `Some(sum)`. |
| `src/sources/prometheus/parser.rs:154` | Passes the parser's `Option` straight through. |
| `src/sources/prometheus/parser.rs:117` | The `skip_nan_values` filter becomes `metric.sum.is_some_and(f64::is_nan)`. |
| `src/transforms/log_to_metric.rs:692` | A missing `aggregated_histogram.sum` path yields `None` instead of `TransformError::PathNotFound`. A present-but-non-float value still errors. |

Sites that always produce `Some`, because they compute the sum themselves:

- `MetricValue::distribution_to_agg_histogram` (`value.rs:131`), via `samples_to_buckets`.
- `Histogram::make_metric` in the internal metrics registry (`lib/vector-core/src/metrics/storage.rs:119`),
  which accumulates an `AtomicF64`.
- `src/sinks/prometheus/collector.rs:64`, the distribution-to-histogram path.

## 5. Consumers

### The fix

`AgentDDSketch::transform_to_sketch` (`lib/vector-core/src/metrics/ddsketch.rs:802`) already has a
guard for "the exact sum is not usable", covering count mismatch and non-finite sums. `None` joins
it as one more disqualifier, folded in with a let-chain (edition 2024, Rust 1.95):

```rust
if let Some(true_sum) = *sum
    && true_count > 0
    && true_count == u64::from(sketch.count())
    && true_sum.is_finite()
```

When the guard fails, the sketch keeps the `sum`/`avg` that `insert_interpolate_buckets` derived
from the buckets — self-consistent with `sketch.count()`, which is the property the existing guard
exists to protect.

### Omit rather than fabricate

| Site | Change |
| --- | --- |
| `src/sinks/prometheus/collector.rs:163` | Skip the `<name>_sum` series. Spec-compliant: `_sum` is SHOULD-only. |
| `src/sinks/influxdb/metrics.rs:335` | Omit the `sum` field from the line protocol. |
| `src/sinks/greptimedb/metrics/request_builder.rs:121` | Omit the `sum` column. The sketch path at `:198` already does exactly this. |
| `lib/vector-core/src/event/lua/metric.rs:189` | `raw_set` with `None` leaves the key unset, so scripts see `nil`. |
| `src/transforms/metric_to_log.rs:192` | `with_known("sum", Kind::float())` becomes `with_known("sum", Kind::float().or_undefined())`, matching the `.or_undefined()` idiom already used on the enclosing object at `:194`. The emitted JSON omits the key via serde, so the declared schema would otherwise lie. The `aggregated_summary` equivalent at `:212` is left alone. |

The Lua change can break a script that assumes `metric.aggregated_histogram.sum` is always a
number ("attempt to perform arithmetic on a nil value"), but only for histograms that genuinely
report no sum. The decode direction (`metric.rs:286`) gets strictly more permissive: a missing key
was a hard `FromLuaConversionError` and becomes `None`.

### Unaffected

Confirmed to need no change: all VRL code (`From<MetricValue> for vrl::value::Value` only exposes
`as_name()`), `src/sinks/prometheus/exporter.rs` (keys on bucket bounds only),
`src/sinks/datadog/metrics/sink.rs`, `src/sinks/util/buffer/metrics/split.rs` (splits summaries,
not histograms), `src/transforms/aggregate.rs`, and `src/api` (metrics cross it as opaque
`EventWrapper`). `src/sinks/greptimedb/metrics/batch.rs:38` over-counts a size estimate by 8 bytes
when the sum is absent, which is harmless.

## 6. Wire formats

### Protobuf

`lib/vector-core/proto/event.proto` cannot express absence: `AggregatedHistogram3.sum` is a
proto3 `double` with implicit presence. Following the convention that a semantic change mints a new
numbered message rather than editing an existing one (v1 to v2 unzipped parallel arrays into a
sub-message, v2 to v3 widened `count` to `uint64`, and old arms stay decodable forever), add:

```protobuf
message AggregatedHistogram4 {
  repeated HistogramBucket3 buckets = 1;
  uint64 count = 2;
  optional double sum = 3;
}
```

at the next free `Metric.value` oneof tag, `22`. Buckets are unchanged, so `HistogramBucket3` is
reused.

**Encode selects the version by content** (`lib/vector-core/src/event/proto.rs:377`):
`AggregatedHistogram3` when the sum is `Some`, `AggregatedHistogram4` only when it is `None`. This
needs a comment explaining why, since it is otherwise surprising, but it buys two things:

- No fixture churn. Always emitting v4 would change the bytes of every histogram, requiring the
  current 1024 proto and 1024 JSON fixtures to be archived into a `pre-v57/` directory, all 1024
  regenerated, and new `reserialize_pre_v57_*` tests added. Selecting by content leaves every
  existing fixture byte-identical, because none has a `None` sum.
- An older Vector peer or a disk buffer written by an older version keeps decoding every histogram
  it could already represent. Only the genuinely sum-less case is unreadable to it, and it had no
  way to represent that anyway.

**Decode** accepts all four arms (`proto.rs:165` onward). v1, v2, and v3 map to `Some(sum)`, which
is the correct reading of a format where absence was inexpressible. v4 maps its `Option` straight
through.

The alternative of adding `optional` to `AggregatedHistogram3.sum` in place was rejected: it is
wire-identical, but because proto3 implicit presence never writes a zero, new code decoding
old-encoded data would silently read a genuine `sum == 0.0` as `None`.

Note `lib/vector-core/build.rs:4` already passes `--experimental_allow_proto3_optional`, and
`TagValue.value` (`event.proto:134`) is already an `optional string` compiled by both `build.rs`
files, so proto3 optional is proven to work in both codegen paths.

### Native JSON

Driven entirely by the derived serde impl; `native_json.rs` has no field-specific logic. The
`skip_serializing_if` and `default` in section 1 cover it. The fixture schema at
`lib/codecs/tests/data/native_encoding/schema.cue:21` becomes `sum?: number`.

Note that `sum` has no `serialize_with`/`deserialize_with` for non-finite values, unlike
`Bucket.upper_limit` (`value.rs:663`). A `NaN` or infinite sum already breaks native-JSON
serialization today; this design does not change that and does not fix it.

## 7. Config schema and generated docs

`MetricValue` is user-facing configuration via `src/sources/static_metrics.rs:69`, so
`make check-generated-docs` will fail until
`website/cue/reference/components/sources/generated/static_metrics.cue:77` is regenerated to
`required: false`.

The existing changelog fragment
`changelog.d/25752_ddsketch_histogram_exact_sum.fix.md` gains a sentence noting that histograms
reporting no sum now keep the bucket-derived estimate instead of being treated as having a sum of
zero.

## 8. Testing

New or updated coverage, roughly in dependency order:

1. **DDSketch** (`ddsketch.rs`): a histogram with `sum: None` falls back to the bucket-interpolated
   `sum`/`avg` rather than reporting zero. Complements the four existing guard tests.
2. **Mutator semantics** (`value.rs` / `mod.rs`): `add` and `subtract` yield `None` when either
   operand is `None` and the arithmetic result when both are `Some`; `zero` preserves absence;
   `PartialEq` distinguishes `None` from `Some(0.0)`.
3. **Protobuf** (`proto.rs`): `Some` round-trips through the v3 arm with unchanged bytes, `None`
   round-trips through the new v4 arm, and a decoded v1/v2/v3 histogram yields `Some`. The existing
   `roundtrip_current_native_proto_fixtures` must still pass untouched.
4. **Native JSON**: `None` omits the key; a payload without the key decodes to `None`. Extends
   `histogram_metric_roundtrip` in `lib/codecs/tests/native_json.rs:34`.
5. **Prometheus parser**: a histogram exposition without a `_sum` line parses to `None`; one with
   `_sum` still parses to `Some`; a summary without `_sum` still yields `0.0`.
6. **OTLP source** (`src/sources/opentelemetry/tests.rs`): a histogram data point with no `sum`
   becomes `None`, and an exponential histogram likewise.
7. **Prometheus sink** (`collector.rs`): no `<name>_sum` line is emitted for a `None` sum, while
   `_bucket` and `_count` lines are unchanged.
8. **Lua** (`lua/metric.rs`): `None` presents as `nil`, and a table without a `sum` key decodes to
   `None`.
9. **`log_to_metric`**: a log without `aggregated_histogram.sum` produces `None` instead of an
   error; a non-float value still errors.

Property-generator updates: `lib/vector-core/src/event/metric/arbitrary.rs:35` and
`lib/vector-core/src/event/arbitrary_impl.rs:296` (including the sum shrinker at `:326`) should
generate `None` alongside `Some`. These feed proptest and the fixture generator binary; the
checked-in fixtures are not regenerated as part of this change, so introducing `None` into the
generators does not disturb them.

## 9. Out of scope

- `AggregatedSummary.sum` and `lib/prometheus-parser`'s `SummaryMetric.sum` stay `f64`, per the
  specified summary behavior in section "Scope decision".
- No bucket-estimation fallback outside DDSketch. Consumers omit their sum output; they do not
  reconstruct one.
- Non-finite sums in native JSON remain unfixed (see section 6).
- Regenerating the native-encoding fixtures, which the content-selected proto encoding avoids
  needing.

## 10. Risks

| Risk | Assessment |
| --- | --- |
| Lua scripts doing arithmetic on `sum` | Breaks only for genuinely sum-less histograms. Accepted deliberately; the alternative was Lua fabricating a zero that no other surface fabricates. |
| Two protobuf encode paths for one variant | A future field added to the histogram message must be added to both. Mitigated by a comment at the encode site and by the round-trip tests in section 8. |
| An older Vector peer cannot decode `AggregatedHistogram4` | Only affects sum-less histograms, which that peer could not have represented. Strictly better than always emitting v4, which would make it unable to decode any histogram. |
| `Display` output changes | Cosmetic, and only when the sum is absent. |
| Absence spreading further than expected through `add`/`subtract` | Intended: one unknown operand makes the total unknown. Section 8 item 2 pins the behavior. |
