# RFC 25329 - 2026-04-29 - Internal Trace Data Model

This RFC replaces the inner representation of Vector's `TraceEvent`, today a thin newtype over
`LogEvent`, with a strongly-typed container that mirrors the wire-level batching of OTLP and
Datadog APM traces. Each `TraceEvent` carries one `Resource`, one `Scope`, one Datadog-specific
`ChunkContext`, and the `Vec<Span>` belonging to that grouping, plus the existing
`EventMetadata`. The container shape, together with the wire-format mappings specified in the
two sub-RFCs below, yields zero-loss `OTLP -> Vector -> OTLP` and
`Datadog -> Vector -> Datadog` round trips, including across Vector's disk buffers, and gives
transforms a uniform typed surface across the two source formats.

The full proposal is split across three documents:

- This document defines the typed data model, the VRL surface, the migration coexistence
  enum and shim mechanism, and Vector's internal protobuf serialization.
- [Trace Data Model: OTLP Mapping](2026-04-29-25329-trace-data-model/otlp-mapping.md)
  specifies the bidirectional mapping between `TraceEvent` and the OTLP wire format.
- [Trace Data Model: Datadog Mapping](2026-04-29-25329-trace-data-model/datadog-mapping.md)
  specifies the bidirectional mapping between `TraceEvent` and the Datadog agent-to-backend
  protobuf, including the cross-format conformance rule for `OTLP -> Vector -> datadog_traces`.

The three documents are proposed together and share a single approval.

## Context

- [RFC 11851 -- OpenTelemetry traces source](2022-03-15-11851-ingest-opentelemetry-traces.md)
  was accepted on the condition that an internal trace model be established before the work
  was completed. The OTLP mapping sub-RFC completes that condition for the OTLP side.
- [RFC 9572 -- Accept Datadog traces](2021-10-15-9572-accept-datadog-traces.md) introduced the
  `datadog_agent` trace ingest path, which the `datadog_traces` sink can consume but which
  does not have a well-defined internal representation. The Datadog mapping sub-RFC supplies
  that representation.
- An earlier draft of an internal trace model is available at
  [2024-03-22-20170-trace-data-model](https://github.com/hdost/vector/blob/add-trace-data-model/rfcs/2024-03-22-20170-trace-data-model.md);
  this RFC supersedes that draft.
- The current implementation in
  [`lib/vector-core/src/event/trace.rs`](../lib/vector-core/src/event/trace.rs) is
  `TraceEvent(LogEvent)` -- a thin newtype with no type structure. Transforms depend on the
  ingesting source's key layout, and cross-format conversions are ad-hoc per sink.
- [vectordotdev/vector#22659 -- Transform between opentelemetry and datadog traces](https://github.com/vectordotdev/vector/issues/22659).

## Glossary

This RFC defines the OpenTelemetry-side and informational vocabulary the data model
depends on. Datadog-specific format definitions (`Datadog APM trace format`, `Datadog
Agent OTLP ingest`, `Datadog tracer-to-agent API`) live in the
[Datadog mapping sub-RFC's Glossary](2026-04-29-25329-trace-data-model/datadog-mapping.md#glossary);
the entries below are the format-agnostic shared vocabulary.

- **OTLP (OpenTelemetry Protocol)**: the wire format the OpenTelemetry project defines for
  traces, metrics, and logs. The traces schema lives in
  [`opentelemetry/proto/trace/v1/trace.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto),
  with shared value types in
  [`common/v1/common.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/common/v1/common.proto)
  and resource types in
  [`resource/v1/resource.proto`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/resource/v1/resource.proto).
  When this document says "OTLP" it means that wire schema and the data model it defines
  (`ResourceSpans`, `ScopeSpans`, `Span`, `AnyValue`, etc.).
- **OpenTelemetry**: the broader project under which OTLP is one component. References in
  this RFC to "OpenTelemetry" name the project's non-wire artefacts: the
  [specification](https://github.com/open-telemetry/opentelemetry-specification) and the
  [semantic conventions](https://github.com/open-telemetry/semantic-conventions) (the
  registry of attribute keys such as `service.name` and `http.request.method`).
- **W3C Trace Context** (informational): the W3C recommendation defining the
  [`traceparent` and `tracestate` HTTP headers](https://www.w3.org/TR/trace-context/). The
  proposed `TraceFlags` and `TraceState` types correspond to these headers.
- **Zipkin v2, Jaeger, OpenTracing** (informational): other trace data models referenced in
  passing for context. None are targeted by this RFC and they are not constraints on the
  design. Zipkin v2 is documented at the [Zipkin API](https://zipkin.io/zipkin-api/#/default/get_spans);
  Jaeger at [jaegertracing.io](https://www.jaegertracing.io/docs/latest/architecture/#span);
  OpenTracing at the [OpenTracing spec](https://github.com/opentracing/specification/blob/master/specification.md).

## Cross cutting concerns

- First-class OpenTelemetry signal support
  ([vectordotdev/vector#1444](https://github.com/vectordotdev/vector/issues/1444)).
- VRL trace-specific semantics on the new typed surface (`.resource.service`,
  `.chunk.priority`, `.spans[i].name`, etc.).

## Scope

### In scope

- Define `TraceEvent` as an array of spans plus supporting resource data, replacing the
  current `TraceEvent(LogEvent)`.
- Define the typed surface that supports both wire formats: `TraceEvent`, `Span`,
  `Resource`, `Scope`, `ChunkContext`, `Attributes`, `SpanEvent`, `SpanLink`, `TraceId`,
  `SpanId`, the closed-with-escape-hatch enums (`SpanKind`, `SpanStatus`,
  `SamplingPriority`), `TraceFlags`, and `TraceState`.
- Define the VRL surface for the typed `TraceEvent`, including the `del()` semantics and
  the typed-slot/attribute-map pairs (with precedence semantics owned by each mapping
  sub-RFC).
- Define the migration strategy that lets each trace-producing or trace-consuming component
  migrate independently: the `enum TraceEvent { Legacy(LogEvent), Typed { … } }`
  coexistence, the per-source `Legacy -> Typed` shim mechanism keyed on
  `vector.trace_legacy_layout`, and the compile-time gating that catches unmigrated
  consumers.
- Extend Vector's internal event protobuf with a `TypedTrace` variant alongside the renamed
  `LegacyTrace` so trace events cross disk-buffer and `vector` source/sink boundaries
  unchanged.
- Specify the effective-equivalence round-trip guarantee for `OTLP -> Vector -> OTLP` and
  `Datadog -> Vector -> Datadog` as a model-level claim; the per-wire-format mappings that
  satisfy this claim are in the OTLP mapping and Datadog mapping sub-RFCs respectively.
  Effective equivalence means backend-observable identity, not byte-level identity; details
  the backend does not observe (e.g. span order within a chunk, specific chunk grouping)
  may differ. The guarantee applies to pure-relay pipelines only: any VRL write to a
  trace event is best-effort and forfeits the round-trip claim for the modified event.

### Out of scope

- VRL function additions for trace-specific operations (e.g. `decode_trace_state`).
- New trace sources/sinks (Zipkin, Jaeger, etc.).
- APM stats computation semantics (already covered by RFC 9862).
- Zero-loss cross-format round-trip (`Datadog -> OTLP -> Datadog` or
  `OTLP -> Datadog -> OTLP`).
- `TracerPayload.containerDebug` (Datadog-internal container-tag-resolution diagnostic);
  dropped on ingest, not synthesized on egress.

### Zero-loss round-trip exclusions

The effective-equivalence guarantee does not cover the following model-level input shapes.
Each is justified by a paragraph in the Implementation or Rationale section below.
Wire-format-specific exclusions (NaN doubles, the OTLP deprecated-environment rewrite,
Datadog `Span.error` normalization, `meta`/`metrics` producer-side disjointness, etc.) are
declared in the corresponding sub-RFC's Scope section.

- **All-zero `TraceId` or `SpanId`** are rejected on every ingress path; the span (or
  link) carrying the zero ID is dropped. See "Identifiers" for the drop granularity and
  the sub-RFCs for per-format detection.
- **`Span.duration` exceeding the encoding wire field's representable maximum** (constructible via
  the Rust API or VRL integer-nanosecond writes once that view lands) is clamped on encode; each
  encoder increments a counter and emits a warning log identifying the affected span. The internal `TypedTrace` `fixed64`
  encoder clamps to `u64::MAX` nanoseconds (~584 years); per-wire-format clamps are documented in
  the sub-RFCs.
- **Pre-epoch timestamps** (negative nanoseconds-since-epoch) are clamped to epoch-zero
  on encode at every wire boundary. The internal `TypedTrace` proto's `fixed64`
  timestamp fields enforce this directly; OTLP and Datadog egress apply the same clamp
  per the corresponding sub-RFC. The internal proto matches OTLP's `fixed64`
  representation rather than Datadog's `int64`; production trace data with pre-epoch
  start times is effectively nonexistent and the alignment with the dominant wire
  format is preferred over a wider internal type.
- **Multi-hop topologies that relay traces through intermediate `vector` source/sink hops**
  may lose Datadog agent-envelope state by default; the Datadog mapping sub-RFC's
  "Envelope reconstruction policy" documents the mechanism and the operator-configurable
  passthrough.

## Pain

- Transforms written against today's `TraceEvent` depend on the exact key layout the
  ingesting source produced. A remap that works for `datadog_agent` traces does not work
  for OTLP traces, even when the semantic intent is identical. This is the opposite of how
  `Metric` behaves and is the primary blocker to useful trace transforms.
- Cross-format routing (e.g. `opentelemetry` source -> `datadog_traces` sink) requires
  bespoke translation reading undocumented magic keys. Each new sink duplicates this work.
- `TraceEvent` corrupts numeric ID precision via `trace_id as i64` on both the
  `datadog_agent` source and the `datadog_traces` sink
  ([#14687](https://github.com/vectordotdev/vector/issues/14687)).
- VRL programs authoring spans without typed events, links, or status can produce
  structurally invalid output that is only discovered at sink encoding time.

## Proposal

### User Experience

A `TraceEvent` carries one `Resource`, one `Scope`, one `ChunkContext`, and a `Vec<Span>`.
VRL accesses these directly:

```coffee
# Route by resource service.
if .resource.service == "checkout" { ... }

# Read a Datadog chunk-scoped tag (null for OTLP-sourced events).
decision_maker = .chunk.tags."_dd.p.dm"

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
.user_id = .spans[0].attributes."user.id" || .spans[0].attributes."usr.id"
```

The typed surface is uniform across both wire formats: a VRL program reading
`.resource.service`, `.spans[i].name`, or `.chunk.priority` behaves the same whether the
event originated from the `opentelemetry` source or the `datadog_agent` source. Format-
specific encoding details (how Datadog's three span-attribute partitions merge into
`Span.attributes`, how the agent and tracer envelopes populate
`Resource.attributes."_dd.payload"` and `Resource.attributes."_dd.tracer"`) are documented
in the OTLP mapping and Datadog mapping sub-RFCs and do not affect VRL semantics.

This uniformity is also the ingest invariant for every trace source: when the source wire
format carries data that has a typed home in the model, ingest stores it in the
corresponding typed struct field rather than leaving it encoded only under source-specific
attribute keys. Attribute maps and reserved keys are used only for data with no dedicated
typed slot, or for source-native wire state the mapping sub-RFC explicitly preserves. Sink
egress then projects from that typed surface (plus those explicitly preserved wire-only
payloads), not from source-specific ingest layouts.

The `trace_to_log` transform is retained, but its output shape changes from a source-
defined key layout to a uniform, source-independent one; the migration guide provides a
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

Each `TraceEvent` carries spans that share a single `Resource` -- meaning a single service.
The mapping to wire-level structures differs by format:

- **OTLP**: one `TraceEvent` per `ScopeSpans` (1:1). The enclosing `ResourceSpans`
  provides `Resource`. See the OTLP mapping sub-RFC for the per-field mapping.
- **Datadog**: one `TraceEvent` per `(TracerPayload, distinct Span.service, TraceChunk)`
  triple. A single `TraceChunk` whose spans use more than one `Span.service` is split into
  multiple `TraceEvent`s (one per service); the Datadog mapping sub-RFC specifies the
  split and the corresponding re-coalescence on egress.

#### `Span`

```rust
pub struct Span {
    pub trace_id:       TraceId,
    pub span_id:        SpanId,
    pub parent_span_id: Option<SpanId>,
    pub trace_state:    TraceState,
    pub flags:          TraceFlags,

    pub name:           String,
    pub kind:           SpanKind,

    pub start_time:     DateTime<Utc>,
    /// Span duration with nanosecond precision.
    pub duration:       Duration,
    pub status:         SpanStatus,

    /// Datadog-native, no OTLP equivalent: human-readable identifier of
    /// the resource being traced.
    pub resource_name:  Option<String>,

    /// Datadog-native, no OTLP equivalent: free-form classification of
    /// the span.
    pub span_type:      Option<String>,

    /// Per-span attribute map.
    pub attributes:     Attributes,

    pub events:         Vec<SpanEvent>,
    pub links:          Vec<SpanLink>,

    pub dropped_attributes_count: u32,
    pub dropped_events_count:     u32,
    pub dropped_links_count:      u32,
}
```

`Span` includes two Datadog-shaped slots (`resource_name`, `span_type`) and the typed
surface defines several reserved attribute keys (`Span.attributes."_dd.meta_struct"`,
`Resource.attributes."_dd.payload"`, `Resource.attributes."_dd.tracer"`) whose wire
semantics live in the Datadog mapping sub-RFC. They appear in the format-agnostic data
model because:

- The fields and reserved keys are present in valid `TraceEvent` values regardless of
  source format. An OTLP-sourced event carries `resource_name = None`,
  `span_type = None`, and no `_dd.*` entries, but the slots and the schema points exist.
- VRL programs and Vector internals must be able to read and write these fields
  uniformly. Typed slots are preferable to format-discriminated structs because the
  cross-format relay (`OTLP -> datadog_traces`) must be able to derive Datadog wire
  fields from typed values without introspecting the event's source. See "OTLP-only
  schema with Datadog round-trip via import/export encoding" under Alternatives for the
  rejected alternative.

#### `Resource` and `Scope`

```rust
pub struct Resource {
    pub service:     Option<String>,   // service.name
    pub environment: Option<String>,   // deployment.environment.name
    pub host:        Option<String>,   // host.name
    pub attributes:  Attributes,
    pub schema_url:  Option<String>,
    pub dropped_attributes_count: u32,
}

pub struct Scope {
    /// `None` carries the OTLP "instrumentation scope name unknown" semantics.
    pub name:       Option<String>,
    pub version:    Option<String>,
    pub attributes: Attributes,
    pub schema_url: Option<String>,
    pub dropped_attributes_count: u32,
}
```

#### Identifiers

```rust
pub struct TraceId(NonZeroU128);
pub struct SpanId(NonZeroU64);

impl TraceId {
    /// Low 64 bits, emitted as the wire `Span.traceID` on Datadog egress.
    /// May be zero when the high half is non-zero (the combined 128-bit ID is still valid;
    /// the Datadog backend reconstructs the full ID from `Span.traceID` + `_dd.p.tid`).
    pub fn low_u64(self)  -> u64 { self.0.get() as u64 }
    /// High 64 bits, emitted as `meta["_dd.p.tid"]` on Datadog egress when non-zero.
    /// `u128 >> 64` yields the high half in the low 64 bits; `as u64` is a no-op truncation.
    pub fn high_u64(self) -> u64 { (self.0.get() >> 64) as u64 }
}
```

Conversions to and from `u128`/`u64` and OTLP's 16/8-byte big-endian representations are
provided as cheap copies via `From` (when the source is statically non-zero) and
`TryFrom` (otherwise).

Zero `TraceId` and `SpanId` values are unrepresentable in a well-formed event by the `NonZero` types
above. Every construction site rejects zero inputs, increments a counter, and emits a warning log.
The internal `TypedTrace` proto decode applies the same rule
(covering disk-buffer replay after partial writes and `vector` source/sink transport errors): a
buffered or wire-transported event whose `trace_id` or `span_id` decodes to zero is treated as
corruption. A `trace_id` whose decoded byte length is not exactly 16 is treated identically to a
zero `trace_id`: the same per-link or per-span drop applies, a counter is incremented, and a
warning log identifies the drop.

Drop granularity is structural and uniform across sources: a zero `SpanLink.span_id` or
`SpanLink.trace_id` drops only the affected link (`Span.dropped_links_count` is incremented, a
counter is incremented, and a warning log identifies the drop); a zero `Span.trace_id` or
`Span.span_id` drops the enclosing span; a candidate `TraceEvent` whose every span was rejected is
dropped as a whole with an additional counter increment and warning log.

Any future relay-side drop of a `SpanEvent` or attribute follows the same convention: the
corresponding `dropped_events_count` / `dropped_attributes_count` field on the enclosing item is
incremented, a counter is incremented, a warning log identifies the drop, and the relay never
silently shrinks an event's in-band counters relative to what was received.

A `TraceEvent` whose `spans` vector is otherwise empty -- a wire-level empty grouping forwarded
as-is, or a transform filtering every span out -- passes through unchanged. Sinks emit the
corresponding empty wire shape, fire finalizers on successful delivery, increment a counter,
and emit a warning log so the condition is observable without breaking ack-chain durability
semantics (Kafka offset commits, source disk buffers, etc.). The internal proto encoder applies the
same rule.

##### VRL surface for `TraceId` and `SpanId`

`TraceId` and `SpanId` are exposed to VRL as lowercase hex strings without a leading `0x`:
`TraceId` as 32 characters (16 bytes), `SpanId` as 16 characters (8 bytes), zero-padded
on the left. Reading returns the canonical lowercase form; writing accepts case-
insensitive hex with optional zero-padding (so `"abc"` and `"0000000000000abc"` both
round-trip to the same `SpanId`). A non-hex string, an over-length string (more than
32 / 16 characters after trimming), or an all-zero string raises a VRL runtime error --
the all-zero rejection mirrors the construction-time `NonZeroU128` / `NonZeroU64`
invariant. `Span.parent_span_id` is `Option<SpanId>`; deleting the field via `del()`
clears it to `None`, and writing the empty string `""` is equivalent to `del()`.

#### Status, kind, chunk context

```rust
pub enum SpanKind {
    Unspecified,
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
    /// Unrecognized enum number from a newer OpenTelemetry version. Stored
    /// verbatim so an OTLP -> Vector -> OTLP relay emits the original wire
    /// value unchanged. See "Closed-with-escape-hatch enum invariant" below
    /// for the construction-time normalization rule.
    Other(i32),
}

pub enum SpanStatus {
    Unset,
    Ok,
    Error(String),
    /// Unrecognized status code from a newer OpenTelemetry version. The raw
    /// code integer and any status message are stored verbatim so an
    /// OTLP -> Vector -> OTLP relay emits the original wire values unchanged.
    /// See "Closed-with-escape-hatch enum invariant" below.
    Other(i32, String),
}

/// Datadog `TraceChunk`-scoped state. Default-empty for OTLP-sourced events.
pub struct ChunkContext {
    pub priority: Option<SamplingPriority>,
    pub origin:   Option<String>,
    pub dropped:  bool, /// `TraceChunk.droppedTrace`
    pub tags:     Attributes,
}

pub enum SamplingPriority {
    UserReject, // -1
    AutoReject, //  0
    AutoKeep,   //  1
    UserKeep,   //  2
    /// Out-of-range value. Datadog tracing libraries may uncommonly emit
    /// these. See "Closed-with-escape-hatch enum invariant" below.
    Other(i32),
}
```

##### Closed-with-escape-hatch enum invariant

`SpanKind`, `SpanStatus`, and `SamplingPriority` share a single invariant: a value
matching a known variant's wire number is always carried as that variant, never as
`Other(n)`. The `Other` constructor enforces this, rejecting (debug-assert; normalizing
in release) any input whose payload matches a known variant. Every construction site --
wire-format sources, the internal `TypedTrace` proto decode, VRL writes, and direct Rust
constructors -- routes through this constructor. As a consequence, for example,
`SpanKind::Other(3)`, `SpanStatus::Other(2, _)`, and `SamplingPriority::Other(1)` are
unrepresentable in any well-formed `TraceEvent`, and pattern matches on the canonical
variants are exhaustive for the known-value space.

##### VRL surface for the closed-with-escape-hatch enums

`SpanKind`, `SpanStatus`, and `SamplingPriority` share a single VRL access pattern:

- The discriminator is a snake_case string for each known variant (`SpanKind`:
  `"unspecified"` / `"internal"` / `"server"` / `"client"` / `"producer"` / `"consumer"`;
  `SpanStatus`: `"unset"` / `"ok"` / `"error"`; `SamplingPriority`: `"user_reject"` /
  `"auto_reject"` / `"auto_keep"` / `"user_keep"`) and an integer for the `Other`
  variant. Reading returns the snake_case string when the value matches a known variant
  and the raw integer otherwise. Writing a recognized string sets the corresponding
  variant; writing an integer that matches a known variant's wire number sets that
  variant (so `.spans[i].kind = 3` is equivalent to `.spans[i].kind = "client"`); other
  integers set `Other(n)`. Writing any other value (e.g. a non-canonical string) raises a
  VRL runtime error.
- `SpanStatus` exposes `code` and `message` as two independent fields. Reading `message`
  returns the inner string for `Error(s)` and `Other(_, s)`, and the empty string for
  `Unset` / `Ok`. Writing `message` to a non-empty value when `code` is `"unset"` or
  `"ok"` promotes `code` to `"error"` (matching the OpenTelemetry
  [Set Status](https://opentelemetry.io/docs/specs/otel/trace/api/#set-status) rule that
  only `Error` carries a description); writing `message` to the empty string is a no-op
  for `Unset` / `Ok`. Writing `code` to `"unset"` or `"ok"` clears any existing message.

#### Empty-string invariant for optional string slots

Every `Option<String>` typed slot in the data model -- `Resource.service` /
`environment` / `host` / `schema_url`, `Scope.name` / `version` / `schema_url`,
`Span.resource_name` / `span_type`, and `ChunkContext.origin` -- carries `None` for the
absent / unset case and a non-empty string otherwise. `Some("")` is unrepresentable in a
well-formed `TraceEvent`, so consumers do not need to discriminate between `None` and a
present-but-empty value at the typed surface. Every construction site normalizes an
empty input to `None`; the per-format applications are specified in the corresponding
sub-RFC.

#### `TraceFlags` and `TraceState`

`TraceFlags` is the OTLP `Span.flags` / `Link.flags` bitfield: a 32-bit word whose low
byte is the W3C trace-flags byte and whose remaining bits carry OTLP- and Datadog-defined
context information. Sources construct via `TraceFlags::from_bits_retain(word)` and sinks
read the raw value via `flags.bits()`, so unknown bits (including OTLP's reserved bits
10-31) round-trip unchanged. The W3C trace-flags byte is exposed as a derived view via
`flags.w3c_byte()` for emission in `traceparent` headers and similar W3C-only surfaces.

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

`TraceState` stores the W3C `tracestate` header verbatim and exposes map-like accessors
that parse on demand. Sources copy the header in unchanged; sinks emit it unchanged
unless a transform mutated it. Vector does not validate the header against the W3C
grammar, enforce the 32-entry or 512-byte limits, reject invalid members, or deduplicate
keys: the raw string is preserved and re-emitted as-is. Validation is the responsibility
of the producing tracing SDK, not the relay. This is consistent with `TraceFlags`, which
also preserves unknown bits without validation.

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

`insert` is eager: it re-parses the raw string, places the new entry at the head, removes
any duplicate for the same key, and rebuilds the string immediately. The `TraceState` is
therefore always spec-conformant after each call, satisfying the W3C Trace Context
[Section 3.3.1](https://www.w3.org/TR/trace-context/#mutating-the-tracestate-field)
requirement that updated members be moved to the beginning.

The Rust map-like accessors are not exposed to VRL. `.spans[i].trace_state` and
`.spans[i].links[j].trace_state` are read and written as raw header strings; programs
needing structured access wait for the deferred VRL helpers (`parse_trace_state`,
`encode_trace_state`) listed under "Future Improvements". A direct write to the raw
string bypasses the spec-conformance maintenance that `insert` provides; producers retain
that responsibility (consistent with the non-validating relay stance above).

#### Events and links

```rust
pub struct SpanEvent {
    pub name: String,
    /// Epoch (`1970-01-01T00:00:00Z`) represents "timestamp unknown" per OTLP.
    /// On both OTLP and Datadog egress, epoch round-trips as `time_unix_nano = 0`
    /// (the proto3 default).
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

#### `Attributes` and `AttrValue`

```rust
pub struct Attributes(BTreeMap<KeyString, AttrValue>);

/// Mirrors OTLP `AnyValue`.
pub enum AttrValue {
    String(String),
    Bytes(Bytes),
    Bool(bool),
    Int(i64),
    Double(f64),
    Array(Vec<AttrValue>),
    Map(BTreeMap<KeyString, AttrValue>),
    Null,
}
```

`AttrValue` is the storage type for every attribute leaf in the model
(`Span.attributes`, `SpanEvent.attributes`, `SpanLink.attributes`,
`Resource.attributes`, `Scope.attributes`, `ChunkContext.tags`, and recursively into
nested `Map` / `Array` values). The newtype around the `BTreeMap` exists so future
invariants (key validation, size bounds) can be added without requiring a migration.
Per-format wire mappings live in the sub-RFCs.

##### VRL surface for `AttrValue`

VRL accesses an `Attributes` map through the existing `Value` API; the `VrlTarget`
boundary inspects `Value::Bytes` writes for UTF-8 to choose between `AttrValue::String`
and `AttrValue::Bytes`, so a VRL read-and-unchanged-write preserves the discriminator
whenever the bytes are valid UTF-8. Other conversions fall out of the `Value`
definitions (`Value` has no `String` distinct from `Bytes`; `Value::Float` is
`NotNan<f64>`). `AttrValue::Null` reads as `Value::Null` and a `Value::Null` write
stores `AttrValue::Null` -- preserving the explicit-null versus absent-entry
distinction that OTLP `AnyValue` carries on the wire. Removing an entry from an
`Attributes` map requires `del()` per the typed-path rules below.

#### Typed slot/attribute-map pairs

Several typed slots on `Resource`, `Span`, and `ChunkContext` correspond to attribute-map keys that
wire formats also use. The pairs the model knows about are:

- `Resource.service` versus `Resource.attributes."service.name"`.
- `Resource.environment` versus `Resource.attributes."deployment.environment.name"`.
- `Resource.host` versus `Resource.attributes."host.name"`.
- `Span.trace_id.high_u64` versus `Span.attributes."_dd.p.tid"`.
- `Span.status` (`Error` / `Other` message) versus `Span.attributes."error.message"`.
- `TraceEvent.chunk.{priority, origin, dropped, tags}` versus
  `Span.attributes."datadog.chunk.*"` (cross-format only; see the OTLP mapping sub-RFC).

The in-memory model permits both forms to coexist. Reading from the typed slot is the
supported VRL pattern; the matching attribute-map key exists only as a wire-shape
detail. Ingress lifting (when an attribute key populates a typed slot) and egress
synthesis (when a typed slot populates an attribute key or a canonical wire location)
are wire-format-specific and are specified by each mapping sub-RFC.

#### VRL `del()` semantics on typed paths

The rules below mirror the existing `Metric` VRL surface (`VrlTarget` in
`lib/vector-core/src/event/vrl_target.rs`). VRL's `del()` operator removes a key from
its parent; on the typed `TraceEvent` structure the result depends on what the path
resolves to:

- **`Option`-wrapped typed slot** (`Span.parent_span_id`, `Resource.service` /
  `environment` / `host` / `schema_url`, `Scope.name` / `version` / `schema_url`,
  `Span.resource_name`, `Span.span_type`, `ChunkContext.priority` / `origin`): `del()`
  clears the slot to `None`. For each `Option<String>` slot in this list, writing the
  empty string `""` is equivalent to `del()` -- a consequence of the empty-string
  invariant above; the analogous rule for `Span.parent_span_id` is documented under
  "VRL surface for `TraceId` and `SpanId`".
- **`Attributes` map entry** (e.g. `.spans[i].attributes."foo"`,
  `.resource.attributes."bar"`, `.scope.attributes.*`, `.chunk.tags.*`,
  `.spans[i].events[j].attributes.*`, `.spans[i].links[j].attributes.*`): `del()` removes
  the entry from its map.
- **`Vec` element** (`.spans[i]`, `.spans[i].events[j]`, `.spans[i].links[j]`): `del()`
  removes the i-th / j-th element; the vector shrinks and subsequent indices renumber.
- **Required typed sub-field with a representable default value**: `del()` is
  equivalent to writing the sub-field's default value. Examples:
  `del(.spans[i].status.code)` sets `code` to `"unset"` (which per the `SpanStatus`
  write rules above also clears any existing message); `del(.spans[i].status.message)`
  sets the message to the empty string (a no-op for `Unset` / `Ok`, clearing the inner
  string for `Error` / `Other`); `del(.chunk.tags)` sets the map to `{}`;
  `del(.spans[i].duration)` sets the duration to `0`;
  `del(.spans[i].events[j].attributes)` sets the map to `{}`.
- **Required typed field without a representable default value, or a top-level required
  container** (the `NonZero` IDs `.spans[i].trace_id`, `.spans[i].span_id`,
  `.spans[i].links[j].trace_id`, `.spans[i].links[j].span_id`; plus the top-level
  `.resource`, `.scope`, `.chunk`, and `.spans` containers): `del()` raises a VRL
  runtime error. Replace `NonZero` IDs by writing a non-zero value; replace a top-level
  container by writing the desired value (e.g. `.spans = []` to clear the spans vector).
- **Root path (`del(.)`)**: raises a VRL runtime error.

Reading through the same paths is the inverse: `Option`-wrapped slots that are `None`
read as `null`, absent attribute-map entries read as `null`, out-of-bounds vector indices
read as `null`, and required typed fields are always present and always read a concrete
value.

#### Retention of `TraceEvent` and `Event::Trace`

The `Event::Trace(TraceEvent)` variant on the outer `Event` enum is retained. Only the
inner representation changes:

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

Trace event-producing sources each set the reserved sub-key `vector.trace_legacy_layout`
in `EventMetadata.value` to a static string identifying themselves on every `Legacy`
trace they emit. `to_typed()` reads this sub-key to select the corresponding
`Legacy -> Typed` shim. A `Legacy` event whose hint is absent or maps to no registered
shim returns an error, increments a counter, and emits a warning log.

The end-state `struct TraceEvent { resource, scope, chunk, spans, metadata }` shown above
is reached by deleting the `Legacy` arm once every component has migrated; the `Typed`
arm's fields become the struct's fields verbatim.

Both accessor families coexist on `TraceEvent` and dispatch on the variant:

- `metadata()` / `metadata_mut()` and finalizer methods return the inner `LogEvent`'s
  metadata when `Legacy`, and the typed `metadata` field when `Typed`. Callers see no
  behaviour change.
- The existing untyped accessors (`get(path)`, `insert(path, value)`, `as_map()`, etc.)
  are not forwarded on `TraceEvent` itself; they are accessible only by pattern-matching
  into the `Legacy(LogEvent)` arm directly. Any call site that invokes them through the
  `TraceEvent` type therefore fails to compile as soon as these forwarding methods are
  removed, making the migration of remaining consumers a compile-error-driven mechanical
  task rather than a runtime-failure audit.
- The new typed accessors operate on the `Typed` form only. Calling them on a `Legacy`
  variant panics with a clear diagnostic message; the design rationale for the panicking
  accessor (as opposed to implicit conversion) is in "Migration approach" under
  Rationale. The VRL boundary is the exception: `VrlTarget` has `&mut self` access, so
  it auto-converts Legacy -> Typed on first typed-path access, ensuring VRL programs
  work uniformly regardless of source migration state. If `to_typed()` fails (absent or
  unrecognized `vector.trace_legacy_layout` hint), `VrlTarget` aborts the VRL expression
  with a runtime error; the event is forwarded to the topology error path unchanged,
  a counter is incremented, and a warning log identifies the failing event.
- Explicit `to_typed(&mut self)` rewrites a `Legacy` variant in place into `Typed` by reading the
  `vector.trace_legacy_layout` hint from `EventMetadata.value` and invoking the corresponding
  source-specific shim. Already-`Typed` events are a no-op. There is no symmetric `to_legacy`. When
  called by a typed-aware sink or transform (i.e. outside `VrlTarget`), a `to_typed()` failure
  causes the consumer to drop the event, increment a counter, and emit a warning log. Once
  dead-letter routing lands, failed events may instead be forwarded to an error path; until then,
  dropping is the uniform fallback.

Per-component shims are unidirectional (`Legacy -> Typed` only). The `datadog_agent`
source ships with a shim that knows the source's `LogEvent` key layout and produces a
typed container; the OTLP source ships with the equivalent shim for its layout.

The removal of untyped forwarders (above) is a compile-time gate that catches unmigrated
call sites. Enforcement against a missed `to_typed()` call at runtime is layered:

1. **Code-review gate.** Each component-migration PR adds `to_typed()` at the component's
   event-intake entry point. The PR checklist and review template for trace-migration
   PRs require the reviewer to confirm the intake-convert call is present for all
   trace-event paths through the component.
2. **Test coverage.** Each component-migration PR includes a test exercising a
   `Legacy`-sourced trace event end-to-end through the component. The test fails
   immediately if the intake conversion is absent (the typed accessor panics on the
   first event).
3. **Integration test suite.** The property-based round-trip tests (Plan of Attack)
   exercise full `Legacy`-source-to-`Typed`-sink paths, catching any component in the
   chain that fails to convert.
4. **Runtime backstop.** If a missed conversion reaches production despite the above
   layers, the typed accessor panics the task on the first `Legacy` event -- immediate,
   deterministic, with a diagnostic message identifying the fix. This is a task crash,
   not a controlled error (see Drawbacks for the tradeoff). The intent is that this
   layer never fires; it exists as a fail-loud safety net, not as the primary
   enforcement mechanism.

VRL programs are exempt from this layering: `VrlTarget` auto-converts on typed-path
access, so operators never need to reason about the migration state of upstream sources.

After every source, sink, and transform has been migrated, the `Legacy` variant and the
shims are deleted, leaving only the typed struct.

#### Wire serialization

Trace events cross internal-wire boundaries through disk buffers and the `vector`
source/sink, so Vector's event protobuf (`lib/vector-core/proto/event.proto`) needs wire
shapes for both migration variants. The full set of changes:

```protobuf
// Discriminator changes: existing oneofs gain typed-trace variants.

message EventWrapper {
  oneof event {
    Log log = 1;
    Metric metric = 2;
    LegacyTrace legacy_trace = 3;  // was: Trace trace = 3
    TypedTrace typed_trace = 4;    // new
  }
}

message EventArray {
  oneof events {
    LogArray logs = 1;
    MetricArray metrics = 2;
    LegacyTraceArray legacy_traces = 3;  // was: TraceArray traces = 3
    TypedTraceArray typed_traces = 4;    // new
  }
}

// Legacy messages: renames of Trace / TraceArray; field tags and shapes unchanged.

message LegacyTrace {
  map<string, Value> fields = 1;
  Value metadata = 2 [deprecated = true];
  Metadata metadata_full = 3;
}

message LegacyTraceArray {
  repeated LegacyTrace traces = 1;
}

// Typed wire shape: new messages mirroring the Rust types in earlier subsections.

message TypedTrace {
  Resource resource = 1;
  Scope scope = 2;
  ChunkContext chunk = 3;
  repeated Span spans = 4;
  Metadata metadata_full = 5;
}

message TypedTraceArray {
  repeated TypedTrace traces = 1;
}

message Resource {
  optional string service = 1;          // service.name
  optional string environment = 2;      // deployment.environment.name
  optional string host = 3;             // host.name
  Attributes attributes = 4;
  optional string schema_url = 5;
  uint32 dropped_attributes_count = 6;
}

message Scope {
  optional string name = 1;     // absent or empty on wire -> None in Rust
  optional string version = 2;  // absent or empty on wire -> None in Rust
  Attributes attributes = 3;
  optional string schema_url = 4;
  uint32 dropped_attributes_count = 5;
}

message ChunkContext {
  optional int32 priority = 1;          // -1=UserReject, 0=AutoReject, 1=AutoKeep, 2=UserKeep, other=Other
  optional string origin = 2;
  bool dropped = 3;
  Attributes tags = 4;
}

message Span {
  bytes trace_id = 1;                   // 16-byte big-endian, non-zero
  fixed64 span_id = 2;                  // non-zero
  optional fixed64 parent_span_id = 3;
  string trace_state = 4;               // raw W3C tracestate header
  fixed32 flags = 5;                    // OTLP Span.flags u32 verbatim

  string name = 6;
  int32 kind = 7;                       // 0=Unspecified, 1=Internal, 2=Server, 3=Client, 4=Producer, 5=Consumer, other=Other

  fixed64 start_time_unix_nano = 8;
  fixed64 duration_nanos = 9;
  SpanStatus status = 10;

  optional string resource_name = 11;   // Datadog-only
  optional string span_type = 12;       // Datadog-only

  Attributes attributes = 13;

  repeated SpanEvent events = 14;
  repeated SpanLink links = 15;

  uint32 dropped_attributes_count = 16;
  uint32 dropped_events_count = 17;
  uint32 dropped_links_count = 18;
}

message SpanStatus {
  int32 code = 1;                       // 0=Unset, 1=Ok, 2=Error, other=Other
  string message = 2;                   // populated for Error and Other; on decode, a non-empty
                                        // message with code Unset or Ok is dropped;
                                        // a counter incremented and a warning log is emitted
}

message SpanEvent {
  string name = 1;
  fixed64 time_unix_nano = 2;
  Attributes attributes = 3;
  uint32 dropped_attributes_count = 4;
}

message SpanLink {
  bytes trace_id = 1;                   // 16-byte big-endian, non-zero
  fixed64 span_id = 2;                  // non-zero
  string trace_state = 3;
  fixed32 flags = 4;                    // full u32 verbatim
  Attributes attributes = 5;
  uint32 dropped_attributes_count = 6;
}

message Attributes {
  map<string, AttrValue> entries = 1;
}

message AttrValue {
  oneof value {
    string string = 1;
    bytes bytes = 2;
    bool bool = 3;
    int64 int = 4;
    double double = 5;
    AttrArray array = 6;
    AttrMap map = 7;
  }
  // Unset oneof represents AttrValue::Null.
}

message AttrArray {
  repeated AttrValue values = 1;
}

message AttrMap {
  map<string, AttrValue> entries = 1;
}
```

The oneof tag is the discriminator. The fallible decode boundary (see Plan of Attack) is
a hard prerequisite for the typed proto step and must ship first. An older Vector that
has the fallible decode boundary but not the `TypedTrace` variant receives a
`typed_*`-tagged message, decodes it as `event: None` / `events: None`, and surfaces a
`DecodeError::UnknownEventVariant`; the consumer logs a warning and
drops the affected message. The pipeline continues running. All-Legacy traffic decodes
correctly on any Vector version that supports field tag 3, since
`LegacyTrace` / `LegacyTraceArray` keep that tag. `vector` source/sink chains that span
the typed migration must run a release line that includes at least the migration-
boundary release (the fallible decode boundary plus the legacy-layout hint precursor,
shipped together; see "Plan of Attack").

Single-event encoding via `EventWrapper` is 1:1. Array encoding is 1:1-or-2: in-memory
`TraceArray` (a `Vec<TraceEvent>`) can hold a mix of variants when a source that emits
`Typed` natively and one that still emits `Legacy` fan in to the same downstream
component, but the wire `EventArray.events` oneof must select one variant. The encoder
splits mixed arrays into a homogeneous-`Legacy` half and a homogeneous-`Typed` half,
emitting two consecutive `EventArray`s. `From` for the wire `EventArray` returns a
`SmallVec<[event::EventArray; 1]>` so the homogeneous case is allocation-free, and the
disk-buffer write and `vector`-sink encode sites loop over the result. `Finalizable` is
per-event, so split halves carry their own finalizers and ack independently. Decoders
see only homogeneous wire arrays; mixing reappears at fan-in points downstream. Per-
event ordering across variants within a mixed batch is not preserved by the split
(`[Legacy, Typed, Legacy, Typed]` emerges as `[Legacy, Legacy, Typed, Typed]`); trace
event ordering across a batch is not a spec-defined property, and no Vector sink derives
partition keys from event position within a batch.

Removing the `Legacy` Rust variant does not immediately retire the proto: `LegacyTrace`,
`LegacyTraceArray`, and the `legacy_*` oneof variants are first marked
`deprecated = true` for a release window so events written by older Vector instances
continue to decode. The per-component shim functions persist alongside the deprecated
proto as wire decoders -- dispatched on the `vector.trace_legacy_layout` hint (which
travels in `EventMetadata.value` inside `LegacyTrace`) -- so that decoded legacy records
materialise as typed events through the same per-source conversion logic the migration
used in-process. After the window passes, the proto messages are removed, field tag 3 is
added to `reserved` in both oneofs, and the shim functions are deleted.

Verification covers four cases: (1) both `EventWrapper` variants round-trip byte-exact;
(2) all-`Legacy` and all-`Typed` `TraceArray`s each round-trip byte-exact through
`EventArray`; (3) a mixed in-memory `TraceArray` encodes to two homogeneous wire
`EventArray`s and decodes back containing the same events (per-variant ordering
preserved within each half, across-variant interleaving not); (4) an older-Vector
simulation reads a `typed_*`-tagged message against a schema in which those variants are
unknown and surfaces a controlled `DecodeError::UnknownEventVariant` from which the
consumer recovers by logging a warning and dropping the message.
This pins the failure-mode property that motivated the sibling-variant design over the
alternatives in "Alternatives".

## Rationale

### Architectural choices

- The container shape mirrors the wire-level batching of both OTLP and Datadog: each
  `TraceEvent` is one `(resource, scope, chunk)` grouping. Source ingest and sink egress
  are pure mechanical translations between the wire shape and the container.
- Sharing `Resource` / `Scope` / `ChunkContext` across sibling spans is structural (a
  struct field), not pointer-based (an `Arc`). Disk-buffer serialization preserves the
  sharing for free; no `Arc` reconstruction or read-side interning is needed.
- The shape a user sees is the same whether the event arrived via OTLP or from the
  Datadog Agent. Source-native attribute maps are preserved on the appropriate typed
  level; nothing is copied into a parallel "extensions" map. Transforms can be written
  once and applied uniformly.
- Typed fields let transforms be written once. `Metric` demonstrates this model in
  Vector's architecture; extending to traces gives them parity and unblocks RFC 11851.
- The ingest boundary is typed-first: if an incoming wire field has a faithful typed home
  in `TraceEvent`, `Resource`, `Scope`, `ChunkContext`, `Span`, `SpanEvent`, or
  `SpanLink`, the source populates that typed field directly. Attribute maps and reserved
  keys remain only for payload that has no typed home or is intentionally preserved as
  wire-format state. This keeps cross-format egress a projection from one shared model
  rather than a translation from source-specific legacy layouts.
- Keeping the outer `Event::Trace(TraceEvent)` variant unchanged minimises churn at
  every call site that dispatches on `Event` (topology, buffers, finalizers, etc.); only
  the inner representation changes.

### Per-type design choices

- `Resource` promotes only the three semantic-convention fields both wire formats agree
  on (`service.name`, `deployment.environment.name`, `host.name`); other resource
  attributes stay in `Resource.attributes` under standard semantic convention keys.
  Promoting more would force Vector to track upstream semantic convention evolution or
  ossify a stale subset; promoting fewer would force every cross-format transform to
  read source-specific keys for common metadata. Format-specific consequences (legacy-
  key acceptance for `deployment.environment`, derivation fallbacks on Datadog egress)
  are in the corresponding mapping sub-RFC.
- Encoding `TraceId` / `SpanId` non-zero invariants in the type itself eliminates a
  class of malformed values by construction. OTLP defines all-zero IDs as invalid, and
  Datadog uses zero only as the "no parent" sentinel (already represented as `None`).
  Using unsigned integer types fixes the existing `i64`-coercion precision bug
  ([#14687](https://github.com/vectordotdev/vector/issues/14687)). The VRL surface
  exposes IDs as lowercase hex strings to match the dominant external representation
  across W3C `traceparent` headers, OTLP debug logs, the Datadog UI, and Jaeger / Zipkin
  search APIs, so VRL programs comparing IDs against external sources do not need to
  format-convert at the boundary.
- The closed-with-escape-hatch enum VRL surface (`SpanKind`, `SpanStatus`,
  `SamplingPriority`) exposes a single shared access pattern: a snake_case string for
  known variants and a raw integer for `Other(n)`. This lets VRL construct an error
  status with two field assignments (`.status.code = "error"; .status.message = "..."`)
  without any helper functions, and gives `Other(n)` values typed access through the
  integer form without an additional accessor surface.
- VRL `del()` against typed paths follows the existing `Metric` VRL surface: it clears
  `Option`-wrapped slots, removes attribute-map entries, shrinks vectors at the element,
  and raises a runtime error on required-by-construction fields and on the root path.
  Reusing the established `Metric` pattern keeps a single mental model for typed-event
  VRL access across event variants and means the underlying `VrlTarget` implementation
  can share the same `MetricPathError::InvalidPath` strategy that already errors loudly
  on disallowed paths.
- `TraceFlags` is sized to the OTLP wire field (`u32`), not the W3C `traceparent` byte
  (`u8`), so an `OTLP -> Vector -> OTLP` relay round-trips the full `Span.flags` /
  `Link.flags` word: the W3C trace-flags byte (bits 0-7), OTLP's parent- / link-target-
  remote tristate (bits 8-9, `CONTEXT_HAS_IS_REMOTE` and `CONTEXT_IS_REMOTE`), and OTLP's
  reserved bits 10-31. The same width is needed for the Datadog round trip on the link
  path, where `SpanLink.flags` is also `uint32` and the Datadog convention reserves bit
  31 as a "flags-are-meaningful" sentinel; a `u8` storage would clear that bit on every
  Datadog link. `bitflags::from_bits_retain` preserves the full word, so unknown bits
  (including forward-compat W3C additions such as the Level 2 `random` flag) round-trip
  without changing the type or its serialization. The W3C trace-flags byte is exposed as
  a derived view via `flags.w3c_byte()` for emission in `traceparent` headers, and the
  OTLP parent-remote tristate is exposed via `flags.context_is_remote() -> Option<bool>`.
- `Span.duration` is stored as `std::time::Duration` (nanosecond integer). Both wire
  formats carry non-negative durations as integer nanoseconds (OTLP as the difference
  between two `fixed64` epoch nanoseconds, Datadog as a single `int64` nanoseconds
  field), and `Duration` covers every value either wire format can carry. Wire-domain
  corner cases (negative Datadog `int64` on ingress, OTLP reversed timestamps, and
  overflow of either wire field on egress) are clamped on the corresponding boundary
  and declared as exclusions in the relevant sub-RFC. The VRL surface exposes
  `.spans[i].duration` as float seconds for ergonomic comparisons.
- `Attributes` stores leaves as `AttrValue`, an OTLP `AnyValue`-shaped enum, rather
  than reusing `Value`. The reuse alternative is recorded under "Reusing VRL `Value`
  for attribute storage" in Alternatives; `AttrValue` preserves the wire string-
  versus-bytes discriminator and NaN doubles structurally. This type avoids several
  round-trip exclusions from both sub-RFCs at the cost of one conversion layer at the
  VRL boundary (parallel to the existing typed-to-untyped conversion `VrlTarget`
  applies to the `Metric` event variant).

### Migration approach

- The migration uses an `enum TraceEvent { Legacy, Typed }` so each trace source, sink,
  and transform can migrate in its own PR while the rest of the system continues to
  operate against the representation it expects. See "Wholesale migration" under
  Alternatives for why a single atomic replacement was rejected.
- Per-component shims convert `Legacy -> Typed` only, never the reverse: a `Typed` event
  has no source provenance on which to base a back-conversion to a source-specific
  `LogEvent` shape. This forces the migration sequencing in the Plan of Attack --
  trace-aware consumers (sinks, transforms, VRL programs) must accept `Typed` input
  before any source flips to emitting `Typed` natively. The untyped forwarding methods
  (`get(path)`, `as_map()`, etc.) are removed from `TraceEvent` before the source steps;
  every remaining call site then fails to compile, making the consumer migration a
  mechanical fix-the-build task rather than a runtime-failure audit.
- Shim selection is keyed on a reserved sub-key `vector.trace_legacy_layout` in
  `EventMetadata.value` set by the producing trace source. The `vector` metadata
  namespace is read-only to VRL programs ([`compile_vrl`](../lib/vector-core/src/vrl.rs)
  calls `config.set_read_only_path(metadata."vector", true)`), so transforms between
  source and sink cannot accidentally delete or overwrite the hint. The metadata `Value`
  is serialized with every event record and passes through fan-in, disk buffers, and
  `vector` source/sink hops unchanged (unlike `EventMetadata.source_type`, which the
  topology source pump rewrites on every emission and so cannot serve as the selector
  across a serialised hop). Conversion is invoked explicitly by `to_typed(&mut self)`;
  immutable typed accessors panic on `Legacy` rather than converting on demand, because
  returning typed references through a `&self` accessor would require either mutating
  `self` or returning owned / `Cow` shapes that would have to be torn out again post-
  migration. The convention lives only for the duration of the migration and disappears
  with the `Legacy` variant; no new struct field or wire-format extension is needed.

## Drawbacks

- Breaking change for VRL configurations against today's `TraceEvent` key layout. Users
  must migrate to typed paths.
- The `trace_to_log` transform's output also changes; downstream VRL programs against
  its output must update.
- Topology granularity is coarser than per-span: each event carries up to a chunk's
  worth of spans (typically tens to hundreds, larger in deep call trees). Buffer-size
  limits expressed in events bound span counts less directly than the previous
  `LogEvent`-per-span design. `EventCount::event_count()` continues to return `1` per
  `TraceEvent` (not per span), so `component_received_events_total` and related
  accounting count container events, not individual spans. This is consistent with how
  `Metric` reports (one event per metric, not per sample); dashboards that expect
  per-span event counts will read lower values than the actual span throughput.
- Per-span operations (filter, sample, mutate one span) require VRL iteration over
  `.spans` rather than per-event treatment. A topology-level expand-on-input / collapse-
  on-output shim could let single-span transforms operate unchanged; that mechanism is
  deferred to implementation.
- The internal `event.proto` gains a new `TypedTrace` variant alongside the renamed
  `LegacyTrace`. `vector` source/sink chains spanning the typed migration must run a
  release line that includes at least the migration-boundary release (the fallible
  decode boundary plus the legacy-layout hint precursor, shipped together; see "Plan of
  Attack"). An older Vector at the migration-boundary release but without the
  `TypedTrace` variant surfaces a controlled `DecodeError::UnknownEventVariant` and
  drops the message; the pipeline continues running. See "Wire serialization" for
  details. This is documented in the release notes alongside the VRL-path migration.
- Every trace source and sink must be rewritten to produce/consume the typed container.
  The Plan of Attack sequences this so each component migrates independently, but it is
  non-trivial work.
- If the code-review, test-coverage, and integration-test enforcement layers described
  in "Migration: coexistence of `LogEvent` and typed representations" all fail to catch
  a missed `to_typed()` call, the runtime backstop panics the affected task on the first
  `Legacy` event. This is a task crash rather than a controlled error; it is production-
  reachable via fan-in from sources that have not yet migrated to `Typed`, disk-buffer
  replay, and `vector` source traffic from older peers. The intent is that the earlier layers prevent this from
  occurring; the panic is a fail-loud safety net, not the primary enforcement. A
  panicked task may leave in-flight event finalizers unfired, causing upstream
  backpressure to stall until the task is restarted by the topology supervisor. VRL
  programs are not affected (`VrlTarget` auto-converts).
- Wire-format-specific drawbacks (Datadog producer-side keyset-disjointness convention,
  Datadog `Span.error` normalization, OTLP `deployment.environment` legacy-key rewrite,
  etc.) are listed in the corresponding mapping sub-RFC.

## Prior Art

- [OTLP traces protocol](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)
  -- the primary shape this RFC adopts. The container `TraceEvent` is structurally one
  `ScopeSpans` plus its `Resource` and the Datadog-only `ChunkContext`.
- [Datadog APM agent-to-backend protobuf](https://github.com/DataDog/datadog-agent/tree/main/pkg/proto/datadog/trace)
  -- the second native format Vector targets.
- [Datadog Agent OTLP ingest](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go)
  -- the normative reference for OTLP-to-Datadog field mappings; see the
  [Datadog mapping sub-RFC](2026-04-29-25329-trace-data-model/datadog-mapping.md) for the
  role specification and the cross-format conformance rule. Adopting an existing reference
  rather than defining a parallel mapping minimises divergence between Vector's
  `OTLP -> datadog_traces` path and the Datadog Agent's own OTLP ingest.
- [2024-03-22-20170 draft](https://github.com/hdost/vector/blob/add-trace-data-model/rfcs/2024-03-22-20170-trace-data-model.md)
  -- an earlier draft that modelled the event as a `ResourceSpans` (batch of multiple
  scope/spans groupings). The current RFC adopts a similar container shape but at finer
  granularity (one event per `ScopeSpans` rather than per `ResourceSpans`).

## Alternatives

Wire-format-specific alternatives are documented in the corresponding mapping sub-RFC.

### OTLP-only schema with Datadog round-trip via import/export encoding

Adopt the OTLP wire schema unchanged as the internal model -- `TraceEvent` carries one
`Resource`, one `Scope`, and a `Vec<Span>`, with no Datadog-specific typed fields -- and
achieve `Datadog -> Vector -> Datadog` round-trip transparency through an
import/export layer that encodes every Datadog-specific concept under reserved attribute
keys. This is the limit case of the reserved-key pattern the proposal already applies to
`_dd.payload`, `_dd.tracer`, and `_dd.meta_struct`: extend it to chunk-scoped state,
`Span.resource_name`, `Span.span_type`, and `SamplingPriority`, and let one container
shape carry both formats.

The appeal is OTLP's status as the de facto industry trace schema. A single canonical
container removes `TraceEvent.chunk`, the `SamplingPriority` enum, and the typed
Datadog-native span fields from the API surface, leaving only the OpenTelemetry-shaped
`Resource` / `Scope` / `Span`. Cross-format consumers see one schema. Future OTLP
signals (logs, metrics) inherit the same approach with no additional design.

Rejected because the encoding required to carry all Datadog-specific concepts under OTLP
attributes without data loss is not uniform with how OTLP-sourced data sits in the same
attribute maps, and the non-uniformity is observable to every transform on the typed
surface:

- **Chunk-scoped state has no faithful per-span encoding.** `TraceChunk.{priority,
  origin, droppedTrace, tags}` apply uniformly to every span in the chunk; the only
  place to carry them under a pure-OTLP schema is on every `Span.attributes` map in the
  chunk. Per-span duplication encodes a structural invariant -- every span in a chunk
  shares the same value -- as an arithmetic coincidence that any single-span attribute
  mutation silently breaks, inflates the wire by a factor proportional to chunk size,
  and forces Datadog egress to recover the chunk grouping by attribute comparison rather
  than by container traversal. Promotion to `Resource.attributes` is not a workaround: a
  Datadog `TracerPayload` may contain multiple chunks against the same resource, so the
  resource grouping does not coincide with the chunk grouping. The proposed
  `TraceEvent.chunk` field reflects the structural fact directly; the encoding is one
  slot per chunk-scoped value rather than `N spans × one entry per chunk-scoped value`.
- **Datadog-native span fields lose typed access.** `Span.resource_name` and
  `Span.span_type` are core inputs to Datadog routing and APM stats aggregation.
  Encoding them as `Span.attributes."_dd.span.resource"` / `"_dd.span.type"` is
  mechanically lossless but forces every Datadog-aware transform, sink, and VRL program
  to read them as string-keyed attribute lookups rather than typed accessors. The same
  loss applies to `SamplingPriority`: typed as an enum with an `Other(i32)` escape hatch
  in the proposal, it degrades to a string-encoded integer under the alternative,
  surrendering both the well-known-values ergonomic and construction-time validation.
- **Reserved-key partitioning becomes a per-span cost.** The proposal's reserved-key
  pattern is contained to two locations -- `Resource.attributes` (`_dd.payload`,
  `_dd.tracer`) and `Span.attributes` (`_dd.meta_struct`) -- where no typed home exists.
  A pure-OTLP design extends the pattern to every Datadog concept, so every transform
  walking `Span.attributes` must partition the map into user attributes and Datadog
  wire-state encoding to avoid mishandling either, and every sink must do the same on
  egress. The proposal's typed fields make the partition once at the type level.
- **The round-trip guarantee weakens from structural to conventional.** The proposal's
  `Datadog -> Vector -> Datadog` guarantee rests on structural identity:
  `TraceEvent.chunk` is read back into one `TraceChunk` per event by container
  traversal. Under the alternative, the guarantee rests on every transform respecting
  the reserved-key convention; any transform that drops `_dd.chunk.priority` from a
  span's attributes silently loses the chunk's sampling priority on egress. Today's
  `TraceEvent(LogEvent)` exhibits the same convention-dependent failure mode and is
  part of why this RFC exists.

The proposal already adopts OTLP as the primary shape: `Resource`, `Scope`, and `Span`
are OTLP types, semantic conventions name the typed resource fields, attribute keys
follow OpenTelemetry naming, and the Datadog mapping is expressed as projections onto
that primary shape. The minimal Datadog-specific delta (`TraceEvent.chunk`,
`Span.resource_name`, `Span.span_type`, `SamplingPriority`) is the smallest set of
extensions that keeps Datadog-trace concepts on the typed surface and chunk-scoped state
structurally distinct from per-span state. The pure-OTLP alternative trades that delta
for a uniform type signature, paying the cost on every consumer of the surface in
exchange for a single-schema invariant at the type-definition site.

### One span per event (`TraceEvent { span: Span, metadata }`)

Carry a single span per event. This shape offers two ergonomic advantages: the internal memory usage
of a single span (with the resources shared) is more consistent and granular, and per-span
operations (filter, sample, mutate one span) work directly without iteration. Recovering the latter
in the container shape is a topology-level shim concern, deferred to implementation. This, however,
requires the `Resource`, `Scope`, and `ChunkContext` to either be duplicated for each span or to be
shared via `Arc`.

Rejected because Vector's disk buffers serialize each event as one record: `Arc` sharing collapses
on serialization, every span on disk gets a full inline copy of resource/scope/chunk, and on read
every span gets an independent allocation, thus costing Vector both extra costs in serialization and
deserialization as well as the associated memory expansion and sink-level reassembly mechanics. The
container shape eliminates the inflation by aligning the event boundary with the wire-batching
boundary, so the shared context appears once per grouping on disk and in memory regardless of how
the path is buffered, and `Arc` machinery is not needed.

### Parallel `Event` variants for new and old trace formats

Introduce `Event::NewTrace` alongside `Event::Trace`, leaving the existing `TraceEvent`
untouched. Rejected because it splits trace handling across two `Event` variants for the
duration of the migration, forcing every topology-level dispatch site to handle both.
The tagged-inner approach contains the duality inside `TraceEvent`, leaving
`Event::Trace` as the single dispatch arm.

### Discriminated union (`Trace::{Otel, Datadog}` or `Span::{Otel, Datadog})

Carry each format as-is and dispatch at every consumer. Rejected because it directly
inverts the stated pain -- every transform and every cross-format sink would handle two
shapes with the possibility of more later. This is effectively the status quo over
`LogEvent` just with predefined fields.

### Single merged `attributes` map with richer typed fields

Promote additional concepts (service, env, host *and* all semantic-convention
equivalents) to typed fields. Rejected because the semantic convention space is large
and evolving; fixing it in typed fields either forces Vector to track upstream releases
or ossifies a stale subset. The proposal types only the three resource fields both
formats agree on; the rest stay in source-native attributes where users already expect
them.

### Reusing VRL `Value` for attribute storage

Store attribute leaves directly in VRL's `Value`, reusing the existing accessor API and disk-buffer
encoding. The trace surface gains no new types and trace VRL programs share their type system with
`LogEvent` and `Metric`. Rejected because `Value::Bytes` collapses the wire string-versus-bytes
discriminator onto a single variant and `Value::Float` (`NotNan<f64>`) cannot carry NaN. A number of
round-trip exclusions (OTLP `bytes_value`-as-UTF-8, OTLP `double_value = NaN`, and Datadog `metrics`
NaN handling) therefore become unavoidable. The proposal's `AttrValue` preserves both axes
structurally and limits the conversion cost to the VRL boundary, where `VrlTarget` already performs
analogous typed-to-untyped conversions for the `Metric` event variant. The wider fix -- a
`Value::String` variant and a NaN-admitting float carrier in `Value` itself -- is in scope for VRL
rather than the trace data model and is recorded under Future Improvements.

### Timing as `start_time` + `end_time` (OTLP-native)

OTLP stores `start_time_unix_nano` and `end_time_unix_nano` as two independent
`fixed64`s. Datadog stores `start` plus `duration`. The driving factor for adopting
`start + duration` is that `duration` is more useful than `end_time` in realistic
transforms (filtering slow spans, computing percentiles, classifying long-running
requests), so the chosen representation also matches transform access patterns.

### Duration as `f64` seconds

Storing `duration` as `f64` was considered for VRL ergonomics. Rejected because both
OTLP (`fixed64` nanoseconds) and Datadog (`int64` nanoseconds) carry duration as integer
nanoseconds, and `f64`'s 53-bit mantissa cannot exactly represent every integer
nanosecond beyond `2^53 ns` (about 104 days). Storing `duration` as
`std::time::Duration` preserves the wire domain for all non-negative values. Datadog's
`int64` wire field permits negative values that `std::time::Duration` cannot represent;
these are clamped to zero on ingress and declared in the Datadog mapping sub-RFC. The
VRL surface exposes float seconds at the boundary; a complementary integer-nanosecond
view (`.spans[i].duration_nanos`) is documented under "Future Improvements".

### `SpanStatus` as a closed enum

Defining `SpanStatus` without an escape hatch would silently coerce any unrecognized
status code introduced by a future OpenTelemetry version to `Unset` (the proto3
default), breaking the `OTLP -> Vector -> OTLP` relay guarantee for those spans. The
`Other(i32, String)` variant stores the raw code and message verbatim and egresses them
unchanged, preserving relay fidelity by the same mechanism used for `SpanKind`. The
Datadog egress path has no status-code wire field; `Other` values follow the
`Span.error` rule (non-zero code maps to `error = 1`, zero code to `error = 0`).

Only `Error` carries a string because the OpenTelemetry trace specification's
[Set Status](https://opentelemetry.io/docs/specs/otel/trace/api/#set-status) rule states
"Description MUST only be used with the Error StatusCode value." A wire `Status.message`
paired with `code = UNSET` or `OK` is non-conformant and is dropped on ingest.

### `TraceFlags` via `enumflags2`

[`enumflags2`](https://crates.io/crates/enumflags2) was considered as the bitfield
generator for `TraceFlags`. Rejected because `enumflags2` rejects undefined bits at
construction time, which would silently lose forward-compatibility data on the OTLP
`Span.flags` / `Link.flags` word: OTLP's reserved bits 10-31, the W3C Trace Context
Level 2 `random` flag once defined, and Datadog's bit-31 link sentinel would all be
discarded when read by an unaware Vector build. [`bitflags`](https://crates.io/crates/bitflags)
supports `from_bits_retain`, which preserves the full 32-bit word intact, so the same
Vector build round-trips spans with not-yet-defined flag bits without modification.

### Parsed `TraceState`

Storing `TraceState` as `IndexMap<KeyString, String>` would let transforms operate
on entries without an accessor layer. Rejected because every source and sink would have
to invoke the parser/serializer even for pure-relay pipelines, and because the W3C-
imposed bounds (32 entries, 512 bytes total) and typical real-world headers (a single
short entry) mean per-entry allocation costs more than re-parsing the raw header per
accessor call.

### Wholesale migration

Replace `TraceEvent(LogEvent)` with the typed container in one PR. Rejected because the
resulting PR would touch every trace source, every trace sink, the APM stats
aggregator, every trace-aware transform, and a large body of tests simultaneously. The
chosen `enum TraceEvent { Legacy, Typed }` coexistence design lets each component
migrate in its own PR, subject to a partial-order constraint that consumers migrate
before producers (see "Plan Of Attack").

### Feature-flagged switch

Gate the new representation behind a Cargo feature or runtime flag until all components
are migrated, then flip the default. Rejected because feature combinations proliferate
quickly across every trace source/sink and VRL, and because a runtime flag would
require duplicate code paths in performance-sensitive components.

### Wire serialization shape

The chosen design is selected against two imperatives: incompatibility with older
Vector instances must surface loudly (not as silent data drops), and the post-migration
wire schema should carry no vestiges of the migration. The encoder's 1:1-or-2 boundary
-- a mixed in-memory array splits into a homogeneous-`Legacy` half and a
homogeneous-`Typed` half on encode -- is the cost paid for both imperatives. Each
rejected alternative fails at least one:

- **Extend `Trace` with a typed-fields field**, discriminator by field-presence. Fails
  loud-incompatibility: an older Vector ignores the unknown field and decodes the rest
  as a legacy event with empty `fields`, silently corrupting the receiver's view of the
  batch.

- **Per-element oneof inside a `MixedTraceArray`**, each array element internally
  discriminating between `LegacyTrace` and `TypedTrace`. Fails post-conversion-vestige:
  the end-state oneof has a single remaining variant once `LegacyTrace` is retired, and
  flattening it to a plain `repeated TypedTrace traces = 1` requires a second wire-
  format migration.

- **Two repeated fields inside a single `TraceArray`**, encoder always 1:1. Fails
  loud-incompatibility: an older Vector silently drops the unknown second field,
  decoding a typed-only message as an empty `TraceArray`. Loud failure on older Vector
  requires the discriminator to live at the oneof level, where the older Vector
  recognizes "unknown variant"; a sibling field at a known message level is invisible
  to it.

## Outstanding Questions

- N/A.

## Plan Of Attack

This Plan of Attack covers the format-agnostic data-model and migration work owned by
this RFC. The per-format work (OTLP shim and encoder, Datadog shim and encoder, source
and sink flips, format-specific tests) is sequenced inside the
[OTLP mapping](2026-04-29-25329-trace-data-model/otlp-mapping.md) and
[Datadog mapping](2026-04-29-25329-trace-data-model/datadog-mapping.md) sub-RFC Plans
of Attack. The overall sequencing across all three RFCs is:

1. Format-agnostic prerequisites (this RFC), in order: fallible decode boundary; legacy-
   layout hint precursor; `TraceEvent` migration enum; internal `TypedTrace` proto
   extension; VRL typed-path dispatch.
2. Per-format shim landings (sub-RFCs), in either order: OTLP `Legacy -> Typed` shim and
   encoder; Datadog `Legacy -> Typed` shim and encoder. Independent of each other.
3. Cross-cutting transform and VRL work (this RFC): VRL auto-convert on typed-path
   access of `Legacy` events; `sample` and `trace_to_log` transform migrations.
4. Removal of untyped forwarders (this RFC) -- the compile-time gate that catches any
   unmigrated consumer.
5. Per-format source flips (sub-RFCs), in either order: `opentelemetry` source emits
   `Typed`; `datadog_agent` source emits `Typed`.
6. Post-migration cleanup (this RFC): collapse the `TraceEvent` enum to a struct; mark
   legacy proto deprecated; eventual proto removal.

The `enum TraceEvent { Legacy, Typed }` coexistence is what makes the sequence
possible. The sequencing rule is for trace-aware consumers (sinks, transforms, VRL
programs) to migrate to `Typed`-native input before any source flips to emitting
`Typed` natively, because per-component shims are unidirectional (`Legacy -> Typed`
only) and a `Typed` event has no source provenance on which to base a `Typed -> Legacy`
conversion.

The format-agnostic PRs owned by this RFC are:

- [ ] Make the proto decode boundary fallible. Replace the infallible
  `From<EventWrapper>` / `From<EventArray>` impls in
  `lib/vector-core/src/event/proto.rs`, which today call `proto.event.unwrap()` /
  `events.events.unwrap()`, with fallible conversions that surface a
  `DecodeError::UnknownEventVariant` when prost decodes a message with an unknown oneof
  tag (oneof variant `None`). Update call sites in sources, sinks, and the disk-buffer
  reader to increment a counter, emit a warning log, and drop the affected message rather
  than panicking the task. This is an existing bug independent of the trace migration
  -- any unknown oneof tag added to `event.proto` today crashes the decoding task. Hard
  prerequisite for every subsequent step: the typed proto extension, the cross-version
  `vector` source/sink compatibility story, and the disk-buffer backward-compat path
  all depend on the controlled error behaviour this step introduces (see "Wire
  serialization"). Must ship in a release line before any `TypedTrace` producer ships.
- [ ] Land the legacy-layout hint in the `opentelemetry` and `datadog_agent` sources as
  a precursor. Purely additive -- no consumer reads the key yet. Carrier and sub-key are
  specified in "Migration: coexistence of `LogEvent` and typed representations". Must
  ship in the same release line as the fallible decode boundary above so that "the
  migration-boundary release" is a single version operators can target -- a producer at
  this version emits hints for any future receiver that consumes them, and a receiver
  at this version handles unknown variants gracefully.
- [ ] Convert `TraceEvent` to the migration enum and introduce the supporting types per
  "Migration: coexistence of `LogEvent` and typed representations". Every component
  continues to produce and consume `Legacy`; no functional change.
- [ ] Extend Vector's internal event proto with the typed wire shape, per "Wire
  serialization". Hard prerequisite for any source-flip step: without it, disk buffers
  and the `vector` source/sink panic on the first `Typed` event.
- [ ] Migration guide for users (consolidated across all three RFCs):
  - field-by-key VRL programs against the old `TraceEvent` (must move to typed paths;
    legacy paths break against `Typed` events);
  - field-by-key VRL programs against the old `trace_to_log` output (must move to the
    new uniform layout);
  - the wire-mapping documentation contributed by each sub-RFC's Plan of Attack;
  - removal of the legacy `tracerPayloads`-empty Datadog ingest path (owned by the
    Datadog mapping sub-RFC's Plan of Attack).
  - cross-version `vector` source/sink chains spanning the typed migration must run a
    release line that includes at least the migration-boundary release (see Drawbacks).
- [ ] Add VRL typed-path dispatch for `.resource.*`, `.scope.*`, `.chunk.*`, and
  `.spans[*].*` on `VrlTarget`. Typed paths against `Typed` events resolve normally.
  Typed paths against `Legacy` events return `null` without attempting conversion (the
  shim registry is not yet populated at this step). Untyped paths against `Typed` events
  return a deterministic runtime error. Lands after the migration enum step; no
  dependency on the per-format shims.
- [ ] Enable VRL auto-convert in `VrlTarget`: replace the `Legacy`-event `null` return
  introduced in the previous step with an in-place call to `to_typed()` on first
  typed-path access (`VrlTarget` has `&mut self` access). If `to_typed()` fails (absent
  or unrecognized `vector.trace_legacy_layout` hint), `VrlTarget` aborts the VRL
  expression with a runtime error; the event is forwarded to the topology error path
  unchanged, a counter is incremented, and a warning log identifies the failing event.
  Lands after both per-format shims (sub-RFC Plans of Attack) so that auto-convert
  succeeds for all registered layout hints.
- [ ] Migrate the `sample` transform (and tests) to typed access. The `sample`
  transform operates per-event; today's per-`TraceChunk` atomicity is incidental (one
  `LogEvent` per chunk), not an intentional sampling guarantee. After the typed
  migration, the per-event unit narrows from `TraceChunk` to
  `(TraceChunk, Span.service)` for Datadog-sourced events because multi-service chunks
  are split on ingest. The migration guide documents this change for operators who rely
  on the incidental atomicity and points at the chunk- / trace-stable sampling Future
  Improvement as the path to an intentional guarantee.
- [ ] Migrate the `trace_to_log` transform to typed access; emit a uniform, source-
  independent `LogEvent` layout. Document the new key layout in the migration guide.
- [ ] Remove the untyped forwarding methods (`get`, `insert`, `as_map`, `as_ref` to
  `LogEvent`, etc.) from `TraceEvent`. Call sites that still use them through
  `TraceEvent` become compile errors; each migrates to the typed accessor API or
  pattern-matches into the `Legacy(LogEvent)` arm explicitly. The build must be green
  before the source-flip steps in the sub-RFC Plans of Attack.
- [ ] Collapse the `TraceEvent` enum to a struct with only the typed variant's fields,
  after both source flips land. The per-component shim functions are retained as wire
  decoders for the deprecation window so that `LegacyTrace`-encoded events from disk
  buffers and older peers continue to decode. Mark the legacy proto messages
  `deprecated = true` per "Wire serialization".
- [ ] A follow-up PR after the deprecation window removes the legacy proto messages,
  reserves the field tags, and retires the shim functions.

## Future Improvements

- Topology-level per-span shim: a transform mode that fans out a `TraceEvent` into per-
  span events, runs a downstream transform once per span, and collapses results back
  into the container. Lets single-span transforms be authored without explicit iteration
  while keeping the wire-aligned event shape as the source of truth.
- VRL helpers for trace-state parsing/encoding: `parse_trace_state`,
  `encode_trace_state`, `merge_span_attributes`. Format-specific decode helpers
  (`decode_otlp_span`, `decode_datadog_span`) are listed in the respective mapping sub-
  RFCs.
- Lossless integer-nanosecond view for span duration: `.spans[i].duration` is exposed
  as float seconds, exact for any duration under `2^53 ns` (about 104 days). Workloads
  needing access to durations beyond that limit can have a complementary
  `.spans[i].duration_nanos` view added without affecting the underlying data model.
- Link-based routing: a trace-aware router transform that emits to different sinks
  based on `SpanLink` targets.
- Stateful trace-aggregator transforms: tail-based sampling, per-trace APM-stats
  aggregation, and similar trace-scoped operations expressed as transforms over the
  wire-aligned container shape.
- Trace- or chunk-stable sampling: an intentional sampling guarantee that makes a
  single keep/drop decision per `trace_id` (or per chunk identifier) and applies it
  consistently to every event derived from that trace/chunk. Today's `sample` transform
  has no such guarantee; it operates per-event, and any per-chunk atomicity is
  incidental to the pre-migration `LogEvent`-per-chunk layout. This may be added to
  `sample` as a configurable mode or shipped as a separate component (e.g.
  `trace_sample`); the choice is deferred to the implementation.
- Distinct `Value::String` variant in VRL's `Value` separate from `Value::Bytes`, plus
  a NaN-admitting float carrier. With `AttrValue` in place the trace round-trip no
  longer drives this work. The remaining motivation is in `LogEvent` and `Metric`:
  string-typed values are carried in `Value::Bytes` today, forcing every consumer to
  repeat a UTF-8 validation scan, and `Value::Float`'s `NotNan<f64>` constraint forces
  the trace VRL boundary to degrade `AttrValue::Double(NaN)` to `Value::Null` on read.
  Adopting both changes in `Value` would amortize the validation cost across log and
  metric paths and let the trace VRL boundary surface NaN as a typed `Float`. The
  cross-cutting cost gates the work: every `Value` consumer in Vector is affected, and
  admitting NaN requires a `Value: Ord` / `Eq` / `Hash` redesign (`f64::total_cmp`
  ordering, IEEE-754 versus structural equality, hashing the raw bits).
