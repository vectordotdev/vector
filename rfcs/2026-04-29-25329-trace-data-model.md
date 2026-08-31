# RFC 25329 - 2026-04-29 - Internal Trace Data Model

This RFC replaces the inner representation of Vector's `TraceEvent`, today a thin newtype over
`LogEvent`, with a strongly-typed container that mirrors the wire-level batching of OTLP and
Datadog APM traces. Each `TraceEvent` carries one `Resource`, one `Scope`, an optional
Datadog-specific `ChunkContext`, and the `Vec<Span>` belonging to that grouping, plus the existing
`EventMetadata`. The container shape, together with the wire-format mappings specified in the
two sub-RFCs below, yields zero-loss `OTLP -> Vector -> OTLP` and
`Datadog -> Vector -> Datadog` round trips, including across Vector's disk buffers, and gives
transforms a uniform typed surface across the two source formats.

The full proposal is split across three documents:

- This document defines the typed data model, the VRL surface, the temporary
  `TraceEventCompat` coexistence enum and shim mechanism, and Vector's internal
  protobuf serialization.
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
- Add the fallible `trace_span_status`, `trace_span_link`, `trace_span_event`, and
  `trace_flags` VRL functions for atomic construction and related-field updates.
- Define the migration strategy that lets each trace-producing or trace-consuming component
  migrate independently: the temporary
  `enum TraceEventCompat { Legacy(LegacyTraceEvent), Typed(TraceEvent) }`
  coexistence, the per-source `Legacy -> TraceEvent` shim mechanism keyed on
  `vector.trace_legacy_layout`, and the compile-time gating that catches unmigrated
  consumers. The permanent `TraceEvent` type is always the typed structure.
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

- New trace sources/sinks (Zipkin, Jaeger, etc.).
- APM stats computation semantics (already covered by RFC 9862).
- Zero-loss cross-format round-trip (`Datadog -> OTLP -> Datadog` or
  `OTLP -> Datadog -> OTLP`).
- `TracerPayload.containerDebug` (Datadog-internal container-tag-resolution diagnostic);
  dropped on ingest, not synthesized on egress.
- Implementation mechanics that do not change a specified contract. Rust API
  shapes, crate and constructor internals, protobuf field layouts beyond the
  discriminator tags and presence rules, review checklists, and similar
  decisions remain implementation-time choices. This RFC and the mapping
  sub-RFCs specify the invariants, VRL surface, and wire mappings those
  choices must satisfy.

### Zero-loss round-trip exclusions

The effective-equivalence guarantee does not cover the following model-level input shapes.
Each is justified by a paragraph in the Implementation or Rationale section below.
Wire-format-specific exclusions (the OTLP deprecated-environment rewrite,
Datadog `Span.error` normalization, `meta`/`metrics` producer-side disjointness, etc.) are
declared in the corresponding sub-RFC's Scope section.

- **All-zero `TraceId` or `SpanId`** are rejected on every ingress path; the span (or
  link) carrying the zero ID is dropped. See "Identifiers" for the drop granularity and
  the sub-RFCs for per-format detection.
- **Wire-domain normalization**: durations and timestamps outside a destination field's
  numeric domain are clamped to the nearest representable endpoint on encode and
  reported. Derived timestamps follow the same rule. The mapping sub-RFCs identify
  format-specific consequences.
- **Multi-hop topologies that relay traces through intermediate `vector` source/sink hops**
  may lose Datadog agent-envelope state by default; the Datadog mapping sub-RFC's
  "Envelope reconstruction policy" documents the mechanism and the operator-configurable
  passthrough.

When these RFCs say a failure, drop, or normalization is "reported", the implementation
uses Vector's standard component error or data-loss telemetry. Exact category names,
metric labels, log fields, and emission mechanics are implementation details governed
by Vector's instrumentation specification.

Numeric boundary behavior follows one policy throughout the model and mappings:

- Explicit writes must fit the target field's numeric domain and fail atomically
  otherwise.
- Encoding and derived arithmetic never wrap or panic. Values outside the destination
  field's domain are clamped to the nearest endpoint and reported.
- Mapping- or relay-generated dropped-count increments saturate and remain observable
  after saturation.

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

A `TraceEvent` carries one `Resource`, one `Scope`, an optional `ChunkContext`, and a
`Vec<Span>`. VRL accesses these directly:

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
        span.status = trace_span_status!("error", message: "slow query")
    }
    span
})

# Read a semantic-convention attribute on the root span, falling back to a
# Datadog-native key.
.user_id = .spans[0].attributes."user.id" || .spans[0].attributes."usr.id"
```

The typed surface is uniform across both wire formats: the same paths are valid regardless
of source, with `.chunk` reading as `null` when the event has no Datadog chunk context.
Format-specific encoding details (how Datadog's three span-attribute partitions merge into
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

The `trace_to_log` transform is retained and emits exactly one `LogEvent` per
`TraceEvent`; it does not fan out spans. The log root is the canonical typed VRL object
with exactly the `resource`, `scope`, `chunk`, and `spans` fields and the same nested
projections, nulls, strings, bytes, and numeric values that typed-path reads expose.
`EventMetadata` and finalizers transfer to the log as event metadata rather than being
inserted into its fields. This defines one source-independent output shape; the user
migration guide provides the old-to-new field mapping and examples.

### Implementation

#### `TraceEvent`

```rust
pub struct TraceEvent {
    resource: Resource,
    scope:    Scope,
    /// Datadog-only chunk-scoped state; absent when the source has no chunk concept.
    chunk:    Option<ChunkContext>,
    /// Spans belonging to this resource/scope and, when present, chunk grouping.
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
```

Wire mappings must reject malformed structural input rather than truncate, pad, or
guess its meaning unless that mapping explicitly defines a normalization. The mapping
sub-RFCs define the rejection granularity and telemetry.

Unless a mapping defines a stricter disposition, an unrepresentable malformed leaf drops
the smallest independently countable model item that contains it: an invalid attribute
value or nested element drops that attribute entry; an invalid link or event field drops
that link or event; and an invalid required span field drops the span. The corresponding
in-band dropped count is saturating-incremented when the receiving or destination
representation carries one, and the drop is reported. Sibling items continue through
the mapping.

Zero `TraceId` and `SpanId` values are unrepresentable in a well-formed event by the
`NonZero` types above. Every construction site rejects and reports zero inputs.
The internal `TypedTrace` proto decode applies the same rule
(covering disk-buffer replay after partial writes and `vector` source/sink transport errors): a
buffered or wire-transported event whose `trace_id` or `span_id` decodes to zero is treated as
corruption. A `trace_id` whose decoded byte length is not exactly 16 is treated identically to a
zero `trace_id`: the same per-link or per-span disposition and reporting apply.

Drop granularity is structural and uniform across sources: a zero `SpanLink.span_id` or
`SpanLink.trace_id` drops only the affected link and saturating-increments
`Span.dropped_links_count`; a zero `Span.trace_id` or `Span.span_id` drops the enclosing
span. Every rejected span is reported. If every span in a candidate `TraceEvent` is
rejected, the event is dropped and reported.

Any future relay-side drop of a `SpanEvent` or attribute follows the same convention: the
corresponding `dropped_events_count` / `dropped_attributes_count` field on the enclosing
item is saturating-incremented and the drop is reported. All in-band dropped counts use
saturating addition while every additional drop is still reported out of band. The relay
never silently shrinks an in-band count relative to what was received.

A `TraceEvent` whose `spans` vector is otherwise empty -- a wire-level empty grouping forwarded
as-is, or a transform filtering every span out -- passes through unchanged. Sinks emit the
corresponding empty wire shape, fire finalizers on successful delivery, and report the
condition without breaking ack-chain durability semantics (Kafka offset commits, source
disk buffers, etc.). The internal proto encoder applies the same rule.

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

##### VRL surface for `Span.duration`

VRL reads `Span.duration` as floating-point seconds and accepts integer or float seconds
on write. A write must be finite and non-negative. It is converted to nanoseconds by
rounding to the nearest integer nanosecond, with exact halfway values rounded up, and
must fit `std::time::Duration`; other values raise a runtime error. Conversion and
validation complete before the stored duration is changed; `-0.0` is accepted as zero.

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

/// Datadog `TraceChunk`-scoped state.
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
`Other(n)`. Every construction site must normalize raw values through a shared validated
constructor. As a consequence, for example,
`SpanKind::Other(3)`, `SpanStatus::Other(2, _)`, and `SamplingPriority::Other(1)` are
unrepresentable in any well-formed `TraceEvent`, and pattern matches on the canonical
variants are exhaustive for the known-value space.

##### VRL surface for the closed-with-escape-hatch enums

`SpanKind` and `SamplingPriority` share a VRL access pattern:

- The discriminator is a snake_case string for each known variant (`SpanKind`:
  `"unspecified"` / `"internal"` / `"server"` / `"client"` / `"producer"` / `"consumer"`;
  `SamplingPriority`: `"user_reject"` / `"auto_reject"` / `"auto_keep"` / `"user_keep"`)
  and an integer for the `Other`
  variant. Reading returns the snake_case string when the value matches a known variant
  and the raw integer otherwise. Writing a recognized string sets the corresponding
  variant; writing an integer that matches a known variant's wire number sets that
  variant (so `.spans[i].kind = 3` is equivalent to `.spans[i].kind = "client"`); other
  integers within the `i32` domain set `Other(n)`. An integer outside the `i32` domain,
  or any other value (e.g. a non-canonical string), raises a VRL runtime error before
  mutation.
- On typed event paths, `SpanStatus.code` and `SpanStatus.message` are read-only
  projections. `code` reads as `"unset"` / `"ok"` / `"error"` for known variants and as
  the raw integer for `Other(n)`; `message` reads the inner string for `Error(s)` /
  `Other(_, s)` and `""` for `Unset` / `Ok`. Writing or deleting either child path
  raises a VRL runtime error. Status values are replaced atomically:

  ```coffee
  .spans[i].status = trace_span_status!("error", message: "slow query")
  .spans[i].status = trace_span_status!("ok")
  del(.spans[i].status) # resets to Unset
  ```

  `trace_span_status` returns the canonical `{ code, message }` object accepted by
  whole-status assignment and accepts the same string / integer discriminator domain as
  `SpanStatus`; integer codes must fit `i32`. An `"error"` code requires a non-empty
  `message`; `"unset"` / `"ok"` reject a `message`; `Other(n)` accepts an optional
  message defaulting to `""`.
  Invalid combinations return a descriptive error before assignment, so the target
  remains unchanged. `del(.spans[i].status)` atomically replaces the status with `Unset`
  and returns the previous status value. Local copies (for example, `span` inside
  `map_values`) remain ordinary VRL objects; their complete status is validated when
  written back to the typed event.

The typed model and wire codecs can transport `Error("")`, which is required for
Datadog `error != 0` spans that carry no `error.message`. VRL intentionally cannot
create that value: `trace_span_status` and whole-status writes reject it. A pure relay
therefore preserves the value, while a VRL program that writes the status directly or
as part of a containing object must supply a non-empty error message. This is within the
Scope rule that VRL-modified events are best-effort rather than covered by the
round-trip guarantee.

#### Empty strings in optional string slots

`Option<String>` typed slots preserve `Some("")` when the source mapping supplies an
empty value; `None` means absent or unset. There is no model-wide empty-string
normalization. A mapping may collapse empty to `None` only where its wire format makes
the values indistinguishable or defines them as equivalent; each such case is specified
in the corresponding sub-RFC.

#### `TraceFlags` and `TraceState`

`TraceFlags` is the OTLP `Span.flags` / `Link.flags` bitfield: a 32-bit word whose low
byte is the W3C trace-flags byte and whose remaining bits carry OTLP- and Datadog-defined
context information. Its implementation must retain the full raw word, including unknown
bits, so OTLP's reserved bits 10-31 and future additions round-trip unchanged. It exposes
the low W3C trace-flags byte and the OTLP parent- / link-target-remoteness tristate as
derived views without narrowing the stored value.

VRL reads and writes `Span.flags` / `SpanLink.flags` as the raw `u32`. The fallible
`trace_flags` function returns an atomically updated raw integer without losing unknown
bits:

```coffee
.spans[i].flags = trace_flags!(
    raw: .spans[i].flags,
    sampled: true,
    context_is_remote: false,
)
```

`raw` defaults to `0`; omitted derived arguments preserve their bits in `raw`.
`context_is_remote: null` clears both remoteness bits, `false` sets known-local, and
`true` sets known-remote. Values outside the `u32` range or arguments of the wrong type
return a descriptive error before assignment.

`TraceState` stores the W3C `tracestate` header verbatim. Sources copy the header in
unchanged; sinks emit it unchanged
unless a transform mutated it. Vector does not validate the header against the W3C
grammar, enforce the 32-entry or 512-byte limits, reject invalid members, or deduplicate
keys: the raw string is preserved and re-emitted as-is. Validation is the responsibility
of the producing tracing SDK, not the relay. This is consistent with `TraceFlags`, which
also preserves unknown bits without validation.

`.spans[i].trace_state` and
`.spans[i].links[j].trace_state` are read and written as raw header strings; programs
needing structured access wait for the deferred VRL helpers (`parse_trace_state`,
`encode_trace_state`) listed under "Future Improvements". A direct write to the raw
string remains the producer's responsibility, consistent with the non-validating relay
stance above. Any structured Rust accessor API is an implementation-time choice.

#### VRL surface for dropped counts

Every `u32` `dropped_*_count` field reads as a VRL integer. Explicit assignment, whether
direct or through a containing object or trace constructor, accepts only integers in the
field's domain; other values raise a runtime error before mutation. Saturating arithmetic
applies only to mapping- or relay-generated increments, not to explicit VRL writes.

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

VRL constructs complete events and links before inserting them into their vectors:

```coffee
span.events = push(span.events, trace_span_event!(
    name: "exception",
    time: now(),
    attributes: { "exception.type": "Timeout" },
))

span.links = push(span.links, trace_span_link!(
    trace_id: "4bf92f3577b34da6a3ce929d0e0e4736",
    span_id: "00f067aa0ba902b7",
))
```

`trace_span_event` requires `name` and `time`; `attributes` and
`dropped_attributes_count` default to `{}` and `0`. `trace_span_link` requires non-zero
`trace_id` and `span_id`; `trace_state`, `flags`, `attributes`, and
`dropped_attributes_count` default to `""`, `0`, `{}`, and `0`. Both functions validate
all arguments before returning the canonical object, so a failed construction does not
partially modify the destination vector.

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
nested `Map` / `Array` values). The `Attributes` newtype exists so future invariants
(key validation, size bounds) can be added without requiring a migration.
Per-format wire mappings live in the sub-RFCs.

##### VRL surface for `AttrValue`

VRL accesses an `Attributes` map through the existing `Value` API. Conversion is
recursive: bytes, booleans, integers, floats, arrays, objects, and null map to the
corresponding `AttrValue` variants. A new `Value::Bytes` write becomes
`AttrValue::String` when it is valid UTF-8 and `AttrValue::Bytes` otherwise. On an
unchanged write back to the same resolved typed path, the original `String` / `Bytes`
discriminator is preserved instead.

`AttrValue::Double(NaN)` reads as `Value::Null`, because VRL floats exclude NaN; an
unchanged null write back to that same path preserves the NaN discriminator and bits.
Other null writes store `AttrValue::Null`, preserving explicit null versus an absent map
entry. `Value::Timestamp` and `Value::Regex` have no `AttrValue` representation and
raise a runtime error. If any recursive conversion fails, the entire typed-path write
fails before mutation. Removing an entry requires `del()` per the typed-path rules below.

#### Typed slot/attribute-map pairs

Several typed slots on `Resource`, `Span`, and `ChunkContext` correspond to attribute-map keys that
wire formats also use. The pairs the model knows about are:

- `Resource.service` versus `Resource.attributes."service.name"`.
- `Resource.environment` versus `Resource.attributes."deployment.environment.name"`.
- `Resource.host` versus `Resource.attributes."host.name"`.
- `Span.status` (`Error` / `Other` message) versus `Span.attributes."error.message"`.
- `TraceEvent.chunk.{priority, origin, dropped, tags}` versus
  `Span.attributes."datadog.chunk.*"` (cross-format only; see the OTLP mapping sub-RFC).

The in-memory model permits both forms to coexist. Reading from the typed slot is the
supported VRL pattern; the matching attribute-map key exists only as a wire-shape
detail. Ingress lifting (when an attribute key populates a typed slot) and egress
synthesis (when a typed slot populates an attribute key or a canonical wire location)
are wire-format-specific and are specified by each mapping sub-RFC.

#### VRL `del()` semantics on typed paths

The rules below mirror the existing `Metric` VRL surface. VRL's `del()` operator removes a key from
its parent; on the typed `TraceEvent` structure the result depends on what the path
resolves to:

- **`Option`-wrapped typed slot** (`Span.parent_span_id`, `Resource.service` /
  `environment` / `host` / `schema_url`, `Scope.name` / `version` / `schema_url`,
  `Span.resource_name`, `Span.span_type`, `TraceEvent.chunk`, and
  `ChunkContext.priority` / `origin`): `del()` clears the slot to `None`. Writing `""`
  to an `Option<String>` slot sets `Some("")`; it is distinct from `del()`. The
  analogous rule for `Span.parent_span_id` is documented under "VRL surface for
  `TraceId` and `SpanId`".
- **`Attributes` map entry** (e.g. `.spans[i].attributes."foo"`,
  `.resource.attributes."bar"`, `.scope.attributes.*`, `.chunk.tags.*`,
  `.spans[i].events[j].attributes.*`, `.spans[i].links[j].attributes.*`): `del()` removes
  the entry from its map.
- **`Vec` element** (`.spans[i]`, `.spans[i].events[j]`, `.spans[i].links[j]`): `del()`
  removes the i-th / j-th element; the vector shrinks and subsequent indices renumber.
- **Semantically unset required slot** (`Span.status`): `del()` atomically writes
  `SpanStatus::Unset` and returns the previous status.
- **Required typed field or container**: `del()` raises a VRL runtime error, whether or
  not the field has a representable default. Write the replacement explicitly (for
  example, `.spans[i].duration = 0`, `.chunk.tags = {}`, or `.spans = []`).
- **Root path (`del(.)`)**: raises a VRL runtime error.

Writing any `.chunk.*` path while `.chunk` is `null` validates the complete operation
against a temporary `ChunkContext::default()` and commits
`Some(ChunkContext { ... })` only on success. A failed write leaves `.chunk` as `null`.

Reading through the same paths is the inverse: `Option`-wrapped slots that are `None`
read as `null`, absent attribute-map entries read as `null`, out-of-bounds vector indices
read as `null`, and required typed fields within present containers always read a
concrete value.

#### Retention of `TraceEvent` and `Event::Trace`

The outer `Event::Trace` variant is retained. During coexistence its payload temporarily
changes from today's `TraceEvent(LogEvent)` to `TraceEventCompat`; after coexistence
`TraceEventCompat` becomes an alias for the typed `TraceEvent`. This avoids adding a
second trace dispatch arm to `Event`.

#### Migration: coexistence of `LogEvent` and typed representations

`TraceEvent` is permanently the typed struct defined above. During migration, the public
temporary compatibility enum carried by `Event::Trace` and `TraceArray` is:

```rust
pub struct LegacyTraceEvent(LogEvent);

pub enum TraceEventCompat {
    /// Pre-migration source output: an untyped `LogEvent` whose key layout is
    /// defined by the producing source. The producing source identifies its
    /// layout in `LogEvent.metadata().value()` under the reserved sub-key
    /// `vector.trace_legacy_layout` so the correct shim can be selected
    /// after fan-in, disk-buffer round-trips, or `vector` source/sink hops.
    Legacy(LegacyTraceEvent),
    /// The permanent typed container.
    Typed(TraceEvent),
}
```

Trace event-producing sources each set the reserved sub-key `vector.trace_legacy_layout`
in `EventMetadata.value` to a static string identifying themselves on every
`TraceEventCompat::Legacy` trace they emit. A source that flips to
`TraceEventCompat::Typed` continues setting the same hint until the coexistence and
deprecated-proto window ends, so independently migrated producers retain durable
origin across disk buffers and `vector` hops. Conversion reads this sub-key to select
the corresponding `LegacyTraceEvent -> TraceEvent` shim. For a pre-precursor record
with no hint, conversion runs each registered shim's format-shape detector. Exactly
one match selects that shim; zero or multiple matches return a reported error.
A present but unrecognized hint returns the same error without attempting detection.

`TraceEventCompat` owns only migration-boundary behavior:

- Metadata, finalizer, allocation-size, and event-count operations dispatch to either
  variant so generic topology and buffering code can operate without converting.
- Temporary untyped forwarding methods (`get(path)`, `insert(path, value)`, `as_map()`,
  etc.) preserve the legacy component API while every producer still emits
  `TraceEventCompat::Legacy`. They are removed before any producer flips to typed
  output; the resulting compiler errors locate every remaining consumer of the legacy
  layout.
- Typed field accessors exist only on `TraceEvent`. `TraceEventCompat` does not expose
  typed accessors, so typed code cannot accidentally access an unconverted legacy event
  or enter a migration-only panic state.
- A typed-aware sink or transform consumes `TraceEventCompat` at every trace-event
  intake path and invokes a fallible `try_into_typed()` conversion. The `Typed` arm
  returns its `TraceEvent` without migration work; the `Legacy` arm selects a
  source-specific shim from the hint or, for a hintless record, the unique
  format-shape detector match. A conversion error retains the original
  `LegacyTraceEvent`, including metadata and finalizers, so the caller can report and
  drop it according to normal acknowledgement semantics. There is no reverse
  conversion. A transform that emits the converted event wraps it as
  `TraceEventCompat::Typed`.
- VRL uses a fallible mutable compatibility-boundary method that converts a `Legacy`
  arm in place and returns `&mut TraceEvent`. An already-`Typed` arm only returns the
  contained reference. If conversion cannot resolve a present hint or uniquely detect
  a hintless legacy layout, VRL aborts the expression with a runtime error; the event
  is forwarded to the topology error path unchanged and the failure is reported.

Per-component shims are unidirectional (`LegacyTraceEvent -> TraceEvent` only). The
`datadog_agent` source ships with a shim and format-shape detector that know the
source's `LogEvent` key layout and produce a typed container; the OTLP source ships
with the equivalent shim and detector for its layout.

The removal of the temporary untyped forwarders is the compile-time gate for unmigrated
call sites. Each migrated consumer must accept `TraceEventCompat` and fallibly produce
a `TraceEvent` at every trace-event intake path. Once conversion succeeds, all of its
internal trace operations are statically typed.

VRL programs perform the same conversion implicitly at their mutable target boundary,
so operators never need to reason about the migration state of upstream sources.

After every source, sink, and transform has been migrated, the compatibility enum and
`LegacyTraceEvent` are removed and `TraceEventCompat` becomes a type alias for
`TraceEvent`. The legacy conversion routines and detectors remain temporarily as
deprecated-proto wire decoders, as described below.

#### Wire serialization

Trace events cross internal-wire boundaries through disk buffers and the `vector`
source/sink, so Vector's event protobuf (`lib/vector-core/proto/event.proto`) needs wire
shapes for both migration variants. `EventWrapper` and `EventArray` retain field tag 3
for the renamed `LegacyTrace` / `LegacyTraceArray` variants and add sibling
`TypedTrace` / `TypedTraceArray` variants at field tag 4. The legacy message field tags
and shapes remain unchanged.

The typed messages mirror the Rust model: `TypedTrace` carries `Resource`, `Scope`,
presence-sensitive `ChunkContext`, repeated `Span`, and full metadata. Nested messages
carry the fields and numeric domains defined above. Proto presence preserves every
`Option<String>` distinction and `chunk = None` versus
`Some(ChunkContext::default())`; an unset `AttrValue` oneof represents
`AttrValue::Null`. Identifiers use their non-zero unsigned wire domains, trace IDs are
16-byte big-endian values, timestamps and durations are fixed-width unsigned
nanoseconds, and flags retain the full 32-bit word. Timestamp encoding applies the
numeric boundary policy defined in Scope.

The oneof tag is the discriminator. The fallible decode boundary (see Plan of Attack) is
a hard prerequisite for the typed proto step and must ship first. An older Vector that
has the fallible decode boundary but not the `TypedTrace` variant receives a
`typed_*`-tagged message and surfaces a controlled unknown-variant error; the consumer
drops and reports the affected message. The pipeline continues running. All-Legacy
traffic decodes correctly on any Vector version that supports field tag 3, since
`LegacyTrace` / `LegacyTraceArray` keep that tag. `vector` source/sink chains that span
the typed migration roll out receiver-first: every downstream `vector` receiver must
support `TypedTrace` before an upstream peer may send typed trace records to it. The
migration-boundary release (fallible decode plus the legacy-layout hint precursor) is
crash-safe but not lossless for typed traffic; it does not satisfy the receiver
prerequisite.

Single-event encoding via `EventWrapper` is 1:1:
`TraceEventCompat::Legacy` encodes as `LegacyTrace` at tag 3 and
`TraceEventCompat::Typed` encodes as `TypedTrace` at tag 4. Array encoding is
1:1-or-N: during coexistence, in-memory `TraceArray` (a
`Vec<TraceEventCompat>`) can hold a mix of variants when a source that emits typed
events natively and one that still emits legacy events fan in to the same downstream
component, but the wire `EventArray.events` oneof must select one variant. The encoder
must therefore emit one or more homogeneous arrays while preserving the original event
order and per-event finalizer and acknowledgement behavior. Decoders see only
homogeneous wire arrays; mixing reappears at fan-in points downstream without changing
the order observed by stateful components.

Retiring the `TraceEventCompat` enum does not immediately retire the proto:
`LegacyTrace`,
`LegacyTraceArray`, and the `legacy_*` oneof variants are first marked
`deprecated = true` for a release window so records written by older Vector instances
continue to decode when their layout can be identified safely. The per-component
conversion routines and detectors persist alongside the deprecated proto as wire
decoders. During coexistence, a tag 3 record decodes as
`TraceEventCompat::Legacy` and converts at a typed consumer boundary. After the
compatibility enum is retired, the tag 3 decoder performs that same fallible conversion
immediately and returns a `TraceEvent`; a tag 4 record decodes directly to
`TraceEvent` after the typed proto's own validation. A
`vector.trace_legacy_layout` hint (which travels in `EventMetadata.value` inside
`LegacyTrace`) selects the decoder directly; a pre-hint record is accepted only when
exactly one registered detector recognizes its legacy `LogEvent` shape. Zero or multiple
matches produce a controlled, reported error rather than guessing.
Deployments carrying a legacy layout that cannot be detected uniquely must drain or
segregate those pre-hint disk buffers and older-peer streams before installing the
typed-only release.

Legacy wire support may retire only after all of the following hold:

- The minimum compatibility interval required by Vector's supported rolling-upgrade
  policy has elapsed. If no such policy is documented, retirement remains blocked.
- No Vector release still inside that supported upgrade set emits legacy trace records.
- The published deadline for draining or segregating pre-hint buffers and older-peer
  streams has elapsed.

Only then are the proto messages removed, field tag 3 reserved in both oneofs, and the
conversion routines and detectors deleted.

## Rationale

### Architectural choices

- The container shape mirrors the wire-level batching of both OTLP and Datadog so source
  ingest and sink egress are mechanical translations rather than regroupings. Sharing
  `Resource` / `Scope` / `ChunkContext` as struct fields, not `Arc`, keeps that sharing
  intact across disk-buffer serialization without reconstruction or read-side interning.
- `Option<ChunkContext>` is the smallest way to distinguish "this source has no chunk
  concept" from an explicitly default-valued Datadog `TraceChunk`; protobuf message
  presence is what carries that distinction across hops.
- Typed fields, with a single shape regardless of source, let transforms be written once.
  `Metric` already demonstrates this in Vector; extending it to traces unblocks RFC 11851.
  Typed-first ingest keeps cross-format egress a projection from that shared model rather
  than a translation from source-specific layouts.
- Keeping the outer `Event::Trace` variant unchanged and temporarily changing only its
  payload to `TraceEventCompat` avoids a second dispatch arm at every topology, buffer,
  and finalizer site. Separating the wrapper from `TraceEvent` keeps the permanent type
  statically typed throughout the migration.

### Per-type design choices

- `Resource` promotes only the three semantic-convention fields both wire formats agree
  on (`service.name`, `deployment.environment.name`, `host.name`). Promoting more would
  force Vector to track upstream convention evolution or ossify a stale subset; promoting
  fewer would force every cross-format transform to read source-specific keys for common
  metadata. Format-specific consequences live in the corresponding mapping sub-RFC.
- Non-zero `TraceId` / `SpanId` types eliminate a class of malformed values by
  construction: OTLP defines all-zero IDs as invalid, and Datadog uses zero only as the
  "no parent" sentinel (already `None`). Unsigned integers also fix the existing
  `i64`-coercion precision bug
  ([#14687](https://github.com/vectordotdev/vector/issues/14687)). VRL hex strings match
  the dominant external representation (W3C `traceparent`, OTLP debug logs, Datadog UI,
  Jaeger / Zipkin search), so programs comparing IDs against those sources do not format-
  convert at the boundary.
- `SpanStatus` uses atomic whole-value construction, with read-only child projections,
  so VRL cannot leave an intermediate or partially authored status. `del(.status)` is
  the narrow exception to required-field deletion because `Unset` is the domain's
  semantic absence, not merely a default. `SpanKind` and `SamplingPriority` permit
  direct discriminator writes because they have no correlated message field.
- VRL `del()` on typed paths follows the existing `Metric` surface so typed-event access
  has one mental model across event variants.
- `TraceFlags` is sized to the OTLP wire field (`u32`), not the W3C `traceparent` byte
  (`u8`), so an `OTLP -> Vector -> OTLP` relay round-trips the full `Span.flags` /
  `Link.flags` word: the W3C trace-flags byte (bits 0-7), OTLP's parent- / link-target-
  remote tristate (bits 8-9), and OTLP's reserved bits 10-31. The same width is required
  on the Datadog link path, where `SpanLink.flags` is `uint32` and bit 31 is a
  "flags-are-meaningful" sentinel; a `u8` storage would clear that bit on every Datadog
  link. Unknown bits, including forward-compat W3C additions such as the Level 2
  `random` flag, must round-trip without changing the type. A bitfield representation
  that dropped undefined bits would lose that data.
- `trace_span_link` and `trace_span_event` validate complete vector elements before
  insertion; `trace_flags` updates the correlated remoteness bits while retaining every
  unknown bit. A full `trace_span` constructor is omitted because it would mirror most
  of the large `Span` structure for an operation expected to be rare in VRL.
- `Span.duration` is stored as integer nanoseconds because both wire formats carry
  duration that way, while conversion to floating-point seconds progressively loses
  nanosecond precision as magnitude grows. Wire-domain corner cases are clamped on the
  corresponding boundary and declared as exclusions in the relevant sub-RFC. VRL
  exposes approximate float seconds at the boundary for ergonomic comparisons; see
  "Duration as `f64` seconds" under Alternatives.
- `Attributes` stores leaves as `AttrValue` rather than VRL `Value` so the wire
  string-versus-bytes discriminator and NaN doubles are preserved structurally. That
  avoids several round-trip exclusions from both sub-RFCs at the cost of one conversion
  layer at the VRL boundary. The reuse alternative and the wider VRL `Value` fix are
  recorded under Alternatives and Future Improvements.

### Migration approach

- The migration uses a temporary
  `enum TraceEventCompat { Legacy(LegacyTraceEvent), Typed(TraceEvent) }` as the
  `Event::Trace` payload so each trace source, sink, and transform can migrate in its
  own PR while the rest of the system continues to operate against the representation
  it expects. `TraceEvent` itself is always typed. See "Wholesale migration" under
  Alternatives for why a single atomic replacement was rejected.
- Per-component shims convert `LegacyTraceEvent -> TraceEvent` only, never the reverse:
  the temporary
  migration hint is not stable post-migration provenance, and the typed model does not
  retain enough source-layout detail to reconstruct a source-specific `LogEvent` shape
  after arbitrary transforms. This forces the migration sequencing in the Plan of Attack --
  trace-aware consumers (sinks, transforms, VRL programs) must accept
  `TraceEventCompat` and convert it before any source flips to emitting typed events
  natively. The temporary untyped forwarding methods (`get(path)`, `as_map()`, etc.)
  are removed from `TraceEventCompat` before the source steps; every remaining call
  site then fails to compile, making the consumer migration a mechanical
  fix-the-build task rather than a runtime-failure audit.
- Shim selection is keyed on a reserved sub-key `vector.trace_legacy_layout` in
  `EventMetadata.value` set by the producing trace source. The `vector` metadata
  namespace is read-only to VRL, so transforms between source and sink cannot
  accidentally delete or overwrite the hint. The metadata `Value` is serialized with
  every event record and passes through fan-in, disk buffers, and `vector` source/sink
  hops unchanged (unlike `EventMetadata.source_type`, which the topology rewrites on
  every emission and so cannot serve as the selector across a serialised hop).
  Conversion is explicit outside VRL; only the resulting `TraceEvent` exposes typed
  accessors. Hintless records written before the precursor use format-shape detectors
  and convert only on exactly one match; this fallback does not rely on
  `EventMetadata.source_type`. The convention lives only for the duration of the
  migration and deprecated-proto window; no new permanent struct field or wire-format
  extension is needed.

## Drawbacks

- Breaking change for VRL configurations against today's `TraceEvent` key layout. Users
  must migrate to typed paths.
- The `trace_to_log` transform's output also changes; downstream VRL programs against
  its output must update.
- Topology granularity is coarser than per-span: each event carries up to a chunk's
  worth of spans (typically tens to hundreds, larger in deep call trees). Buffer-size
  limits expressed in events bound span counts less directly than the previous
  `LogEvent`-per-span design. Event-count accounting continues to return `1` per
  `TraceEvent` (not per span), so `component_received_events_total` and related
  accounting count container events, not individual spans. This is consistent with how
  `Metric` reports (one event per metric, not per sample); dashboards that expect
  per-span event counts will read lower values than the actual span throughput.
- Per-span operations (filter, sample, mutate one span) require VRL iteration over
  `.spans` rather than per-event treatment. A future topology-level expand-on-input /
  collapse-on-output shim could let single-span transforms operate unchanged.
- The internal `event.proto` gains a new `TypedTrace` variant alongside the renamed
  `LegacyTrace`. `vector` source/sink chains spanning the typed migration must upgrade
  receivers before senders. A receiver at the migration-boundary release but without
  the `TypedTrace` variant drops and reports the message; the pipeline continues running,
  but the hop is not lossless. See "Wire serialization" for details. This ordering is
  documented in the release notes alongside the VRL-path migration.
- Pre-hint legacy records depend on unique format-shape detection after consumers become
  typed-only. Any unrecognized or ambiguous historical layout must be drained or
  segregated before the typed-only release rather than being converted heuristically.
- Every trace source and sink must be rewritten to produce/consume the typed container.
  The Plan of Attack sequences this so each component migrates independently, but it is
  non-trivial work.
- The temporary public `TraceEventCompat` type and its wrapping and unwrapping at
  component boundaries add migration-only API and mechanical changes. The foundational
  migration step must rename today's untyped `TraceEvent` representation to
  `LegacyTraceEvent` and adapt existing producers to wrap it, and the cleanup step must
  replace the compatibility enum with an alias to `TraceEvent`.
- Wire-format-specific drawbacks (Datadog producer-side keyset-disjointness convention,
  Datadog `Span.error` normalization, OTLP `deployment.environment` legacy-key rewrite,
  etc.) are listed in the corresponding mapping sub-RFC.

## Prior Art

- [OTLP traces protocol](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)
  -- the primary shape this RFC adopts. The container `TraceEvent` is structurally one
  `ScopeSpans` plus its `Resource` and an optional Datadog-only `ChunkContext`.
- [Datadog APM agent-to-backend protobuf](https://github.com/DataDog/datadog-agent/tree/main/pkg/proto/datadog/trace)
  -- the second native format Vector targets.
- [Datadog Agent OTLP ingest](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go)
  -- the normative reference for the enumerated Agent-aligned derivations; see the
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
  `TraceEvent.chunk` field reflects the structural fact directly when present; the
  encoding is one slot per chunk-scoped value rather than
  `N spans × one entry per chunk-scoped value`.
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
  `TraceEvent.chunk = Some(...)` is read back into one `TraceChunk` per event by container
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
operations (filter, sample, mutate one span) work directly without iteration. The container shape
does not provide that directly; a future topology-level shim could. The single-span shape, however,
requires the `Resource`, `Scope`, and `ChunkContext` to either be duplicated for each span or shared
via `Arc`.

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
structurally and limits the conversion cost to the VRL boundary. The wider fix -- a
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
nanoseconds, while a floating-point seconds view progressively loses nanosecond
precision as duration magnitude grows. Storing `duration` as `std::time::Duration`
preserves the wire domain for all non-negative values. Datadog's `int64` wire field
permits negative values that `std::time::Duration` cannot represent; these are clamped
to zero on ingress and declared in the Datadog mapping sub-RFC. The VRL surface exposes
approximate float seconds at the boundary; a complementary integer-nanosecond view
(`.spans[i].duration_nanos`) is documented under "Future Improvements".

### `SpanStatus` as a closed enum

Defining `SpanStatus` without an escape hatch would silently coerce any unrecognized
status code introduced by a future OpenTelemetry version to `Unset` (the proto3
default), breaking the `OTLP -> Vector -> OTLP` relay guarantee for those spans. The
`Other(i32, String)` variant stores the raw code and message verbatim and egresses them
unchanged, preserving relay fidelity by the same mechanism used for `SpanKind`. The
Datadog egress path has no status-code wire field; `Other` values follow the
`Span.error = 1` rule.

Among the known status variants, only `Error` carries a string because the OpenTelemetry
trace specification's
[Set Status](https://opentelemetry.io/docs/specs/otel/trace/api/#set-status) rule states
"Description MUST only be used with the Error StatusCode value." A wire `Status.message`
paired with `code = UNSET` or `OK` is non-conformant and is dropped on ingest. `Other`
carries its raw message solely as the forward-compatibility escape hatch described
above.

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
chosen temporary `TraceEventCompat` coexistence design lets each component migrate in
its own PR while keeping the permanent `TraceEvent` typed, subject to a partial-order
constraint that consumers migrate before producers (see "Plan Of Attack").

### Feature-flagged switch

Gate the new representation behind a Cargo feature or runtime flag until all components
are migrated, then flip the default. Rejected because feature combinations proliferate
quickly across every trace source/sink and VRL, and because a runtime flag would
require duplicate code paths in performance-sensitive components.

### Wire serialization shape

The chosen design is selected against two imperatives: incompatibility with older
Vector instances must surface loudly (not as silent data drops), and the post-migration
wire schema should carry no vestiges of the migration. Encoding mixed in-memory arrays
as one or more homogeneous wire arrays, while preserving event order, is the cost paid
for both. Each rejected alternative fails at least one:

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
this RFC. Per-format shims, encoders, and source and sink flips are sequenced in the
[OTLP mapping](2026-04-29-25329-trace-data-model/otlp-mapping.md) and
[Datadog mapping](2026-04-29-25329-trace-data-model/datadog-mapping.md) sub-RFCs.
Format-agnostic prerequisites land first; per-format shims may then land in either
order; this RFC's consumer and VRL work and the compile-time gate follow; source flips
are independent after that gate; cleanup is last. Consumers must accept
`TraceEventCompat` and fallibly convert it to `TraceEvent` before any source emits
typed events, because shims are unidirectional.

The format-agnostic work is organized into six stages. A stage may span multiple PRs;
its exit criteria, rather than a prescribed internal implementation, gate the next
stage.

1. **Establish the migration boundary.** Make internal proto decoding fallible so an
   unknown oneof variant is reported and drops only the affected message rather than
   panicking the task. In the same release line, add the
   `vector.trace_legacy_layout` precursor to both trace sources. This release must both
   tolerate future event variants and identify every newly written legacy trace layout,
   and must precede every `TypedTrace` producer.
2. **Introduce the model without changing behavior.** Add the supporting types and the
   permanent typed `TraceEvent`, rename today's untyped representation to
   `LegacyTraceEvent`, and make the temporary
   `TraceEventCompat::{Legacy, Typed}` enum the payload of `Event::Trace` and
   `TraceArray` while all components continue to use `Legacy`. Adapt existing producers
   mechanically to wrap their unchanged output, then add the typed internal-wire
   variant. Before any source emits typed events, legacy behavior must remain
   unchanged, both compatibility variants must survive disk-buffer and `vector`
   source/sink boundaries, and optional-value and chunk-presence distinctions must
   survive those boundaries.
3. **Land both format mappings.** Implement and register both format shims and
   format-shape detectors, then implement both typed encoders per the sub-RFCs. No typed
   VRL path is exposed until both legacy layouts can auto-convert.
4. **Establish typed VRL and migrate consumers.** Implement the typed paths and fallible
   trace constructors. At the VRL target boundary,
   `TraceEventCompat::Legacy` auto-converts on first typed-path access or returns a
   controlled error; it never returns a transitional `null`. Then migrate `sample` and
   `trace_to_log`. Each typed consumer fallibly converts `TraceEventCompat` at every
   intake path and operates only on the resulting `TraceEvent`.
   `sample` remains per-event,
   so a multi-service Datadog chunk changes from incidental whole-chunk atomicity to one
   decision per `(TraceChunk, Span.service)` event; `trace_to_log` emits a uniform
   source-independent layout. Before the next stage, every migrated consumer must accept
   both legacy and typed input and all typed VRL and round-trip contracts must hold.
5. **Use the compile-time gate and flip producers.** Remove untyped forwarding methods
   from `TraceEventCompat` and migrate every resulting call site. The OTLP and Datadog
   sources may begin emitting `TraceEventCompat::Typed` independently once no
   trace-aware consumer depends on the legacy key layout and both source formats have
   typed production paths. Each typed
   source retains its `vector.trace_legacy_layout` hint through the coexistence and
   deprecated-proto window. Across `vector` network hops, each downstream receiver must
   support `TypedTrace` before its upstream sender emits typed trace records.
6. **Complete user migration and retire coexistence.** Publish the user migration and
   compatibility requirements, then remove the compatibility enum and
   `LegacyTraceEvent` after both source flips, replacing `TraceEventCompat` with an alias
   to `TraceEvent`. Legacy proto decoding now converts directly to `TraceEvent`. Retain
   those proto decoders, shape detectors, and typed-source hints until the wire
   retirement criteria above hold; then reserve the legacy tags and stop emitting the
   migration hint.

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
  as approximate float seconds. Workloads needing exact nanosecond access can have a
  complementary `.spans[i].duration_nanos` view added without affecting the underlying
  data model, or functions to set and extract seconds and nanoseconds separately.
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
  `trace_sample`); either approach remains a future design decision.
- Distinct `Value::String` variant in VRL's `Value`, plus a NaN-admitting float carrier.
  With `AttrValue` in place the trace round-trip no longer drives this work. The remaining
  motivation is `LogEvent` / `Metric` UTF-8 handling and letting the trace VRL boundary
  surface NaN as a typed `Float` rather than `Null`. The change is cross-cutting: every
  `Value` consumer is affected, and admitting NaN requires a `Value` equality/ordering
  redesign.
