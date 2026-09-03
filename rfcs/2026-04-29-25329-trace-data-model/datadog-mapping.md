# RFC 25329 - 2026-04-29 - Trace Data Model: Datadog Mapping

This sub-RFC of [RFC 25329 -- Internal Trace Data Model](../2026-04-29-25329-trace-data-model.md)
specifies the bidirectional mapping between the typed `TraceEvent` defined in the parent RFC
and the Datadog agent-to-backend trace protobuf. It establishes the Datadog ingress and
egress paths, the effective-equivalence round-trip guarantee for
`Datadog -> Vector -> Datadog`, the trace-ID-and-service chunk split/coalesce rules, the
reserved Datadog event and span contexts that carry agent-payload, tracer-payload, chunk, and
`meta_struct` state, and
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
    `parentID`, `start`, `duration`, `error`, `meta`, `metrics`, `type`, `meta_struct`,
    `spanLinks`, `spanEvents`).
- **Datadog Agent OTLP ingest**: the OTLP-to-Datadog conversion implemented by the
  Datadog Agent in
  [`pkg/trace/api/otlp.go`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go)
  (and supporting code under
  [`pkg/trace/transform/`](https://github.com/DataDog/datadog-agent/tree/main/pkg/trace/transform)).
  Current `main` is the normative reference for the enumerated Agent-aligned
  derivations below and governs their details if a prose summary lags an upstream
  change. The RFC's explicit mapping remains normative for all other fields; exact
  reproduction of the Agent's complete OTLP converter output is not required.
- **Datadog tracer-to-agent API** (informational): the SDK-to-agent hop upstream of the
  agent-to-backend format Vector targets. Vector does not consume these endpoints
  directly.

## Cross cutting concerns

- APM stats aggregation in the `datadog_traces` sink, today reading magic keys from
  `TraceEvent`, will read typed fields after this RFC and its parent land. The sink
  re-coalesces service partitions for one trace before aggregation so trace-wide inputs
  such as the root span's `_sample_rate` remain available to every span.
- The OTLP-side reservation of `datadog.*` resource and span attributes for synthesis
  of Datadog-native context on OTLP egress is the OTLP mapping sub-RFC's concern; this
  sub-RFC owns the context values those keys carry on cross-format relay.

## Scope

### In scope

- The bidirectional mapping between `TraceEvent` and Datadog `AgentPayload` /
  `TracerPayload` / `TraceChunk` / `Span` messages.
- The parent RFC's effective-equivalence round-trip guarantee, applied to
  `Datadog -> Vector -> Datadog`. Stable partitioning preserves span order within each
  trace-ID group. For a non-conforming multi-trace chunk, the relative positions of
  spans from distinct trace IDs and their resulting egress chunk grouping are details
  the Datadog backend does not observe and so may differ.
- The three Datadog span-attribute partitions (`meta`, `metrics`, `meta_struct`) and how
  the two scalar partitions map into `Span.attributes` by `AttrValue` variant while
  `meta_struct` is preserved in `Span.datadog.meta_struct`.
- The agent-payload envelope and tracer-payload tags in
  `TraceEvent.datadog.{agent,tracer}`.
- The chunk-scoped typed state in `TraceEvent.datadog.chunk = Some(...)`
  (`priority`, `origin`, `dropped`, `tags`).
- The stable partition of each chunk by its wire spans' reconstructed trace ID and
  `Span.service` on ingress, storing the ID in `TraceEvent.trace_id`, and the
  corresponding re-coalescence by trace ID on egress.
- The authoritative reconstruction policy for explicitly present Datadog namespace
  state and the common-field fallback when that state is absent.
- The cross-format conformance rule for `OTLP -> Vector -> datadog_traces`: a
  trace whose Datadog namespace is absent/default follows the enumerated Datadog-Agent
  derivations below and is backend-effectively equivalent under Vector's typed mapping.
  It need not reproduce the Agent converter's exact wire shape.

### Out of scope

- The OTLP wire mapping (see the OTLP mapping sub-RFC).
- Zero-loss cross-format round-trip (`Datadog -> OTLP -> Datadog`); see the parent RFC's
  Out of scope.
- `TracerPayload.containerDebug` (Datadog-internal container-tag-resolution diagnostic);
  dropped on ingest, not synthesized on egress.
- Typed decoding of `AgentPayload.idxTracerPayloads = 11` (the indexed/deduplicated
  tracer-payload form). Until support lands, standard `tracerPayloads` entries are
  processed while indexed entries are discarded; see "Ingress and egress
  mapping."
- Typed decoding of the pre-`tracerPayloads` `/api/v0.2/traces` shape
  (`traces` / `transactions` with empty `tracerPayloads`). Today's source routes that
  payload to `handle_dd_trace_payload_v0`. The typed mapping discards those spans and
  reports the drop; see "Ingress and egress mapping." Removing that decode path, and the
  local proto fields that carry it, may land independently of the typed migration.
- Exact wire parity with the Datadog Agent's complete OTLP converter, including its
  legacy metadata encodings and configuration-dependent synthetic tags.

### Zero-loss round-trip exclusions

The effective-equivalence guarantee for `Datadog -> Vector -> Datadog` does not cover the
following input shapes. Each is justified by a paragraph in the Implementation or
Rationale section below.

- **`Span.error` values other than `0` or `1`** ingest as `SpanStatus::Error(...)` and
  egress as `Span.error = 1`, normalizing the specific integer to the conforming
  bivalent representation.
- **Malformed non-empty `meta["_dd.p.tid"]`** rejects the enclosing span even when its
  low 64-bit `Span.traceID` is non-zero.
- **Datadog wire-domain normalization**: negative wire durations are clamped to zero on
  ingress; durations and timestamps outside their destination fields' domains are
  clamped to the nearest endpoint on egress.
- **`meta`/`metrics` producer-side non-disjointness**: the round-trip guarantee is conditional on
  these two scalar maps being keyset-disjoint. If a producer emits the same key in both, the Datadog
  source resolves the collision deterministically (`metrics` wins) and
  saturating-increments `Span.dropped_attributes_count`; the dropped scalar
  is not recoverable on egress.
- **Unknown `AttributeAnyValue` or array-element type discriminators** drop the
  containing span-event attribute and saturating-increment
  `SpanEvent.dropped_attributes_count`.
The Datadog-side consequences of the parent RFC's zero-ID rejection and wire-domain
normalization also apply.

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
sees is the `TraceEvent.datadog` and `Span.datadog` namespaces. The
event namespace carries the optional agent envelope, tracer tags, and optional chunk;
the span namespace carries `resource_name`, `span_type`, and `meta_struct`. This sub-RFC
specifies how those typed paths map to and from the Datadog wire format.

```coffee
# Read a Datadog chunk-scoped tag.
decision_maker = .datadog.chunk.tags."_dd.p.dm"

# Read agent- and tracer-payload state.
agent_apm_mode = .datadog.agent.tags."_dd.apm_mode"
tracer_apm_mode = .datadog.tracer.tags."_dd.apm_mode"

# Inspect a meta_struct sub-entry (msgpack-encoded; Vector exposes it as bytes).
meta_struct_event = .spans[0].datadog.meta_struct."dd.event_payload"
```

### Implementation

#### Ingress and egress mapping

Until indexed decoding is implemented, the source discards every non-empty
`idxTracerPayloads` entry. Standard `tracerPayloads` in the same `AgentPayload` continue
through the mapping below. An indexed-only payload consequently produces zero events and
is nonetheless acknowledged, avoiding retries that cannot succeed on the same Vector
version.

After indexed entries are handled as above, a modern `AgentPayload` whose
`tracerPayloads` repeated field is empty and whose historical `traces` and
`transactions` fields are also empty, or a `TracerPayload` whose `chunks` repeated
field is empty, produces zero `TraceEvent`s: there is no
`TraceChunk` from which to populate `TraceEvent.datadog.chunk` and no event grouping is
well-defined in isolation, so the wire input has no `TraceEvent` representation. That
standard-empty case is lossless because it carries no span data the Datadog backend
would observe.

A payload whose `tracerPayloads` is empty but whose `traces` or `transactions` fields
carry spans is the pre-`tracerPayloads` shape. Today's source routes it to
`handle_dd_trace_payload_v0`, which emits one event per `APITrace` and one event per
`transactions` span. The typed mapping does not ingest that shape: those spans are
discarded and the request is acknowledged so it is not retried. This is
not a lossless empty payload. Adopting the upstream `AgentPayload` proto, which does not
declare fields 3 and 4, makes the same spans undecodable unknown fields; the independent
removal of that path must still report an empty-`tracerPayloads` payload so an operator
on an Agent old enough to emit it gets a diagnosable signal.

An `AgentPayload` with at least one `TracerPayload` carrying at least one `TraceChunk`
with at least one span expands into one `TraceEvent` per distinct
`(reconstructed wire trace ID, Span.service)` pair within each
`(TracerPayload, TraceChunk)`.

The grouping rules are:

- Scan each `TraceChunk`'s successfully decoded spans in wire order. The first span for
  a `(trace_id, service)` pair creates a group at the end of the group sequence; later
  spans with that pair append to the existing group. Emit one `TraceEvent` per group in
  first-seen pair order, storing the pair's ID in `TraceEvent.trace_id` and preserving
  span order within each group. A
  conforming single-trace, single-service chunk remains one event; a multi-service or
  non-conforming multi-trace chunk splits as needed. Egress re-coalesces service groups
  for the same trace (see below). A `TraceChunk` whose `spans` repeated field is empty
  produces zero `TraceEvent`s, extending the empty-`tracerPayloads` and
  empty-`chunks` rule above one level down: no wire span is available to supply the
  required `TraceEvent.trace_id`, no `Span.service` is available to populate
  `Resource.service`, and a chunk envelope with no spans carries nothing the Datadog
  backend would observe. Datadog ingress therefore never synthesizes an event that
  exists only to satisfy the parent RFC's empty-spans rule; that rule still governs
  Datadog egress for typed input and events that transforms filter empty.
- The enclosing `TracerPayload`'s metadata (`hostname`, `env`, `containerID`,
  `languageName`, `tracerVersion`, etc.) populates the event's `Resource`. Per-span
  `Span.service` populates `Resource.service`.
- The enclosing `AgentPayload`'s envelope (`hostName`, `env`, `agentVersion`, `targetTPS`,
  `errorTPS`, `rareSamplerEnabled`, and `tags`) populates
  `TraceEvent.datadog.agent = Some(...)`;
  `TracerPayload.tags` populates `TraceEvent.datadog.tracer.tags` (see "Datadog
  event-scoped state" below).
- `TraceChunk.{priority, origin, droppedTrace, tags}` populate
  `TraceEvent.datadog.chunk = Some(DatadogChunkContext { ... })`, including when every
  field is default.
  Every decoded `priority` value is present internally: `0` maps to
  `Some(SamplingPriority::AutoReject)` and every other `i32` maps through the parent
  RFC's canonical constructor.
- `Scope` is left default; Datadog has no scope concept.

The `datadog_agent` legacy shim applies the same grouping. Today's
`convert_dd_tracer_payload` emits one `LegacyTraceEvent` per `TraceChunk`, including
when that chunk contains spans with more than one `Span.service` or reconstructed wire
trace ID.
Pre-flip source output keeps that one-event-per-chunk shape so existing legacy VRL is
unchanged. The shim splits
the chunk's successfully converted spans by distinct reconstructed trace ID and
`Span.service` pairs into the same typed events native ingest would have produced, in
first-seen pair order.
Metadata, finalizers, and acknowledgements on the resulting sequence follow the
parent RFC's conversion contract. An empty-spans
legacy chunk converts to zero typed events, matching native ingest.

| Datadog                                                       | Internal                                              |
| ------------------------------------------------------------- | ----------------------------------------------------- |
| `TracerPayload.hostname`                                      | `Resource.host`                                       |
| `TracerPayload.env`                                           | `Resource.environment`                                |
| `Span.service` (per span)                                     | `Resource.service` of the event holding the span      |
| `AgentPayload` envelope (whole message; see below)            | `TraceEvent.datadog.agent = Some(...)`                |
| `TracerPayload.tags`                                          | `TraceEvent.datadog.tracer.tags`                      |
| `TraceChunk.{priority, origin, droppedTrace, tags}`           | `TraceEvent.datadog.chunk = Some(...)`                |
| `TracerPayload` non-host/env scalar fields (see below)        | `Resource.attributes` under defined keys              |
| `Span.traceID` (u64)                                          | `TraceEvent.trace_id.low_u64`                         |
| `Span.meta["_dd.p.tid"]` (hex u64) if present (see below)     | `TraceEvent.trace_id.high_u64`                        |
| `Span.spanID`, `Span.parentID`                                | `Span.span_id`, `Span.parent_span_id`                 |
| `Span.name`                                                   | `Span.name`                                           |
| `Span.resource`                                               | `Span.datadog.resource_name`                          |
| `Span.type`                                                   | `Span.datadog.span_type`                              |
| `Span.start`, `Span.duration`                                 | `Span.start_time`, `Span.duration` (ns-exact)         |
| `Span.error` and `Span.meta["error.message"]`                 | `Span.status` (see below)                             |
| `Span.meta`                                                   | `Span.attributes` (`AttrValue::String`, see below)    |
| `Span.metrics`                                                | `Span.attributes` (`AttrValue::Double`)               |
| `Span.meta_struct`                                            | `Span.datadog.meta_struct` (`Map<Bytes>`)             |
| `Span.spanEvents[*].{time_unix_nano, name}`                   | `SpanEvent.{time, name}`                              |
| `Span.spanEvents[*].attributes` (`AttributeAnyValue`)         | `SpanEvent.attributes` (typed `AttrValue` per variant)|
| `Span.spanLinks[*].traceID` (u64)                             | `SpanLink.trace_id.low_u64` in `Span.links`           |
| `Span.spanLinks[*].traceID_high` (u64)                        | `SpanLink.trace_id.high_u64`                          |
| `Span.spanLinks[*].spanID`                                    | `SpanLink.span_id`                                    |
| `Span.spanLinks[*].tracestate`                                | `SpanLink.trace_state` (verbatim)                     |
| `Span.spanLinks[*].flags` (u32)                               | `SpanLink.flags` (full u32 verbatim)                  |
| `Span.spanLinks[*].attributes`                                | `SpanLink.attributes` (`AttrValue::String`)           |

The cross-format derivation rules later in this section (`datadog.span_type` from
`Span.kind` / `Span.attributes`, `datadog.resource_name` from `Span.attributes` / `Span.name`, the
`TracerPayload` semantic-convention key set, and the flattening of unmapped
`Resource.attributes` into per-span `meta`) are projections of the Datadog Agent OTLP
ingest reference in the Glossary. The precise key sets and lookup orders track current
upstream behavior without requiring a specification amendment.

#### `TracerPayload` semantic-convention key mapping

`TracerPayload` fields mapped to `Resource.attributes` under OpenTelemetry
semantic-convention keys: `containerID`, `languageName`, `languageVersion`,
`tracerVersion`, `runtimeID`, `appVersion`. The specific OpenTelemetry attribute key each
wire field maps to is defined by the Datadog Agent OTLP-ingest reference, which the
Vector mapping mirrors. The `TracerPayload`-envelope-equivalence rule in the egress
section consequently keys off the attribute set defined by the current reference.

`TracerPayload.hostname` and `TracerPayload.env` map to typed
`Resource.host` / `Resource.environment` directly and are not part of the deferred
attribute set. `TracerPayload.containerDebug` is a Datadog-internal diagnostic with no
Vector consumer and is dropped on ingest (see "Out of scope").

#### Empty Datadog string fields

Datadog's string scalars do not expose proto3 presence, so decode cannot distinguish an
omitted field from one explicitly encoded as empty. The mapping nevertheless preserves
the decoded empty value as `Some("")` for `Resource.{service,environment,host}`,
`Span.datadog.{resource_name,span_type}`, and `DatadogChunkContext.origin`; `None` means
no source value was available. Datadog egress therefore reproduces the empty value, and
the `resource_name` / `span_type` fallback applies only to `None`.

#### Signed timing fields on ingress

Datadog's `Span.duration` and `Span.start` wire fields are `int64` and can therefore
carry negative values, which no documented Datadog tracing SDK produces. The two
disagree on ingress disposition, so neither follows from the parent RFC's numeric
boundary policy:

- A negative `Span.duration` is clamped to zero, because `std::time::Duration` cannot
  represent it.
- A negative `Span.start` is preserved as a pre-epoch timestamp, which the typed model
  can represent. It remains subject to the parent RFC's pre-epoch clamp if it later
  crosses a disk buffer or `vector` hop, so a round trip preserves it only within a
  single instance.

Egress clamping for these fields and for `SpanEvent.time` follows the parent RFC's
policy against the concrete wire domains. The negative-duration clamp is declared as a
Datadog-side round-trip exclusion above.

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
hex-encoded `u64`. A value that cannot be parsed that way indicates a malformed span, and
the span is dropped under the parent RFC's malformed-input rule even when the low half is
non-zero, because trace identity cannot be reconstructed. A well-formed value contributes
to the grouping key stored in `TraceEvent.trace_id`. An absent `_dd.p.tid`, or a key
present with an empty value, is treated as equivalent to absent: the high half is zero and the span is not
dropped, yielding a valid 64-bit trace ID. The accepted lexical forms are an
implementation choice.

The tag is sink-owned: Datadog egress derives it exclusively from
`TraceEvent.trace_id.high_u64()`, so the event-level ID is the single source of truth
for trace identity. If the high half is non-zero, egress writes `meta["_dd.p.tid"]` as a zero-
padded 16-character lowercase hex string to match the Datadog Agent's canonical form; if
zero, the tag is omitted. Before writing the event-ID-derived value, any `_dd.p.tid`
entry placed into `meta` by the attribute partition step is removed, so the
event-ID-derived write is the sole source for this key regardless of what a transform
may have written to `attributes["_dd.p.tid"]`. Removing such an entry is reported;
Datadog has no wire dropped-attribute count to update.

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

Datadog ingress applies the parent RFC's zero-ID drop rule, including its drop
granularity and reporting, with one format-specific qualification: because Datadog
splits a 128-bit trace ID across two wire fields, the rule is evaluated against the
*combined* ID rather than either field alone. A `Span.traceID` of zero paired with a
non-zero `_dd.p.tid` high half is a valid ID and is not rejected; the same applies to a
`SpanLink` whose `traceID` is zero but whose `traceID_high` is not.

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
value absent"), parallel to the `Null` handling on the `meta` / `metrics`
and `SpanLink` egress paths. An `AttrValue::Array` maps scalar elements directly.
Elements without an
array-scalar representation (`Bytes`, nested `Array`, `Map`, or `Null`) become
`STRING_VALUE` using `dd_value_to_string`; `Null` uses the literal `"null"`.

On ingress, an unknown `AttributeAnyValue.type` or an unknown type on any element of its
`array_value` makes the containing map attribute unrepresentable. The source drops only
that attribute, saturating-increments `SpanEvent.dropped_attributes_count`, and reports
the drop; other attributes and the event continue through the mapping.

#### Datadog attribute partitions: convention versus invariant

Datadog spans carry attributes in three independent wire-level maps:

- `meta`: keys to UTF-8 strings.
- `metrics`: keys to IEEE-754 doubles.
- `meta_struct`: keys to opaque bytes (msgpack-encoded structured payloads).

Datadog ingress maps the partitions into the common and Datadog span surfaces:

- `meta` entries become top-level entries with `AttrValue::String`.
- `metrics` entries become top-level entries with `AttrValue::Double`.
- `meta_struct` entries populate `Span.datadog.meta_struct`, which maps each key
  directly to its opaque `Bytes` payload.

The protobuf decoder rejects invalid UTF-8 in `map<string, string>` keys or values at
the payload boundary, before this attribute mapping runs; no raw-string fallback decoder
is introduced.

If a producer emits the same key in both `meta` and `metrics`, the Datadog source
resolves the collision deterministically (`metrics` wins), saturating-increments
`Span.dropped_attributes_count`, and reports the drop. A key emitted in `meta_struct`
and either scalar map retains both values because `Span.datadog.meta_struct` and
`Span.attributes` are structurally separate. This includes the literal scalar key
`_dd.meta_struct`; it does not collide with the structured partition.

Datadog egress, in order:

1. Copy `Span.datadog.meta_struct` into the wire `meta_struct` map.
2. Partition the remaining attributes by `AttrValue` variant: `String` and `Bytes` to
   `meta` (the latter as a UTF-8-lossy string), `Double` and `Int` (coerced to `f64`)
   to `metrics`. `Null` is dropped (the wire has no representation for "key present,
   value absent"). Variants with no native Datadog partition (`Bool`,
   `Array`, `Map`) are stringified into `meta` via `dd_value_to_string`.

The result is one entry per non-`Null` key in exactly one wire partition.

**`dd_value_to_string` rule.** Several Datadog wire fields are `map<string, string>` and
therefore require every `AttrValue` to be coerced to a plain `String`. All of them use
one shared coercion, named `dd_value_to_string` throughout this document, which is total
over every `AttrValue` variant, deterministic for a given value (including recursive
ordering within `Array` and `Map`), and independent of any JSON library's non-finite-number
behavior. A top-level `Null` map entry has no wire representation, so it is omitted
rather than coerced. The specific rendering of each variant is an implementation
choice satisfying those properties.

#### Datadog event-scoped state

The `AgentPayload` envelope populates `TraceEvent.datadog.agent`; `None` means that no
agent envelope is present. Its typed scalar fields are `host_name`, `env`,
`agent_version`, `target_tps`, `error_tps`, and `rare_sampler_enabled`, and its `tags`
field carries the wire-level `AgentPayload.tags` map. The double fields preserve NaN
payloads unchanged.

`TracerPayload.tags` populates `TraceEvent.datadog.tracer.tags`.
`TracerPayload.hostname` and `TracerPayload.env` map to the common
`Resource.host` / `Resource.environment` fields directly. The tracer context is
always present because protobuf cannot distinguish an omitted map from an empty map.

#### Datadog chunk context

Datadog `TraceChunk.priority`, `origin`, `droppedTrace`, and `tags` apply uniformly to
every span in the chunk. Every Datadog-sourced `TraceEvent` corresponds to exactly one
chunk by construction and therefore carries `TraceEvent.datadog.chunk = Some(...)`, including
for an explicitly all-default chunk. An OTLP-sourced event with no recovered Datadog
chunk state carries `None`.

Because proto3 provides no scalar presence for `TraceChunk.priority`, every decoded
wire value maps to `Some(SamplingPriority)`: zero is `Some(AutoReject)`, known non-zero
values use their canonical variants, and other `i32` values use `Other`. `None` is
reserved for events with no source priority, such as an OTLP event without recovered
Datadog chunk state. On Datadog egress, that `None` emits `AutoKeep` (wire 1), matching
today's `datadog_traces` sink and the Datadog Agent OTLP ingest at its default sampling
rate. Wire `0` is reserved for an explicit `AutoReject`.

#### Datadog egress derivation rules

When `Span.datadog.span_type` is `None` on Datadog egress (the normal case for OTLP-sourced
spans), the sink derives the wire `Span.type` from `Span.kind` and `Span.attributes`,
following the Datadog Agent's
[`SpanKind2Type`](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/transform/otelutil.go)
logic. Per the deferral above, the individual kind-to-type cases track that upstream
reference and are not fixed by this document.

If `Span.datadog.span_type` is `Some(v)`, the value is emitted as-is (Datadog-sourced spans
carry it directly). Because Datadog has no span-kind wire field, `Span.kind` is always
`Unspecified` for Datadog-sourced events on ingress; the `SpanKind2Type` derivation
therefore never fires on a pure `Datadog -> Vector -> Datadog` round trip.

When `Span.datadog.resource_name` is `None` on Datadog egress (the normal case for OTLP-sourced
spans), the sink derives the wire `Span.resource` from `Span.attributes` following the
Datadog Agent's OTLP ingest reference implementation, falling back to `Span.name` when
no matching attribute is present. If `Span.datadog.resource_name` is `Some(v)`, the value is
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
    `deployment.environment.name`, `host.name`) and the TracerPayload-mapped semantic-
    convention keys per "`TracerPayload` semantic-convention key mapping" above.
  - Tie-breaker: a per-span `Span.attributes` entry at the same key wins over a
    `Resource.attributes` entry; the wire format has no resource-attribute scope, so
    the per-span duplication is the wire shape's nature, not Vector's choice. Each
    discarded resource-scoped value is reported; Datadog has no wire dropped-attribute
    count to update.
  - For Datadog-sourced events these unmapped keys are empty by construction; native
    agent/tracer state lives in `TraceEvent.datadog`, so the round-trip is unaffected.
- Copies `Span.datadog.meta_struct` into the wire `meta_struct` map and partitions
  `Span.attributes` into `meta` / `metrics` by `AttrValue` variant per "Datadog
  attribute partitions" above.
- Reconstructs each `SpanEvent.attributes` entry as an `AttributeAnyValue` from the
  `AttrValue` variant per "`SpanEvent.attributes` typed value mapping" above, not the
  `meta` / `metrics` partitioning rule.
- Emits `TraceEvent.trace_id.low_u64()` as every contained wire `Span.traceID`; writes
  `meta["_dd.p.tid"]` from `TraceEvent.trace_id.high_u64()` if non-zero, omits it if zero
  (see "`_dd.p.tid`" above).
- Resolves the typed slot/attribute-map pair `Span.status` versus
  `Span.attributes."error.message"`:
  - When `Span.status` is `Error(message)` or `Other(_, message)` and `message` is
    non-empty, `meta["error.message"]` is set to the typed message, overwriting
    whatever the attribute partitioning step placed there. If the previous value
    differed from the typed message, the overwrite is reported.
  - When `Span.status.message` is empty (`Unset`, `Ok`, `Error("")`, or `Other(_, "")`),
    the sink does not synthesize a `meta["error.message"]` tag and any value the
    attribute partitioning step placed there is left in place. This empty-message guard
    preserves the round trip for the conforming input `error = 1, no
    meta["error.message"]`, which ingests as `Error("")` with no attribute and must
    egress identically.
- Emits `SpanLink.attributes` as the wire `map<string, string>`: non-`Null`
  `AttrValue` variants are stringified via `dd_value_to_string`, and `Null` entries are
  omitted.
  - The `meta` / `metrics` partitioning rule used for `Span.attributes` does not apply
    to links because the Datadog `SpanLink.attributes` wire type is a flat string map,
    not the `meta` / `metrics` / `meta_struct` triple.
  - For Datadog-sourced events, `SpanLink.attributes` values are already
    `AttrValue::String` on ingress due to the wire type, so the `dd_value_to_string`
    conversion on egress is lossless for `Datadog -> Vector -> Datadog` round trips.

Datadog `Span`, `SpanEvent`, and `SpanLink` provide no dropped-attribute count fields.
Consequently, Datadog-egress attribute losses are reported but cannot propagate an
updated in-band count on that wire.

#### Envelope reconstruction and chunk re-coalescence

Before grouping, Datadog egress normalizes namespace state into its wire shape.
Agent-envelope scalar fields are already typed. Entries in agent, tracer, and chunk tag
maps use `dd_value_to_string` and omit top-level `Null`. Grouping keys and wire
serialization both use this normalized result, so transform-authored state cannot
produce inconsistent grouping or a sink failure.

All equality used by the nested Datadog grouping steps is structural. Double values are
compared by their IEEE-754 `to_bits()` representation, so a NaN equals the same preserved
NaN payload and positive and negative zero remain distinct. This applies to normalized
envelope doubles and doubles nested in resource or chunk attributes.

Datadog egress groups events into wire `AgentPayload` / `TracerPayload` / `TraceChunk`
structures by nested grouping keys:

**`AgentPayload` grouping.** Groups events by their effective envelope and emits one
`AgentPayload` per group. `TraceEvent.datadog.agent = Some(...)` is authoritative
regardless of which source or transform populated it. When it is `None`, the effective
envelope is synthesized from common `Resource` slots and proto3 defaults. This is the
outermost grouping step, so every downstream `TracerPayload` and `TraceChunk` is by
construction confined to a single `AgentPayload`.

- Scalar reconstruction: each `AgentPayload`'s `hostName`, `env`, `agentVersion`,
  `targetTPS`, `errorTPS`, `rareSamplerEnabled`, and `tags` are read from the matching
  fields in a present `datadog.agent`; tags use the normalization above.
- Fallback for an absent agent envelope: events with `datadog.agent = None`
  derive what they can from the typed
  `Resource` slots and default the rest. Specifically: `AgentPayload.hostName` is
  taken from `Resource.host`, `AgentPayload.env` from `Resource.environment`, and the
  agent-internal-only fields (`agentVersion`, `targetTPS`, `errorTPS`,
  `rareSamplerEnabled`, agent-level `tags`) are emitted as their proto3 defaults
  (empty string, `0.0`, `false`, empty map). No `datadog_traces` sink configuration
  governs these fields. Two such events with equal `Resource.host`
  and `Resource.environment` therefore share the same synthesized envelope and land in
  the same `AgentPayload`.
- Grouping on the full envelope preserves the partitioning Vector applies today, so
  two sets of events coming from different agent hosts or envs cannot be coalesced
  into the same `AgentPayload` and relayed traffic stays attributed to its originating
  agent.

**Namespace authority.** Datadog egress does not inspect
`EventMetadata.source_type` or `vector.trace_legacy_layout` when reconstructing typed
events. A source, OTLP bridge, or transform that explicitly populates `.datadog`
expresses an intent to control Datadog-native egress state. Ordinary resource and span
attributes never acquire that authority merely because their keys resemble Datadog
internals. Typed namespace state therefore survives disk buffers and intermediate
`vector` source/sink hops without an origin-specific passthrough option. The migration
hint remains solely the temporary legacy-shim selector defined by the parent RFC.

**`TracerPayload` grouping.** Within each `AgentPayload`, gather events with a
TracerPayload-envelope-equivalent `Resource` into one `TracerPayload`, with each span's
`Span.service` reconstructed from its event's `Resource.service`.

- Equivalence: two events are TracerPayload-envelope-equivalent when their normalized
  `datadog.tracer.tags` are equal and every `Resource` field that maps to a
  `TracerPayload` wire field in the ingest table above is equal.
  `Resource.schema_url`, `Resource.dropped_attributes_count`, and any
  `Resource.attributes` key not mapped to a `TracerPayload` field do not contribute to
  the grouping key. The effective agent envelope is already pinned by the enclosing
  `AgentPayload` step.
- Scalar reconstruction: the wire `TracerPayload`'s scalar fields are reconstructed by
  inverting the ingress mapping. `Resource.host` populates `TracerPayload.hostname`,
  `Resource.environment` populates `TracerPayload.env`, and the semantic-convention
  attributes per "`TracerPayload` semantic-convention key mapping" populate the
  corresponding `TracerPayload` scalars.
- Tags: `TracerPayload.tags` is reconstructed from
  `TraceEvent.datadog.tracer.tags`; entries use the normalization above. The default
  tracer context therefore emits an empty map.

**`TraceChunk` grouping and re-coalescence.** Before grouping, map
`datadog.chunk = None` to a synthesized `DatadogChunkContext` whose other fields are
default and whose missing `priority` is treated as the egress value `AutoKeep`.
Grouping keys use that same effective priority, so `None` and
`Some(DatadogChunkContext::default())` share an egress group as `AutoKeep`. Within each
`TracerPayload`, group spans across events by the effective `DatadogChunkContext` plus
`TraceEvent.trace_id`, and emit one `TraceChunk` per group. This
re-coalesces the service partitions of a conforming chunk while keeping a
non-conforming multi-trace chunk separated by ID. An explicit `Some(AutoReject)`,
including a Datadog-decoded all-default chunk whose wire priority was `0`, does not share
that group.

The sink forms these effective chunk/trace groups before APM stats aggregation, not
only before wire serialization. Every service partition in one group is presented to
the aggregator together, preserving trace-wide context such as `_sample_rate` carried
only by the root span. The computation within a reconstructed group remains governed by
RFC 9862.

- Empty events: an event whose `spans` vector is empty contributes no spans to any
  group; it emits one additional `TraceChunk` whose `priority`, `origin`, `tags`, and
  `dropped` are taken from the effective chunk context and whose `spans` is empty,
  satisfying the parent RFC's empty-spans guideline. The Datadog wire shape has no
  carrier for that empty event's `TraceEvent.trace_id`. Datadog ingress does not
  produce such events, so they reach this step only from typed input or a transform
  that filtered every span out, outside the pure-relay guarantee.
- Tags comparison and serialization: the `tags` comparison is canonical structural
  equality with deterministic key ordering. `DatadogChunkContext.tags` entries are serialized to
  the wire `TraceChunk.tags` (`map<string, string>`) via `dd_value_to_string`. For
  Datadog-sourced events, chunk tags are always `AttrValue::String` on ingress so this
  stringification is lossless on round-trip.
- Cross-grouping invariant: chunk grouping is nested inside `TracerPayload` grouping
  which is nested inside `AgentPayload` grouping, so events in the same chunk group
  are by construction in the same `TracerPayload` and `AgentPayload`. A transform that
  mutates `.datadog.agent` on a subset of events split from the same original chunk
  causes those spans to land in a different `AgentPayload` at the outermost step, and therefore a
  different `TracerPayload` and `TraceChunk` as well, which is correct (the mutated
  envelope should not be coalesced with the original).
- Round-trip shapes: a multi-service wire chunk that was split into multiple events on
  ingest re-coalesces into one chunk on egress; a non-conforming multi-trace chunk,
  including one with several services per trace, produces one egress chunk per
  `trace_id`. Both shapes are equivalent to the input as observed by the Datadog
  backend (see Scope).

#### Cross-format conformance: `OTLP -> Vector -> datadog_traces`

An event whose Datadog-native slots are absent or default follows the enumerated
Agent-aligned derivations below. Explicit Datadog state lifted from reserved OTLP bridge
attributes is authoritative instead. Conformance means backend-effective
equivalence under Vector's typed mapping, not exact reproduction of every field the
Datadog Agent OTLP converter would place on the wire. Current Agent `main`, as linked in
the Glossary, is the reference for these derivations:

- The `Span.type` derivation (`SpanKind2Type` logic) and the `Span.resource` derivation
  (attribute-key lookup with `Span.name` fallback) follow the upstream Agent code.
- The `TracerPayload` semantic-convention key set defining which `Resource.attributes`
  keys populate which `TracerPayload` scalar fields is similarly upstream-tracking.
- Agent-envelope synthesis when `.datadog.agent` is absent uses the common-field-and-
  defaults rule above; it produces no agent-internal fields (`agentVersion`,
  `targetTPS`, `errorTPS`, `rareSamplerEnabled`, agent-level `tags`) because the OTLP
  input has none, matching the Datadog Agent's own behaviour when serving as an OTLP
  receiver.
- Chunk priority when `.datadog.chunk` is absent or `.datadog.chunk.priority` is `None`
  emits `AutoKeep` (wire 1). Vector does not implement the Agent's probabilistic OTLP
  sampler; this matches today's `datadog_traces` sink and the Agent converter at its
  default sampling rate.

The rest follows this RFC's explicit typed mapping. In particular, Vector emits
`SpanEvent` and `SpanLink` through their native Datadog protobuf fields and preserves a
128-bit trace ID through `_dd.p.tid`; it does not reproduce the Agent converter's legacy
`meta["events"]`, `meta["_dd.span_links"]`, `meta["otel.trace_id"]`, or
`meta["span.kind"]` encodings unless a separate mapping rule above requires the same
field. Those wire-shape differences are permitted only where the Datadog backend
observes equivalent trace data.

`datadog_agent -> Vector -> OTLP` is the inverse of the forward mapping for fields the
reference covers. The OTLP mapping sub-RFC defines reserved `datadog.*` resource and
span bridge attributes for the agent envelope, tracer tags, chunk state, span resource
name and type, and `meta_struct`. OTLP egress synthesizes those attributes from the
typed Datadog namespaces and OTLP ingress lifts them back. This entire path is
best-effort and is explicitly out of the zero-loss round-trip guarantee.

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
  from the pre-typed-model sink which preserved arbitrary `int32` values byte-exactly. The
  normalized form is what every Datadog backend documents; the typed `SpanStatus` enum
  has no carrier for non-bivalent values, and preserving the raw integer would require
  an `Option<i32>` shadow field that no consumer would read.
- The `meta_struct` partition is preserved in `Span.datadog.meta_struct` rather than
  merged into the common attribute map. `AttrValue` distinguishes `String` from `Bytes`
  structurally, but the partitions are semantically distinct: `meta_struct` payloads
  are msgpack-encoded structured records, not opaque scalars. A dedicated bytes map
  also prevents a producer scalar named `_dd.meta_struct` from colliding with the wire
  partition.
- Agent-payload- and tracer-payload-scoped state are kept as separate typed contexts
  rather than merged because the two scopes collide on
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
  attribution domains distinct. `DatadogAgentEnvelope` holds the full agent envelope
  while `DatadogTracerContext` carries only tags because the other tracer-payload
  fields have common `Resource` slots.
- Namespace authority is an intent boundary, not a security boundary. Ordinary
  attributes cannot accidentally become an agent envelope, while a transform that
  explicitly writes `.datadog` is asking to control Datadog-native egress state. This
  removes source-type heuristics and preserves the state across `vector` hops.
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
  (`Resource.{service,environment,host}` and `TraceEvent.trace_id.high_u64`).
- The Datadog egress chunk-grouping rule
  `(effective DatadogChunkContext, TraceEvent.trace_id)`
  follows directly from the typed model's single-trace-ID invariant. For the conforming
  case, multi-service chunks split on ingest re-coalesce into one egress chunk and
  single-service chunks pass through unchanged; for a non-conforming multi-trace chunk,
  egress emits one chunk per `trace_id`. Both shapes are effectively equivalent at the
  Datadog backend, since chunk grouping is an ingestion-time transport detail rather
  than a semantic primitive. All four `DatadogChunkContext` fields contribute to the
  grouping key; `dropped` (`droppedTrace` on the wire) is chunk-scoped sampler state,
  not a per-group attribute: two chunks that share the same
  `(priority, origin, tags, TraceEvent.trace_id)` but differ on `droppedTrace` must
  remain distinct egress chunks, otherwise the relay re-emits the second chunk's spans
  with the wrong dropped flag.
- Attribute iteration order within `SpanEvent.attributes` is not preserved by the
  parent RFC's key-sorted `Attributes` carrier. Although the upstream Datadog
  `Span.proto` notes that this order "should be preserved," the comment is not honored
  by Datadog's own producers, so the reordering falls under the Scope clause for details
  the backend does not observe and needs no exclusion.

## Drawbacks

- The Datadog round-trip guarantee depends on a producer-side keyset-disjointness
  convention between `meta` and `metrics`. The Alternatives below describe contained
  fallbacks if this convention ever ceases to hold.
- Events without an explicit Datadog agent envelope reaching the `datadog_traces` sink synthesize empty
  agent-internal envelope fields (no `agentVersion`, default TPS values, etc.). This is
  the same behaviour the Datadog Agent's own OTLP receiver exhibits, but operators who
  expected the relay to forge agent-version-style fields will be surprised.
- Datadog's `SpanLink.flags` bit 31 sentinel is not synthesized on egress for OTLP-
  sourced events, so the W3C trace-flags byte plus the OTLP remote-context tristate
  carried by such links are not surfaced through the Datadog wire. This is a cross-
  format asymmetry consistent with cross-format zero-loss being out of scope in the
  parent RFC.

## Prior Art

- [Datadog APM agent-to-backend
  protobuf](https://github.com/DataDog/datadog-agent/tree/main/pkg/proto/datadog/trace)
  -- the wire format this sub-RFC targets.
- [Datadog Agent OTLP
  ingest](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/otlp.go) --
  the moving reference for the enumerated Agent-aligned derivations. Reusing those
  derivations minimises attribution differences without requiring Vector to reproduce
  the Agent converter's complete wire output.
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
preserved exactly under `Span.datadog.meta_struct` in the proposal and does not
motivate this alternative.

### Namespace-prefixed unified map for span partitions

Encode Datadog's two scalar span-attribute partitions inside `Span.attributes` itself by
prefixing each key with its partition name (`dd.meta.<k>`, `dd.metrics.<k>`), with
`meta_struct` similarly flattened under `dd.meta_struct.<k>`. Rejected because the
prefixes leak Datadog-specific encoding into every transform regardless of source: an
OTLP-only pipeline has to know about the namespace to avoid colliding with it, and an
OTLP-sourced attribute that happens to use a `dd.meta.*` key is silently misclassified
on egress. The `AttrValue`-variant routing for `meta` / `metrics` and the typed
`Span.datadog.meta_struct` map achieve the same egress mapping without imposing any
naming constraint on the flat attribute namespace.

### Reserved resource-attribute envelope objects

Encode `DatadogEventContext.agent` and `.tracer` under
`Resource.attributes."_dd.payload"` / `"_dd.tracer"` and use event provenance to decide
whether Datadog egress should honor them. Rejected primarily because provenance signals
are rewritten at intermediate `vector` hops, losing envelope authority unless operators
configure a separate passthrough rule. Custom OTLP resource attributes may also
legitimately use those keys, requiring a dynamic origin check to distinguish common
attributes from Datadog wire state. The explicit `.datadog` namespace makes authoring
intent structural, survives internal serialization, and leaves ordinary `_dd.*`
resource attributes untouched.

### `DatadogChunkContext.priority` as a raw `i32`

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

Removing the obsolete `tracerPayloads`-empty ingest branch and adopting the upstream
Datadog protos may land independently. That removal must report a payload whose
`tracerPayloads` is empty, including one that still carries `traces` / `transactions`
spans, rather than treating it as a lossless empty modern payload.

The remaining work starts after the format-agnostic prerequisites (fallible proto decode
boundary, temporary `TraceEventCompat` enum, legacy-layout hint precursor, and internal
`TypedTrace` proto extension) in the parent RFC:

1. Implement `LegacyTraceEvent -> TraceEvent` conversion and unique detection of
   historical pre-hint Datadog layouts. The converter must split each chunk by
   reconstructed trace ID and `Span.service` in stable first-seen pair order and apply
   the parent RFC's metadata and acknowledgement contract to the resulting sequence.
2. Implement `TraceEvent -> Datadog` encoding satisfying the mapping and egress contracts
   above.
3. Establish the `Datadog -> Vector -> Datadog` effective-equivalence guarantee,
   and validate every declared exclusion.
4. Establish the enumerated `OTLP -> Vector -> datadog_traces` conformance rules against
   the current Datadog Agent reference.
5. Migrate the `datadog_traces` sink and APM stats aggregation after the parent RFC's
   compile-time consumer gate. Reconstruct effective chunk/trace groups before invoking
   the aggregator so service partitions share root-span context.
6. Publish the user migration guide, then migrate the `datadog_agent` source after typed
   input passes end-to-end through Datadog export. Typed source events retain the
   migration hint for the parent RFC's deprecation window.

## Future Improvements

- Datadog `AgentPayload.idxTracerPayloads = 11` (indexed / deduplicated tracer-payload form): map
  the indexed shape through the same typed model. Until that support is added, indexed
  entries are discarded while standard entries continue. A stricter mode may instead
  reject any payload containing indexed entries when operators prefer all-or-nothing
  acceptance. The indexed shape has been reviewed against the typed slots and fits
  field-for-field with no structural changes required; remaining work is string-table
  codec at the wire boundary.
- VRL helper for decoding raw Datadog `Span` protobuf into the typed surface
  (`decode_datadog_span`), parallel to the OTLP mapping sub-RFC's `decode_otlp_span`
  helper.
- Adopt the Datadog tracer-to-agent endpoints (`/v0.3/traces`, `/v0.4/traces`,
  `/v0.5/traces`, `/v0.7/traces`) as additional Datadog ingress shapes. The endpoints
  are upstream of the agent-to-backend hop this sub-RFC targets; the typed model
  accommodates them with format-specific shims and no schema changes.
