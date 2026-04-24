# RFC NNNNN - 2026-04-24 - Internal Trace Data Model

This RFC documents a replacement for the inner representation of Vector's `TraceEvent`, which today
is only a thin newtype over `LogEvent`, with a strongly-typed `Span`. The outer `TraceEvent` wrapper
is retained so that existing call sites and the `Event::Trace(TraceEvent)` variant remain untouched.
The new `Span` carries its enclosing values via `Arc` for cheap sharing, preserves the source-native
attribute maps so both OTLP and Datadog spans round-trip through Vector (i.e. `OTLP -> Vector ->
OTLP` and `Datadog -> Vector -> Datadog`) without loss, and promotes a small, fixed set of
first-class fields so transforms can operate uniformly regardless of source.

## Context

- [RFC 11851 -- OpenTelemetry traces source](2022-03-15-11851-ingest-opentelemetry-traces.md) was
  accepted on the condition that an internal trace model be established before the work was
  completed.
- [RFC 9572 -- Accept Datadog traces](2021-10-15-9572-accept-datadog-traces.md) introduced the
  `datadog_agent` trace ingest path, which the `datadog_traces` sink can consume but which does not
  have a well-defined internal representation.
- An earlier draft of an internal trace model is available at
  [hdost/vector:2024-03-22-20170-trace-data-model](https://github.com/hdost/vector/blob/add-trace-data-model/rfcs/2024-03-22-20170-trace-data-model.md).
  This RFC supersedes that draft.
- The current implementation is in
  [`lib/vector-core/src/event/trace.rs`](../lib/vector-core/src/event/trace.rs):
  `TraceEvent(LogEvent)`. It provides no type structure, making transforms depend on the source's
  key layout and making cross-format conversions ad-hoc per sink.
- [Transform between opentelemetry and datadog
  traces](https://github.com/vectordotdev/vector/issues/22659)

## Glossary

This RFC references several trace data formats and related specifications. The
summaries below describe the role each plays in the model and link to the
authoritative sources from which the proposed mappings are derived.

- OTLP (OpenTelemetry Protocol): The wire format the OpenTelemetry project
  defines for transporting traces, metrics, and logs. For traces, the schema
  lives in
  [`opentelemetry/proto/trace/v1/trace.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto),
  with shared value types in
  [`common/v1/common.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/common/v1/common.proto)
  and resource types in
  [`resource/v1/resource.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/resource/v1/resource.proto).
  When this document says "OTLP" it means specifically that wire schema and the
  data model it defines (`ResourceSpans`, `ScopeSpans`, `Span`, `AnyValue`,
  etc.).
- OpenTelemetry: The broader observability project under which OTLP is one
  component. References to "OpenTelemetry" in this RFC name the project and
  its non-wire artefacts: the
  [OpenTelemetry specification](https://github.com/open-telemetry/opentelemetry-specification)
  (which defines the semantics OTLP encodes) and the
  [OpenTelemetry semantic conventions](https://github.com/open-telemetry/semantic-conventions)
  (the registry of standard attribute keys such as `service.name`,
  `deployment.environment`, and `http.request.method` that this model uses for
  its typed `Resource` fields and recommends for general `attributes`).
- Datadog APM trace format: The wire format the Datadog Agent accepts and
  emits for traces, documented in the
  [Send traces to the Agent by API](https://docs.datadoghq.com/tracing/guide/send_traces_to_agent_by_api/)
  guide. The protobuf schema lives in the Datadog Agent repository at
  [`pkg/proto/datadog/trace/span.proto`](https://github.com/DataDog/datadog-agent/blob/main/pkg/proto/datadog/trace/span.proto)
  and
  [`agent_payload.proto`](https://github.com/DataDog/datadog-agent/blob/main/pkg/proto/datadog/trace/agent_payload.proto).
  When this document says "Datadog" without further qualification it means
  this format. This RFC also references the
  [Datadog Agent's OTLP ingest](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go)
  implementation as the reference for OTLP-to-Datadog field mappings adopted
  here.
- Zipkin v2: The JSON span format defined by the
  [Zipkin API](https://zipkin.io/zipkin-api/#/default/get_spans). Zipkin
  transports a flat array of self-contained spans with no resource/scope
  grouping on the wire. Cited in the discussion of "array of spans"
  representations and as one of the formats that the proposed `Span` is
  intended to fit, though no Zipkin source/sink is in scope here.
- Jaeger: A distributed tracing system whose legacy data model is documented at
  [jaegertracing.io](https://www.jaegertracing.io/docs/latest/architecture/#span).
  Jaeger ships both a Thrift IDL (`Batch { spans[], process }`) and a
  gRPC/protobuf
  [`api_v2.Span`](https://github.com/jaegertracing/jaeger-idl/blob/main/proto/api_v2/model.proto).
  Jaeger's `Process`, `tags`, `logs`, and `references` correspond to the
  proposed `Resource`, `attributes`, `events`, and `links` respectively.
- OpenTracing: The predecessor specification to OpenTelemetry, with its own
  [span model](https://github.com/opentracing/specification/blob/master/specification.md).
  Cited only as a reference point; no OpenTracing source/sink is in scope.
- W3C Trace Context: The W3C recommendation that defines the
  [`traceparent` and `tracestate` HTTP headers](https://www.w3.org/TR/trace-context/)
  used to propagate trace identity across service boundaries. The proposed
  `TraceFlags` and `TraceState` types correspond to the bitfield and key/value
  list these headers carry; the size bounds quoted in the `TraceState`
  rationale (32 entries, 512 bytes total) come from this specification.

## Cross cutting concerns

- First-class OpenTelemetry signal support
  ([vectordotdev/vector#1444](https://github.com/vectordotdev/vector/issues/1444)).
- APM stats aggregation in the `datadog_traces` sink (today reads magic keys from `TraceEvent`; will
  read typed fields after this RFC lands).
- VRL trace-specific semantics (e.g., `.span.status.code`, `.span.resource.service`).

## Scope

### In scope

- Define a single, typed `Span` type that replaces the `LogEvent` field within `TraceEvent`.
- Define `TraceEvent` as the wrapper of `(Span, EventMetadata)`, parallel to how `LogEvent` and
  `Metric` wrap their payload plus metadata.
- Specify the bidirectional mapping between the internal `Span` and the OTLP
  `ResourceSpans`/`Span` wire format.
- Specify the bidirectional mapping between the internal `Span` and the Datadog
  `TracePayload`/`TracerPayload`/`TraceChunk`/`Span` wire format.
- Guarantee zero-loss round-trip through Vector for both formats when the pipeline does not
  otherwise mutate the data: `OTLP -> Vector -> OTLP` and `Datadog -> Vector -> Datadog` must
  reproduce the input verbatim. The Datadog guarantee is conditional on a producer-side
  convention (disjoint keysets across `meta`/`metrics`/`meta_struct`) that all examined Datadog
  SDKs and the trace agent maintain in practice; see "Datadog attribute partitions: convention
  versus invariant" for the explicit contract and the deterministic behaviour when a
  non-conforming producer violates it.

### Out of scope

- VRL function additions for trace-specific operations (e.g., `decode_trace_state`).
- Adding new trace sources/sinks (Zipkin, Jaeger, etc.).
- APM stats computation semantics (already covered by RFC 9862).
- Topology-level batching/grouping of sibling spans; the `Arc`-based sharing is a memory
  optimisation, not a batching mechanism.
- Zero-loss cross-format round-trip through Vector. Specifically, `OTLP -> Vector -> Datadog ->
  Vector -> OTLP` and `Datadog -> Vector -> OTLP -> Vector -> Datadog` are not required to reproduce
  the input verbatim. Cross-format conversion (`OTLP -> Vector -> Datadog`, `Datadog -> Vector ->
  OTLP`) is supported as a one-way operation, but neither sink emits reserved-key metadata for the
  express purpose of being lifted back into typed fields by the other format's source.

## Pain

- Transforms written against `TraceEvent` today depend on the exact key layout chosen by the
  ingesting source. A remap transform that works for `datadog_agent` traces does not work for
  `opentelemetry` traces, and vice versa, even when the semantic intent is identical. This is the
  opposite of how `Metric` behaves and is the primary blocker to useful trace transforms.
- Cross-format routing (e.g., `opentelemetry` source -> `datadog_traces` sink) requires bespoke
  translation code reading undocumented magic keys from an `ObjectMap`. Every new sink would
  duplicate this work, resulting in a multiplication of translation functionality with each new
  supported source.
- `TraceEvent` currently loses or corrupts numeric ID precision (`trace_id as i64` in both the
  `datadog_agent` source and `datadog_traces` sink; see
  [#14687](https://github.com/vectordotdev/vector/issues/14687)). A typed model fixes this by
  construction.
- Without typed events, links, or status, VRL programs authoring spans may produce structurally
  invalid output that is only discovered at sink encoding time.

## Proposal

### User Experience

Users interact with traces via typed VRL paths analogous to `Metric`:

```coffee
# Route spans from a single service to a dedicated pipeline.
if .span.resource.service == "checkout" { ... }

# Drop health checks regardless of source format.
if .span.name == "GET /health" { .drop = true }

# Mark slow DB spans as errors. `duration` is exposed in seconds as a float
# at the VRL surface; the underlying storage is integer nanoseconds.
if .span.span_type == "db" && .span.duration > 1.0 {
  .span.status.code = "error"
  .span.status.message = "slow query"
}

# Read a semantic-convention attribute, falling back to a Datadog-native key.
.user = .span.attributes."user.id" ?? .span.attributes."usr.id"
```

The fields a user sees are the same whether the span arrived via OTLP, from the Datadog Agent,
or was constructed by VRL. Source-native attribute maps (OTLP `Span.attributes`,
`Resource.attributes`, `Scope.attributes`; Datadog `meta`/`metrics`/`meta_struct`) are preserved
under the single `attributes` field on the appropriate typed level; they are never copied into a
parallel "extensions" map.

Datadog's three span-level attribute partitions (`meta`/`metrics`/`meta_struct`) are merged into
`Span.attributes` by `Value` variant. This merge relies on a producer-side disjointness
convention that all examined Datadog SDKs and the trace agent maintain in practice; see
"Datadog attribute partitions: convention versus invariant" below for the explicit contract
and the deterministic behaviour when a non-conforming producer violates it.

Datadog's three resource-level tag scopes (`AgentPayload.tags`, `TracerPayload.tags`,
`TraceChunk.tags`) are preserved verbatim as three reserved top-level entries in
`Resource.attributes`: `attributes.payload`, `attributes.tracer`, and `attributes.chunk`. Each
entry is an `Object` holding the wire-level map for that scope. This preservation is structural,
not convention-dependent, because the three resource scopes do collide on known keys
(e.g. `_dd.apm_mode`, written by the Datadog Agent at both the agent-payload and tracer-payload
scopes with semantically distinct values) and merging them would lose data. See "Datadog
resource tag scopes" below.

This is a breaking change for existing trace pipelines. The `TraceEvent` type itself is retained
so as to minimize churn at every call site that handles `Event::Trace(TraceEvent)`, but its inner
representation changes from `LogEvent` to `Span`. The key layout today's `datadog_agent` source
produces is replaced by typed access (`.span.trace_id` rather than `.spans[i].trace_id`, etc.).

The `trace_to_log` transform is retained as a component, but its output shape also changes.
Today the transform forwards whatever key layout the ingesting source happened to produce, so its
output is implicitly source-defined. Under this RFC it flattens the typed `Span` into a
documented, source-independent key layout. Users running VRL programs against `trace_to_log`
output must update those programs to the new layout; the migration guide will provide a
field-by-field mapping from the old per-source layouts to the new uniform one.

### Implementation

#### `Span`

```rust
pub struct Span {
    /// Shared across sibling spans of the same source-side grouping: one
    /// OTLP `ResourceSpans`, or one (Datadog `TracerPayload`, distinct
    /// `Span.service`) tuple. See the Datadog mapping for why service is
    /// part of the grouping key on that side.
    pub resource: Arc<Resource>,
    pub scope:    Arc<Scope>,

    pub trace_id:       TraceId,         // 128-bit
    pub span_id:        SpanId,          // 64-bit
    pub parent_span_id: Option<SpanId>,
    pub trace_state:    TraceState,
    pub flags:          TraceFlags,

    /// Span operation name. Equivalent to OTLP `Span.name` and Datadog
    /// `Span.name` (the operation name, not the resource).
    pub name: KeyString,
    pub kind: SpanKind,

    pub start_time: DateTime<Utc>,
    /// Span duration with nanosecond precision. Stored as `std::time::Duration`
    /// rather than floating-point seconds so the full wire-domain of OTLP
    /// (`fixed64` nanos) and Datadog (`int64` nanos) round-trips exactly.
    pub duration:   Duration,

    pub status: SpanStatus,

    /// Datadog-native: a human-meaningful identifier for the resource the
    /// span acted on (URL, handler, SQL statement). Has no OTLP equivalent.
    pub resource_name: Option<KeyString>,

    /// Datadog-native span type (web/db/cache/http/custom). No OTLP equivalent.
    pub span_type: Option<KeyString>,

    /// Datadog-native sampling state, carried per-chunk by Datadog and absent
    /// from OTLP.
    pub sampling: Sampling,

    /// Single canonical attribute map regardless of source format. On OTLP
    /// ingest this is OTLP `Span.attributes` verbatim. On Datadog ingest
    /// this is the union of the wire-level `meta`, `metrics`, and
    /// `meta_struct` maps, distinguished by `Value` variant: `meta`
    /// entries become `Value::String`, `metrics` entries become
    /// `Value::Float`, `meta_struct` entries become `Value::Bytes`. The
    /// inverse mapping is used on Datadog egress. See "Datadog attribute
    /// partitions: convention versus invariant" below for why this
    /// representation preserves all real-world Datadog traffic.
    pub attributes: Attributes,

    pub events: Vec<SpanEvent>,
    pub links:  Vec<SpanLink>,

    pub dropped_attributes_count: u32,
    pub dropped_events_count:     u32,
    pub dropped_links_count:      u32,
}
```

`TraceEvent` itself is the outer wrapper, parallel to `LogEvent` and `Metric`:

```rust
pub struct TraceEvent {
    span:     Span,
    metadata: EventMetadata,
}
```

#### `Resource` and `Scope`

`Resource` promotes the three resource-level semantic-convention fields that both formats care
about. Remaining resource attributes (container id, language, SDK version, cloud provider, etc.)
stay in `attributes` under their standard semconv keys.

```rust
pub struct Resource {
    pub service:     Option<KeyString>,   // service.name
    pub environment: Option<KeyString>,   // deployment.environment
    pub host:        Option<KeyString>,   // host.name
    pub attributes:  Attributes,
    pub schema_url:  Option<KeyString>,
    pub dropped_attributes_count: u32,
}

pub struct Scope {
    pub name:       KeyString,
    pub version:    KeyString,
    pub attributes: Attributes,
    pub schema_url: Option<KeyString>,
    pub dropped_attributes_count: u32,
}
```

Both are wrapped in `Arc` on `Span`. Sources allocate one `Arc<Resource>` per source-side grouping
and clone the `Arc` into each emitted `Span`. The grouping is format-specific:

- OTLP: one `Arc<Resource>` per OTLP `ResourceSpans` message and one `Arc<Scope>` per nested
  `ScopeSpans`, since both are already explicit groupings on the wire.
- Datadog: one `Arc<Resource>` per (TracerPayload, distinct `Span.service`), since Datadog
  carries `service` per span rather than per payload. In the common case a payload has a single
  service and only one `Resource` is allocated; payloads that mix services produce one `Resource`
  per service.

Transforms that need to mutate either structure call `Arc::make_mut` for copy-on-write, which is
the mechanism by which `.span.resource.service = "..."` and similar mutations work without
disturbing sibling spans that happen to share the same `Arc`.

#### Identifiers

`TraceId` is a 128-bit and `SpanId` a 64-bit newtype. Both wrap their respective `NonZero*`
integer: OTLP defines an all-zero ID as invalid, and Datadog uses zero only as the "no parent"
sentinel for `parent_id`, which is already represented in this model as `parent_span_id:
Option<SpanId>`. Encoding the non-zero invariant into the type itself eliminates a class of
malformed values by construction and keeps `Span::trace_id` and `Span::span_id` non-optional.
This also fixes Vector's existing `i64`-coercion bug
([#14687](https://github.com/vectordotdev/vector/issues/14687)).

```rust
pub struct TraceId(NonZeroU128);

impl TraceId {
    /// Low 64 bits, used as Datadog's `trace_id`.
    pub fn low_u64(self)  -> u64 { self.0.get() as u64 }
    /// High 64 bits, stored in Datadog's `_dd.p.tid` meta tag.
    pub fn high_u64(self) -> u64 { (self.0.get() >> 64) as u64 }
}

pub struct SpanId(NonZeroU64);
```

Conversions to and from `u128`/`u64` and OTLP's 16/8-byte big-endian representations are provided
as cheap copies via `From` (when the source is statically non-zero) and `TryFrom` (otherwise);
each conversion is a single integer or byte-swap operation.

#### Status, kind, sampling

```rust
pub enum SpanKind { Unspecified, Internal, Server, Client, Producer, Consumer }

pub struct SpanStatus {
    pub code:    SpanStatusCode,
    /// Only meaningful for `Error`; empty for `Unset` and `Ok`.
    pub message: Option<KeyString>,
}
pub enum SpanStatusCode { Unset, Ok, Error }

pub struct Sampling {
    pub priority: Option<SamplingPriority>,
    pub origin:   Option<KeyString>,
    pub dropped:  bool,            // Datadog `TraceChunk.dropped_trace`
}

pub enum SamplingPriority {
    UserReject, // -1
    AutoReject, //  0
    AutoKeep,   //  1
    UserKeep,   //  2
    /// Any value not covered by the four well-known priorities. Datadog tracing
    /// libraries occasionally emit values outside the documented range.
    Other(i32),
}
```

#### `TraceFlags` and `TraceState`

`TraceFlags` is a W3C trace-flags bitfield. Only the `sampled` bit is defined today but the type
preserves unknown bits verbatim for forward compatibility.

```rust
pub struct TraceFlags(u8);
impl TraceFlags {
    pub fn sampled(&self) -> bool          { self.0 & 0x01 != 0 }
    pub fn set_sampled(&mut self, v: bool) { /* set/clear bit 0 */ }
    pub fn bits(&self) -> u8               { self.0 }
    pub fn from_bits(b: u8) -> Self        { Self(b) }
}
```

`TraceState` stores the W3C `tracestate` header verbatim as the source provided it, and exposes
map-like accessors that parse on demand. Sources copy the header in unchanged; sinks emit it
unchanged unless a transform mutated it. Transforms that read or edit entries do so through the
accessors without forcing a structured representation onto pipelines that simply forward the
header.

```rust
pub struct TraceState(String);

impl TraceState {
    pub fn new() -> Self;
    pub fn from_raw(s: impl Into<String>) -> Self;
    pub fn as_str(&self) -> &str;
    pub fn is_empty(&self) -> bool;

    pub fn get(&self, key: &str) -> Option<&str>;
    pub fn insert(&mut self, key: &str, value: &str);
    pub fn remove(&mut self, key: &str) -> bool;
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> + '_;
}
```

`insert` rewrites the underlying string in-place, preserving entry order and inserting new entries
at the head, per the W3C spec. The parser and rewriter live in `vector-core` so all components
share one implementation.

The W3C spec caps `tracestate` at 32 entries totalling at most 512 bytes, and real-world headers
are typically a single short entry. Parsing such a small string, allocating a separate `String`
per key and value, and maintaining a map representation costs more in allocation traffic and
cache footprint than re-parsing the raw header on each accessor call. Storing the raw string is
therefore both faster and smaller for the expected workload, and pipelines that simply forward
the header pay nothing at all.

#### Events and links

```rust
pub struct SpanEvent {
    pub name: KeyString,
    pub time: DateTime<Utc>,
    pub attributes: Attributes,
    pub dropped_attributes_count: u32,
}

pub struct SpanLink {
    pub trace_id:    TraceId,
    pub span_id:     SpanId,
    pub trace_state: TraceState,
    pub flags:       TraceFlags,
    pub attributes:  Attributes,
    pub dropped_attributes_count: u32,
}
```

#### `Attributes`

```rust
pub struct Attributes(ObjectMap);
```

A newtype over `ObjectMap` (re-exported from `vrl::value`). `Value` already carries `Bytes`,
`Float`, `Integer`, `String`, `Boolean`, `Timestamp`, `Array`, and `Object` variants. Keys follow
OpenTelemetry semantic conventions; snake_case is preferred for new internally-generated keys. The
newtype exists so the RFC can attach future invariants (key validation, size bounds) without
requiring a migration.

The choice to permit nested values (`Array`, `Object`) rather than a flat map with scalar-only
values is deliberate: OTLP's `AnyValue` is recursive and can hold `ArrayValue` and `KvlistValue`,
both of which are themselves lists of `AnyValue`. The OpenTelemetry specification defines
semantic-convention attributes whose values are arrays of strings (e.g. `http.request.header.<key>`
is a `string[]`) and structured records. Storing these as flat scalars would require lossy
flattening (`a.b.c`-style key synthesis) on ingest and a corresponding reconstruction on egress,
neither of which is reliable for arbitrary semconv keys. Since Datadog attributes are always
scalars, the nested capability is unused on the Datadog egress path; the sink stringifies any
non-scalar value via JSON, as documented under "Datadog mapping" below.

#### Datadog attribute partitions: convention versus invariant

Datadog spans carry attributes in three independent wire-level maps:

- `meta`: keys to UTF-8 strings.
- `metrics`: keys to IEEE-754 doubles.
- `meta_struct`: keys to opaque bytes (msgpack-encoded structured payloads).

The Datadog `Span` proto defines all three as separate `map<string, ...>` fields and imposes no
keyset constraint between them; the wire format permits the same key to appear in any subset of the
three with semantically distinct values per partition. This `Span` type does not preserve such
cross-partition collisions verbatim. Instead, the three maps are merged into a single
`Span.attributes` on ingest and reconstructed by `Value` variant on egress:

This representation is lossless whenever each wire key has a single value type, regardless of which
of the three partitions carried it, and is lossy when the same key holds two or more value types
on the wire simultaneously.

The justification rests on the distinction between protocol invariants and producer conventions. The
disjointness of keysets across `meta`, `metrics`, and `meta_struct` is systematically maintained,
but it is a convention enforced at the producer layer, not a protocol invariant. every Datadog SDK
examined keeps the partitions disjoint by construction (string user tags go to `meta`, internal
numeric controls like `_sampling_priority_v1` go to `metrics` under namespaced keys, AppSec-style
structured payloads go to `meta_struct` under namespaced keys), and the trace agent itself does not
introduce cross-partition keys. Vector therefore receives data that, in practice, never exhibits a
cross-partition collision, even though the wire format would allow one.

The risks of relying on the convention are bounded and documented:

- A non-conforming producer (a custom tracer, a third-party adapter, or a future SDK regression)
  could emit a span whose wire form contains a `meta`/`metrics`/`meta_struct` key collision. The
  Datadog source resolves the collision deterministically (last-loaded partition wins, in the
  order `meta` -> `metrics` -> `meta_struct`) and emits a `DatadogAttributeCollision` internal
  event carrying the colliding key, the partitions involved, and the resolved variant. The
  internal event writes a rate-limited `warn!` log so operators can investigate the offending
  producer, and increments `component_errors_total`.
- The contract is asserted in the Datadog source: the source is permitted to assume the
  convention; the model itself does not.
- If the convention ever ceases to hold for production traffic (a future SDK starts emitting
  collisions deliberately), the model must change. The fallback is documented under
  "Alternatives" below: introduce a `Value::Bag` variant or move to a separate
  `Span.datadog_attributes` field. Either change is mechanical and contained.

Datadog egress: The sink scans `Span.attributes` and partitions by `Value` variant per the table
above. `Value::Integer` is coerced to `f64` and routed to `metrics`. Variants with no native Datadog
partition (`Value::Boolean`, `Value::Timestamp`, `Value::Array`, `Value::Object`) are stringified
(JSON for composite variants) and routed to `meta`. The result is one entry per key in exactly one
wire partition.

OTLP egress (cross-format): Datadog `meta`/`metrics`/`meta_struct` entries that arrived through
Vector already live in `Span.attributes` as ordinary `Value::String`/`Float`/`Bytes` entries,
which are themselves valid OTLP `AnyValue` shapes. They flow through OTLP egress identically
to OTLP-sourced attributes. Cross-format round-trip through OTLP
(`Datadog -> OTLP -> Datadog`) is out of scope (see "Scope"); the OTLP source has no inverse
mapping that would re-partition these entries back into the wire-level
`meta`/`metrics`/`meta_struct` shape on a subsequent Datadog egress.

#### Datadog resource tag scopes

Datadog spans carry resource-level tags in three independent wire-level maps, organised
hierarchically per the proto comments on each field:

- `AgentPayload.tags`: tags common in all `tracerPayloads`, written by the Datadog Agent
  (e.g. `_dd.apm_mode` from agent configuration, `_dd.otel.gateway`).
- `TracerPayload.tags`: tags common in all `chunks`, written by the tracer SDK and by the Agent
  on relay (e.g. `_dd.tags.container`, `_dd.tags.process`, `_dd.apm_mode` from a span's `Meta`).
- `TraceChunk.tags`: tags common in all `spans`, written by the Agent's sampling and
  error-tracking pipelines (e.g. `_dd.p.dm`, `_dd.error_tracking_standalone.error`).

Unlike the span-level `meta`/`metrics`/`meta_struct` partitions, the three resource tag scopes
do not maintain a producer-side disjointness convention even within the Datadog Agent itself.
The Agent's trace writer
([`pkg/trace/writer/trace.go`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/writer/trace.go))
writes `_dd.apm_mode` into `AgentPayload.tags` from its own configuration, and the Agent's
processing pipeline ([`pkg/trace/agent/agent.go`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/agent/agent.go))
writes the same key into `TracerPayload.tags` from a span's `Meta`. The two values are
semantically distinct (the Agent's claimed APM mode versus the tracer-reported APM mode) and
appear in the same payload. Merging the three wire scopes into one map would silently drop one
of these values and break the Datadog -> Vector -> Datadog round trip.

To preserve every cross-scope key/value pair without relying on a convention, the three wire scopes
are stored as three reserved top-level entries in `Resource.attributes`:

| Wire scope            | `Resource.attributes` key | Value shape |
| --------------------- | ------------------------- | ----------- |
| `AgentPayload.tags`   | `payload`                 | `Object`    |
| `TracerPayload.tags`  | `tracer`                  | `Object`    |
| `TraceChunk.tags`     | `chunk`                   | `Object`    |

Each entry's value is an `Object` whose contents are the wire-level map verbatim. All three entries
are created for Datadog sources.

VRL access to a specific scope's tag is direct:

```coffee
.agent_apm_mode  = .span.resource.attributes.payload."_dd.apm_mode"
.tracer_apm_mode = .span.resource.attributes.tracer."_dd.apm_mode"
```

Reservations and conflict handling: the keys `payload`, `tracer`, and `chunk` at the top level
of `Resource.attributes` are reserved for this scope encoding. OpenTelemetry resource semantic
conventions are uniformly dotted (`service.name`, `host.id`, `cloud.provider`, etc.), so an
OTLP-sourced resource does not normally produce these bare keys, and a user-set OTLP resource
attribute named exactly `payload`/`tracer`/`chunk` is exceptional. If such a key arrives from
OTLP, the Datadog sink rejects it as ill-typed (its value is unlikely to be an `Object` in the
expected shape) and emits an internal event; the value is dropped on Datadog egress. The
namespaced alternative (`_dd.scope.payload` etc.), which eliminates this conflict risk
entirely, is documented under "Alternatives".

OTLP egress (cross-format): the three scope objects have no OTLP-native home. On the
`Datadog -> OTLP` cross-format path, the OTLP sink emits them as best-effort
`Resource.attributes` entries of `KvlistValue` type under the keys `datadog.tags.payload`,
`datadog.tags.tracer`, and `datadog.tags.chunk` so downstream OTLP consumers can recover
the content via VRL. Cross-format round-trip through OTLP (`Datadog -> OTLP -> Datadog`) is
out of scope (see "Scope"); the OTLP source does not lift these reserved keys back into the
bare scope keys.

#### OTLP mapping

One OTLP `ResourceSpans` message expands into one `Arc<Resource>`, several `Arc<Scope>`s (one per `ScopeSpans`), and N `Span` events.

| OTLP                                                | Internal                                                   |
| --------------------------------------------------- | ---------------------------------------------------------- |
| `ResourceSpans.resource.attributes["service.name"]` | `Resource.service`                                         |
| `ResourceSpans.resource.attributes["deployment.environment"]` | `Resource.env`                                   |
| `ResourceSpans.resource.attributes["host.name"]`    | `Resource.host`                                            |
| `ResourceSpans.resource.attributes` (others)        | `Resource.attributes`                                      |
| `ResourceSpans.schema_url`                          | `Resource.schema_url`                                      |
| `ScopeSpans.scope.*`                                | `Scope.*`                                                  |
| `Span.trace_id` (bytes)                             | `Span.trace_id` (all-zero is rejected per OTLP)            |
| `Span.span_id`, `Span.parent_span_id`               | `Span.span_id`, `Span.parent_span_id`                      |
| `Span.trace_state` (string)                         | `Span.trace_state` (verbatim)                              |
| `Span.flags`                                        | `Span.flags`                                               |
| `Span.name`, `Span.kind`                            | `Span.name`, `Span.kind`                                   |
| `Span.start_time_unix_nano`                         | `Span.start_time` (DateTime<Utc>)                          |
| `Span.end_time_unix_nano - Span.start_time_unix_nano` | `Span.duration` (Duration, nanosecond-exact)             |
| `Span.attributes`                                   | `Span.attributes`                                          |
| `Span.events`, `Span.links`                         | `Span.events`, `Span.links`                                |
| `Span.status.{code,message}`                        | `Span.status.{code,message}`                               |
| `Span.dropped_*_count`                              | `Span.dropped_*_count`                                     |

On the cross-format `Datadog -> OTLP` path, `Span.resource_name`, `Span.span_type`,
`Span.sampling`, and the three reserved `Resource.attributes` scope objects
(`payload`/`tracer`/`chunk`) have no OTLP-native home. The OTLP sink emits them on a
best-effort basis under reserved keys (`datadog.span.resource`, `datadog.span.type`,
`datadog.sampling.priority`, `datadog.origin`, `datadog.dropped_trace` on `Span.attributes`;
`datadog.tags.payload`, `datadog.tags.tracer`, `datadog.tags.chunk` of `KvlistValue` type on
`Resource.attributes`) so downstream OTLP consumers can recover the content via VRL. The OTLP
source does not interpret these reserved keys; cross-format round-trip through OTLP
(`Datadog -> OTLP -> Datadog`) is out of scope (see "Scope").

`Span.duration` is converted to `end_time_unix_nano = start_time_unix_nano + duration.as_nanos() as u64`
on egress. Both quantities are integer nanoseconds on the wire and in memory, so the round-trip is
bit-exact; no rounding step is involved.

#### Datadog mapping

A Datadog `TracePayload` expands into one or more `Arc<Resource>`s and N `Span` events. Datadog
carries `service` per span rather than per payload, so spans within a single `TracerPayload` may
name distinct services. To preserve this verbatim, the source allocates one `Arc<Resource>` per
distinct `Span.service` value seen within a `TracerPayload` (typically one, but unbounded in the
worst case). All other resource-level fields are identical across the spans of a single
`TracerPayload`, so the per-service `Resource`s differ only in `service`. Spans of the same
service within a payload share an `Arc<Resource>`; spans of different services get distinct
`Arc<Resource>`s.

The structural mapping below specifies which Datadog wire fields populate which internal `Span` or
`Resource` concepts. The precise OpenTelemetry semantic-convention keys used inside
`Resource.attributes` for tracer/runtime/app/agent metadata, and the OTLP `kind` -> Datadog
`Span.type` derivation used on egress when `Span.span_type` is absent, follow the [Datadog Agent's
OTLP ingest mapping](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go) and
are deferred to the implementation PRs (where they can be diffed against the agent source rather
than transcribed approximately here).

| Datadog                                                     | Internal                                                        |
| ----------------------------------------------------------- | --------------------------------------------------------------- |
| `TracerPayload.hostname`                                    | `Resource.host`                                                 |
| `TracerPayload.env`                                         | `Resource.env`                                                  |
| `Span.service` (per span)                                   | `Resource.service` of the `Arc<Resource>` selected for that span |
| `AgentPayload.tags`                                         | `Resource.attributes["payload"]` (`Value::Object`) -- see "Datadog resource tag scopes" |
| `TracerPayload.tags`                                        | `Resource.attributes["tracer"]`  (`Value::Object`) -- see "Datadog resource tag scopes" |
| `TraceChunk.tags`                                           | `Resource.attributes["chunk"]`   (`Value::Object`) -- see "Datadog resource tag scopes" |
| `TracerPayload.{containerID, languageName, languageVersion, tracerVersion, runtimeID, appVersion}` and `AgentPayload.agentVersion` | `Resource.attributes` under semconv keys chosen by the implementation per the Datadog Agent's OTLP convention (see note below) |
| `TraceChunk.priority`/`origin`/`dropped_trace`              | `Span.sampling.priority`/`origin`/`dropped`                     |
| `Span.traceID` (u64)                                        | `Span.trace_id.low_u64`                                         |
| `Span.meta["_dd.p.tid"]` (hex u64) if present               | `Span.trace_id.high_u64`                                        |
| `Span.spanID`, `Span.parentID` (u64)                        | `Span.span_id`, `Span.parent_span_id`                           |
| `Span.name`                                                 | `Span.name`                                                     |
| `Span.resource`                                             | `Span.resource_name`                                            |
| `Span.type`                                                 | `Span.span_type`                                                |
| `Span.start` (unix ns)                                      | `Span.start_time`                                               |
| `Span.duration` (ns, int64)                                 | `Span.duration` (Duration, nanosecond-exact)                    |
| `Span.error`                                                | `Span.status.code` (`1` -> `Error`, else `Unset`)                |
| `Span.meta` (string->string)                                 | `Span.attributes[k] = Value::String(v)`                         |
| `Span.metrics` (string->double)                              | `Span.attributes[k] = Value::Float(v)`                          |
| `Span.meta_struct` (string->bytes)                           | `Span.attributes[k] = Value::Bytes(v)`                          |

On egress to Datadog, `Span.attributes` is partitioned back into the three wire maps by `Value`
variant per the rules in "Datadog attribute partitions: convention versus invariant" above. The
short form is:

- `Value::String(_)` -> `Span.meta`
- `Value::Integer(_)` or `Value::Float(_)` -> `Span.metrics` (coerced to `f64`)
- `Value::Bytes(_)` -> `Span.meta_struct`
- `Value::Boolean(_)`, `Value::Timestamp(_)`, `Value::Array(_)`, `Value::Object(_)` -> stringified (JSON for composite values) into `Span.meta`

`Span.kind` has no direct Datadog home but informs `Span.type` derivation when `Span.span_type`
is `None`. The derivation rules and the encoding of `Span.status` (including how `status.message`
is surfaced in `Span.meta`) follow the Datadog Agent's OTLP convention referenced above.

Per-span `Span.service` is reconstructed on egress as `span.resource.service` for each span; the
sink reads the service name off each span's `Arc<Resource>` independently. Spans are grouped into
`TracerPayload`s by their non-service resource fields, so two spans whose `Resource`s differ only
in `service` end up in the same `TracerPayload` with distinct per-span `Span.service` values,
exactly as Datadog itself emits them.

#### Retention of `TraceEvent` and `Event::Trace`

The outer `TraceEvent` type and the `Event::Trace(TraceEvent)` variant are retained. Only the inner
representation changes from `LogEvent` to `Span`:

```rust
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
    Trace(TraceEvent),
}

pub struct TraceEvent {
    span:     Span,
    metadata: EventMetadata,
}
```

Keeping the `TraceEvent` wrapper minimises code churn: the topology, buffers, finalisers, and the
`Event` enum dispatch sites are unchanged. The breaking change is internal to `TraceEvent`'s
accessor surface, where field-by-key access (`get(path)`, `insert(path, value)`) is replaced with
typed accessors on the inner `Span`. A small set of `From` impls is provided for ergonomics
(`Span` -> `TraceEvent` with default metadata).

#### Migration: coexistence of `LogEvent` and `Span` representations

A single atomic replacement of `TraceEvent`'s inner `LogEvent` with `Span` would force every trace
producer, consumer, and transform to be rewritten in one PR, which is impractical given the size
of the surface area (every source, every sink, the APM stats aggregator, every trace-aware
transform, every test). To allow incremental migration, `TraceEvent` will become an enum during
the migration so that the `Legacy` arm reuses `LogEvent`'s existing metadata storage rather than
duplicating it on the outer:

```rust
pub enum TraceEvent {
    /// Pre-migration source output: an untyped `LogEvent` whose key layout
    /// is defined by the producing source. `LogEvent` already carries its
    /// own `EventMetadata`, which is reused as-is.
    Legacy(LogEvent),
    /// Post-migration typed payload.
    Typed { span: Span, metadata: EventMetadata },
}
```

The end-state `struct TraceEvent { span: Span, metadata: EventMetadata }` shown above is reached
by deleting the `Legacy` arm once every component has migrated; the `Typed` arm's fields become
the struct's fields verbatim.

`TraceEvent` exposes both accessor families, and each dispatches on the variant:

- `metadata() -> &EventMetadata` / `metadata_mut() -> &mut EventMetadata` and finaliser methods
  return the inner `LogEvent`'s metadata when `Legacy`, and the `metadata` field when `Typed`.
  The set of metadata methods callers see is unchanged.
- The existing `get(path)`, `insert(path, value)`, `value()`, `value_mut()`, `as_map()` etc.
  operate on the `Legacy` form; if the variant is `Typed`, they convert on demand using the
  per-component shims described below.
- The new `span() -> &Span` and `span_mut() -> &mut Span` operate on the `Typed` form, converting
  on demand from `Legacy`.
- Explicit `to_typed(&mut self)` and `to_legacy(&mut self)` methods rewrite the variant in place
  when a caller knows it is about to perform many operations of one kind.

The `Legacy` to/from `Typed` conversions are defined per producing component: the `datadog_agent`
source ships with `(LogEvent, source-key-layout)` to/from `Span` shims that know about its key
layout, and similarly for the `opentelemetry` source. Each shim is retired as its source is migrated
to emit `Typed` natively. The shim modules live alongside their owning component so they cannot
accumulate hidden coupling; they are deleted when no longer referenced.

Conversions through the shim are observable to callers (for instance, `as_map()` on a `Typed`
inner allocates a transient `LogEvent`). Components that mix old and new APIs incur this cost; it
is the price of coexistence and is removed when `Legacy` is removed.

After every source, sink, and transform has been migrated, the `Legacy` variant and the shims are
deleted, leaving only `TraceEvent { span: Span, metadata: EventMetadata }`.

## Rationale

- This RFC picks a model (OTLP-shaped with typed Datadog accommodations) that makes the
  OTLP to/from Datadog translation a pure mechanical mapping, implementable in a single module and
  usable by every future trace source and sink.
- Typed fields let transforms be written once. The `Metric` data type demonstrates this model
  works in Vector's architecture; extending it to traces gives them the same parity with `Metric`
  and unblocks RFC 11851's finalisation.
- `Arc`-shared `Resource` and `Scope` keep memory overhead close to the current flat representation
  even though each `Span` now carries its full context.
- The `Attributes(ObjectMap)` newtype delegates typed storage to VRL's existing `Value`, which
  already covers the full set of types either format can transmit. No new map is introduced; source
  attribute maps are preserved directly.
- Fixing the `TraceId`/`SpanId` width bug by construction eliminates a whole class of issues that
  cannot be fixed within the current `LogEvent`-based model.

## Drawbacks

- Breaking change for existing VRL configurations that access trace fields by key (e.g.,
  `.trace_id`, `.spans[0].resource`). Users must migrate to typed paths.
- The `trace_to_log` transform's output is also a breaking change: today its output mirrors the
  ingesting source's key layout, so any downstream VRL program is implicitly coupled to that
  layout. Under this RFC `trace_to_log` emits a uniform, source-independent layout, which means
  users running VRL against its output must update their programs even if they were not using any
  of the new typed paths. Mitigated by a migration guide that maps each old per-source layout to
  the new uniform one.
- Every trace source and sink must be rewritten to produce/consume `Span`. The Plan of Attack
  below sequences this so each component can be migrated independently, but it is non-trivial
  work.
- `Arc::make_mut` on shared `Resource`/`Scope` in a transform triggers a deep clone. Workloads that
  mutate resource attributes per span lose the sharing benefit. Expected to be rare in practice.
- The Datadog round-trip guarantee depends on a producer-side disjointness convention rather than
  a wire-format invariant. The Datadog `Span` proto allows the same key in any combination of
  `meta`, `metrics`, and `meta_struct`, so a non-conforming producer can construct a span that
  this model cannot represent verbatim; on observing such a span the Datadog source resolves
  collisions deterministically and increments a error counter. Every Datadog SDK examined keeps
  the partitions disjoint by construction, and the trace agent does not introduce cross-partition
  keys, but Vector cannot enforce this on data it receives. If the convention ever ceases to hold
  for production traffic, the model must be revised; the contained fallbacks (`Value::Bag` or a
  separate `Span.datadog_attributes` field) are documented under "Alternatives".
- The model is shaped to accommodate OTLP and Datadog as the two reference formats. Other formats
  (Zipkin v2, Jaeger Thrift) are expected to fit within the proposed vocabulary, but if a future
  format requires additions the type can be extended in the same way Vector has extended the
  `Metric` value enum to add new metric kinds.

## Prior Art

- [OpenTelemetry Traces
  Protocol](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)
  -- the primary shape this RFC adopts. The OTLP model is the most complete trace specification in
  active use; adopting its vocabulary directly avoids inventing Vector-specific terminology.
- [Datadog APM ingest format](https://docs.datadoghq.com/tracing/guide/send_traces_to_agent_by_api/) -- the second native format Vector must round-trip.
- [Datadog Agent OTLP
  ingest](https://github.com/DataDog/datadog-agent/blob/7.60.1/pkg/trace/api/otlp.go#L489) -- the
  reference implementation for OTLP->Datadog field mappings adopted here.
- [hdost/vector -- 2024-03-22-20170
  draft](https://github.com/hdost/vector/blob/add-trace-data-model/rfcs/2024-03-22-20170-trace-data-model.md)
  -- an earlier draft that modelled the event as a `ResourceSpans` (batch of spans). The current RFC
  diverges in modelling a single span per event, promoting a small set of Datadog-native concepts to
  typed fields, and specifying a concrete Datadog mapping.
- [Zipkin v2 JSON](https://zipkin.io/zipkin-api/#/default/get_spans), [Jaeger
  model](https://www.jaegertracing.io/docs/latest/architecture/#span) (Thrift IDL and the
  gRPC/protobuf
  [`api_v2.Span`](https://github.com/jaegertracing/jaeger-idl/blob/main/proto/api_v2/model.proto)),
  and the [OpenTracing
  model](https://github.com/opentracing/specification/blob/master/specification.md) -- all
  representable within the proposed structure, but not reference-mapped in this RFC. Jaeger's
  `Process` corresponds to `Resource`; its `tags`, `logs`, and `references` map to `attributes`,
  `events`, and `links` respectively.

## Alternatives

### Modelling the event as an array of spans

Each `Event::Trace` would carry a collection of spans rather than a single span. Two natural shapes:
a batch (an OTLP-style `ResourceSpans` -- spans grouped by resource/scope, mixing trace identities)
or a trace (all spans sharing one trace_id, possibly partial). The latter would be the more likely
design choice as it is more granular.

Both reference formats already batch on the wire. OTLP's top-level `ResourceSpans` is a hierarchy of
`(resource, scope_spans[], schema_url)` where each `ScopeSpans` holds `(scope, spans[],
schema_url)`; nothing on the OTLP wire requires the batched spans to share a `trace_id`, and a
single `ResourceSpans` typically interleaves many traces. Datadog's `TracePayload` is a hierarchy of
`(agentVersion, tracerPayloads[])` where each `TracerPayload` contains `chunks[]` (`TraceChunk {
priority, origin, dropped_trace, spans[], tags }`), and a chunk is trace-keyed -- its sampling
decision and `dropped_trace` flag apply to every span in the chunk. So OTLP batches by
resource/scope, Datadog batches by trace (within a tracer payload's resource), and an internal
"array of spans" representation could plausibly mirror either. On the other hand, Zipkin v2
transports spans as a flat JSON array with no wire-level envelope or trace grouping, so each Zipkin
span is fully self-contained on the wire and any "array" representation Vector chose would be a
Vector-side construct rather than something Zipkin gives it. The Jaeger legacy protocol carries a
`Batch { spans[], process }` shape that resembles OTLP's resource grouping more than Datadog's trace
grouping.

Transporting entire traces has some benefits because several trace-aware operations are difficult
or impossible to express on per-span events:

- Tail-based sampling: Decisions of the form "keep all traces that contain an error", "keep the
  slowest 1% of traces", or "keep traces touching service X" are inherently trace-scoped. Per-span
  sampling either over-keeps (sampling each span independently) or under-keeps (dropping spans
  without considering their siblings). A trace-shaped event lets a sampling transform see the trace
  as a unit.
- APM stats aggregation: Datadog's APM stats (hit counts, error counts, top-level span
  identification, latency distributions per service/resource) are computed per trace. The current
  `datadog_traces` sink already buffers spans by trace internally to do this; a trace-shaped event
  would surface that grouping at the topology level rather than hiding it in the sink.
- Trace-scoped enrichment: Resolving the root span, computing critical-path latency, attaching
  trace-level annotations (e.g. "this trace was retried"), or projecting per-trace derived
  attributes onto every span in the trace are all natural at the trace level and awkward at the
  per-span level (they require a stateful join transform that reconstructs the trace from per-span
  events).
- Wire-shape preservation: OTLP's resource/scope deduplication and Datadog's chunk-level sampling
  fields map naturally onto a trace-shaped event without requiring `Arc` sharing across events.

Rejected because:

- Trace assembly is not a wire-level operation for every source Vector cares about. OTLP and Datadog
  deliver spans already grouped (by resource/scope and by chunk respectively), but Zipkin and
  OpenTracing do not, and even within OTLP/Datadog a single trace can arrive across multiple wire
  batches as it unfolds in real time. Materialising "the trace" as an event requires the source to
  wait for some completion signal that the wire format does not provide, and waiting introduces
  unbounded buffering, indefinite latency, and decisions about partial-trace timeouts that the data
  model would have to take a position on.
- Per-span operations (filter a single span, drop health-check spans, redact a single attribute)
  become awkward: every transform has to either iterate the array or accept trace-as-a-unit
  semantics. Vector's `sample`, `filter`, and `remap` transforms today operate per-event; aligning
  trace-as-event semantics with those transforms forces dismantling and re-packing the array on
  every transform invocation.
- Backpressure and buffering granularity become coarser. Buffer size limits expressed in events
  result in a less predictable span count or memory bound.
- The `Metric` type is a precedent, with caveats. It's value type does carry multi-element internal
  structures of aggregates, distributions, and sketches. However, the broader precedent is that one
  event corresponds to one atomic unit as delivered by sources, and that unit's internal
  multi-element structure is preserved verbatim if and only if the source already carries it
  pre-aggregated. The trace analogue is asymmetric: OTLP and Datadog deliver spans (one at a time,
  possibly grouped by resource or by chunk) and Zipkin delivers individual spans. None of the four
  reference trace formats deliver a pre-assembled trace as a wire-level unit. Picking "one span per
  event" for traces matches what `Metric` does for non-aggregate metrics like a single counter
  sample: Vector's event boundary aligns with the wire's atomic delivery unit. Picking "one trace
  per event" would require Vector to construct the aggregate in the source -- something `Metric`
  does not do for histograms or sketches (those arrive aggregated). Trace-aware transforms that
  genuinely need the trace as a unit can be added later as stateful aggregators (e.g., a
  `tail_sample` transform that groups events by `trace_id`, holds them for a configurable window,
  and emits a sampling decision for the whole trace), the same way Vector's `aggregate` transform
  composes per-event metrics into time-bucketed aggregates without forcing every metric to be an
  aggregate at the data-model level.

If trace-aware operations become a primary use case, the contained extension is a stateful
trace-aggregator transform that reads `Event::Trace(TraceEvent)` and emits a buffered result --
not a redefinition of `TraceEvent` itself.

### Separate `Span.datadog_attributes` field preserving the three wire partitions verbatim

Carry a `DatadogAttributes { meta, metrics, meta_struct }` field on `Span` alongside the canonical
`attributes`, populated only on Datadog ingest, consumed only on Datadog egress. This represents
the wire format exactly and preserves any cross-partition collision a Datadog producer could
emit. Rejected at present because: it splits the attribute surface in two (transforms must use a
unified read helper or know which surface to query); it forces every attribute-aware component to
handle both surfaces or accept that it works on only one of them; and the cost is paid against a
collision case that no examined Datadog SDK or agent actually emits. Listed here because it is
the contained mechanical fallback if the producer-side disjointness convention ever ceases to
hold for production traffic: the change is local to `Span`, the Datadog source, the Datadog sink,
and a unified read helper, with no impact on the OTLP side.

### `Value::Bag` variant for cross-partition collisions

Add a `Value::Bag(SmallVec<[Value; 2]>)` variant carrying multiple values per key. The Datadog
source emits `Bag` only when it observes an actual cross-partition collision; the common case is a
singleton and unchanged from the proposed model. Rejected at present for the same reason as the
separate-field alternative: the collision case is contractually empty in real-world traffic.
Further, it is far more intrusive than the separate-field alternative because `Value` comes from the
VRL crate and is shared across `LogEvent`, `Metric`, and `Span`.

### Opaque wire-data fast-path on `EventMetadata`

Carry the original Datadog wire bytes on `EventMetadata` and let the Datadog sink re-emit them
verbatim on the relay path (no transforms touched the span). Rejected because it conflates wire
fidelity with event lifecycle metadata, doubles memory for unmutated Datadog spans, and only
helps the relay case -- mutated spans still go through the typed serialiser and inherit whatever
loss the in-memory model has.

### Namespace-prefixed unified map

Encode Datadog's three partitions inside `attributes` itself by prefixing each key with its
partition name (`dd.meta.<k>`, `dd.metrics.<k>`, `dd.meta_struct.<k>`). Rejected because the
prefixes leak Datadog-specific encoding into every transform regardless of source: an OTLP-only
pipeline has to know about the Datadog namespace to avoid colliding with it, and an OTLP-sourced
attribute that happens to use a `dd.meta.*` key is silently misclassified on egress. The
proposed `Value`-variant routing achieves the same egress mapping without imposing any naming
constraint on transforms.

### Namespace-prefixed resource scope keys

Use `_dd.scope.payload`, `_dd.scope.tracer`, `_dd.scope.chunk` as the three reserved top-level
keys in `Resource.attributes` instead of the bare `payload`/`tracer`/`chunk` adopted in the
proposal. The contents are identical in both forms: an `Object` per scope holding the
wire-level map verbatim.

Pros:

- Eliminates any conflict risk with OTLP-sourced or user-set resource attributes named
  `payload`/`tracer`/`chunk`. The `_dd.*` namespace is reserved for Datadog-internal keys
  throughout the rest of the model (`_dd.tags.container`, `_dd.apm_mode`, `_dd.p.dm`, etc.),
  so adding `_dd.scope.*` to that namespace is consistent and forever-safe: future
  OpenTelemetry semantic-convention additions cannot collide because semconv does not emit
  `_dd.*`-prefixed keys.

Cons:

- Slightly longer VRL paths:
  `.span.resource.attributes."_dd.scope.tracer"."_dd.apm_mode"` versus the bare form's
  `.span.resource.attributes.tracer."_dd.apm_mode"`.
- The reserved keys are more visible in the attribute namespace of every Datadog-sourced
  `Resource`, slightly noisier than the bare form for users iterating `Resource.attributes`.

Rejected at present because the three bare keys (`payload`, `tracer`, `chunk`) do not collide
with any current OpenTelemetry resource semantic convention (semconv keys are uniformly
dotted), and because user-set OTLP resource attributes under those exact bare names are
exceptional. If a future semconv addition or a real-world conflict observed in production
violates this assumption, the bare form is mechanically migratable to the namespaced form in
a single PR (rename three keys at the source/sink boundary, update the OTLP reserved-key
mapping, update VRL documentation).

### Single merged `attributes` map with richer typed fields

Promote additional concepts (service, env, host and all semantic-convention equivalents) to typed
fields, reducing reliance on keyed access. Rejected because the semconv space is large and
evolving, fixing it in typed fields either forces Vector to track upstream semconv releases or
ossifies a stale subset. The proposal typed only the three resource fields both formats agree on;
the rest remain in source-native attributes, which is where users already expect them.

### Discriminated union (`Span::Otel | Span::Datadog`)

Carry each format as-is and dispatch at every consumer. Rejected because it directly inverts the
stated pain--every transform and every cross-format sink would handle two shapes. This is
effectively the status quo in spirit and offers no real improvement over `LogEvent`.

### Timing as `start_time` + `end_time` (OTLP-native)

OTLP stores `start_time_unix_nano` and `end_time_unix_nano` as two independent `fixed64`s. Datadog,
Zipkin v2, and Jaeger all store `start` plus `duration`. The driving factor for adopting `start +
duration` is that the majority of trace formats already use it and OTLP is the outlier. A secondary
benefit is that `duration` is more useful than `end_time` in realistic transforms (filtering slow
spans, computing percentiles, classifying long-running requests), so the chosen representation also
matches transform access patterns.

### Duration as `f64` seconds

Storing `duration` as `f64` seconds was considered for VRL numeric ergonomics and consistency with
Vector's existing metric timing conventions. Rejected because both OTLP (`fixed64` nanoseconds) and
Datadog (`int64` nanoseconds) carry duration as integer nanoseconds on the wire, and `f64`'s
53-bit mantissa cannot exactly represent every integer nanosecond beyond `2^53 ns` (about 104
days). Converting `start + round(duration * NANOSECONDS)` would silently shift the recovered
end-time for any duration past that limit, breaking the zero-loss round-trip guarantee. Storing
`duration` as `std::time::Duration` (nanosecond-precision integer internally) preserves the wire
domain exactly. VRL exposure of `duration` as a float-seconds view, an integer-nanoseconds view,
or both is a separate VRL surface decision; neither view's choice affects the underlying data
model.

### `Sampling.priority` as a raw `i32`

The wire representation in Datadog is a signed integer with four well-known values
(`UserReject = -1`, `AutoReject = 0`, `AutoKeep = 1`, `UserKeep = 2`). Storing the raw `i32`
directly is simpler and round-trips by definition. Rejected because transforms that condition on
priority then need to compare against magic numbers, and there is no way to surface "this is a
non-standard value" to the user. A strict enum with an `Other(i32)` escape hatch keeps the typed
ergonomics for the common path while still preserving any out-of-range value a tracer may emit.

### Parsed `TraceState`

Storing `TraceState` as `IndexMap<KeyString, KeyString>` would let transforms operate on entries
without an accessor layer. Rejected for two reasons: First, every source and every sink would
then have to invoke the parser/serialiser even for pipelines that are pure relays of unmodified
spans. Second, the W3C-imposed bounds on the header (at most 32 entries totalling 512 bytes, and
typically a single short entry in practice) mean the per-entry allocation cost of building and
holding a map exceeds the cost of re-parsing the raw string each time an accessor is called.
Storing the raw header makes the relay path a no-op and is faster and more compact for the
expected workload, while still exposing a map-like API to callers that need it.

### Wholesale migration: Replace `LogEvent` with `Span` in a single PR

The simplest implementation strategy is to delete `TraceEvent(LogEvent)` and replace it with
`TraceEvent { span: Span, metadata: EventMetadata }` in one change. Rejected because the resulting
PR would touch every trace source, every trace sink, the APM stats aggregator, every trace-aware
transform, and a large body of tests simultaneously, with no opportunity for partial review or
incremental rollout. The chosen `enum TraceEvent { Legacy, Typed }` coexistence design lets each
component migrate in its own PR while the rest of the system continues to operate against the
representation it expects, at the cost of transient `Legacy` to/from `Typed` shims per source layout
that is deleted once the corresponding component is migrated.

### Parallel `Event::Span(Span)` variant

Introduce `Event::Span(Span)` alongside the existing `Event::Trace(TraceEvent)`, leaving
`TraceEvent` untouched and migrating each component to emit/consume the new variant. Rejected
because it splits trace handling across two `Event` variants for the duration of the migration,
forcing every topology-level dispatch site (buffers, sinks of mixed event kinds, the metrics
publisher, etc.) to handle both. The tagged-inner approach contains the duality inside
`TraceEvent` itself, leaving `Event::Trace` as the single dispatch arm.

### Feature-flagged switch

Gate the new representation behind a Cargo feature or runtime flag until all components are
migrated, then flip the default and remove the flag. Rejected because feature combinations
proliferate quickly (the typed model interacts with every trace source/sink and with VRL), and
because a runtime flag would require duplicate code paths in performance-sensitive components.
The tagged-inner approach achieves the same per-component opt-in granularity without a flag.

## Outstanding Questions

- N/A

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

Each step below is intended to land as an independent PR. The `enum TraceEvent { Legacy, Typed }`
coexistence (described above) is what makes this incremental sequence possible.

- [ ] Submit a PR with spike-level code roughly demonstrating the `Span` and supporting types in
  `lib/vector-core/src/event/span/` (no consumers yet).
- [ ] Convert `TraceEvent` to an enum: `Legacy(LogEvent)` and `Typed { span: Span, metadata:
  EventMetadata }`. At this point every component still produces and consumes `Legacy`; nothing
  functionally changes. All accessor methods dispatch on the variant. Add the
  `span()`/`span_mut()`/`to_typed()`/`to_legacy()` API and default `Legacy` to/from `Typed` shims
  that returns an error if exercised before any source-specific shim is registered, so accidental
  mixed access is loud.
- [ ] Add VRL typed-path support for `.span.*` on `VrlTarget`.
- [ ] Write a migration guide for users with field-by-key VRL programs against the old
  `TraceEvent` and against the old `trace_to_log` output.
- [ ] Implement the OTLP to/from `Span` conversions in `lib/opentelemetry-proto`. Implement both
  the conversion the `opentelemetry` source will eventually use and as the `Legacy` to/from `Typed`
  shims used by OTLP-shaped `LogEvent`s.
- [ ] Implement the Datadog to/from `Span` conversions in `src/sources/datadog_agent/traces.rs` and
  `src/sinks/datadog/traces/`, both as the eventual native conversion and as the `Legacy` to/from
  `Typed` shims for Datadog-shaped `LogEvent`s.
- [ ] Migrate the `opentelemetry` source to produce `Typed` natively. Existing
  `Legacy`-consuming sinks and transforms keep working via the shim.
- [ ] Migrate the `datadog_agent` source to produce `Typed` natively.
- [ ] Migrate the `datadog_traces` sink to consume `Typed` natively; update APM stats
  aggregation to read typed fields. Until upstream sources are migrated this sink pulls through
  the shim with no functional change.
- [ ] Migrate the `sample` transform (and tests) to typed access.
- [ ] Migrate the `trace_to_log` transform to operate on `Typed` and emit a uniform,
  source-independent `LogEvent` layout. Document the new key layout.
- [ ] Collapse the `TraceEvent` enum to a struct containing only the `Typed` variant data. Remove
  the per-component shims.

## Future Improvements

- VRL helpers: `parse_trace_state`, `encode_trace_state`, `merge_span_attributes`, `decode_otlp_span`, `decode_datadog_span`.
- VRL surface for `Span.duration`: decide whether `.span.duration` is exposed as float seconds,
  integer nanoseconds, or both (e.g. `.span.duration_secs` and `.span.duration_nanos`). The data
  model preserves nanosecond precision either way.
- First-class Zipkin and Jaeger sources/sinks mapped onto `Span`.
- Link-based routing: a trace-aware router transform that emits to different sinks based on `SpanLink` targets.
- Reduce `Arc<Resource>`/`Arc<Scope>` clone rate in hot paths by interning on a per-topology basis rather than per-source-payload basis.
- Unify `SpanEvent` with Vector's `LogEvent` so span events can be routed to log sinks without a transform.
