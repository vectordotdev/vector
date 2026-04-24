# RFC 25329 - 2026-04-29 - Trace Data Model: Datadog Mapping

This sub-RFC of [RFC 25329 -- Internal Trace Data Model](../2026-04-29-25329-trace-data-model.md)
specifies the bidirectional mapping between the typed `TraceEvent` defined in the parent RFC
and the Datadog agent-to-backend trace protobuf. It establishes the Datadog ingress and
egress paths, the effective-equivalence round-trip guarantee for
`Datadog -> Vector -> Datadog`, the multi-service chunk split/coalesce rules, the reserved
`_dd.*` sub-objects that carry agent-payload, tracer-payload, and `meta_struct` state, and
the cross-format conformance rule for `OTLP -> Vector -> datadog_traces`.

## Context

- The parent RFC defines the typed data model, migration mechanics, and internal wire
  serialization. This sub-RFC assumes those definitions and the parent's Glossary, In/Out
  scope clauses, and User Experience as background. The Datadog-specific wire-format and
  cross-format references this sub-RFC depends on are defined in the Glossary below; OTLP
  and other shared vocabulary is defined in the parent's Glossary.
- [RFC 9572 -- Accept Datadog traces](../2021-10-15-9572-accept-datadog-traces.md) introduced
  the `datadog_agent` trace ingest path, which the `datadog_traces` sink can consume but
  which today has no well-defined internal representation. This sub-RFC, together with the
  parent and the OTLP mapping sub-RFC, supplies that representation.

## Glossary

This sub-RFC defines the Datadog-specific format vocabulary. OTLP, OpenTelemetry, W3C
Trace Context, and other informational entries are defined in the
[parent RFC's Glossary](../2026-04-29-25329-trace-data-model.md#glossary).

- **Datadog APM trace format**: Vector targets exactly one hop in the Datadog tracing
  pipeline -- the agent-to-backend protobuf served at `/api/v0.2/traces`. When this
  sub-RFC says "Datadog" unqualified, it means that format. The schema lives in three
  protobuf files in the Datadog Agent repository:
  - [`agent_payload.proto`](https://github.com/DataDog/datadog-agent/blob/main/pkg/proto/datadog/trace/agent_payload.proto)
    -- `AgentPayload` (`tracerPayloads[]`, agent-level `tags`, `agentVersion`,
    `targetTPS`, `errorTPS`).
  - [`tracer_payload.proto`](https://github.com/DataDog/datadog-agent/blob/main/pkg/proto/datadog/trace/tracer_payload.proto)
    -- `TracerPayload` (`chunks[]`, tracer-level fields) and `TraceChunk`
    (`priority` / `origin` / `droppedTrace` / `tags`, `spans[]`).
  - [`span.proto`](https://github.com/DataDog/datadog-agent/blob/main/pkg/proto/datadog/trace/span.proto)
    -- the per-span shape (`service`, `name`, `resource`, `traceID`, `spanID`,
    `parentID`, `start`, `duration`, `error`, `meta`, `metrics`, `type`, `meta_struct`).
- **Datadog Agent OTLP ingest**: the OTLP-to-Datadog conversion implemented by the
  Datadog Agent in
  [`pkg/trace/api/otlp.go`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go)
  (and supporting code under
  [`pkg/trace/transform/`](https://github.com/DataDog/datadog-agent/tree/main/pkg/trace/transform)).
  This is the **normative reference** for every cross-format derivation Vector applies
  between OTLP and Datadog. The "Cross-format conformance" section below specifies the
  conformance rule and the asymmetry of the inverse direction.
- **Datadog tracer-to-agent API** (informational): tracer SDKs send traces to the Datadog
  Agent over a separate set of HTTP endpoints (`/v0.3/traces`, `/v0.4/traces`,
  `/v0.5/traces`, `/v0.7/traces`) using JSON, msgpack, or protobuf. These are upstream of
  the agent-to-backend hop Vector consumes; the public guide
  [Send traces to the Agent by API](https://docs.datadoghq.com/tracing/guide/send_traces_to_agent_by_api/)
  documents the legacy v0.3 JSON shape and is cited only as a reference for per-span
  field semantics. Vector does not consume these endpoints directly.

## Cross cutting concerns

- APM stats aggregation in the `datadog_traces` sink, today reading magic keys from
  `TraceEvent`, will read typed fields after this RFC and its parent land.
- The OTLP-side reservation of `datadog.*` span-attribute keys (`datadog.chunk.priority`,
  `datadog.chunk.origin`, `datadog.chunk.dropped`, `datadog.chunk.tags`,
  `datadog.span.resource`, `datadog.span.type`) for synthesis of typed-only Datadog
  state on OTLP egress is the OTLP mapping sub-RFC's concern; this sub-RFC owns the
  contents those keys carry on cross-format relay.

## Scope

### In scope

- The bidirectional mapping between `TraceEvent` and Datadog `AgentPayload` /
  `TracerPayload` / `TraceChunk` / `Span` messages.
- Effective-equivalence round-trip through a single Vector instance for
  `Datadog -> Vector -> Datadog` when the pipeline does not otherwise mutate the data:
  byte-for-byte identity is not required, but the output must be ingested by the Datadog
  backend as the same data as the original. Details the backend does not observe (e.g.
  span order within a chunk, specific chunk grouping when the producer-side grouping was
  non-conforming) may differ.
- The three Datadog span-attribute partitions (`meta`, `metrics`, `meta_struct`) and how
  they map to the typed `Span.attributes`. The single reserved key
  `Span.attributes."_dd.meta_struct"` carries the bytes-typed partition; the two scalar
  partitions merge by `AttrValue` variant.
- The two resource-scoped reserved keys (`Resource.attributes."_dd.payload"` and
  `Resource.attributes."_dd.tracer"`) carrying the agent-payload envelope and
  tracer-payload tags.
- The chunk-scoped typed state on `TraceEvent.chunk` (`priority`, `origin`, `dropped`,
  `tags`).
- The multi-service chunk split rule on ingress and the corresponding re-coalescence rule
  on egress, including the cross-grouping invariant for non-conforming multi-trace chunks.
- The envelope reconstruction policy for `AgentPayload` and `TracerPayload` on egress, both
  during the migration (`vector.trace_legacy_layout`-keyed) and post-migration
  (`EventMetadata.source_type`-keyed).
- The cross-format conformance rule for `OTLP -> Vector -> datadog_traces`: a
  non-`datadog_agent`-sourced event reaching the `datadog_traces` sink produces the wire
  output the Datadog Agent itself would produce for the same OTLP input. This is a positive
  specification mirroring the Datadog Agent OTLP ingest reference (see the parent RFC's
  Glossary).

### Out of scope

- The OTLP wire mapping (see the OTLP mapping sub-RFC).
- Zero-loss cross-format round-trip (`Datadog -> OTLP -> Datadog`); see the parent RFC's
  Out of scope.
- `TracerPayload.containerDebug` (Datadog-internal container-tag-resolution diagnostic);
  dropped on ingest, not synthesized on egress.
- `AgentPayload.idxTracerPayloads = 11` (the indexed/deduplicated tracer-payload form);
  rejected on ingest with `unsupported_payload_version`, with a typed mapping deferred to
  Future Improvements.

### Zero-loss round-trip exclusions

The effective-equivalence guarantee for `Datadog -> Vector -> Datadog` does not cover the
following input shapes. Each is justified by a paragraph in the Implementation or
Rationale section below.

- **`Span.error` values other than `0` or `1`** ingest as `SpanStatus::Error(...)` and
  egress as `Span.error = 1`, normalizing the specific integer to the conforming
  bivalent representation.
- **`Span.duration` wire-domain corner cases**: negative values on ingress (the wire field is
  `int64`) are clamped to zero, and values exceeding `i64::MAX` nanoseconds (~292 years) on egress
  are clamped to `i64::MAX`. Both clamps increment a counter and emit a warning log identifying the
  affected span.
- **`meta`/`metrics` producer-side non-disjointness**: the round-trip guarantee is conditional on
  these two scalar maps being keyset-disjoint. If a producer emits the same key in both, the Datadog
  source resolves the collision deterministically (`metrics` wins), increments a counter, and emits
  a warning log; the dropped scalar is not recoverable on egress.
- **Producer `meta` or `metrics` key `_dd.meta_struct`** collides with the reserved
  sub-object for the `meta_struct` partition; the `meta_struct` content wins on both
  ingress and egress and the scalar is dropped. A counter is incremented and a warning log is emitted.
- **`SamplingPriority::Other(n)` where `n` happens to equal one of the four known wire
  values (-1, 0, 1, 2)** is unrepresentable by the parent RFC's closed-with-escape-hatch
  invariant. This is a model-level constraint and is mentioned here only for completeness;
  no round-trip data is at risk.
- **Multi-hop topologies that relay traces through intermediate `vector` source/sink
  hops** may lose Datadog agent-envelope state (`_dd.payload` / `_dd.tracer`)
  post-migration: `vector` hops reset `EventMetadata.source_type` to `"vector"`, so the
  default envelope-reconstruction policy treats the relayed event as non-Datadog-
  originated and synthesizes a defaults-only envelope on egress. Operators can restore
  envelope passthrough via sink configuration; see "Envelope reconstruction policy"
  below.
- **Pre-epoch `Span.start` and `SpanEvent.time`** are clamped to epoch-zero on Datadog
  egress; a counter is incremented and a warning log identifies the affected span. See
  the "Pre-epoch `Span.start` and `SpanEvent.time` handling" subsection.

The Datadog-side consequences of model-level exclusions defined in the parent RFC
(zero-ID rejection and pre-epoch timestamps via the internal proto's `fixed64` encoding)
also apply.

## Pain

- Today's `datadog_agent` source produces an untyped `TraceEvent(LogEvent)` whose key
  layout encodes the wire field locations directly. The `datadog_traces` sink reads these
  magic keys, and the APM stats aggregator does the same. Transforms written against this
  layout are tightly coupled to source-side decisions and break under cross-format relay.
- The wire `Span.traceID` is 64 bits but Datadog's 128-bit traces extend the high half via
  `meta["_dd.p.tid"]`. The current sink coerces `trace_id as i64`
  ([#14687](https://github.com/vectordotdev/vector/issues/14687)), corrupting precision
  for non-i64-representable values; a typed `TraceId(NonZeroU128)` (parent RFC)
  eliminates the coercion by construction.
- Datadog chunk-scoped state (`priority`, `origin`, `droppedTrace`, `tags`) applies
  uniformly to every span in a chunk, but the current `LogEvent`-per-chunk shape forces
  the sink to recover this from per-span attribute keys, encoding a structural invariant
  as a positional convention.

## Proposal

### User Experience

The Datadog wire mapping is invisible to VRL: programs read and write the typed
`TraceEvent` surface defined in the parent RFC. The Datadog-specific surface a VRL author
sees is `TraceEvent.chunk` (sampling priority, origin, dropped flag, chunk tags),
`Span.resource_name`, `Span.span_type`, and the two reserved keys
`Resource.attributes."_dd.payload"` / `Resource.attributes."_dd.tracer"` plus
`Span.attributes."_dd.meta_struct"`. All of these are typed-surface entries the parent
RFC defines; this sub-RFC specifies how they map to and from the Datadog wire format.

```coffee
# Read a Datadog chunk-scoped tag.
decision_maker = .chunk.tags."_dd.p.dm"

# Read agent-payload envelope state on a Datadog-sourced trace.
agent_apm_mode = .resource.attributes."_dd.payload"."tags"."_dd.apm_mode"
tracer_apm_mode = .resource.attributes."_dd.tracer"."_dd.apm_mode"

# Inspect a meta_struct sub-entry (msgpack-encoded; Vector exposes it as bytes).
metastruct_event = .spans[0].attributes."_dd.meta_struct"."dd.event_payload"
```

### Implementation

#### Ingress and egress mapping

An `AgentPayload` whose `tracerPayloads` repeated field is empty, or a `TracerPayload`
whose `chunks` repeated field is empty, produces zero `TraceEvent`s: there is no
`TraceChunk` from which to populate `TraceEvent.chunk` and no `Resource` envelope is
well-defined in isolation, so the wire input has no `TraceEvent` representation. The
discard is lossless because the payload carries no span data the Datadog backend would
observe.

An `AgentPayload` with at least one `TracerPayload` carrying at least one `TraceChunk`
expands into one `TraceEvent` per `(TracerPayload, distinct Span.service, TraceChunk)`
triple. (Vector's local protobuf
[`proto/vector/dd_trace.proto`](../../proto/vector/dd_trace.proto) currently carries two
historical fields, `repeated APITrace traces = 3` and `repeated Span transactions = 4`,
selected at runtime by `handle_dd_trace_payload` when `tracerPayloads` is empty. These
fields were removed from the upstream Datadog `AgentPayload` more than five years ago
and are not produced by any currently supported Datadog Agent. The typed model defines
no mapping for them; ingest of `tracerPayloads`-empty payloads is removed as part of
this RFC's implementation -- see "Plan Of Attack" -- and `proto/vector/dd_trace.proto`
is replaced by direct use of the upstream
`agent_payload.proto`/`tracer_payload.proto`/`span.proto`.)

The grouping rules are:

- Each `TraceChunk` becomes one `TraceEvent`. A chunk whose spans use more than one
  `Span.service` is split into one event per distinct service; egress re-coalesces such
  events back into a single chunk (see below). A `TraceChunk` whose `spans` repeated
  field is empty produces one `TraceEvent` with `spans = []` (the chunk envelope still
  populates `TraceEvent.chunk` and the enclosing `TracerPayload` / `AgentPayload`
  populate `Resource`); `Resource.service` is `None` because no `Span.service` is
  available. The event is forwarded per the empty-spans rule under the parent RFC's
  Identifiers.
- The enclosing `TracerPayload`'s metadata (`hostname`, `env`, `containerID`,
  `languageName`, `tracerVersion`, etc.) populates the event's `Resource`. Per-span
  `Span.service` populates `Resource.service`.
- The enclosing `AgentPayload`'s envelope (`hostName`, `env`, `agentVersion`, `targetTPS`,
  `errorTPS`, `rareSamplerEnabled`, and `tags`) populates
  `Resource.attributes."_dd.payload"` as a structured sub-object;
  `TracerPayload.tags` populates `Resource.attributes."_dd.tracer"` (see "Datadog
  resource-scoped state" below).
- `TraceChunk.{priority, origin, droppedTrace, tags}` populate `TraceEvent.chunk`.
- `Scope` is left default; Datadog has no scope concept.

| Datadog                                                       | Internal                                              |
| ------------------------------------------------------------- | ----------------------------------------------------- |
| `TracerPayload.hostname`                                      | `Resource.host`                                       |
| `TracerPayload.env`                                           | `Resource.environment`                                |
| `Span.service` (per span)                                     | `Resource.service` of the event holding the span      |
| `AgentPayload` envelope (whole message; see below)            | `Resource.attributes."_dd.payload"`                   |
| `TracerPayload.tags`                                          | `Resource.attributes."_dd.tracer"`                    |
| `TraceChunk.{priority, origin, droppedTrace, tags}`           | `TraceEvent.chunk`                                    |
| `TracerPayload` non-host/env scalar fields (see below)        | `Resource.attributes` under defined keys              |
| `Span.traceID` (u64)                                          | `Span.trace_id.low_u64`                               |
| `Span.meta["_dd.p.tid"]` (hex u64) if present (see below)     | `Span.trace_id.high_u64`                              |
| `Span.spanID`, `Span.parentID`                                | `Span.span_id`, `Span.parent_span_id`                 |
| `Span.name`                                                   | `Span.name`                                           |
| `Span.resource` (empty-string normalization, see below)       | `Span.resource_name`                                  |
| `Span.type` (empty-string normalization, see below)           | `Span.span_type`                                      |
| `Span.start`, `Span.duration`                                 | `Span.start_time`, `Span.duration` (ns-exact)         |
| `Span.error` and `Span.meta["error.message"]`                 | `Span.status` (see below)                             |
| `Span.meta`                                                   | `Span.attributes` (`AttrValue::String`, see below)    |
| `Span.metrics`                                                | `Span.attributes` (`AttrValue::Double`)               |
| `Span.meta_struct`                                            | `Span.attributes."_dd.meta_struct"` (`Map<Bytes>`)    |
| `Span.spanEvents[*].{time_unix_nano, name}`                   | `SpanEvent.{time, name}`                              |
| `Span.spanEvents[*].attributes` (`AttributeAnyValue`)         | `SpanEvent.attributes` (typed `AttrValue` per variant)|
| `Span.spanLinks[*].traceID` (u64)                             | `SpanLink.trace_id.low_u64` in `Span.links`           |
| `Span.spanLinks[*].traceID_high` (u64)                        | `SpanLink.trace_id.high_u64`                          |
| `Span.spanLinks[*].spanID`                                    | `SpanLink.span_id`                                    |
| `Span.spanLinks[*].tracestate`                                | `SpanLink.trace_state` (verbatim)                     |
| `Span.spanLinks[*].flags` (u32)                               | `SpanLink.flags` (full u32 verbatim)                  |
| `Span.spanLinks[*].attributes`                                | `SpanLink.attributes` (`AttrValue::String`)           |

The cross-format derivation rules later in this section (`span_type` from
`Span.kind` / `Span.attributes`, `resource_name` from `Span.attributes` / `Span.name`, the
`TracerPayload` semantic-convention key set, and the flattening of unmapped
`Resource.attributes` into per-span `meta`) are projections of the Datadog Agent OTLP
ingest reference (see Glossary above); the precise key sets and lookup orders are
deferred to implementation PRs that mirror the upstream code.

#### `TracerPayload` semantic-convention key mapping

`TracerPayload` fields mapped to `Resource.attributes` under OpenTelemetry
semantic-convention keys: `containerID`, `languageName`, `languageVersion`,
`tracerVersion`, `runtimeID`, `appVersion`. The specific OpenTelemetry attribute key each
wire field maps to is defined by the Datadog Agent OTLP-ingest reference, which the
implementation is required to mirror; the RFC does not pin a key set so the mapping
tracks upstream changes without a spec amendment. The `TracerPayload`-envelope-
equivalence rule in the egress section consequently keys off whatever attribute set the
implementation produces, in lockstep with the upstream reference.

`TracerPayload.hostname` and `TracerPayload.env` map to typed
`Resource.host` / `Resource.environment` directly and are not part of the deferred
attribute set; an empty wire value normalises to `None` per the parent RFC's
"Empty-string invariant for `Option<KeyString>` slots". `TracerPayload.containerDebug`
is a Datadog-internal diagnostic with no Vector consumer and is dropped on ingest (see
"Out of scope").

#### `Span.resource` and `Span.type` empty-string egress consequence

`Span.resource` and `Span.type` follow the parent RFC's "Empty-string invariant for
`Option<KeyString>` slots": an empty wire value normalises to `None` on ingress. The
Datadog-egress derivation fallback ("When `Span.span_type` is `None` on Datadog egress…",
below) then fires for the `None` value, including for Datadog-sourced spans whose producer
wrote the empty string -- this is the standard Datadog-Agent behaviour and matches
operator expectations that empty wire values are equivalent to "derive me". An originally
populated value is preserved verbatim.

#### `Span.duration` wire-domain handling

`Span.duration` on the wire is `int64` nanoseconds. Two corner cases sit outside the
wire field's representable range:

- A negative wire value on ingress is clamped to zero (`std::time::Duration` is
  non-negative); a counter is incremented and a warning log identifies the affected span.
- A `Duration` value exceeding `i64::MAX` nanoseconds (~292 years) on egress is clamped to
  `i64::MAX`; a counter is incremented and a warning log identifies the affected span. This
  clamp is tighter than the OTLP and internal `TypedTrace` `fixed64` wire clamps
  (`u64::MAX` nanoseconds, ~584 years), which both carry every value up to `u64::MAX`
  exactly.

Both cases are declared as Datadog-side round-trip exclusions above.

#### Pre-epoch `Span.start` and `SpanEvent.time` handling

Datadog's `Span.start` and `SpanEvent.time_unix_nano` wire fields are `int64` and
`uint64` respectively; a negative wire `Span.start` is technically representable on the
wire but is not produced by any documented Datadog tracing SDK. On Datadog ingress, a
negative `Span.start` is preserved as a pre-epoch `DateTime<Utc>` in the typed model
(matching the wire's representable range); the value is then subject to the parent
RFC's pre-epoch internal-proto clamp on any subsequent disk-buffer or `vector`
source/sink hop.

On Datadog egress, a pre-epoch `DateTime<Utc>` in `Span.start_time` or `SpanEvent.time`
(writable via the Rust API or VRL) is clamped to epoch-zero on encode; a counter is
incremented and a warning log identifies the affected span. This matches the parent
RFC's internal-proto clamp and the OTLP-side clamp, keeping Datadog egress consistent
with the model-level pre-epoch exclusion. A `Datadog -> Vector -> Datadog` round trip
through a pipeline that does not cross a disk-buffer or `vector` hop will therefore
preserve a pre-epoch `Span.start` only on the typed-model side; the clamped egress is
the documented behaviour.

#### `Span.error` and `Span.status`

`Span.error != 0` maps to `Error(meta["error.message"].cloned().unwrap_or_default())`,
else `Unset`. The `error.*` meta entries also flow into `Span.attributes` per the meta
merge rule below, keeping `error.type` / `error.stack` accessible alongside the typed
status. Datadog's wire `Span.error` is `int32`; values other than `0` and `1` are
non-conformant with the field's documented bivalent semantics. Such values ingest as
`SpanStatus::Error(...)` and egress as `Span.error = 1`, normalizing the specific
integer to the conforming bivalent representation; this is declared as a Datadog-side
round-trip exclusion above.

#### `_dd.p.tid` (128-bit trace-ID high half)

On ingress, `meta["_dd.p.tid"]` is consumed *before* the meta-merge step: the key is read from the
wire `meta` map, parsed, and removed before the remaining `meta` entries flow into
`Span.attributes`. It never appears in `Span.attributes` even transiently. The value is parsed as a
hex-encoded `u64`: trimmed of whitespace and parsed case- insensitively via `u64::from_str_radix(_,
16)`, accepting 1-16 hex characters (with or without zero-padding). Values that contain non-hex
characters or exceed 16 hex digits indicate a malformed span -- the span is dropped under the parent
RFC's Identifiers zero-`TraceId` handling (counter incremented, warning log identifying the dropped
span). A well-formed value is consumed into `Span.trace_id.high_u64`. Absent `_dd.p.tid`, or a key
present with a whitespace-only value (empty after trimming), is treated as equivalent to absent: the
high half is zero and the span is not dropped. This yields a valid 64-bit trace ID (high half zero).

The tag is sink-owned: Datadog egress derives it exclusively from the typed
`Span.trace_id.high_u64()`, so `trace_id` is the single source of truth for trace
identity. If the high half is non-zero, egress writes `meta["_dd.p.tid"]` as a zero-
padded 16-character lowercase hex string to match the Datadog Agent's canonical form; if
zero, the tag is omitted. Before writing the trace_id-derived value, any `_dd.p.tid`
entry placed into `meta` by the attribute partition step is removed, so the
trace_id-derived write is the sole source for this key regardless of what a transform
may have written to `attributes["_dd.p.tid"]`.

#### `SpanLink.traceID_high`

Unlike `Span` itself -- whose proto carries only a 64-bit `traceID` and stores the high
half out-of-band in `meta["_dd.p.tid"]` -- `SpanLink` carries the high 64 bits in a
dedicated wire field, `traceID_high`. Combining `traceID` and `traceID_high` into the
typed 128-bit `SpanLink.trace_id` on ingest, and splitting it back on egress, is required
for the `Datadog -> Vector -> Datadog` round trip to preserve links to 128-bit trace
IDs. A `traceID_high` of zero on the wire is equivalent to absent and yields a
`SpanLink.trace_id` whose high half is zero; on egress, a zero high half is emitted as
field-absent (or zero, which is byte-identical under proto3). The link-target
`_dd.p.tid` is not consulted on either direction: links may reference a different trace
than the enclosing span, and the wire field is the canonical carrier.

#### Zero-ID detection

Datadog ingress applies the parent RFC's zero-ID drop rule against the *combined* 128-bit
trace IDs, not the individual wire fields. A `Span` is dropped when `Span.traceID == 0`
and `meta["_dd.p.tid"]` is absent or parses to zero, since the resulting `TraceId` would
be all-zero; a `Span.traceID == 0` paired with a non-zero `_dd.p.tid` high half is valid
and is not rejected. A `Span` is also dropped when `Span.spanID == 0`. `SpanLink` is
dropped when `(spanLinks[*].traceID == 0 && spanLinks[*].traceID_high == 0)` or when
`spanLinks[*].spanID == 0`. All drops increment a counter
and emit a warning log defined by the parent RFC's Identifiers section.

Datadog `Span.parentID == 0` is a "no parent" sentinel and is not a zero-ID failure: it
maps to `Span.parent_span_id = None` rather than to a zero `SpanId`. On egress, a `None`
parent emits `parentID = 0` to match the agent's convention.

#### `SpanLink.flags`

Datadog's `SpanLink.flags` is `uint32`, and the Datadog convention is that bit 31 must
be set whenever the field is meaningful (the proto comment: "If set, the high bit (bit
31) must be set"). Storing the full word in `TraceFlags(u32)` preserves both bit 31 and
the W3C / OTLP-defined low bits so the round trip is bit-exact.

Datadog `Span` itself has no flags wire field and no trace-state wire field; on
cross-format Datadog egress, OTLP-sourced `Span.flags` and `Span.trace_state` are
dropped (in line with cross-format zero-loss being out of scope in the parent RFC). For
Datadog-sourced events, `Span.flags` and `Span.trace_state` are always their default
values on ingress, so this drop has no effect on a Datadog round trip. The same
constraint applies asymmetrically on the link path: on Datadog egress, `SpanLink.flags`
is emitted verbatim. For OTLP-sourced events bit 31 is not set, so the Datadog backend
treats the field as not meaningful and the W3C trace-flags byte plus the OTLP
`CONTEXT_HAS_IS_REMOTE` / `CONTEXT_IS_REMOTE` tristate carried by the link are not
surfaced through the Datadog wire. The sink does not synthesize bit 31. This is the
link-path analogue of the `Span.flags` cross-format drop and is out of scope for the
cross-format guarantee.

#### `SpanEvent.attributes` typed value mapping

Datadog `SpanEvent.attributes` is `map<string, AttributeAnyValue>`, where
`AttributeAnyValue` carries an explicit type tag (`STRING_VALUE`, `BOOL_VALUE`,
`INT_VALUE`, `DOUBLE_VALUE`, `ARRAY_VALUE`). This is distinct from the flat
`Span.meta` / `Span.metrics` partitions and maps directly to `AttrValue` variants.

Datadog's `AttributeAnyValue` has no native `bytes` or `kvlist` form. On Datadog egress,
`AttrValue::Bytes` is stringified to `STRING_VALUE` via `dd_value_to_string` (defined
below) and `AttrValue::Map` is stringified to a JSON `STRING_VALUE`. `AttrValue::Null`
entries are dropped from the wire map (the wire has no representation for "key present,
value absent"), parallel to the `Null` handling on the `meta` / `metrics` and `SpanLink`
egress paths.

#### Datadog attribute partitions: convention versus invariant

Datadog spans carry attributes in three independent wire-level maps:

- `meta`: keys to UTF-8 strings.
- `metrics`: keys to IEEE-754 doubles.
- `meta_struct`: keys to opaque bytes (msgpack-encoded structured payloads).

Datadog ingress maps each partition into `Span.attributes`:

- `meta` entries become top-level entries with `AttrValue::String` (or
  `AttrValue::Bytes` if the wire bytes fail UTF-8 validation, a non-conforming-producer
  case).
- `metrics` entries become top-level entries with `AttrValue::Double`.
- `meta_struct` entries are placed under the reserved key
  `Span.attributes."_dd.meta_struct"`, whose value is an `AttrValue::Map` mapping each
  `meta_struct` key to an `AttrValue::Bytes` payload.

If a producer emits the same key in both `meta` and `metrics`, the Datadog source
resolves the collision deterministically (`metrics` wins), increments a counter,
and emits a warning log. A key emitted in `meta_struct` and
either `meta` or `metrics`
normally retains both values (the `meta_struct` entry under
`attributes."_dd.meta_struct"`, the scalar entry as a top-level attribute) because the
two surfaces target different keys: the `meta_struct` sub-object lives at the reserved
key `_dd.meta_struct`, and any other producer-supplied `meta` or `metrics` key is
necessarily distinct from that reserved name.

The single exception is a producer that emits the literal key `_dd.meta_struct` in `meta` or
`metrics`: the scalar entry and the `meta_struct` sub-object both target
`Span.attributes."_dd.meta_struct"`. In this collision `meta_struct` wins on both ingress and
egress: on ingress the sub-object is placed after the scalar merge, overwriting any scalar at that
key; on egress step 1 drains the sub-object first, and step 2 skips the key because it has already
been consumed, so the `meta` scalar at `_dd.meta_struct` is dropped. A counter is incremented and a
  warning log is emitted in either direction for visibility. This case is declared as an explicit
round-trip exclusion above.

Datadog egress, in order:

1. Drain `Span.attributes."_dd.meta_struct"` into the wire `meta_struct` map (each
   sub-entry's `AttrValue::Bytes` payload becomes one `meta_struct` entry). A
   non-`Map` value at the reserved key, or a non-`Bytes` sub-entry within it, is
   dropped; a counter is incremented and a warning log identifies the dropped
   entry.
2. Partition the remaining attributes by `AttrValue` variant: `String` and `Bytes` to
   `meta` (the latter as a UTF-8-lossy string), `Double` and `Int` (coerced to `f64`)
   to `metrics`. `Null` is dropped (the wire has no representation for "key present,
   value absent"). Variants with no native Datadog partition (`Bool`, `Array`, `Map`)
   are stringified into `meta` via `dd_value_to_string`.

The result is one entry per non-`Null` key in exactly one wire partition.

**`dd_value_to_string` rule.** Wherever a Datadog `map<string, string>` wire field
requires every `AttrValue` to be coerced to a plain `String`, the following rule
applies: `String` is emitted verbatim; `Bytes` is emitted as its UTF-8 lossy string;
all other variants are emitted as their JSON encoding. The rule is total over
non-`Null` variants. `Null` entries are filtered out by every callsite -- map
iterations drop the entry rather than emit a wire string, since the wire
`map<string, string>` has no representation for "key present, value absent."

#### Datadog resource-scoped state

Datadog's agent-payload and tracer-payload envelopes carry resource-scoped metadata that
is preserved as two reserved top-level entries in `Resource.attributes`:

| Wire scope                       | `Resource.attributes` key | Value shape       |
| -------------------------------- | ------------------------- | ----------------- |
| `AgentPayload` (whole message)   | `_dd.payload`             | `AttrValue::Map`  |
| `TracerPayload.tags`             | `_dd.tracer`              | `AttrValue::Map`  |

`_dd.payload` mirrors the wire `AgentPayload` envelope under sub-keys: `host_name`,
`env`, `agent_version`, `target_tps`, `error_tps`, `rare_sampler_enabled` (the scalar
fields), and `tags` -- a nested `AttrValue::Map` of the wire-level `AgentPayload.tags`
map. The double-typed `target_tps` and `error_tps` slots use `AttrValue::Double` and
round-trip NaN unchanged. `_dd.tracer` carries only the wire-level `TracerPayload.tags`
map; `TracerPayload.hostname` and `TracerPayload.env` map to the typed
`Resource.host` / `Resource.environment` fields directly.

VRL access:

```coffee
.agent_host      = .resource.attributes."_dd.payload"."host_name"
.agent_apm_mode  = .resource.attributes."_dd.payload"."tags"."_dd.apm_mode"
.tracer_apm_mode = .resource.attributes."_dd.tracer"."_dd.apm_mode"
```

The two keys live under the `_dd.*` namespace alongside other Datadog-internal keys
(`_dd.apm_mode`, `_dd.tags.container`, `_dd.tags.process`, `_dd.p.dm`, `_dd.p.tid`,
`_dd.error_tracking_*`, `_dd.otel.gateway`).

#### Datadog chunk context

Datadog `TraceChunk.priority`, `origin`, `droppedTrace`, and `tags` apply uniformly to
every span in the chunk. Each `TraceEvent` corresponds to exactly one chunk by
construction, so these fields live on `TraceEvent.chunk` directly. OTLP-sourced events
carry a default-empty `ChunkContext` (no Datadog wire concept).

VRL access:

```coffee
.priority       = .chunk.priority
.origin         = .chunk.origin
.dropped        = .chunk.dropped
.decision_maker = .chunk.tags."_dd.p.dm"
```

#### Datadog egress derivation rules

When `Span.span_type` is `None` on Datadog egress (the normal case for OTLP-sourced
spans), the sink derives the wire `Span.type` from `Span.kind` and `Span.attributes`,
following the Datadog Agent's
[`SpanKind2Type`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/transform/otelutil.go)
logic:

- `Server` -> `"web"`.
- `Client` -> `"db"` if `db.system` attribute names a database system other than
  `redis` or `memcached`; `"cache"` if `db.system` is `redis` or `memcached`; `"http"`
  otherwise.
- All other kinds (`Internal`, `Producer`, `Consumer`, `Unspecified`, `Other`) ->
  `"custom"`.

If `Span.span_type` is `Some(v)`, the value is emitted as-is (Datadog-sourced spans
carry it directly). Because Datadog has no span-kind wire field, `Span.kind` is always
`Unspecified` for Datadog-sourced events on ingress; the `SpanKind2Type` derivation
therefore never fires on a pure `Datadog -> Vector -> Datadog` round trip.

When `Span.resource_name` is `None` on Datadog egress (the normal case for OTLP-sourced
spans), the sink derives the wire `Span.resource` from `Span.attributes` following the
Datadog Agent's OTLP ingest reference implementation, falling back to `Span.name` when
no matching attribute is present. The exact attribute key lookup logic (e.g.
`http.route`, `rpc.method`, `db.statement`) is deferred to the implementation PR
alongside the `span_type` derivation. If `Span.resource_name` is `Some(v)`, the value is
emitted as-is.

On Datadog egress, the sink:

- Sets each wire `Span.error` from `Span.status`: `Error(_)` or `Other(_, _)` -> `1`;
  `Unset` / `Ok` -> `0`. (`Other(code, _)` enforces `code` outside the known set
  `{0, 1, 2}` by construction, so every `Other` value represents a non-zero status
  code.) Datadog spans whose original wire `Span.error` was not `0` or `1` lose the
  specific integer on round trip; see the Datadog-side exclusions above.
- Flattens unmapped `Resource.attributes` entries into each span's wire `meta` under
  the attribute key.
  - Scope: applies to keys other than the typed-slot promotions (`service.name`,
    `deployment.environment.name`, `host.name`), the reserved cross-format envelope
    sub-objects (`_dd.payload`, `_dd.tracer`), and the TracerPayload-mapped semantic-
    convention keys per "`TracerPayload` semantic-convention key mapping" above.
  - Tie-breaker: a per-span `Span.attributes` entry at the same key wins over a
    `Resource.attributes` entry; the wire format has no resource-attribute scope, so
    the per-span duplication is the wire shape's nature, not Vector's choice.
  - For Datadog-sourced events these unmapped keys are empty by construction (Datadog
    ingest places non-promoted resource state in `_dd.payload` / `_dd.tracer`), so the
    round-trip is unaffected.
- Drains `Span.attributes."_dd.meta_struct"` into the wire `meta_struct` map and
  re-partitions the remaining attributes into `meta` / `metrics` by `AttrValue` variant
  per "Datadog attribute partitions" above.
- Reconstructs each `SpanEvent.attributes` entry as an `AttributeAnyValue` from the
  `AttrValue` variant per "`SpanEvent.attributes` typed value mapping" above, not the
  `meta` / `metrics` partitioning rule.
- Emits `Span.trace_id.low_u64()` as the wire `Span.traceID`; writes
  `meta["_dd.p.tid"]` from `Span.trace_id.high_u64()` if non-zero, omits it if zero
  (see "`_dd.p.tid`" above).
- Resolves the typed slot/attribute-map pair `Span.status` versus
  `Span.attributes."error.message"`:
  - When `Span.status` is `Error(message)` or `Other(_, message)` and `message` is
    non-empty, `meta["error.message"]` is set to the typed message, overwriting
    whatever the attribute partitioning step placed there. If the previous value
    differed from the typed message, the sink increments a counter and emits a warning log.
  - When `Span.status.message` is empty (`Unset`, `Ok`, `Error("")`, or `Other(_, "")`),
    the sink does not synthesize a `meta["error.message"]` tag and any value the
    attribute partitioning step placed there is left in place. This empty-message guard
    preserves the round trip for the conforming input `error = 1, no
    meta["error.message"]`, which ingests as `Error("")` with no attribute and must
    egress identically.
- Emits `SpanLink.attributes` as the wire `map<string, string>`: all `AttrValue`
  variants are stringified via `dd_value_to_string`.
  - The `meta` / `metrics` partitioning rule used for `Span.attributes` does not apply
    to links because the Datadog `SpanLink.attributes` wire type is a flat string map,
    not the `meta` / `metrics` / `meta_struct` triple.
  - For Datadog-sourced events, `SpanLink.attributes` values are already
    `AttrValue::String` on ingress due to the wire type, so the stringification on
    egress is lossless for `Datadog -> Vector -> Datadog` round trips.

#### Envelope reconstruction and chunk re-coalescence

Datadog egress groups events into wire `AgentPayload` / `TracerPayload` / `TraceChunk`
structures by nested grouping keys:

**`AgentPayload` grouping.** Groups events by their envelope and emits one
`AgentPayload` per group. The envelope used as the grouping key is origin-dependent,
matching the envelope-reconstruction policy below: for Datadog-originated events it is
`Resource.attributes."_dd.payload"`; for all other events it is the synthesized envelope
from typed `Resource` slots and proto3 defaults (per the Fallback sub-bullet below). A
`_dd.payload` attribute on a non-Datadog-originated event (e.g. set by a transform) does
not contribute to the grouping key, consistent with the reconstruction policy ignoring
it. This is the outermost grouping step, so every downstream `TracerPayload` and
`TraceChunk` is by construction confined to a single `AgentPayload`.

- Scalar reconstruction: each `AgentPayload`'s `hostName`, `env`, `agentVersion`,
  `targetTPS`, `errorTPS`, `rareSamplerEnabled`, and `tags` are read from the matching
  `_dd.payload` sub-keys; `tags` entries are coerced to the wire `map<string, string>`
  via `dd_value_to_string`.
- Fallback for non-Datadog-originated events: events with no `_dd.payload` envelope
  (e.g. OTLP-sourced or transform-synthesized) derive what they can from the typed
  `Resource` slots and default the rest. Specifically: `AgentPayload.hostName` is
  taken from `Resource.host`, `AgentPayload.env` from `Resource.environment`, and the
  agent-internal-only fields (`agentVersion`, `targetTPS`, `errorTPS`,
  `rareSamplerEnabled`, agent-level `tags`) are emitted as their proto3 defaults
  (empty string, `0.0`, `false`, empty map). No `datadog_traces` sink configuration
  governs these fields. Two non-Datadog-originated events with equal `Resource.host`
  and `Resource.environment` therefore share the same synthesized envelope and land in
  the same `AgentPayload`.
- Grouping on the full envelope preserves the partitioning Vector applies today, so
  two sets of events coming from different agent hosts or envs cannot be coalesced
  into the same `AgentPayload` and relayed traffic stays attributed to its originating
  agent.

**Envelope reconstruction policy.** The Datadog sink consults `_dd.payload` and
`_dd.tracer` for `AgentPayload` / `TracerPayload` envelope reconstruction only for
events identified as Datadog-originated; all other events use the typed-slot-and-
defaults derivation specified in the Fallback sub-bullet above for `_dd.payload`, and
emit an empty `TracerPayload.tags` for `_dd.tracer`.

During the migration the sink reads `vector.trace_legacy_layout` to identify
Datadog-originated events; because the hint is preserved across `vector` source/sink
hops, relay pipelines continue to forward the original agent envelope without operator
intervention. Post-migration the sink reads `EventMetadata.source_type` (set by the
topology source pump on every emission and reset to `"vector"` at each hop); operators
who want to relay Datadog envelope state across `vector` hops must enable that
explicitly in the sink configuration.

**`TracerPayload` grouping.** Within each `AgentPayload`, gather events with a
TracerPayload-envelope-equivalent `Resource` into one `TracerPayload`, with each span's
`Span.service` reconstructed from its event's `Resource.service`.

- Equivalence: two `Resource`s are TracerPayload-envelope-equivalent when every field
  that maps to a `TracerPayload` wire field in the ingest table above is equal.
  `Resource.schema_url`, `Resource.dropped_attributes_count`, and any
  `Resource.attributes` key not mapped to a `TracerPayload` field do not contribute to
  the grouping key. `Resource.attributes."_dd.payload"` is already pinned by the
  enclosing `AgentPayload` step, so it is by construction equal across every event in
  the group and is not part of this key either.
- Scalar reconstruction: the wire `TracerPayload`'s scalar fields are reconstructed by
  inverting the ingress mapping. `Resource.host` populates `TracerPayload.hostname`,
  `Resource.environment` populates `TracerPayload.env`, and the semantic-convention
  attributes per "`TracerPayload` semantic-convention key mapping" populate the
  corresponding `TracerPayload` scalars.
- Tags: `TracerPayload.tags` is reconstructed from
  `Resource.attributes."_dd.tracer"` per the envelope-reconstruction policy above;
  entries are coerced to the wire `map<string, string>` via `dd_value_to_string`.

**`TraceChunk` grouping and re-coalescence.** Within each `TracerPayload`, group spans across events
by all `ChunkContext` fields plus `trace_id`, and emit one `TraceChunk` per group.

- Empty events: an event whose `spans` vector is empty contributes no spans to any
  group; it emits one additional `TraceChunk` whose `priority`, `origin`, `tags`, and
  `dropped` are taken directly from `TraceEvent.chunk` and whose `spans` is empty,
  satisfying the parent RFC's empty-spans guideline.
- Tags comparison and serialization: the `tags` comparison is `BTreeMap` structural
  equality, which is canonical because `Attributes` is BTreeMap-backed and key
  ordering is therefore deterministic. `ChunkContext.tags` entries are serialized to
  the wire `TraceChunk.tags` (`map<string, string>`) via `dd_value_to_string`. For
  Datadog-sourced events, chunk tags are always `AttrValue::String` on ingress so this
  stringification is lossless on round-trip.
- Cross-grouping invariant: chunk grouping is nested inside `TracerPayload` grouping
  which is nested inside `AgentPayload` grouping, so events in the same chunk group
  are by construction in the same `TracerPayload` and `AgentPayload`. A transform that
  mutates `_dd.payload` on a subset of spans from the same original chunk causes those
  spans to land in a different `AgentPayload` at the outermost step, and therefore a
  different `TracerPayload` and `TraceChunk` as well, which is correct (the mutated
  envelope should not be coalesced with the original).
- Round-trip shapes: a multi-service wire chunk that was split into multiple events on
  ingest re-coalesces into one chunk on egress; a non-conforming multi-trace chunk
  produces one egress chunk per `trace_id`. Both shapes are equivalent to the input as
  observed by the Datadog backend (see Scope).

#### Cross-format conformance: `OTLP -> Vector -> datadog_traces`

A non-`datadog_agent`-sourced event reaching the `datadog_traces` sink produces the
wire output the Datadog Agent itself would produce for the same OTLP input; for
OTLP-sourced fields on Datadog egress, Datadog-Agent parity is the normative
requirement. The specific derivations are projections of the Datadog Agent OTLP
ingest reference (see Glossary above):

- The `Span.type` derivation (`SpanKind2Type` logic) and the `Span.resource` derivation
  (attribute-key lookup with `Span.name` fallback) are deferred to implementation PRs
  that mirror the upstream Agent code.
- The `TracerPayload` semantic-convention key set defining which `Resource.attributes`
  keys populate which `TracerPayload` scalar fields is similarly upstream-tracking.
- The `_dd.payload` envelope synthesis for OTLP-sourced events uses the typed-slot-
  and-defaults rule above; it produces no agent-internal fields (`agentVersion`,
  `targetTPS`, `errorTPS`, `rareSamplerEnabled`, agent-level `tags`) because the OTLP
  input has none, matching the Datadog Agent's own behaviour when serving as an OTLP
  receiver.

`datadog_agent -> Vector -> OTLP` is the inverse of the forward mapping for fields the
reference covers. Datadog-only concepts that the Datadog Agent does not produce on its
OTLP output have no inverse: chunk-scoped state and `Span.resource_name` /
`Span.span_type` are emitted under the reserved `datadog.*` OTLP span-attribute keys
defined by the OTLP mapping sub-RFC; the `_dd.payload` / `_dd.tracer` resource-scoped
envelopes and the `_dd.meta_struct` span sub-object flow through as `Resource.attributes`
and `Span.attributes` entries under their model-level keys via the generic `AttrValue` ->
`AnyValue` mapping. This entire path is best-effort and is explicitly out of the
zero-loss round-trip guarantee.

No upstream reference implementation for the reverse direction (Datadog wire -> OTLP) is
cited as normative; the OpenTelemetry Collector's
[`datadogreceiver`](https://github.com/open-telemetry/opentelemetry-collector-contrib/tree/main/receiver/datadogreceiver)
exists in `opentelemetry-collector-contrib` and may be consulted as a secondary
reference for fields that the Datadog Agent OTLP ingest does not document, but it is
not authoritative.

## Rationale

- The `meta` / `metrics` merge into `Span.attributes` relies on a producer-side
  disjointness convention rather than a wire-format invariant. The Datadog `Span` proto
  does not constrain keysets across the two scalar maps, but every examined Datadog SDK
  and the trace agent maintain disjointness by construction, and the two maps carry
  distinct value types (`AttrValue::String` versus `AttrValue::Double`) even in the
  rare collision case. The model treats the keyset disjointness as a contract the
  Datadog source asserts. If the convention ever ceases to hold for production traffic,
  the contained fallback (a separate `Span.datadog_attributes` field) is documented
  under "Alternatives".
- Datadog's wire `Span.error` is `int32` but documented as bivalent (`0` or `1`). The
  typed model normalizes non-conforming values to `Span.error = 1` on egress, diverging
  from the pre-typed-model sink which preserved arbitrary `int32` values byte-exactly
  (e.g. the `error = 404` unit test in `src/sinks/datadog/traces/tests.rs`). The
  normalized form is what every Datadog backend documents; the typed `SpanStatus` enum
  has no carrier for non-bivalent values, and preserving the raw integer would require
  an `Option<i32>` shadow field that no consumer would read.
- The `meta_struct` partition is preserved as a reserved sub-object
  (`Span.attributes."_dd.meta_struct"`) rather than merged into the flat attribute map.
  `AttrValue` distinguishes `String` from `Bytes` structurally so the wire types
  themselves would not collide, but the three partitions are semantically distinct
  (`meta_struct` payloads are msgpack-encoded structured records, not opaque scalars)
  and the reserved-key form documents that distinction at the typed surface. The
  encoding parallels the resource-level treatment of `AgentPayload.tags` and
  `TracerPayload.tags`.
- Agent-payload- and tracer-payload-scoped state are kept as separate sub-objects
  inside `Resource.attributes` rather than merged because the two scopes collide on
  known keys at both the tag-map level and the scalar level. The Datadog Agent's trace
  writer
  ([`pkg/trace/writer/trace.go`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/writer/trace.go))
  writes `_dd.apm_mode` into `AgentPayload.tags` from its own configuration, and the
  Agent's processing pipeline
  ([`pkg/trace/agent/agent.go`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/agent/agent.go))
  writes the same key into `TracerPayload.tags` from a span's `Meta`. The two values
  are semantically distinct (Agent's claimed mode versus tracer-reported mode) and
  appear in the same payload. The same collision class applies to the scalar fields:
  `AgentPayload.hostName` / `env` describe the collector and routinely differ from
  `TracerPayload.hostname` / `env` (which describe the application), and Vector's
  existing egress sink already partitions on the agent-level values to keep the two
  attribution domains distinct. The `_dd.payload` sub-object is structured to hold the
  full `AgentPayload` envelope (scalars plus `tags`) so egress can reconstruct that
  partitioning exactly; `_dd.tracer` carries only the tracer-tags map because the
  other tracer-payload fields have typed `Resource` slots.
- The envelope reconstruction policy is a default-behaviour choice, not a security
  boundary: a transform can freely overwrite `_dd.payload` values on a Datadog-
  originated event and those writes are honoured at egress. The policy's purpose is
  narrower -- ensuring that non-Datadog-sourced events do not accidentally use Datadog
  envelope fields from a `_dd.payload` attribute present for unrelated reasons (e.g.
  an OTLP producer that happens to use the `_dd.*` namespace, or a cross-format
  pipeline where the same `datadog_traces` sink receives both Datadog and OTLP input).
  The migration-period and post-migration mechanisms differ by design: the hint is
  preserved across hops so relay pipelines work without operator intervention; the
  post-migration `source_type` reset at hops tightens the default for new deployments
  at the cost of requiring operator opt-in for multi-hop Datadog relay.
- The Datadog egress rule for the `Span.status` / `error.message` typed-slot/attribute
  pair preserves the pure round-trip property by construction. A Datadog-sourced span
  with `error = 1` and no `meta["error.message"]` ingests as `Error("")` with no
  attribute, and egress emits no meta tag (the empty-message guard suppresses the
  overwrite). A span with `error = 1` and `meta["error.message"] = "x"` ingests as
  `Error("x")` with `attributes."error.message" = "x"`, and the typed overwrite writes
  the same `"x"` back -- no divergence event fires because the values are equal. The new
  behaviour relative to the pre-RFC sink appears only when a transform mutates one of the
  two without the other; in that case the typed value is selected and the divergence is
  observable, matching the precedent set on the other Datadog typed slot/attribute pairs
  (`Resource.{service,environment,host}` and `Span.trace_id.high_u64`).
- The Datadog egress chunk-grouping rule `(ChunkContext, trace_id)` relies on a
  producer-side convention parallel to the `meta` / `metrics` story: the `TraceChunk`
  proto describes a chunk as "a list of spans with the same trace ID", and Datadog
  producers honor this by construction. For the conforming case, multi-service chunks
  split on ingest re-coalesce into one egress chunk and single-service chunks pass
  through unchanged; for a non-conforming multi-trace chunk, egress emits one chunk
  per `trace_id`. Both shapes are effectively equivalent at the Datadog backend,
  since chunk grouping is an ingestion-time transport detail rather than a semantic
  primitive. All four `ChunkContext` fields contribute to the grouping key; `dropped`
  (`droppedTrace` on the wire) is chunk-scoped sampler state, not a per-group
  attribute: two chunks that share the same `(priority, origin, tags, trace_id)` but
  differ on `droppedTrace` must remain distinct egress chunks, otherwise the relay
  re-emits the second chunk's spans with the wrong dropped flag.
- Attribute iteration order within `SpanEvent.attributes` is not preserved by the
  parent RFC's `Attributes` carrier (BTreeMap-backed, sorted by key). The
  upstream Datadog `Span.proto` notes that this order "should be preserved," but the
  comment is not honored by Datadog's primary tracer SDKs (`dd-trace-go`'s msgpack
  encoder iterates a Go `map[string]*…`, randomizing on every emission) or by the
  Datadog Agent's OTLP receiver path (which stores OTLP-sourced events as a JSON
  string in `Meta["events"]` rather than the native `spanEvents` field). The
  reordering is therefore not backend-observable and falls under the parent RFC's
  Scope clause for "details the backend does not observe." No exclusion or carrier
  change is warranted.

## Drawbacks

- The Datadog round-trip guarantee depends on a producer-side keyset-disjointness
  convention between `meta` and `metrics`. The Alternatives below describe contained
  fallbacks if this convention ever ceases to hold.
- The `_dd.payload` / `_dd.tracer` reserved-key approach overloads `Resource.attributes`
  with what is semantically wire-protocol state. A purely-typed alternative would add
  more fields to `Resource` for each envelope component; the reserved-key form keeps
  the typed surface minimal and parallels the `_dd.meta_struct` treatment.
- Non-Datadog-originated events reaching the `datadog_traces` sink synthesize empty
  agent-internal envelope fields (no `agentVersion`, default TPS values, etc.). This is
  the same behaviour the Datadog Agent's own OTLP receiver exhibits, but operators who
  expected the relay to forge agent-version-style fields will be surprised.
- Datadog's `SpanLink.flags` bit 31 sentinel is not synthesized on egress for OTLP-
  sourced events, so the W3C trace-flags byte plus the OTLP remote-context tristate
  carried by such links are not surfaced through the Datadog wire. This is a cross-
  format asymmetry consistent with cross-format zero-loss being out of scope in the
  parent RFC.
- Additional parent-RFC-level drawbacks (VRL-config breakage on typed-path migration,
  per-span operations requiring `.spans` iteration, etc.) apply to Datadog-sourced and
  Datadog-bound events as well.

## Prior Art

- [Datadog APM agent-to-backend
  protobuf](https://github.com/DataDog/datadog-agent/tree/main/pkg/proto/datadog/trace)
  -- the wire format this sub-RFC targets.
- [Datadog Agent OTLP
  ingest](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go) --
  the normative reference for OTLP-to-Datadog field mappings (see the parent RFC's
  Glossary for the full role specification). Adopting an existing reference rather
  than defining a parallel mapping minimises divergence between Vector's
  `OTLP -> datadog_traces` path and the Datadog Agent's own OTLP ingest, so users
  moving traffic from the Agent's OTLP receiver to Vector see no change in attribution
  at the Datadog backend.
- The OpenTelemetry Collector
  [`datadogreceiver`](https://github.com/open-telemetry/opentelemetry-collector-contrib/tree/main/receiver/datadogreceiver)
  -- a secondary reference for the reverse direction (Datadog wire -> OTLP), used only
  for fields the Datadog Agent OTLP ingest does not document. Not authoritative.

## Alternatives

### Separate `Span.datadog_attributes` field preserving the three wire partitions verbatim

Carry a `DatadogAttributes { meta, metrics, meta_struct }` field on `Span` alongside the
canonical `attributes`, populated only on Datadog ingest. This represents the wire
format exactly and preserves any cross-partition collision between `meta` and `metrics`.
Rejected because it splits the attribute surface in two, forces every attribute-aware
component to handle both, and is paid against a `meta` / `metrics` collision case no
examined Datadog SDK or agent emits. Listed as the contained mechanical fallback if the
producer-side disjointness convention ever ceases to hold for production traffic; the
change is local to `Span`, the Datadog source, the Datadog sink, and a unified read
helper, with no impact on the OTLP side. The `meta_struct` partition is already
preserved exactly under `Span.attributes."_dd.meta_struct"` in the proposal and does not
motivate this alternative.

### Namespace-prefixed unified map for span partitions

Encode Datadog's two scalar span-attribute partitions inside `Span.attributes` itself by
prefixing each key with its partition name (`dd.meta.<k>`, `dd.metrics.<k>`), with
`meta_struct` similarly flattened under `dd.meta_struct.<k>`. Rejected because the
prefixes leak Datadog-specific encoding into every transform regardless of source: an
OTLP-only pipeline has to know about the namespace to avoid colliding with it, and an
OTLP-sourced attribute that happens to use a `dd.meta.*` key is silently misclassified
on egress. The `AttrValue`-variant routing for `meta` / `metrics` and the reserved-
sub-object form for `meta_struct` achieve the same egress mapping without imposing any
naming constraint on the flat attribute namespace.

### Bare top-level resource scope keys

Use `payload` and `tracer` as the two reserved top-level keys in `Resource.attributes`
instead of the namespaced `_dd.payload` / `_dd.tracer` adopted in the proposal. The
contents are identical in both forms. Rejected because the bare names are plausible
attribute keys that legitimate OTLP-sourced or transform-generated resource attributes
can already use: OpenTelemetry semantic conventions are uniformly dotted, but user-set
resource attributes (via `OTEL_RESOURCE_ATTRIBUTES=payload=...`), transform-generated
attributes (`.resource.attributes.payload = ...`), and future OpenTelemetry additions
are not bound by that convention. A collision under the bare-keys design is silent on
Datadog egress: the sink would either misclassify a legitimate user attribute as
Datadog wire data, or drop it as ill-typed. The namespaced form reduces the collision
class: no stable OpenTelemetry semantic-convention attribute uses a `_dd.*`-prefixed
key, so convention-defined attributes cannot collide. However, OTLP permits any custom
key, so a producer or transform may legitimately set
`Resource.attributes["_dd.payload"]` or `Resource.attributes["_dd.tracer"]`. When that
occurs, the value is interpreted as a Datadog envelope on Datadog egress; on OTLP egress
the key flows through under its model-level name via the generic attribute mapping. The
bare-keys design would produce the same collision class for any `payload` or `tracer`
attribute, with no path to distinguishing user data from envelope data. The `_dd.*`
namespace limits the collision to two specific reserved keys whose names signal
Datadog-internal intent; the residual collision is declared as an explicit exclusion
under "Datadog-specific zero-loss round-trip exclusions" rather than being silent, and
operators are advised to avoid setting these two keys outside of Datadog-sourced
pipelines.

### `ChunkContext.priority` as a raw `i32`

Datadog's wire representation is a signed integer with four well-known values
(`UserReject = -1`, `AutoReject = 0`, `AutoKeep = 1`, `UserKeep = 2`). Storing the raw
`i32` directly is simpler. Rejected because transforms that condition on priority then
have to compare against magic numbers, and there is no way to surface "this is a
non-standard value" to the user. A strict enum with an `Other(i32)` escape hatch
(parent RFC) keeps typed ergonomics for the common path while preserving any out-of-
range value.

## Outstanding Questions

- N/A.

## Plan Of Attack

The Datadog-mapping PRs sequence as follows. The format-agnostic prerequisites
(fallible proto decode boundary, migration enum, legacy-layout hint precursor, internal
`TypedTrace` proto extension) are owned by the parent RFC's Plan of Attack and must
land first.

- [ ] Remove the legacy `tracerPayloads`-empty Datadog ingest branch: delete
  `handle_dd_trace_payload_v0` from `src/sources/datadog_agent/traces.rs` and replace
  `proto/vector/dd_trace.proto` with the upstream agent-payload / tracer-payload / span
  protos. After the replacement, the upstream proto's unknown-field handling silently
  discards any historical `APITrace traces = 3` / `Span transactions = 4` data, and the
  resulting payload is processed via the standard path (zero events, lossless, per the
  Implementation section). Payloads carrying only `idxTracerPayloads = 11` (the upstream
  deduplicated tracer-payload form, deferred to Future Improvements) are rejected on
  ingest with an `unsupported_payload_version` component-spec error counter increment
  and an error log. Lands independently of the typed migration.
- [ ] Datadog `Legacy -> Typed` shim in `src/sources/datadog_agent/traces.rs`. Registers
  under the Datadog legacy-layout hint emitted by the precursor step in the parent.
- [ ] `Typed -> Datadog-wire` encoder in `src/sinks/datadog/traces/`. Implements the
  egress derivation rules above (`Span.type` and `Span.resource` derivation from
  `Span.kind` / `Span.attributes`, `Span.error` from `Span.status`, the typed-slot
  precedence rule for `Span.status` / `error.message` including the empty-message guard,
  `_dd.p.tid` from `Span.trace_id.high_u64`), the meta/metrics/meta_struct partitioning,
  the `SpanEvent.attributes` typed reconstruction, the `AgentPayload` / `TracerPayload`
  / `TraceChunk` grouping, and the envelope reconstruction policy.
- [ ] Property-based round-trip unit tests for `Datadog -> Vector -> Datadog`,
  asserting effective equivalence per Scope. Required coverage: multi-service
  single-trace chunks, non-conforming multi-trace chunks, 128-bit `SpanLink` trace
  IDs, `_dd.p.tid` consumption and emission, `_dd.meta_struct` byte preservation,
  `_dd.payload` envelope reconstruction across `vector` source/sink hops, `Span.error`
  normalization, NaN round-trip through `Span.metrics` / `SpanEvent.attributes` /
  `_dd.payload.{target_tps,error_tps}`, two same-`TracerPayload` chunks with identical
  `(priority, origin, tags, trace_id)` but differing `droppedTrace` values (must egress
  as two distinct chunks each preserving its own `droppedTrace`).
- [ ] Cross-format conformance tests for `OTLP -> Vector -> datadog_traces`,
  comparing the wire output against the Datadog Agent's OTLP receiver on representative
  OTLP inputs. The reference outputs are captured from a Datadog Agent and committed
  alongside the tests; updates to the upstream agent track via test refresh.
- [ ] Migrate the `datadog_traces` sink to consume `Typed` natively; update APM stats
  aggregation to read typed fields (`Resource.service`, `Span.duration`,
  `TraceEvent.chunk.priority`, etc.).
- [ ] Migrate the `datadog_agent` source to produce `Typed` natively. Lands after the
  parent's "remove untyped forwarding methods" step so the build catches any
  unmigrated consumer.
- [ ] Document the Datadog mapping in the trace migration guide section the parent
  RFC's POA owns: the three-partition merge and the reserved
  `Span.attributes."_dd.meta_struct"` key, the
  `Resource.attributes."_dd.payload"` / `"_dd.tracer"` envelope sub-objects,
  `TraceEvent.chunk`, and the typed-slot precedence rule for `Span.status` versus
  `Span.attributes."error.message"`. Document the `Span.error` normalization as a
  behavioural change relative to the pre-typed sink.

## Future Improvements

- Datadog `AgentPayload.idxTracerPayloads = 11` (indexed / deduplicated tracer-payload form): map
  the indexed shape through the same typed model. Initial implementation rejects
  `idxTracerPayloads`-only payloads (see Plan of Attack). The indexed shape has been reviewed
  against the typed slots and fits field-for-field with no structural changes required; remaining
  work is string-table codec at the wire boundary.
- VRL helper for decoding raw Datadog `Span` protobuf into the typed surface
  (`decode_datadog_span`), parallel to the OTLP mapping sub-RFC's `decode_otlp_span`
  helper.
- Adopt the Datadog tracer-to-agent endpoints (`/v0.3/traces`, `/v0.4/traces`,
  `/v0.5/traces`, `/v0.7/traces`) as additional Datadog ingress shapes. The endpoints
  are upstream of the agent-to-backend hop this sub-RFC targets; the typed model
  accommodates them with format-specific shims and no schema changes.
