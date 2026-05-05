# RFC 25329 - 2026-04-29 - Internal Trace Data Model

This RFC replaces the inner representation of Vector's `TraceEvent`, today a thin newtype over
`LogEvent`, with a strongly-typed container that mirrors the wire-level batching of OTLP and
Datadog APM traces. Each `TraceEvent` carries one `Resource`, one `Scope`, one Datadog-specific
`ChunkContext`, and the `Vec<Span>` belonging to that grouping, plus the existing `EventMetadata`.
The container shape yields zero-loss `OTLP -> Vector -> OTLP` and `Datadog -> Vector -> Datadog`
round trips, including across Vector's disk buffers, and gives transforms a uniform typed surface
across the two source formats.

## Context

- [RFC 11851 -- OpenTelemetry traces source](2022-03-15-11851-ingest-opentelemetry-traces.md) was
  accepted on the condition that an internal trace model be established before the work was
  completed.
- [RFC 9572 -- Accept Datadog traces](2021-10-15-9572-accept-datadog-traces.md) introduced the
  `datadog_agent` trace ingest path, which the `datadog_traces` sink can consume but which does
  not have a well-defined internal representation.
- An earlier draft of an internal trace model is available at
  [2024-03-22-20170-trace-data-model](https://github.com/hdost/vector/blob/add-trace-data-model/rfcs/2024-03-22-20170-trace-data-model.md);
  this RFC supersedes that draft.
- The current implementation in
  [`lib/vector-core/src/event/trace.rs`](../lib/vector-core/src/event/trace.rs) is
  `TraceEvent(LogEvent)` -- a thin newtype with no type structure. Transforms depend on the
  ingesting source's key layout, and cross-format conversions are ad-hoc per sink.
- [vectordotdev/vector#22659 -- Transform between opentelemetry and datadog traces](https://github.com/vectordotdev/vector/issues/22659).

## Glossary

This RFC references several trace data formats. The two Vector targets are OTLP and the Datadog
agent-to-backend protobuf; everything else is informational.

- **OTLP (OpenTelemetry Protocol)**: the wire format the OpenTelemetry project defines for traces,
  metrics, and logs. The traces schema lives in
  [`opentelemetry/proto/trace/v1/trace.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto),
  with shared value types in
  [`common/v1/common.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/common/v1/common.proto)
  and resource types in
  [`resource/v1/resource.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/resource/v1/resource.proto).
  When this document says "OTLP" it means that wire schema and the data model it defines
  (`ResourceSpans`, `ScopeSpans`, `Span`, `AnyValue`, etc.).
- **OpenTelemetry**: the broader project under which OTLP is one component. References in this RFC
  to "OpenTelemetry" name the project's non-wire artefacts: the
  [specification](https://github.com/open-telemetry/opentelemetry-specification) and the
  [semantic conventions](https://github.com/open-telemetry/semantic-conventions) (the registry of
  attribute keys such as `service.name` and `http.request.method`).
- **Datadog APM trace format**: Vector targets exactly one hop in the Datadog tracing pipeline --
  the agent-to-backend protobuf served at `/api/v0.2/traces`. When this RFC says "Datadog"
  unqualified, it means that format. The schema lives in three protobuf files in the Datadog
  Agent repository:
  - [`agent_payload.proto`](https://github.com/DataDog/datadog-agent/blob/main/pkg/proto/datadog/trace/agent_payload.proto)
    -- `AgentPayload` (`tracerPayloads[]`, agent-level `tags`, `agentVersion`, `targetTPS`,
    `errorTPS`).
  - [`tracer_payload.proto`](https://github.com/DataDog/datadog-agent/blob/main/pkg/proto/datadog/trace/tracer_payload.proto)
    -- `TracerPayload` (`chunks[]`, tracer-level fields) and `TraceChunk`
    (`priority`/`origin`/`droppedTrace`/`tags`, `spans[]`).
  - [`span.proto`](https://github.com/DataDog/datadog-agent/blob/main/pkg/proto/datadog/trace/span.proto)
    -- the per-span shape (`service`, `name`, `resource`, `traceID`, `spanID`, `parentID`,
    `start`, `duration`, `error`, `meta`, `metrics`, `type`, `meta_struct`).

  The [Datadog Agent's OTLP ingest](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go)
  is cited as the reference implementation for OTLP-to-Datadog field mappings adopted here.
- **Datadog tracer-to-agent API** (informational): tracer SDKs send traces to the Datadog Agent
  over a separate set of HTTP endpoints (`/v0.3/traces`, `/v0.4/traces`, `/v0.5/traces`,
  `/v0.7/traces`) using JSON, msgpack, or protobuf. These are upstream of the agent-to-backend
  hop Vector consumes; the public guide
  [Send traces to the Agent by API](https://docs.datadoghq.com/tracing/guide/send_traces_to_agent_by_api/)
  documents the legacy v0.3 JSON shape and is cited only as a reference for per-span field
  semantics. Vector does not consume these endpoints directly.
- **W3C Trace Context** (informational): the W3C recommendation defining the
  [`traceparent` and `tracestate` HTTP headers](https://www.w3.org/TR/trace-context/). The
  proposed `TraceFlags` and `TraceState` types correspond to these headers; the size bounds
  quoted in the `TraceState` rationale (32 entries, 512 bytes total) come from this spec.
- **Zipkin v2, Jaeger, OpenTracing** (informational): other trace data models referenced in passing
  for context. None are targeted by this RFC and they are not constraints on the design. Zipkin v2
  is documented at the [Zipkin API](https://zipkin.io/zipkin-api/#/default/get_spans); Jaeger at
  [jaegertracing.io](https://www.jaegertracing.io/docs/latest/architecture/#span); OpenTracing at
  the [OpenTracing spec](https://github.com/opentracing/specification/blob/master/specification.md).

## Cross cutting concerns

- First-class OpenTelemetry signal support
  ([vectordotdev/vector#1444](https://github.com/vectordotdev/vector/issues/1444)).
- APM stats aggregation in the `datadog_traces` sink, today reading magic keys from `TraceEvent`,
  will read typed fields after this RFC lands.
- VRL trace-specific semantics on the new typed surface (`.resource.service`, `.chunk.priority`,
  `.spans[i].name`, etc.).

## Scope

### In scope

- Define `TraceEvent` as an array of spans plus supporting resource data, replacing the current
  `TraceEvent(LogEvent)`.
- Specify the bidirectional mapping between `TraceEvent` and the OTLP wire format.
- Specify the bidirectional mapping between `TraceEvent` and the Datadog agent-to-backend protobuf.
- Guarantee effective-equivalence round-trip through Vector for both formats when the pipeline
  does not otherwise mutate the data: `OTLP -> Vector -> OTLP` and `Datadog -> Vector -> Datadog`
  produce output that the backend ingests as the same data as the original. Effective equivalence
  means backend-observable identity, not byte-level identity; details the backend does not observe
  (e.g. span order within a chunk, specific chunk grouping) may differ. The OTLP mapping targets
  fields at OpenTelemetry's `Stable` stability tier or higher; fields marked
  [`Development`](https://opentelemetry.io/docs/specs/otel/versioning-and-stability/) or `Alpha`
  in the upstream proto are out of scope and are revisited when they stabilize (see "Zero-loss
  round-trip exclusions" for the current list and "Future Improvements" for the adoption path).

### Out of scope

- VRL function additions for trace-specific operations (e.g. `decode_trace_state`).
- New trace sources/sinks (Zipkin, Jaeger, etc.).
- APM stats computation semantics (already covered by RFC 9862).
- Zero-loss cross-format round-trip (`Datadog -> OTLP -> Datadog`,
  `OTLP -> Datadog -> OTLP`). Cross-format conversion is supported as a one-way operation;
  best-effort encoding under reserved keys is described in the mapping sections.
- `TracerPayload.containerDebug` (Datadog-internal container-tag-resolution diagnostic).
  Dropped on ingest; not synthesized on egress.

### Zero-loss round-trip exclusions

The effective-equivalence guarantee does not cover the following input shapes. In each case the
Rationale and/or Attributes sections explain the constraint and the tradeoffs considered.

**OTLP `AnyValue.string_value` / `AnyValue.bytes_value` oneof discriminator.** Both variants
collapse to `Value::Bytes` in the typed model. OTLP egress recovers the discriminator by
inspecting the payload: valid UTF-8 emits as `string_value`, otherwise as `bytes_value`. A
`bytes_value` whose payload is valid UTF-8 therefore egresses as `string_value` with the same
bytes; every other input shape round-trips exactly. See "Attributes" for details.

**NaN doubles: OTLP `AnyValue.double_value`, Datadog `Span.metrics`, Datadog
`SpanEvent.attributes` `DOUBLE_VALUE`, and Datadog `AgentPayload.targetTPS` /
`AgentPayload.errorTPS`.** `Value::Float` is backed by `NotNan<f64>` and cannot represent NaN.
An OTLP `AnyValue.double_value = NaN` ingests as `Value::Null` -- the same representation as
an empty `AnyValue` -- so egress cannot distinguish the two and emits an unset `AnyValue`
rather than the original `double_value = NaN`. A Datadog `metrics` entry whose value is NaN
also ingests as `Value::Null`; on Datadog egress `Value::Null` is stringified into `meta` as
the literal `"null"`, so the entry changes both partition (`metrics` → `meta`) and value (NaN
double → the string `"null"`). A Datadog `SpanEvent.attributes` entry with
`DOUBLE_VALUE = NaN` likewise ingests as `Value::Null`; on Datadog egress it is stringified to
`STRING_VALUE = "null"`, changing both the type tag and the value. The same applies to a NaN
double inside an `ARRAY_VALUE` element: the affected element ingests as `Value::Null` and
egresses as `STRING_VALUE = "null"`. A Datadog `AgentPayload.targetTPS` or
`AgentPayload.errorTPS` whose value is NaN ingests as `Value::Null` at the matching
`_dd.payload` sub-key (`target_tps` / `error_tps`); on Datadog egress the typed `f64` slot
cannot accept `Value::Null` and the field is emitted as the proto3 default (`0.0`) rather than
the original NaN. See "Attributes", "Datadog attribute partitions", "Datadog resource-scoped
state", and [^dd-se] for details.

**Deprecated `deployment.environment` resource attribute key.** Both
`deployment.environment.name` (current) and `deployment.environment` (deprecated) are accepted on
OTLP ingress and populate `Resource.environment`. Egress emits the slot only as
`deployment.environment.name`. Consequently: a pre-stabilization producer's
`deployment.environment` key is rewritten to `deployment.environment.name` (value bytes
unchanged), and when both keys are present with different values the deprecated key's value is
dropped. See [^otlp-env] and "Per-type design choices" for details.

**OTLP resource attributes keyed `_dd.payload` or `_dd.tracer`.** These two keys are reserved
for Datadog envelope storage. An OTLP producer or transform that sets either key produces a
value that is interpreted as a Datadog envelope on Datadog egress, and rewritten to
`datadog.payload` / `datadog.tracer.tags` rather than the original key on OTLP egress. No
stable OpenTelemetry semantic-convention attribute uses a `_dd.*`-prefixed key; operators
should avoid setting these two keys outside of Datadog-sourced pipelines. See "Datadog
resource-scoped state" for details.

**OTLP fields at `Development` or `Alpha` stability tier.** Per the Scope statement, the mapping
targets `Stable`-or-higher fields. Such fields are dropped on OTLP ingress. They are revisited for
typed-surface support when upstream marks them `Stable`; see "Future Improvements".

**Datadog `Span.duration` negative values.** `std::time::Duration` is non-negative; a Datadog
span with a negative `int64` duration is clamped to zero on ingress. See "Per-type design
choices" for details.

**OTLP `Span.end_time_unix_nano < start_time_unix_nano`.** OTLP carries timing as two unsigned
`fixed64` nanoseconds-since-epoch fields whose ordering is not constrained by the wire schema;
the typed model derives `duration = end − start` and stores it as the non-negative
`std::time::Duration`. A span whose end timestamp precedes its start timestamp would yield a
negative difference that `Duration` cannot represent and is clamped to zero on ingress. On OTLP
egress `end_time_unix_nano = start_time_unix_nano + duration.as_nanos()` is therefore equal to
the original `start_time_unix_nano`, losing the reversed ordering. See "Per-type design
choices" for details.

**Datadog `meta` / `metrics` producer-side disjointness.** The round-trip guarantee for the
Datadog path is conditional on `meta` and `metrics` being keyset-disjoint in the producer's
output. See "Datadog attribute partitions" for details.

**Datadog `Span.error` values other than `0` or `1`.** The wire field is `int32` but its
documented semantics are bivalent (`0` = not errored, non-zero = errored). The typed
`SpanStatus` carrier captures the bivalent meaning; the specific integer is not preserved on
egress. A span with `error != 0` ingests as `SpanStatus::Error(...)` (so the bivalent meaning
round-trips correctly) and egresses with `Span.error = 1`, normalizing any non-conformant
non-`0`/`1` input to the conforming bivalent representation. See [^dd-err] for details.

**Datadog `meta` or `metrics` keyed literally `_dd.meta_struct`.** This key is reserved at the
top level of `Span.attributes` for the `meta_struct` partition's sub-object. A producer that
emits the literal key `_dd.meta_struct` in `meta` or `metrics` collides with that reservation:
the scalar entry and any `meta_struct` entries for the same span both target
`Span.attributes."_dd.meta_struct"`, so one side overwrites or is reclassified on egress.
Operators should avoid setting this key via transforms outside of Datadog-sourced pipelines.
See "Datadog attribute partitions" for details.

## Pain

- Transforms written against today's `TraceEvent` depend on the exact key layout the ingesting
  source produced. A remap that works for `datadog_agent` traces does not work for OTLP traces,
  even when the semantic intent is identical. This is the opposite of how `Metric` behaves and
  is the primary blocker to useful trace transforms.
- Cross-format routing (e.g. `opentelemetry` source -> `datadog_traces` sink) requires bespoke
  translation reading undocumented magic keys. Each new sink duplicates this work.
- `TraceEvent` currently corrupts numeric ID precision (`trace_id as i64` in both the
  `datadog_agent` source and the `datadog_traces` sink, see
  [#14687](https://github.com/vectordotdev/vector/issues/14687)). A typed model fixes this by
  construction.
- VRL programs authoring spans without typed events, links, or status can produce structurally
  invalid output that is only discovered at sink encoding time.

## Proposal

### User Experience

A `TraceEvent` carries one `Resource`, one `Scope`, one `ChunkContext`, and a `Vec<Span>`. VRL
accesses these directly:

```coffee
# Route by resource service.
if .resource.service == "checkout" { ... }

# Read a Datadog chunk-scoped tag (no-op for OTLP-sourced events).
.decision_maker = .chunk.tags."_dd.p.dm"

# Filter health-check spans across the whole event.
.spans = filter(.spans, |_, span| { span.name != "GET /health" })

# Mark slow DB spans as errors.
.spans = map_values(.spans, |span| {
    if span.span_type == "db" && span.duration > 1.0 {
        span.status.code = "error"
        span.status.message = "slow query"
    }
    span
})

# Read a semantic-convention attribute on the root span, falling back to a
# Datadog-native key.
.user_id = .spans[0].attributes."user.id" ?? .spans[0].attributes."usr.id"
```

Datadog's two scalar span-level partitions (`meta`, `metrics`) are merged into
`Span.attributes`; the byte-valued `meta_struct` partition is preserved under the reserved key
`.spans[i].attributes."_dd.meta_struct"`; agent-payload- and tracer-payload-scoped state live
under `.resource.attributes."_dd.payload"` and `.resource.attributes."_dd.tracer"`; and
chunk-scoped state lives on `.chunk`. Each is specified under its own subsection below.

The `trace_to_log` transform is retained, but its output shape changes from a source-defined
key layout to a uniform, source-independent one; the migration guide provides a
field-by-field mapping.

### Implementation

#### `TraceEvent`

```rust
pub struct TraceEvent {
    resource: Resource,
    scope:    Scope,
    /// Datadog-only chunk-scoped state. Default-empty when the event is
    /// OTLP-sourced.
    chunk:    ChunkContext,
    /// Spans belonging to this resource/scope/chunk grouping.
    spans:    Vec<Span>,
    metadata: EventMetadata,
}
```

Each `TraceEvent` corresponds to one wire-level grouping:

- OTLP: one `ScopeSpans` (with its enclosing `ResourceSpans` providing `Resource`).
- Datadog: one `(TracerPayload, distinct Span.service, TraceChunk)` triple.

#### `Span`

```rust
pub struct Span {
    pub trace_id:       TraceId,
    pub span_id:        SpanId,
    pub parent_span_id: Option<SpanId>,
    pub trace_state:    TraceState,
    pub flags:          TraceFlags,

    pub name:           KeyString,
    pub kind:           SpanKind,

    pub start_time:     DateTime<Utc>,
    /// Span duration with nanosecond precision. The VRL surface exposes
    /// `.spans[i].duration` as float seconds.
    pub duration:       Duration,
    pub status:         SpanStatus,

    /// Datadog-native, no OTLP equivalent: human-readable identifier of the
    /// resource being traced (URL, handler, SQL statement).
    pub resource_name:  Option<KeyString>,

    /// Datadog-native, no OTLP equivalent: free-form span type
    /// (web, db, cache, http, ...).
    pub span_type:      Option<KeyString>,

    /// Per-span attribute map. On OTLP ingest this is `Span.attributes`
    /// verbatim; on Datadog ingest it is the union of the wire-level
    /// `meta` and `metrics` maps distinguished by `Value` variant. The
    /// wire-level `meta_struct` map is preserved under the reserved key
    /// `attributes."_dd.meta_struct"` (see "Datadog attribute partitions").
    pub attributes:     Attributes,

    pub events:         Vec<SpanEvent>,
    pub links:          Vec<SpanLink>,

    pub dropped_attributes_count: u32,
    pub dropped_events_count:     u32,
    pub dropped_links_count:      u32,
}
```

#### `Resource` and `Scope`

```rust
pub struct Resource {
    pub service:     Option<KeyString>,   // service.name
    pub environment: Option<KeyString>,   // deployment.environment.name (and deprecated deployment.environment on ingress)
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

#### Identifiers

```rust
pub struct TraceId(NonZeroU128);
pub struct SpanId(NonZeroU64);

impl TraceId {
    /// Low 64 bits, used as Datadog's `trace_id`.
    pub fn low_u64(self)  -> u64 { self.0.get() as u64 }
    /// High 64 bits, stored in Datadog's `_dd.p.tid` meta tag.
    pub fn high_u64(self) -> u64 { (self.0.get() >> 64) as u64 }
}
```

Conversions to and from `u128`/`u64` and OTLP's 16/8-byte big-endian representations are provided
as cheap copies via `From` (when the source is statically non-zero) and `TryFrom` (otherwise).

#### Status, kind, chunk context

```rust
pub enum SpanKind {
    Unspecified,
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
    /// Unrecognized enum number from a newer OpenTelemetry version. Stored verbatim
    /// so an OTLP -> Vector -> OTLP relay emits the original wire value unchanged.
    Other(i32),
}

pub enum SpanStatus {
    Unset,
    Ok,
    Error(String),
    /// Unrecognized status code from a newer OpenTelemetry version. The raw
    /// code integer and any status message are stored verbatim so an
    /// OTLP -> Vector -> OTLP relay emits the original wire values unchanged.
    Other(i32, String),
}

/// Datadog `TraceChunk`-scoped state. Default-empty for OTLP-sourced events.
pub struct ChunkContext {
    pub priority: Option<SamplingPriority>,
    pub origin:   Option<KeyString>,
    pub dropped:  bool, /// `TraceChunk.droppedTrace`
    pub tags:     Attributes,
}

pub enum SamplingPriority {
    UserReject, // -1
    AutoReject, //  0
    AutoKeep,   //  1
    UserKeep,   //  2
    /// Out-of-range value. Datadog tracing libraries may uncommonly emit these.
    Other(i32),
}
```

#### `TraceFlags` and `TraceState`

`TraceFlags` is the OTLP `Span.flags` / `Link.flags` bitfield: a 32-bit word whose low byte is
the W3C trace-flags byte and whose remaining bits carry OTLP- and Datadog-defined context
information. Sources construct via `TraceFlags::from_bits_retain(word)` and sinks read the
raw value via `flags.bits()`, so unknown bits (including OTLP's reserved bits 10-31) round-
trip unchanged. The W3C trace-flags byte is exposed as a derived view via `flags.w3c_byte()`
for emission in `traceparent` headers and similar W3C-only surfaces.

```rust
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TraceFlags: u32 {
        /// W3C `sampled` flag (bit 0).
        const SAMPLED               = 0x0000_0001;
        /// OTLP `SPAN_FLAGS_CONTEXT_HAS_IS_REMOTE_MASK` (bit 8): when set,
        /// `CONTEXT_IS_REMOTE` carries a known value; when clear, parent-
        /// (or link-target-) remoteness is unspecified.
        const CONTEXT_HAS_IS_REMOTE = 0x0000_0100;
        /// OTLP `SPAN_FLAGS_CONTEXT_IS_REMOTE_MASK` (bit 9): meaningful
        /// only when `CONTEXT_HAS_IS_REMOTE` is also set.
        const CONTEXT_IS_REMOTE     = 0x0000_0200;
    }
}

impl TraceFlags {
    /// View of the W3C trace-flags byte (bits 0-7), suitable for emission
    /// in a `traceparent` header.
    pub fn w3c_byte(self) -> u8 { self.bits() as u8 }

    /// Tristate view of OTLP's parent/link-target remoteness:
    /// `None` when `CONTEXT_HAS_IS_REMOTE` is clear, `Some(bool)` otherwise.
    pub fn context_is_remote(self) -> Option<bool> {
        self.contains(Self::CONTEXT_HAS_IS_REMOTE)
            .then(|| self.contains(Self::CONTEXT_IS_REMOTE))
    }
}
```

`TraceState` stores the W3C `tracestate` header verbatim and exposes map-like accessors that
parse on demand. Sources copy the header in unchanged; sinks emit it unchanged unless a
transform mutated it.

```rust
#[derive(Clone, Default, Eq, PartialEq)]
pub struct TraceState(String);

impl TraceState {
    pub fn from_raw(s: impl Into<String>)          -> Self;
    pub fn as_str(&self)                           -> &str;
    pub fn is_empty(&self)                         -> bool;

    pub fn get(&self, key: &str)                   -> Option<&str>;
    pub fn insert(&mut self, key: &str, val: &str);
    pub fn remove(&mut self, key: &str)            -> bool;
    pub fn iter(&self)                             -> impl Iterator<Item = (&str, &str)> + '_;
}
```

`insert` rewrites the underlying string in-place, preserving entry order and inserting new
entries at the head, per the W3C spec.

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

A newtype over `ObjectMap` (re-exported from `vrl::value`). `Value` carries `Bytes`, `Float`,
`Integer`, `Boolean`, `Timestamp`, `Array`, and `Object` variants for typed payloads, plus
`Value::Null` reserved for OTLP's empty-`AnyValue` case (see below); the `Regex` variant is
only used within VRL. UTF-8 strings and opaque byte payloads alike are stored as
`Value::Bytes`. Keys follow OpenTelemetry semantic conventions; snake_case is preferred for new
internally-generated keys.

OTLP's `AnyValue` distinguishes `string_value` (UTF-8 string) from `bytes_value` (opaque bytes) via
a oneof discriminator. Both variants store as `Value::Bytes` in this model. OTLP egress recovers the
discriminator by inspecting the payload: a `Value::Bytes` whose contents are valid UTF-8 is emitted
as `string_value`; otherwise it is emitted as `bytes_value`. The payload bytes pass through
unchanged in either branch. The rule applies recursively to nested byte-typed leaves. A
`bytes_value` whose payload is valid UTF-8 is the one input shape the rule cannot distinguish from a
genuine `string_value` and emerges on egress as `string_value` carrying the same bytes.

OTLP's `AnyValue` proto further permits the oneof to be unset, i.e. an `AnyValue` message with no
variant selected (and, equivalently for proto3, a `KeyValue` whose `value` field is absent). This
shape ingests as `Value::Null` and OTLP egress emits it as an `AnyValue` with no oneof variant set.
The mapping applies recursively, so empty entries within `ArrayValue.values` or empty values within
`KeyValueList.values` round-trip through `Value::Null` array elements and object members. Empty
`AnyValue` is consequently round-tripped unchanged.

`Value::Float` is backed by `NotNan<f64>` and cannot represent IEEE-754 NaN. An
`AnyValue.double_value` whose payload is NaN therefore also ingests as `Value::Null`, colliding
with the empty-`AnyValue` representation. OTLP egress cannot distinguish the two and emits
`Value::Null` as an unset `AnyValue` in both cases: an attribute whose original value was a NaN
double emerges on egress as an empty `AnyValue` rather than `double_value = NaN`.

Datadog attributes are always scalars on the wire, so the nested-value capability is unused on
the Datadog egress path; the Datadog sink stringifies any non-scalar value via JSON.

#### Datadog attribute partitions: convention versus invariant

Datadog spans carry attributes in three independent wire-level maps:

- `meta`: keys to UTF-8 strings.
- `metrics`: keys to IEEE-754 doubles.
- `meta_struct`: keys to opaque bytes (msgpack-encoded structured payloads).

Datadog ingress maps each partition into `Span.attributes`:

- `meta` entries become top-level entries with `Value::Bytes`.
- `metrics` entries become top-level entries with `Value::Float`. A NaN double cannot be stored
  in `Value::Float` (`NotNan<f64>`) and ingests as `Value::Null`.
- `meta_struct` entries are placed under the reserved key `Span.attributes."_dd.meta_struct"`,
  whose value is an `Object` mapping each `meta_struct` key to a `Value::Bytes` payload.

If a producer emits the same key in both `meta` and `metrics`, the Datadog source resolves the
collision deterministically (`metrics` wins) and emits a `DatadogAttributeCollision` internal
event that increments `component_errors_total` and writes a rate-limited `warn!` log. A key
emitted in `meta_struct` and either `meta` or `metrics` normally retains both values (the
`meta_struct` entry under `attributes."_dd.meta_struct"`, the scalar entry as a top-level
attribute) because the two surfaces target different keys: the `meta_struct` sub-object lives
at the reserved key `_dd.meta_struct`, and any other producer-supplied `meta` or `metrics` key
is necessarily distinct from that reserved name.

The single exception is a producer that emits the literal key `_dd.meta_struct` in `meta` or
`metrics`: the scalar entry and the `meta_struct` sub-object both target
`Span.attributes."_dd.meta_struct"` and one side overwrites the other. This case is declared
as an explicit round-trip exclusion under "Zero-loss round-trip exclusions".

Datadog egress, in order:

1. Drain `Span.attributes."_dd.meta_struct"` into the wire `meta_struct` map (each sub-entry's
   `Value::Bytes` payload becomes one `meta_struct` entry). A non-`Object` value at the
   reserved key, or a non-`Bytes` sub-entry within it, is stringified into `meta` under the
   same flattening rule below and emits a `DatadogMetaStructShape` internal event.
2. Partition the remaining attributes by `Value` variant: `Value::Bytes` to `meta`,
   `Value::Float` and `Value::Integer` (coerced to `f64`) to `metrics`. Variants with no
   native Datadog partition (`Value::Boolean`, `Value::Timestamp`, `Value::Array`,
   `Value::Object`, `Value::Null`) are stringified into `meta` -- the JSON encoding of the
   value for composite variants, the literal `null` for `Value::Null`.

The result is one entry per key in exactly one wire partition.

OTLP egress (cross-format): the top-level entries flow through OTLP egress identically to
OTLP-sourced attributes; their `Value` variants are valid OTLP `AnyValue` shapes. The reserved
`_dd.meta_struct` key is emitted as a nested `KvlistValue` of `BytesValue` entries.

#### Datadog resource-scoped state

Datadog's agent-payload and tracer-payload envelopes carry resource-scoped metadata that is
preserved as two reserved top-level entries in `Resource.attributes`:

| Wire scope                       | `Resource.attributes` key | Value shape |
| -------------------------------- | ------------------------- | ----------- |
| `AgentPayload` (whole message)   | `_dd.payload`             | `Object`    |
| `TracerPayload.tags`             | `_dd.tracer`              | `Object`    |

`_dd.payload` mirrors the wire `AgentPayload` envelope under sub-keys: `host_name`, `env`,
`agent_version`, `target_tps`, `error_tps`, `rare_sampler_enabled` (the scalar fields), and
`tags` -- a nested `Object` of the wire-level `AgentPayload.tags` map. The double-typed
`target_tps` and `error_tps` slots use `Value::Float`, and a NaN wire value ingests as
`Value::Null`; on Datadog egress that `Value::Null` is emitted as the proto3 default `0.0`
rather than the original NaN (see "Zero-loss round-trip exclusions"). `_dd.tracer` carries
only the wire-level `TracerPayload.tags` map; `TracerPayload.hostname` and `TracerPayload.env`
map to the typed `Resource.host`/`Resource.environment` fields directly (see "Datadog
mapping").

VRL access:

```coffee
.agent_host      = .resource.attributes."_dd.payload"."host_name"
.agent_apm_mode  = .resource.attributes."_dd.payload"."tags"."_dd.apm_mode"
.tracer_apm_mode = .resource.attributes."_dd.tracer"."_dd.apm_mode"
```

The two keys live under the `_dd.*` namespace alongside other Datadog-internal keys (`_dd.apm_mode`,
`_dd.tags.container`, `_dd.tags.process`, `_dd.p.dm`, `_dd.p.tid`, `_dd.error_tracking_*`,
`_dd.otel.gateway`).

OTLP egress (cross-format): neither sub-object has an OTLP-native home. The OTLP sink emits
them as best-effort `Resource.attributes` entries of `KvlistValue` type under the reserved
keys `datadog.payload` and `datadog.tracer.tags`.

#### Datadog chunk context

Datadog `TraceChunk.priority`, `origin`, `droppedTrace`, and `tags` apply uniformly to every
span in the chunk. Each `TraceEvent` corresponds to exactly one chunk by construction (see
"Datadog mapping"), so these fields live on `TraceEvent.chunk` directly. OTLP-sourced events
carry a default-empty `ChunkContext` (no Datadog wire concept).

VRL access:

```coffee
.priority       = .chunk.priority
.origin         = .chunk.origin
.dropped        = .chunk.dropped
.decision_maker = .chunk.tags."_dd.p.dm"
```

OTLP egress (cross-format): the four chunk fields are emitted as best-effort `Span.attributes`
entries on every span of the event under the reserved keys `datadog.chunk.priority`,
`datadog.chunk.origin`, `datadog.chunk.dropped`, and `datadog.chunk.tags` (`KvlistValue`).

#### OTLP mapping

Each OTLP `ScopeSpans` is one `TraceEvent`. The containing `ResourceSpans.resource` populates
`TraceEvent.resource`; the `ScopeSpans.scope` populates `TraceEvent.scope`; the spans inside
populate `TraceEvent.spans`; `TraceEvent.chunk` is default-empty.

| OTLP                                                               | Internal                                      |
| ------------------------------------------------------------------ | --------------------------------------------- |
| `ResourceSpans.resource.attributes["service.name"]`                | `Resource.service`                            |
| `ResourceSpans.resource.attributes["deployment.environment.name"]` | `Resource.environment` [^otlp-env]            |
| `ResourceSpans.resource.attributes["deployment.environment"]`      | `Resource.environment` (legacy fallback)      |
| `ResourceSpans.resource.attributes["host.name"]`                   | `Resource.host`                               |
| `ResourceSpans.resource.attributes` (others) [^otlp-promote]       | `Resource.attributes`                         |
| `ResourceSpans.schema_url`                                         | `Resource.schema_url`                         |
| `ScopeSpans.scope.*`                                               | `Scope.{name, version, …}`                    |
| `ScopeSpans.schema_url`                                            | `Scope.schema_url`                            |
| `Span.trace_id`, `Span.span_id`, `Span.parent_span_id`             | same (all-zero rejected per OTLP)             |
| `Span.trace_state`                                                 | `Span.trace_state` (verbatim)                 |
| `Span.flags`, `Link.flags` [^otlp-flags]                           | `Span.flags`, `SpanLink.flags` (full u32)     |
| `Span.name`, `Span.kind`                                           | `Span.name`, `Span.kind`                      |
| `Span.start_time_unix_nano`, `end_time_unix_nano` [^otlp-time]     | `Span.start_time`, `Span.duration` (ns-exact) |
| `Span.attributes`                                                  | `Span.attributes`                             |
| `Span.events`, `Span.links`                                        | `Span.events`, `Span.links`                   |
| `Span.status.{code,message}` [^otlp-status]                        | `Span.status.{code,message}`                  |
| `Span.dropped_*_count`                                             | `Span.dropped_*_count`                        |

[^otlp-promote]: Promotion to a typed `Resource` field is conditional on the attribute value
    being a `string_value`. If the `AnyValue` for any of these three keys is a non-string variant
    (e.g. `int_value`, `bytes_value`, `bool_value`, `array_value`, `kvlist_value`, or an unset
    oneof), the key is not promoted and remains in `Resource.attributes` under its original key,
    so OTLP egress emits it unchanged. A non-string `service.name`, `deployment.environment.name`,
    or `host.name` therefore round-trips exactly; the typed `Resource` slot is left empty
    (`None`). Producers that violate the semantic-convention string typing for these keys are
    uncommon but produce valid OTLP wire data, and this rule ensures they are not silently
    truncated.

[^otlp-env]: The OTLP source promotes whichever of the two keys is present; if both are present,
    `deployment.environment.name` wins and the duplicate value at `deployment.environment` is
    dropped. On OTLP egress, `Resource.environment` is emitted only as
    `deployment.environment.name`. The relay-path consequences (key rewrite from deprecated to
    stable, and divergent-value drop when both keys are present) are declared as OTLP
    partial-exclusion cases under "Scope". See "Per-type design choices" under Rationale for the
    spec history and cross-format motivation.

[^otlp-time]: Computed as `end_time_unix_nano − start_time_unix_nano` on ingress; reconstructed as `start_time_unix_nano + duration` on egress. Both are integer nanoseconds; the round trip is bit-exact for any span where `end_time_unix_nano >= start_time_unix_nano`. A span with `end_time_unix_nano < start_time_unix_nano` cannot be represented in the non-negative `std::time::Duration` carrier; it is clamped to zero on ingress and an `OtlpReversedTimestamps` internal event is emitted. The OTLP round-trip guarantee does not cover such spans; see "Round-trip exclusions".

[^otlp-flags]: OTLP defines `Span.flags` and `Link.flags` as `fixed32`, with bits 0-7 the W3C trace-flags byte, bits 8-9 the parent-/link-target-remote tristate (`CONTEXT_HAS_IS_REMOTE`, `CONTEXT_IS_REMOTE`), and bits 10-31 reserved. The full word is stored verbatim in `TraceFlags(u32)`, so all defined bits and any future spec additions round-trip unchanged.

[^otlp-status]: `Status.message` round-trips when `code = ERROR` (carried by `SpanStatus::Error(String)`) or when `code` is an unrecognized future value (carried by `SpanStatus::Other(i32, String)`). For `code = UNSET` or `OK` the message is dropped on ingest because the OpenTelemetry [Set Status](https://opentelemetry.io/docs/specs/otel/trace/api/#set-status) rule restricts `Description` to the `Error` code. See "`SpanStatus` as a closed enum" under Rationale.

On OTLP egress, `TraceEvent`s sharing a `Resource` are gathered into one `ResourceSpans`; each event
becomes one `ScopeSpans`. Datadog-native fields (`Span.resource_name`, `Span.span_type`,
`TraceEvent.chunk.*`, `Resource.attributes."_dd.payload"`, `Resource.attributes."_dd.tracer"`) have
no OTLP-native home; the OTLP sink emits them best-effort under the reserved keys documented in
"Datadog resource-scoped state" and "Datadog chunk context".

`Span.duration` converts to `end_time_unix_nano = start_time_unix_nano + duration.as_nanos()`
on egress. Both quantities are integer nanoseconds in memory and on the wire; the round trip
is bit-exact, with no rounding.

OTLP defines several "field absent" / "field default-valued" pairs as semantically equivalent
at the spec level, in which case the model represents both forms as the default value and
the round-trip preserves spec-defined semantic equivalence even when the wire bytes differ:

- `ResourceSpans.resource` (proto comment: "If this field is not set then no resource info
  is known") and `ScopeSpans.scope` (proto comment: "Semantically when InstrumentationScope
  isn't set, it is equivalent with an empty instrumentation scope name (unknown)") -- absent
  on the wire is spec-equivalent to a default-valued message. The model carries
  `TraceEvent.resource` and `TraceEvent.scope` as values rather than `Option`, and egress
  emits the field unconditionally.
- `Span.status` (proto comment: "Semantically when Status isn't set, it means span's status
  code is unset, i.e. assume STATUS_CODE_UNSET (code = 0)") -- the model represents this as
  `SpanStatus::Unset` and egress emits the corresponding zero-coded `Status`. A status code
  outside the three known values (`UNSET = 0`, `OK = 1`, `ERROR = 2`) ingests as
  `SpanStatus::Other(code, message)` and egresses as the same code and message, so unknown
  status codes introduced by future OpenTelemetry versions round-trip unchanged.
- `Span.kind` -- `SPAN_KIND_UNSPECIFIED = 0` is the proto3 default; absent and zero-valued
  are byte-identical anyway. A value outside the six known enum numbers ingests as
  `SpanKind::Other(n)` and egresses as the same integer, so unknown kind values introduced
  by future OpenTelemetry versions round-trip unchanged.

#### Datadog mapping

An `AgentPayload` expands into one `TraceEvent` per
`(TracerPayload, distinct Span.service, TraceChunk)` triple.[^dd-v0] The grouping rules are:

- Each `TraceChunk` becomes one `TraceEvent`. A chunk whose spans use more than one
  `Span.service` is split into one event per distinct service; egress re-coalesces such
  events back into a single chunk (see below).
- The enclosing `TracerPayload`'s metadata (`hostname`, `env`, `containerID`, `languageName`,
  `tracerVersion`, etc.) populates the event's `Resource`. Per-span `Span.service` populates
  `Resource.service`.
- The enclosing `AgentPayload`'s envelope (`hostName`, `env`, `agentVersion`, `targetTPS`,
  `errorTPS`, `rareSamplerEnabled`, and `tags`) populates `Resource.attributes."_dd.payload"`
  as a structured sub-object; `TracerPayload.tags` populates `Resource.attributes."_dd.tracer"`
  (see "Datadog resource-scoped state").
- `TraceChunk.{priority, origin, droppedTrace, tags}` populate `TraceEvent.chunk`.
- `Scope` is left default; Datadog has no scope concept.

| Datadog                                                       | Internal                                              |
| ------------------------------------------------------------- | ----------------------------------------------------- |
| `TracerPayload.hostname`                                      | `Resource.host`                                       |
| `TracerPayload.env`                                           | `Resource.environment`                                |
| `Span.service` (per span)                                     | `Resource.service` of the event holding the span      |
| `AgentPayload` envelope (whole message) [^dd-agent]           | `Resource.attributes."_dd.payload"`                   |
| `TracerPayload.tags`                                          | `Resource.attributes."_dd.tracer"`                    |
| `TraceChunk.{priority, origin, droppedTrace, tags}`           | `TraceEvent.chunk`                                    |
| `TracerPayload` non-host/env scalar fields [^dd-tp]           | `Resource.attributes` under defined keys (see note)   |
| `Span.traceID` (u64)                                          | `Span.trace_id.low_u64`                               |
| `Span.meta["_dd.p.tid"]` (hex u64) if present [^dd-tid]       | `Span.trace_id.high_u64`                              |
| `Span.spanID`, `Span.parentID`                                | `Span.span_id`, `Span.parent_span_id`                 |
| `Span.name`                                                   | `Span.name`                                           |
| `Span.resource`                                               | `Span.resource_name`                                  |
| `Span.type`                                                   | `Span.span_type`                                      |
| `Span.start`, `Span.duration` (ns int64) [^dd-duration]       | `Span.start_time`, `Span.duration` (ns-exact)         |
| `Span.error` and `Span.meta["error.message"]`                 | `Span.status` [^dd-err]                               |
| `Span.meta`                                                   | `Span.attributes` (`Value::Bytes`)                    |
| `Span.metrics`                                                | `Span.attributes` (`Value::Float`)                    |
| `Span.meta_struct`                                            | `Span.attributes."_dd.meta_struct"` (`Object<Bytes>`) |
| `Span.spanEvents[*].{time_unix_nano, name}`                   | `SpanEvent.{time, name}`                              |
| `Span.spanEvents[*].attributes` (`AttributeAnyValue`) [^dd-se]| `SpanEvent.attributes` (typed `Value` per variant)    |
| `Span.spanLinks[*].traceID` (u64)                             | `SpanLink.trace_id.low_u64` in `Span.links`           |
| `Span.spanLinks[*].traceID_high` (u64) [^dd-link-tid]         | `SpanLink.trace_id.high_u64`                          |
| `Span.spanLinks[*].spanID`                                    | `SpanLink.span_id`                                    |
| `Span.spanLinks[*].tracestate`                                | `SpanLink.trace_state` (verbatim)                     |
| `Span.spanLinks[*].flags` (u32) [^dd-link-flags]              | `SpanLink.flags` (full u32 verbatim)                  |
| `Span.spanLinks[*].attributes`                                | `SpanLink.attributes` (`Value::Bytes`)                |

[^dd-tp]: `TracerPayload` fields mapped to `Resource.attributes` under semantic convention keys:
    `containerID`, `languageName`, `languageVersion`, `tracerVersion`, `runtimeID`, `appVersion`.
    `TracerPayload.hostname` and `TracerPayload.env` map to typed `Resource.host`/
    `Resource.environment`. `TracerPayload.containerDebug` is a Datadog-internal diagnostic
    with no Vector consumer and is dropped on ingest (see "Out of scope").

[^dd-agent]: The `_dd.payload` sub-object preserves the full `AgentPayload` envelope -- the
    `hostName`, `env`, `agentVersion`, `targetTPS`, `errorTPS`, `rareSamplerEnabled` scalars
    plus the `tags` map -- under `host_name`/`env`/`agent_version`/`target_tps`/`error_tps`/
    `rare_sampler_enabled`/`tags` sub-keys. See "Datadog resource-scoped state".

[^dd-tid]: On ingress, `meta["_dd.p.tid"]` is consumed into `Span.trace_id.high_u64` and also
    retained as a regular string attribute so Datadog-sourced spans preserve the tag on a Datadog
    round-trip. On Datadog egress, `meta["_dd.p.tid"]` is derived from `Span.trace_id.high_u64()`:
    if non-zero it is formatted as a lowercase hex string and written into `meta`, overwriting any
    existing attribute value at that key; if zero it is omitted. This ensures OTLP-sourced 128-bit
    trace IDs are not silently truncated on Datadog egress even when the attribute was never
    populated by an ingesting source.

[^dd-duration]: `Span.duration` on the wire is `int64` nanoseconds. `std::time::Duration` is
    non-negative; a negative wire value is clamped to zero on ingress and a
    `DatadogNegativeDuration` internal event is emitted. The Datadog round-trip guarantee does
    not cover spans with negative `duration`; see "Round-trip exclusions".

[^dd-err]: `Span.error != 0` maps to `Error(meta["error.message"].cloned().unwrap_or_default())`,
    else `Unset`. The `error.*` meta entries also flow into `Span.attributes` per the meta
    merge rule, keeping `error.type`/`error.stack` accessible alongside the typed status.
    Datadog's wire `Span.error` is `int32`; values other than `0` and `1` are non-conformant
    with the field's documented bivalent semantics. Such values ingest as
    `SpanStatus::Error(...)` (so the bivalent meaning is preserved through the typed model)
    and egress as `Span.error = 1`, normalizing the specific integer to the conforming
    bivalent representation. Vector's pre-typed-model implementation preserves arbitrary
    `int32` values byte-exactly and pins that behaviour with a unit test
    (`src/sinks/datadog/traces/tests.rs` exercises `error = 404`); the typed model normalizes
    the same input to `Span.error = 1`, dropping the specific integer. This case is declared
    as a round-trip exclusion under "Zero-loss round-trip exclusions".

[^dd-link-tid]: Unlike `Span` itself -- whose proto carries only a 64-bit `traceID` and stores
    the high half out-of-band in `meta["_dd.p.tid"]` -- `SpanLink` carries the high 64 bits in
    a dedicated wire field, `traceID_high`. Combining `traceID` and `traceID_high` into the
    typed 128-bit `SpanLink.trace_id` on ingest, and splitting it back on egress, is required
    for the `Datadog -> Vector -> Datadog` round trip to preserve links to 128-bit trace IDs.
    A `traceID_high` of zero on the wire is equivalent to absent and yields a `SpanLink.trace_id`
    whose high half is zero; on egress, a zero high half is emitted as field-absent (or zero,
    which is byte-identical under proto3). The link-target `_dd.p.tid` is not consulted on either
    direction: links may reference a different trace than the enclosing span, and the wire field
    is the canonical carrier.

[^dd-link-flags]: Datadog's `SpanLink.flags` is `uint32`, and the Datadog convention is that
    bit 31 must be set whenever the field is meaningful (the proto comment: "If set, the high
    bit (bit 31) must be set"). Storing the full word in `TraceFlags(u32)` preserves both
    bit 31 and the W3C/OTLP-defined low bits so the round trip is bit-exact. Datadog `Span`
    itself has no flags field; OTLP-sourced `Span.flags` has no Datadog-`Span` wire home and
    is dropped on Datadog egress per the cross-format scope statement.

[^dd-se]: Datadog `SpanEvent.attributes` is `map<string, AttributeAnyValue>`, where
    `AttributeAnyValue` carries an explicit type tag (`STRING_VALUE`, `BOOL_VALUE`, `INT_VALUE`,
    `DOUBLE_VALUE`, `ARRAY_VALUE`). This is distinct from the flat `Span.meta`/`Span.metrics`
    partitions and maps directly to typed `Value` variants: `STRING_VALUE` → `Value::Bytes`,
    `BOOL_VALUE` → `Value::Boolean`, `INT_VALUE` → `Value::Integer`, `DOUBLE_VALUE` →
    `Value::Float` (NaN ingests as `Value::Null`; see "Round-trip exclusions"),
    `ARRAY_VALUE` → `Value::Array` of the corresponding scalar variants (NaN array elements
    likewise ingest as `Value::Null`). On Datadog egress, `SpanEvent.attributes` is
    reconstructed from `Value` variants using this same mapping, not the `meta`/`metrics`
    partitioning rule used for `Span.attributes`. `Value::Null` entries (whether from NaN
    doubles or from any other source) egress as `STRING_VALUE = "null"`.

[^dd-v0]: Vector's local protobuf
    ([`proto/vector/dd_trace.proto`](../proto/vector/dd_trace.proto)) carries two historical
    fields, `repeated APITrace traces = 3` and `repeated Span transactions = 4`, selected at
    runtime by `handle_dd_trace_payload` when `tracerPayloads` is empty. These fields were
    removed from the upstream Datadog `AgentPayload` more than five years ago and are not
    produced by any currently supported Datadog Agent. The typed model defines no mapping for
    them; ingest of `tracerPayloads`-empty payloads is removed as part of this RFC's
    implementation (see "Plan Of Attack"), and `proto/vector/dd_trace.proto` is replaced by
    direct use of the upstream `agent_payload.proto`/`tracer_payload.proto`/`span.proto`.

The precise OpenTelemetry semantic convention keys for tracer/runtime/app/agent metadata in
`Resource.attributes`, and the OTLP-`kind`-to-Datadog-`Span.type` derivation used on egress
when `Span.span_type` is absent, follow the [Datadog Agent's OTLP ingest
mapping](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go) and are
deferred to implementation PRs.

On Datadog egress, the sink:

- Sets each wire `Span.error = 1` if `Span.status` is `Error(_)`, else `0`. Datadog spans
  whose wire `Span.error` was not `0` or `1` lose the specific integer on round trip; see
  "Round-trip exclusions" and [^dd-err].
- Drains `Span.attributes."_dd.meta_struct"` into the wire `meta_struct` map and re-partitions
  the remaining attributes into `meta`/`metrics` by `Value` variant per "Datadog attribute
  partitions" above.
- Reconstructs each `SpanEvent.attributes` entry as an `AttributeAnyValue` from the `Value`
  variant (per [^dd-se]), not the `meta`/`metrics` partitioning rule. Non-scalar `Value`
  variants with no `AttributeAnyValue` equivalent (`Value::Null`, `Value::Timestamp`,
  `Value::Object`) are stringified into `STRING_VALUE`.
- Emits `Span.trace_id.low_u64()` as the wire `Span.traceID`. If `Span.trace_id.high_u64()` is
  non-zero, sets `meta["_dd.p.tid"]` to the hex-encoded high 64 bits, overwriting any
  existing attribute value at that key. If the high half is zero, `meta["_dd.p.tid"]` is
  omitted (or retained as-is if a transform explicitly set it, since it round-trips through
  `Span.attributes` as a normal string attribute). This ensures OTLP-sourced 128-bit trace IDs
  are not silently truncated to 64 bits on Datadog egress.
- For spans where `Span.status = Error(message)` and `Span.attributes."error.message"` is
  absent, sets `meta["error.message"] = message` so OTLP-sourced spans (whose status was
  populated from `Span.status.message` rather than `meta`) emit Datadog-conventional error
  detail. Spans whose `Span.attributes."error.message"` is already set retain that value
  unchanged.
- Gathers events sharing a non-service `Resource` into one `TracerPayload`, with each span's
  `Span.service` reconstructed from its event's `Resource.service`.
- Within each `TracerPayload`, groups spans across events by `(ChunkContext, trace_id)` and
  emits one `TraceChunk` per group. A multi-service wire chunk that was split into multiple
  events on ingest re-coalesces into one chunk on egress; a non-conforming multi-trace chunk
  produces one egress chunk per `trace_id`. Both shapes are equivalent to the input as
  observed by the Datadog backend (see "Scope").
- Groups `TracerPayload`s by their events' `Resource.attributes."_dd.payload"` envelope and
  emits one `AgentPayload` per group. Each `AgentPayload`'s `hostName`, `env`, `agentVersion`,
  `targetTPS`, `errorTPS`, `rareSamplerEnabled`, and `tags` are read from the matching
  `_dd.payload` sub-keys. Events with no `_dd.payload` envelope (e.g. OTLP-sourced or
  transform-synthesized) fall back to Vector configuration. Grouping on the full envelope
  preserves the partitioning Vector applies today, so two `TracerPayload`s coming from
  different agent hosts or envs cannot be coalesced into the same `AgentPayload` and relayed
  traffic stays attributed to its originating agent.

#### Retention of `TraceEvent` and `Event::Trace`

The `Event::Trace(TraceEvent)` variant on the outer `Event` enum is retained. Only the inner
representation changes:

```rust
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
    Trace(TraceEvent),
}
```

#### Migration: coexistence of `LogEvent` and typed representations

During the migration, `TraceEvent` is an enum:

```rust
pub enum TraceEvent {
    /// Pre-migration source output: an untyped `LogEvent` whose key layout is
    /// defined by the producing source. The producing source identifies its
    /// layout in `LogEvent.metadata().value()` under the reserved sub-key
    /// `vector.trace_legacy_layout` so the correct shim can be selected
    /// after fan-in, disk-buffer round-trips, or `vector` source/sink hops.
    Legacy(LogEvent),
    /// Post-migration typed container.
    Typed {
        resource: Resource,
        scope:    Scope,
        chunk:    ChunkContext,
        spans:    Vec<Span>,
        metadata: EventMetadata,
    },
}
```

Trace event-producing sources each set the reserved sub-key `vector.trace_legacy_layout` in
`EventMetadata.value` to a static string identifying themselves on every `Legacy` trace they emit.
`to_typed()` reads this sub-key to select the corresponding `Legacy -> Typed` shim. A `Legacy` event
whose hint is absent or maps to no registered shim returns an error and emits a rate-limited `warn!`
log.

The end-state `struct TraceEvent { resource, scope, chunk, spans, metadata }` shown above is
reached by deleting the `Legacy` arm once every component has migrated; the `Typed` arm's
fields become the struct's fields verbatim.

Both accessor families coexist on `TraceEvent` and dispatch on the variant:

- `metadata()` / `metadata_mut()` and finalizer methods return the inner `LogEvent`'s
  metadata when `Legacy`, and the typed `metadata` field when `Typed`. Callers see no
  behaviour change.
- The existing untyped accessors (`get(path)`, `insert(path, value)`, `as_map()`, etc.)
  are not forwarded on `TraceEvent` itself; they are accessible only by pattern-matching into
  the `Legacy(LogEvent)` arm directly. Any call site that invokes them through the `TraceEvent`
  type therefore fails to compile as soon as these forwarding methods are removed, making the
  migration of remaining consumers a compile-error-driven mechanical task rather than a
  runtime-failure audit.
- The new typed accessors operate on the `Typed` form only. Calling them on a `Legacy` variant
  panics with a clear diagnostic message; they are not implicit converters because returning a
  `&Resource` or `&[Span]` from a `&self` accessor would require either mutating `self` to
  materialize the typed value or returning an owned/`Cow` shape, neither of which fits the desired
  zero-overhead post-migration signature. The panic is the loud failure that the intake-convert
  pattern below avoids.
- Explicit `to_typed(&mut self)` rewrites a `Legacy` variant in place into `Typed` by reading the
  `vector.trace_legacy_layout` hint from `EventMetadata.value` and invoking the corresponding
  source-specific shim. Already-`Typed` events are a no-op. There is no symmetric `to_legacy`.

Per-component shims are unidirectional (`Legacy -> Typed` only). The `datadog_agent` source
ships with a shim that knows the source's `LogEvent` key layout and produces a typed
container; the OTLP source ships with the equivalent shim for its layout. Trace-aware sinks
and transforms call `to_typed()` at intake to convert `Legacy` events before any typed
accessor is invoked, and operate on the typed view from that point on. A trace-aware
component that forgets the intake step encounters the typed-accessor panic on its first
event in dev/test rather than failing silently in production.

After every source, sink, and transform has been migrated, the `Legacy` variant and the shims
are deleted, leaving only the typed struct.

#### Wire serialization

The migration enum has two variants in memory; Vector's internal event-protobuf
(`lib/vector-core/proto/event.proto`) needs corresponding wire shapes for both, since trace
events cross internal-wire boundaries through disk buffers and the `vector` source/sink. The
existing `Trace` message is renamed to `LegacyTrace` (keeping its field tags and sub-message
shape) and a new sibling `TypedTrace` message is added. The `EventWrapper.event` and
`EventArray.events` oneofs each gain a new variant for the typed shape:

```protobuf
message EventWrapper {
  oneof event {
    Log log = 1;
    Metric metric = 2;
    LegacyTrace legacy_trace = 3;  // was: Trace trace = 3
    TypedTrace typed_trace = 4;    // new
  }
}

message TypedTrace {
  Resource resource = 1;
  Scope scope = 2;
  ChunkContext chunk = 3;
  repeated Span spans = 4;
  Metadata metadata_full = 5;
}
```

Plus the typed sub-messages (`Resource`, `Scope`, `ChunkContext`, `Span`, `SpanEvent`,
`SpanLink`, identifier and attribute types). `EventArray.events` gains an analogous
`TypedTraceArray typed_traces` variant alongside the renamed `legacy_traces`.

The discriminator is the oneof tag, mirroring the in-memory `enum TraceEvent { Legacy, Typed }`
exactly: `Legacy` ↔ `legacy_trace`, `Typed` ↔ `typed_trace`. Encode dispatches on the variant;
decode picks the variant from the oneof tag. An older Vector receiving a `TypedTrace`-tagged
event sees an unknown oneof variant, decodes it as `event: None`, and surfaces a loud "unknown
event type" error rather than silently materialising an empty Legacy event. The renamed
`LegacyTrace` keeps field tag 3, so the wire encoding for legacy events is byte-identical to
today's `Trace` and older Vector instances continue to decode them correctly.

Removing the `Legacy` Rust variant does not immediately retire the proto: `LegacyTrace`,
`LegacyTraceArray`, and the `legacy_trace` oneof variant are first marked `deprecated = true`
for a release window so events written by older Vector instances -- disk buffers and inflight
`vector`-source streams -- continue to decode. After the window passes, the messages are
removed and field tag 3 is added to `reserved` in both oneofs so the tag cannot be silently
reused by a future field.

Verification of the encode/decode change covers both variants for byte-exact round-trip and
explicitly exercises the older-Vector decode case: a `TypedTrace`-encoded message decoded
against a proto schema in which the `typed_trace` variant is unknown must surface a loud
"unknown event type" error from the consumer of `EventWrapper`/`EventArray`, not a silent drop
or an empty-`Legacy` decode. This pins the failure-mode property that motivated the
sibling-variant design over an in-`Trace` field-presence discriminator.

## Rationale

### Architectural choices

- The container shape mirrors the wire-level batching of both OTLP and Datadog: each
  `TraceEvent` is one `(resource, scope, chunk)` grouping. Source ingest and sink egress are
  pure mechanical translations between the wire shape and the container.
- Sharing `Resource`/`Scope`/`ChunkContext` across sibling spans is structural (a struct
  field), not pointer-based (an `Arc`). Disk-buffer serialization preserves the sharing for
  free; no `Arc` reconstruction or read-side interning is needed.
- The shape a user sees is the same whether the event arrived via OTLP or from the Datadog
  Agent. Source-native attribute maps are preserved on the appropriate typed level; nothing is
  copied into a parallel "extensions" map. Transforms can be written once and applied
  uniformly.
- Typed fields let transforms be written once. `Metric` demonstrates this model in Vector's
  architecture; extending to traces gives them parity and unblocks RFC 11851.
- Keeping the outer `Event::Trace(TraceEvent)` variant unchanged minimises churn at every
  call site that dispatches on `Event` (topology, buffers, finalizers, etc.); only the inner
  representation changes.

### Per-type design choices

- `Resource` promotes only the three semantic-convention fields both wire formats agree on
  (`service.name`, `deployment.environment.name`, `host.name`); other resource attributes stay in
  `Resource.attributes` under standard semantic convention keys. Promoting more would force Vector
  to track upstream semantic convention evolution or ossify a stale subset; promoting fewer would
  force every cross-format transform to read source-specific keys for common metadata.
- The OTLP source accepts both `deployment.environment.name` and the deprecated
  `deployment.environment` as sources for `Resource.environment`. OpenTelemetry stabilized the
  attribute as `deployment.environment.name` in semantic conventions
  [v1.27.0](https://github.com/open-telemetry/semantic-conventions/releases/tag/v1.27.0) ([PR
  #3584](https://github.com/open-telemetry/semantic-conventions/pull/3584)), with the experimental
  `deployment.environment` listed under [deprecated
  attributes](https://opentelemetry.io/docs/specs/semconv/registry/attributes/deployment/#deployment-deprecated-attributes)
  as "Replaced by `deployment.environment.name`." Accepting only the new key would silently drop the
  value for any producer still on pre-stabilization conventions; accepting only the old key would
  silently drop it for any producer on the current conventions. Both matter because
  `Resource.environment` populates Datadog's `TracerPayload.env` on cross-format egress: an `OTLP ->
  Datadog` route that fails to recognize the producer's chosen key emits an empty
  `TracerPayload.env` and loses environment attribution at the Datadog backend. The collision rule
  (current key wins) and egress emission (current key only) are documented in [^otlp-env]. The
  consequence on the OTLP relay path -- a pre-stabilization producer's `deployment.environment`
  is rewritten to `deployment.environment.name` on egress with the value bytes preserved
  unchanged, and the deprecated value is dropped when both keys are present with different values
  -- is the accepted tradeoff against cross-format env attribution and is declared as an OTLP
  partial-exclusion case under "Scope". A bit-exact relay alternative would require either adding
  provenance state to `Resource` (recording which key form was the source) or moving the typed
  `Resource.environment` slot to a derived view over `Resource.attributes`; both pay substantive
  design and surface-area cost for one transitional attribute.
- Encoding `TraceId`/`SpanId` non-zero invariants in the type itself eliminates a class of
  malformed values by construction. OTLP defines all-zero IDs as invalid, and Datadog uses
  zero only as the "no parent" sentinel (already represented as `None`). Using unsigned
  integer types fixes the existing `i64`-coercion precision bug
  ([#14687](https://github.com/vectordotdev/vector/issues/14687)).
- `TraceFlags` is sized to the OTLP wire field (`u32`), not the W3C `traceparent` byte
  (`u8`), so an `OTLP -> Vector -> OTLP` relay round-trips the full `Span.flags` /
  `Link.flags` word: the W3C trace-flags byte (bits 0-7), OTLP's parent-/link-target-remote
  tristate (bits 8-9, `CONTEXT_HAS_IS_REMOTE` and `CONTEXT_IS_REMOTE`), and OTLP's reserved
  bits 10-31. The same width is needed for the Datadog round trip on the link path, where
  `SpanLink.flags` is also `uint32` and the Datadog convention reserves bit 31 as a
  "flags-are-meaningful" sentinel; a `u8` storage would clear that bit on every Datadog
  link. `bitflags::from_bits_retain` preserves the full word, so unknown bits (including
  forward-compat W3C additions such as the Level 2 `random` flag) round-trip without
  changing the type or its serialization. The W3C trace-flags byte is exposed as a derived
  view via `flags.w3c_byte()` for emission in `traceparent` headers, and the OTLP
  parent-remote tristate is exposed via `flags.context_is_remote() -> Option<bool>`.
- `Span.duration` is stored as `std::time::Duration` (nanosecond integer). Both wire formats
  carry non-negative durations natively (OTLP as the difference between two `fixed64` epoch
  nanoseconds, Datadog as a single `int64` nanoseconds field), and `Duration` matches that
  domain exactly. Two corner cases fall outside the carrier: a Datadog span with a negative
  `int64` `duration` (Datadog's wire type permits it), and an OTLP span with
  `end_time_unix_nano < start_time_unix_nano` (the wire schema does not order the two
  timestamps). Both shapes are clamped to zero on ingress; the Datadog case emits a
  `DatadogNegativeDuration` internal event and the OTLP case emits an `OtlpReversedTimestamps`
  internal event, so neither is silently discarded but neither can be preserved. The Datadog
  round-trip guarantee does not cover spans with negative `duration`, and the OTLP round-trip
  guarantee does not cover spans with reversed timestamps; both exclusions are declared under
  "Round-trip exclusions". The VRL surface exposes `.spans[i].duration` as float seconds for
  ergonomic comparisons.
- The `Attributes(ObjectMap)` newtype delegates typed storage to VRL's existing `Value`,
  which already covers the full set of types either wire format can transmit. The newtype
  exists so future invariants (key validation, size bounds) can be added without requiring a
  migration. Nested `Array`/`Object` values are supported because OTLP's `AnyValue` is
  recursive and the OpenTelemetry semantic conventions define attributes whose values are
  arrays of strings or structured records; flat-scalar storage would require lossy flattening
  on ingest and reconstruction on egress.
- The OTLP `AnyValue.string_value` / `AnyValue.bytes_value` discriminator collapses to a
  single `Value::Bytes` variant in storage, with OTLP egress recovering the tag by
  payload-inspection (UTF-8 emits as `string_value`, otherwise as `bytes_value`). The
  collapse is not a regression introduced by this RFC: today's `TraceEvent(LogEvent)`
  already stores both variants as `Value::Bytes` via VRL, so the proposed model matches
  existing behaviour, and the egress inspection rule strictly improves on the status quo
  for non-UTF-8 `bytes_value`. The residual case -- `bytes_value` whose payload is
  valid UTF-8 -- is acceptable because the variant's wire-level purpose is to carry
  payloads that aren't UTF-8 strings, no stable OpenTelemetry semantic-convention attribute
  exercises that case, and the value bytes themselves are still preserved bit-exact even
  when the rule produces a different wire tag than the input.

### Datadog-specific design choices

- The `meta`/`metrics` merge into `Span.attributes` relies on a producer-side disjointness
  convention rather than a wire-format invariant. The Datadog `Span` proto does not constrain
  keysets across the two scalar maps, but every examined Datadog SDK and the trace agent
  maintain disjointness by construction, and the two maps carry distinct value types
  (`Value::Bytes` versus `Value::Float`) even in the rare collision case. The model treats
  the keyset disjointness as a contract the Datadog source asserts. If the convention ever
  ceases to hold for production traffic, the contained fallbacks (`Value::Bag` variant or a
  separate `Span.datadog_attributes` field) are documented under "Alternatives".
- The `meta_struct` partition is preserved as a reserved sub-object
  (`Span.attributes."_dd.meta_struct"`) rather than merged into the flat attribute map,
  because its opaque-bytes values would be indistinguishable from UTF-8 string `meta` entries
  under `vrl::value::Value` (both materialise as `Value::Bytes`). The reserved-key form is
  preserved on the Datadog round-trip path without depending on any producer-side
  convention, and parallels the resource-level treatment of `AgentPayload.tags` and
  `TracerPayload.tags`.
- Agent-payload- and tracer-payload-scoped state are kept as separate sub-objects inside
  `Resource.attributes` rather than merged because the two scopes collide on known keys at
  both the tag-map level and the scalar level. The Datadog Agent's trace writer
  ([`pkg/trace/writer/trace.go`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/writer/trace.go))
  writes `_dd.apm_mode` into `AgentPayload.tags` from its own configuration, and the Agent's
  processing pipeline ([`pkg/trace/agent/agent.go`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/agent/agent.go))
  writes the same key into `TracerPayload.tags` from a span's `Meta`. The two values are
  semantically distinct (Agent's claimed mode versus tracer-reported mode) and appear in the
  same payload. The same collision class applies to the scalar fields:
  `AgentPayload.hostName`/`env` describe the collector and routinely differ from
  `TracerPayload.hostname`/`env` (which describe the application), and Vector's existing
  egress sink already partitions on the agent-level values to keep the two attribution
  domains distinct. The `_dd.payload` sub-object is structured to hold the full
  `AgentPayload` envelope (scalars plus `tags`) so egress can reconstruct that partitioning
  exactly; `_dd.tracer` carries only the tracer-tags map because the other tracer-payload
  fields have typed `Resource` slots.
- The Datadog egress chunk-grouping rule `(ChunkContext, trace_id)` relies on a producer-side
  convention parallel to the `meta`/`metrics` story: the `TraceChunk` proto
  describes a chunk as "a list of spans with the same trace ID", and Datadog producers honor
  this by construction. For the conforming case, multi-service chunks split on ingest re-
  coalesce into one egress chunk and single-service chunks pass through unchanged; for a
  non-conforming multi-trace chunk, egress emits one chunk per `trace_id`. Both shapes are
  effectively equivalent at the Datadog backend, since chunk grouping is an ingestion-time
  transport detail rather than a semantic primitive.

### Migration approach

- The migration uses an `enum TraceEvent { Legacy, Typed }` so each trace source, sink, and
  transform can migrate in its own PR while the rest of the system continues to operate
  against the representation it expects. See "Wholesale migration" under Alternatives for
  why a single atomic replacement was rejected.
- Per-component shims convert `Legacy -> Typed` only, never the reverse: a `Typed` event has
  no source provenance on which to base a back-conversion to a source-specific `LogEvent`
  shape. This forces the migration sequencing in the Plan of Attack -- trace-aware
  consumers (sinks, transforms, VRL programs) must accept `Typed` input before any source
  flips to emitting `Typed` natively. The untyped forwarding methods (`get(path)`, `as_map()`,
  etc.) are removed from `TraceEvent` before the source steps; every remaining call site then
  fails to compile, making the consumer migration a mechanical fix-the-build task rather than a
  runtime-failure audit.
- Shim selection is keyed on a reserved sub-key `vector.trace_legacy_layout` in
  `EventMetadata.value` set by the producing trace source. The `vector` metadata namespace is
  read-only to VRL programs (configured by `compile_vrl`), so transforms between source and sink
  cannot accidentally delete or overwrite the hint. The metadata `Value` is serialized with every
  event record and passes through fan-in, disk buffers, and `vector` source/sink hops unchanged
  (unlike `EventMetadata.source_type`, which the topology source pump rewrites on every emission
  and so cannot serve as the selector across a serialised hop). Conversion is invoked explicitly
  by `to_typed(&mut self)`; immutable typed accessors panic on `Legacy` rather than converting on
  demand, because returning typed references through a `&self` accessor would require either
  mutating `self` or returning owned/`Cow` shapes that would have to be torn out again
  post-migration. The convention lives only for the duration of the migration and disappears with
  the `Legacy` variant; no new struct field or wire-format extension is needed.

## Drawbacks

- Breaking change for VRL configurations against today's `TraceEvent` key layout. Users must
  migrate to typed paths.
- The `trace_to_log` transform's output also changes; downstream VRL programs against its
  output must update.
- Topology granularity is coarser than per-span: each event carries up to a chunk's worth of
  spans (typically tens to hundreds, larger in deep call trees). Buffer-size limits expressed
  in events bound span counts less directly than the previous `LogEvent`-per-span design.
- Per-span operations (filter, sample, mutate one span) require VRL iteration over `.spans`
  rather than per-event treatment. A topology-level expand-on-input/collapse-on-output shim
  could let single-span transforms operate unchanged; that mechanism is deferred to
  implementation.
- The Datadog round-trip guarantee depends on a producer-side keyset-disjointness convention
  between `meta` and `metrics`. Every Datadog SDK examined keeps the two scalar partitions
  disjoint by construction, and the value types differ (`Bytes` versus `Float`) even when a
  collision occurs; the model treats keyset disjointness as a contract the source asserts.
  The `meta_struct` partition is preserved exactly under a reserved sub-object and is not
  subject to this convention.
- The internal `event.proto` gains a new `TypedTrace` variant alongside the renamed `LegacyTrace`. A
  Vector instance running a release line older than the typed-trace work, receiving a
  `TypedTrace`-encoded event over `vector` source/sink, decodes it with `event: None` (the unknown
  oneof variant) and surfaces a loud "unknown event type" error rather than silently emitting an
  empty trace. `vector` source/sink chains spanning the typed migration must run a single release
  line; this is documented in the release notes alongside the VRL-path migration.
- Every trace source and sink must be rewritten to produce/consume the typed container. The
  Plan of Attack sequences this so each component migrates independently, but it is non-trivial
  work.

## Prior Art

- [OTLP traces protocol](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)
  -- the primary shape this RFC adopts. The container `TraceEvent` is structurally one
  `ScopeSpans` plus its `Resource` and the Datadog-only `ChunkContext`.
- [Datadog APM agent-to-backend
  protobuf](https://github.com/DataDog/datadog-agent/tree/main/pkg/proto/datadog/trace) -- the
  second native format Vector targets.
- [Datadog Agent OTLP
  ingest](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go) --
  reference implementation for the OTLP-to-Datadog field mappings adopted here.
- [2024-03-22-20170 draft](https://github.com/hdost/vector/blob/add-trace-data-model/rfcs/2024-03-22-20170-trace-data-model.md)
  -- an earlier draft that modelled the event as a `ResourceSpans` (batch of multiple
  scope/spans groupings). The current RFC adopts a similar container shape but at finer
  granularity (one event per `ScopeSpans` rather than per `ResourceSpans`).

## Alternatives

### OTLP-only schema with Datadog round-trip via import/export encoding

Adopt the OTLP wire schema unchanged as the internal model -- `TraceEvent` carries one
`Resource`, one `Scope`, and a `Vec<Span>`, with no Datadog-specific typed fields -- and achieve
`Datadog -> Vector -> Datadog` round-trip transparency through an import/export layer that encodes
every Datadog-specific concept under reserved attribute keys. This is the limit case of the
reserved-key pattern the proposal already applies to `_dd.payload`, `_dd.tracer`, and
`_dd.meta_struct`: extend it to chunk-scoped state, `Span.resource_name`, `Span.span_type`, and
`SamplingPriority`, and let one container shape carry both formats.

The appeal is OTLP's status as the de facto industry trace schema. A single canonical container
removes `TraceEvent.chunk`, the `SamplingPriority` enum, and the typed Datadog-native span fields
from the API surface, leaving only the OpenTelemetry-shaped `Resource`/`Scope`/`Span`. Cross-format
consumers see one schema. Future OTLP signals (logs, metrics) inherit the same approach with no
additional design.

Rejected because the encoding required to carry all Datadog-specific concepts under OTLP attributes
without data loss is not uniform with how OTLP-sourced data sits in the same attribute maps, and the
non-uniformity is observable to every transform on the typed surface:

- **Chunk-scoped state has no faithful per-span encoding.** `TraceChunk.{priority, origin,
  droppedTrace, tags}` apply uniformly to every span in the chunk; the only place to carry them
  under a pure-OTLP schema is on every `Span.attributes` map in the chunk. Per-span duplication
  encodes a structural invariant -- every span in a chunk shares the same value -- as an arithmetic
  coincidence that any single-span attribute mutation silently breaks, inflates the wire by a factor
  proportional to chunk size, and forces Datadog egress to recover the chunk grouping by attribute
  comparison rather than by container traversal. Promotion to `Resource.attributes` is not a
  workaround: a Datadog `TracerPayload` may contain multiple chunks against the same resource, so
  the resource grouping does not coincide with the chunk grouping. The proposed `TraceEvent.chunk`
  field reflects the structural fact directly; the encoding is one slot per chunk-scoped value
  rather than `N spans × one entry per chunk-scoped value`.
- **Datadog-native span fields lose typed access.** `Span.resource_name` and `Span.span_type` are
  core inputs to Datadog routing and APM stats aggregation. Encoding them as
  `Span.attributes."_dd.span.resource"` / `"_dd.span.type"` is mechanically lossless but forces
  every Datadog-aware transform, sink, and VRL program to read them as string-keyed attribute
  lookups rather than typed accessors. The same loss applies to `SamplingPriority`: typed as an enum
  with an `Other(i32)` escape hatch in the proposal, it degrades to a string-encoded integer under
  the alternative, surrendering both the well-known-values ergonomic and construction-time
  validation.
- **Reserved-key partitioning becomes a per-span cost.** The proposal's reserved-key pattern is
  contained to two locations -- `Resource.attributes` (`_dd.payload`, `_dd.tracer`) and
  `Span.attributes` (`_dd.meta_struct`) -- where no typed home exists. A pure-OTLP design extends
  the pattern to every Datadog concept, so every transform walking `Span.attributes` must partition
  the map into user attributes and Datadog wire-state encoding to avoid mishandling either, and
  every sink must do the same on egress. The proposal's typed fields make the partition once at the
  type level.
- **The round-trip guarantee weakens from structural to conventional.** The proposal's
  `Datadog -> Vector -> Datadog` guarantee rests on structural identity: `TraceEvent.chunk` is read
  back into one `TraceChunk` per event by container traversal. Under the alternative, the guarantee
  rests on every transform respecting the reserved-key convention; any transform that drops
  `_dd.chunk.priority` from a span's attributes silently loses the chunk's sampling priority on
  egress. Today's `TraceEvent(LogEvent)` exhibits the same convention-dependent failure mode and is
  part of why this RFC exists.

The proposal already adopts OTLP as the primary shape: `Resource`, `Scope`, and `Span` are OTLP
types, semantic conventions name the typed resource fields, attribute keys follow OpenTelemetry
naming, and the Datadog mapping is expressed as projections onto that primary shape. The minimal
Datadog-specific delta (`TraceEvent.chunk`, `Span.resource_name`, `Span.span_type`,
`SamplingPriority`) is the smallest set of extensions that keeps Datadog-trace concepts on the
typed surface and chunk-scoped state structurally distinct from per-span state. The pure-OTLP
alternative trades that delta for a uniform type signature, paying the cost on every consumer of
the surface in exchange for a single-schema invariant at the type-definition site.

### One span per event (`TraceEvent { span: Span, metadata }`)

An earlier draft of this RFC carried a single span per event. This, however, requires the
`Resource`, `Scope`, and `ChunkContext` to either be duplicated for each span or to be shared via
`Arc`. Rejected because Vector's disk buffers serialize each event as one record: `Arc` sharing
collapses on serialization, every span on disk gets a full inline copy of resource/scope/chunk, and
on read every span gets an independent allocation, thus costing Vector both extra costs in
serialization and deserialization as well as the associated memory expansion and sink-level
reassembly mechanics. The container shape eliminates the inflation by aligning the event boundary
with the wire-batching boundary, so the shared context appears once per grouping on disk and in
memory regardless of how the path is buffered, and `Arc` machinery is not needed.

The per-span shape did offer two ergonomic advantages: the internal memory usage of a single span
(with the resources shared) is more consistent and granular, and per-span operations (filter,
sample, mutate one span) work directly without iteration. Recovering the latter in the container
shape is a topology-level shim concern, deferred to implementation.

### Parallel `Event` variants for new and old trace formats

Introduce `Event::NewTrace` alongside `Event::Trace`, leaving the existing `TraceEvent` untouched.
Rejected because it splits trace handling across two `Event` variants for the duration of the
migration, forcing every topology-level dispatch site to handle both. The tagged-inner approach
contains the duality inside `TraceEvent`, leaving `Event::Trace` as the single dispatch arm.

### Discriminated union (`Trace::{Otel, Datadog}` or `Span::{Otel, Datadog})

Carry each format as-is and dispatch at every consumer. Rejected because it directly inverts the
stated pain -- every transform and every cross-format sink would handle two shapes with the
possibility of more later. This is effectively the status quo over `LogEvent` just with predefined
fields.

### Separate `Span.datadog_attributes` field preserving the three wire partitions verbatim

Carry a `DatadogAttributes { meta, metrics, meta_struct }` field on `Span` alongside the
canonical `attributes`, populated only on Datadog ingest. This represents the wire format
exactly and preserves any cross-partition collision between `meta` and `metrics`. Rejected
because it splits the attribute surface in two, forces every attribute-aware component to
handle both, and is paid against a `meta`/`metrics` collision case no examined Datadog SDK or
agent emits. Listed as the contained mechanical fallback if the producer-side disjointness
convention ever ceases to hold for production traffic; the change is local to `Span`, the
Datadog source, the Datadog sink, and a unified read helper, with no impact on the OTLP side.
The `meta_struct` partition is already preserved exactly under
`Span.attributes."_dd.meta_struct"` in the proposal and does not motivate this alternative.

### `Value::Bag` variant for cross-partition collisions

Add a `Value::Bag(SmallVec<[Value; 2]>)` variant carrying multiple values per key. Rejected
for the same reason as the separate-field alternative; further, more intrusive because `Value`
is shared across `LogEvent`, `Metric`, and `Span`.

### Namespace-prefixed unified map for span partitions

Encode Datadog's two scalar span-attribute partitions inside `Span.attributes` itself by
prefixing each key with its partition name (`dd.meta.<k>`, `dd.metrics.<k>`), with
`meta_struct` similarly flattened under `dd.meta_struct.<k>`. Rejected because the prefixes
leak Datadog-specific encoding into every transform regardless of source: an OTLP-only
pipeline has to know about the namespace to avoid colliding with it, and an OTLP-sourced
attribute that happens to use a `dd.meta.*` key is silently misclassified on egress. The
`Value`-variant routing for `meta`/`metrics` and the reserved-sub-object form for
`meta_struct` achieve the same egress mapping without imposing any naming constraint on the
flat attribute namespace.

### Bare top-level resource scope keys

Use `payload` and `tracer` as the two reserved top-level keys in `Resource.attributes`
instead of the namespaced `_dd.payload` / `_dd.tracer` adopted in the proposal. The contents
are identical in both forms. Rejected because the bare names are plausible attribute keys
that legitimate OTLP-sourced or transform-generated resource attributes can already use:
OpenTelemetry semantic conventions are uniformly dotted, but user-set resource attributes
(via `OTEL_RESOURCE_ATTRIBUTES=payload=...`), transform-generated attributes
(`.resource.attributes.payload = ...`), and future OpenTelemetry additions are not bound by
that convention. A collision under the bare-keys design is silent on Datadog egress: the
sink would either misclassify a legitimate user attribute as Datadog wire data, or drop it
as ill-typed. The namespaced form reduces the collision class: no stable OpenTelemetry semantic-convention
attribute uses a `_dd.*`-prefixed key, so convention-defined attributes cannot collide.
However, OTLP permits any custom key, so a producer or transform may legitimately set
`Resource.attributes["_dd.payload"]` or `Resource.attributes["_dd.tracer"]`. When that occurs,
the value is interpreted as a Datadog envelope on Datadog egress and rewritten to
`datadog.payload` / `datadog.tracer.tags` rather than the original key on OTLP egress. The
bare-keys design would produce the same collision class for any `payload` or `tracer` attribute,
with no path to distinguishing user data from envelope data. The `_dd.*` namespace limits the
collision to two specific reserved keys whose names signal Datadog-internal intent; the residual
collision is declared as an explicit exclusion under "Round-trip exclusions" rather than being
silent, and operators are advised to avoid setting these two keys outside of Datadog-sourced
pipelines.

### Single merged `attributes` map with richer typed fields

Promote additional concepts (service, env, host *and* all semantic-convention equivalents) to typed
fields. Rejected because the semantic convention space is large and evolving; fixing it in typed
fields either forces Vector to track upstream releases or ossifies a stale subset. The proposal
types only the three resource fields both formats agree on; the rest stay in source-native
attributes where users already expect them.

### Timing as `start_time` + `end_time` (OTLP-native)

OTLP stores `start_time_unix_nano` and `end_time_unix_nano` as two independent `fixed64`s.
Datadog stores `start` plus `duration`. The driving factor for adopting `start + duration` is
that `duration` is more useful than `end_time` in realistic transforms (filtering slow spans,
computing percentiles, classifying long-running requests), so the chosen representation also
matches transform access patterns.

### `AnyValue.double_value = NaN` relay exclusion

`Value::Float` is backed by `NotNan<f64>`, so NaN cannot be stored and a NaN double ingests as
`Value::Null`. Because `Value::Null` is also the representation of an empty `AnyValue` (the oneof
unset case), egress cannot reconstruct the original `double_value = NaN` and emits an unset
`AnyValue` instead. Two approaches were considered and rejected:

- Widening `Value::Float` to admit NaN: `Value` is shared across `LogEvent`, `Metric`, and `Span`;
  the change touches every `Value` consumer in Vector and is out of scope for this RFC.
- Introducing a parallel `AnyValue` type for trace attributes solely to distinguish NaN doubles
  from empty values: the type would need to be threaded through VRL and every attribute-aware
  component, imposing adoption cost disproportionate to one edge case.

The exclusion is declared under "Round-trip exclusions". No stable OpenTelemetry
semantic-convention attribute specifies a NaN value, so the case is expected to affect no
production traffic.

### Duration as `f64` seconds

Storing `duration` as `f64` was considered for VRL ergonomics. Rejected because both OTLP (`fixed64`
nanoseconds) and Datadog (`int64` nanoseconds) carry duration as integer nanoseconds, and `f64`'s
53-bit mantissa cannot exactly represent every integer nanosecond beyond `2^53 ns` (about 104 days).
Storing `duration` as `std::time::Duration` preserves the wire domain for all non-negative values.
Datadog's `int64` wire field permits negative values that `std::time::Duration` cannot represent;
these are clamped to zero on ingress and declared as a round-trip exclusion (see "Round-trip
exclusions"). The VRL surface exposes float seconds at the boundary; a complementary
integer-nanosecond view (`.spans[i].duration_nanos`) is documented under "Future Improvements".

### `SpanStatus` as a closed enum

Defining `SpanStatus` without an escape hatch would silently coerce any unrecognized status
code introduced by a future OpenTelemetry version to `Unset` (the proto3 default), breaking
the `OTLP -> Vector -> OTLP` relay guarantee for those spans. The `Other(i32, String)` variant
stores the raw code and message verbatim and egresses them unchanged, preserving relay
fidelity by the same mechanism used for `SpanKind`. The Datadog egress path has no status-code
wire field; `Other` values follow the `Span.error` rule (non-zero code maps to `error = 1`,
zero code to `error = 0`) and emit the message into `meta["error.message"]` when absent.

Only `Error` carries a string because the OpenTelemetry trace specification's
[Set Status](https://opentelemetry.io/docs/specs/otel/trace/api/#set-status) rule states
"Description MUST only be used with the Error StatusCode value." A wire `Status.message`
paired with `code = UNSET` or `OK` is non-conformant and is dropped on ingest.

### `ChunkContext.priority` as a raw `i32`

Datadog's wire representation is a signed integer with four well-known values
(`UserReject = -1`, `AutoReject = 0`, `AutoKeep = 1`, `UserKeep = 2`). Storing the raw `i32`
directly is simpler. Rejected because transforms that condition on priority then have to
compare against magic numbers, and there is no way to surface "this is a non-standard value"
to the user. A strict enum with an `Other(i32)` escape hatch keeps typed ergonomics for the
common path while preserving any out-of-range value.

### `TraceFlags` via `enumflags2`

[`enumflags2`](https://crates.io/crates/enumflags2) was considered as the bitfield generator
for `TraceFlags`. Rejected because `enumflags2` rejects undefined bits at construction time,
which would silently lose forward-compatibility data on the OTLP `Span.flags` / `Link.flags`
word: OTLP's reserved bits 10-31, the W3C Trace Context Level 2 `random` flag once defined,
and Datadog's bit-31 link sentinel would all be discarded when read by an unaware Vector
build. [`bitflags`](https://crates.io/crates/bitflags) supports `from_bits_retain`, which
preserves the full 32-bit word intact, so the same Vector build round-trips spans with
not-yet-defined flag bits without modification.

### Parsed `TraceState`

Storing `TraceState` as `IndexMap<KeyString, KeyString>` would let transforms operate on
entries without an accessor layer. Rejected because every source and sink would have to invoke
the parser/serializer even for pure-relay pipelines, and because the W3C-imposed bounds (32
entries, 512 bytes total) and typical real-world headers (a single short entry) mean per-entry
allocation costs more than re-parsing the raw header per accessor call.

### Wholesale migration

Replace `TraceEvent(LogEvent)` with the typed container in one PR. Rejected because the resulting PR
would touch every trace source, every trace sink, the APM stats aggregator, every trace-aware
transform, and a large body of tests simultaneously. The chosen `enum TraceEvent { Legacy, Typed }`
coexistence design lets each component migrate in its own PR, subject to a partial-order
constraint that consumers migrate before producers (see "Plan Of Attack").

### Feature-flagged switch

Gate the new representation behind a Cargo feature or runtime flag until all components are
migrated, then flip the default. Rejected because feature combinations proliferate quickly
across every trace source/sink and VRL, and because a runtime flag would require duplicate
code paths in performance-sensitive components.

## Outstanding Questions

- N/A.

## Plan Of Attack

Each step below is intended to land as an independent PR. The
`enum TraceEvent { Legacy, Typed }` coexistence is what makes the sequence possible. The
sequencing rule is for trace-aware consumers (sinks, transforms, VRL programs) to migrate to
`Typed`-native input before any source flips to emitting `Typed` natively, because per-component
shims are unidirectional (`Legacy -> Typed` only) and a `Typed` event has no source provenance
on which to base a `Typed -> Legacy` conversion.

- [ ] Land the legacy-layout hint in the `opentelemetry` and `datadog_agent` sources as a
  precursor. Purely additive -- no consumer reads the key yet. Carrier and sub-key are
  specified in "Migration: coexistence of `LogEvent` and typed representations".
- [ ] Convert `TraceEvent` to the migration enum and introduce the supporting types per
  "Migration: coexistence of `LogEvent` and typed representations". Every component continues
  to produce and consume `Legacy`; no functional change.
- [ ] Extend Vector's internal event proto with the typed wire shape, per "Wire serialization".
  Hard prerequisite for any source-flip step: without it, disk buffers and the `vector`
  source/sink panic on the first `Typed` event.
- [ ] Add VRL typed-path support for `.resource.*`, `.scope.*`, `.chunk.*`, and `.spans[*].*`
  on `VrlTarget`. Untyped VRL paths against `Typed` events return a deterministic error.
- [ ] Migration guide for users:
  - field-by-key VRL programs against the old `TraceEvent` (must move to typed paths;
    legacy paths break against `Typed` events);
  - field-by-key VRL programs against the old `trace_to_log` output (must move to the
    new uniform layout);
  - removal of the legacy `tracerPayloads`-empty Datadog ingest path (see [^dd-v0]).
    Payloads in the legacy shape now emit
    `component_errors_total{error_type="unsupported_payload_version"}` and a rate-limited
    error log instead of being silently translated.
  - cross-version `vector` source/sink chains spanning the typed migration must run a single
    release line (see Drawbacks).
- [ ] OTLP `Legacy -> Typed` shim and `Typed -> OTLP-wire` conversion in
  `lib/opentelemetry-proto`. Registers under the OTLP layout hint; consumed by the eventual
  native `opentelemetry` source emission and any OTLP sink.
- [ ] Remove the legacy `tracerPayloads`-empty Datadog ingest branch (see [^dd-v0]): delete
  `handle_dd_trace_payload_v0` from `src/sources/datadog_agent/traces.rs` and replace
  `proto/vector/dd_trace.proto` with the upstream agent-payload/tracer-payload/span protos.
  Payloads in the legacy shape emit
  `component_errors_total{error_type="unsupported_payload_version"}` and a rate-limited
  error log. Lands independently of the typed migration.
- [ ] Datadog `Legacy -> Typed` shim and `Typed <-> Datadog-wire` conversions in
  `src/sources/datadog_agent/traces.rs` and `src/sinks/datadog/traces/`. Parallels the OTLP
  shim step.
- [ ] Property-based round-trip unit tests for `OTLP -> Vector -> OTLP` and
  `Datadog -> Vector -> Datadog`, asserting effective equivalence per Scope. Required Datadog
  coverage: multi-service single-trace chunks, non-conforming multi-trace chunks, and 128-bit
  `SpanLink` trace IDs.
- [ ] Migrate the `datadog_traces` sink to consume `Typed` natively; update APM stats
  aggregation to read typed fields.
- [ ] Migrate the `sample` transform (and tests) to typed access. Behavioural change: for
  Datadog-sourced events the keep/drop unit shifts from `TraceChunk` to
  `(TraceChunk, Span.service)`. Trace-stable sampling is deferred to Future Improvements.
- [ ] Migrate the `trace_to_log` transform to typed access; emit a uniform, source-independent
  `LogEvent` layout. Document the new key layout in the migration guide.
- [ ] Remove the untyped forwarding methods (`get`, `insert`, `as_map`, `as_ref` to
  `LogEvent`, etc.) from `TraceEvent`. Call sites that still use them through `TraceEvent`
  become compile errors; each migrates to the typed accessor API or pattern-matches into the
  `Legacy(LogEvent)` arm explicitly. The build must be green before the source-flip steps.
- [ ] Migrate the `opentelemetry` source to produce `Typed` natively. By this point every
  trace-aware downstream component is `Typed`-capable.
- [ ] Migrate the `datadog_agent` source to produce `Typed` natively.
- [ ] Collapse the `TraceEvent` enum to a struct with only the typed variant's fields. Remove
  the per-component shims. Mark the legacy proto messages `deprecated = true` per "Wire
  serialization"; a follow-up PR after the deprecation window removes them and reserves the
  field tags.

## Future Improvements

- Topology-level per-span shim: a transform mode that fans out a `TraceEvent` into per-span events,
  runs a downstream transform once per span, and collapses results back into the container. Lets
  single-span transforms be authored without explicit iteration while keeping the wire-aligned event
  shape as the source of truth.
- VRL helpers for trace-state parsing/encoding: `parse_trace_state`, `encode_trace_state`,
  `merge_span_attributes`, `decode_otlp_span`, `decode_datadog_span`.
- Lossless integer-nanosecond view for span duration: `.spans[i].duration` is exposed as float
  seconds, exact for any duration under `2^53 ns` (about 104 days). Workloads needing access to
  durations beyond that limit can have a complementary `.spans[i].duration_nanos` view added without
  affecting the underlying data model.
- Link-based routing: a trace-aware router transform that emits to different sinks based on
  `SpanLink` targets.
- Stateful trace-aggregator transforms: tail-based sampling, per-trace APM-stats aggregation, and
  similar trace-scoped operations expressed as transforms over the wire-aligned container shape.
- Trace- or chunk-stable sampling: a sampling strategy that makes a single keep/drop decision per
  `trace_id` (or per `TraceChunk` identifier) and applies it consistently to every event derived
  from that trace/chunk, restoring atomicity that the per-event `sample` transform cannot guarantee
  after the typed migration's per-service split. This may be added to `sample` as a configurable
  mode or shipped as a separate component (e.g. `trace_sample`); the choice is deferred to the
  implementation.
- Adopt typed support for OTLP fields as they reach `Stable` stability. The current scope excludes
  `Development`/`Alpha`-tier additions; when upstream stabilizes any of these, evaluate adding the
  corresponding typed slot and round-trip support, including a cross-format storage convention for
  fields with no Datadog wire analog.
- Distinct `Value::String` variant in VRL's `Value`, separate from `Value::Bytes`. Today both UTF-8
  strings and opaque byte payloads share `Value::Bytes`, which forces the OTLP egress rule to
  recover the `AnyValue.string_value`/`AnyValue.bytes_value` discriminator by inspecting the payload
  (see "Attributes"). The rule round-trips every input except a `bytes_value` whose payload is valid
  UTF-8; that case flips to `string_value` on egress. A separate `Value::String` would carry the
  discriminator structurally and round-trip that input shape exactly. Out of scope for this RFC
  because `Value` is shared across `LogEvent`, `Metric`, and `Span`, and the change touches every
  `Value` consumer in Vector (encoders, VRL, disk-buffer schema, every transform). Adopting it would
  supersede the reserved-key-wrapper alternative and bring the OTLP round trip to bit-exact for
  traces, plus any other signal that inherits the same collapse. The same change could also benefit
  Vector string handling more broadly: because `Value::Bytes` is used today to carry string-typed
  values throughout `LogEvent`, `Metric`, and `Span`, every site that needs to operate on the value
  as a string repeats a UTF-8 validation scan. A `Value::String` variant whose invariant is
  "validated once at construction" would let those sites skip the scan and use the bytes as a `&str`
  directly. The cross-cutting cost is the same either way; the amortized win is wider than the trace
  round-trip case alone.
