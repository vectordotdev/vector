# RFC 25329 - 2026-04-29 - Trace Data Model: OTLP Mapping

This sub-RFC of [RFC 25329 -- Internal Trace Data Model](../2026-04-29-25329-trace-data-model.md)
specifies the bidirectional mapping between the typed `TraceEvent` defined in the parent RFC
and the OTLP wire format. It establishes the
[`OpenTelemetry Protocol`](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)
ingress and egress paths, the effective-equivalence round-trip guarantee for
`OTLP -> Vector -> OTLP`, and the per-attribute encoding rules between OTLP's `AnyValue` and the
parent RFC's `AttrValue`.

## Context

- The parent RFC defines the typed data model, migration mechanics, and internal wire
  serialization. This sub-RFC assumes those definitions and the parent's Glossary, In/Out
  scope clauses, and User Experience as background. In particular, every failure, drop,
  and normalization below is reported per the parent's global reporting rule and the
  [instrumentation specification](../../docs/specs/instrumentation.md) it cites, whether
  or not the rule here restates it.
- [RFC 11851 -- OpenTelemetry traces source](../2022-03-15-11851-ingest-opentelemetry-traces.md)
  was accepted on the condition that an internal trace model be established before the work was
  completed. This sub-RFC, together with the parent and the Datadog mapping sub-RFC, satisfies
  that condition.

## Cross cutting concerns

- First-class OpenTelemetry signal support
  ([vectordotdev/vector#1444](https://github.com/vectordotdev/vector/issues/1444)).
- Cross-format relay (OTLP source -> `datadog_traces` sink): the conformance rule is specified
  by the Datadog mapping sub-RFC; this sub-RFC defines only OTLP-side ingress and egress.

## Scope

### In scope

- The bidirectional mapping between `TraceEvent` and OTLP `ResourceSpans` / `ScopeSpans` /
  `Span` messages.
- The stable partition of each `ScopeSpans` by its wire spans' `trace_id`, producing one
  `TraceEvent` per distinct ID with that ID stored at `TraceEvent.trace_id`.
- The parent RFC's effective-equivalence round-trip guarantee, applied to
  `OTLP -> Vector -> OTLP`. Attribute iteration order within `Span.attributes`, when the
  producer-side ordering was non-canonical, and the partition of spans among otherwise
  equivalent `ScopeSpans` messages are details an OTLP backend does not observe and so
  may differ.
- The promotion rule for the three semantic-convention attributes (`service.name`,
  `deployment.environment.name`, `host.name`) into typed `Resource` slots, including the
  legacy-key acceptance for `deployment.environment`.
- The 1:1 mapping between `AttrValue` variants and OTLP's `string_value` / `bytes_value` /
  `int_value` / `bool_value` / `double_value` / `array_value` / `kvlist_value` (with unset
  oneof representing `AttrValue::Null`).
- Reservation of the `datadog.` attribute-key prefix at the OTLP wire boundary, at both
  resource and span scope, for synthesis on OTLP egress and lift on OTLP ingress of the
  Datadog-native contexts, for best-effort cross-format relay. This sub-RFC reserves the
  prefix while the Datadog sub-RFC specifies the context values those keys carry.
- The OTLP mapping targets fields at the OpenTelemetry
  [`Stable`](https://opentelemetry.io/docs/specs/otel/versioning-and-stability/)
  stability tier or higher.

### Out of scope

- The Datadog wire mapping (see the Datadog mapping sub-RFC).
- Zero-loss cross-format round-trip (`OTLP -> Datadog -> OTLP`); see the parent RFC's Out of
  scope.
- OTLP fields at `Development` or `Alpha` stability tier; see Future Improvements for the
  adoption path.

### Zero-loss round-trip exclusions

The effective-equivalence guarantee for `OTLP -> Vector -> OTLP` does not cover the following
input shapes. Each is justified by a paragraph in the Implementation or Rationale section
below.

- **Promoted deprecated `deployment.environment` key** is rewritten to
  `deployment.environment.name` on OTLP egress. If both keys are present on ingress, the
  deprecated value is dropped (the stable key wins). See the deprecated-environment
  paragraph under Implementation.
- **Reserved cross-format OTLP attributes** under the `datadog.` prefix are lifted into
  the typed `TraceEvent.datadog` / `Span.datadog` slots on OTLP ingress and stripped,
  then synthesized from those slots on OTLP egress, removing or replacing any existing
  common attribute at the same key.
- **OTLP fields at `Development` or `Alpha` stability tier** are dropped on OTLP ingress.
- **`Span.end_time_unix_nano < start_time_unix_nano`** is clamped to zero duration on
  ingress. The egress reconstruction emits
  `end_time_unix_nano = start_time_unix_nano`, byte-different from the original input.
- **`Status.message` paired with `UNSET` or `OK`** is dropped on ingress;
  OpenTelemetry permits a description only for `ERROR`.
- **Duplicate keys in an OTLP attribute list or nested `KeyValueList`** use wire-order
  last-wins normalization. Each overwritten value saturating-increments the nearest
  enclosing item's `dropped_attributes_count`.
- **Empty `ScopeSpans`** produce no event because the wire grouping supplies no trace ID
  for required `TraceEvent.trace_id`. Resource and scope state carried only by empty
  groupings is therefore not relayed.

The OTLP-side consequences of the parent RFC's zero-ID rejection and wire-domain
normalization also apply. The derived end-timestamp case is documented under "Span
timing" below.

## Pain

- The `opentelemetry` trace source today emits an untyped `TraceEvent(LogEvent)` whose key
  layout is the source's choice. Cross-format relay to the `datadog_traces` sink requires
  bespoke per-key translation; relay back to OTLP (if Vector grows an OTLP sink) would have
  to re-discover the original wire shape. Both directions duplicate work that a typed model
  removes by construction.
- The OTLP `AnyValue` oneof discriminator (`string_value` versus `bytes_value` versus
  `int_value`, etc.) is lost when attribute values are stored as raw bytes in today's
  `LogEvent`-backed trace events. Egress must guess the variant; for non-UTF-8 byte
  payloads the guess is always wrong. The parent RFC's `AttrValue` storage preserves the
  discriminator structurally.

## Proposal

### User Experience

The OTLP wire mapping is invisible to VRL: programs read and write the typed `TraceEvent`
surface defined in the parent RFC. The only OTLP-specific surface the user sees is the
specification below of which wire fields populate which typed slots, used by operators
diagnosing relay-path discrepancies and by component authors writing OTLP encoders /
decoders.

### Implementation

#### Ingress and egress mapping

Each OTLP `ScopeSpans` is partitioned by the wire `Span.trace_id`. The grouping
algorithm is stable and deterministic:

1. Scan its successfully decoded spans in wire order.
2. On the first span for a `trace_id`, create that ID's group at the end of the group
   sequence.
3. Append every later span with that ID to the existing group, preserving span order
   within the group.
4. Emit one `TraceEvent` per group in first-seen `trace_id` order.

For example, spans ordered `[A1, B1, A2, C1, B2]` produce events
`A: [A1, A2]`, `B: [B1, B2]`, then `C: [C1]`.

Every resulting event receives the containing `ResourceSpans.resource` as
`TraceEvent.resource` and the `ScopeSpans.scope` as `TraceEvent.scope`.
`TraceEvent.datadog` and each `Span.datadog` start at their defaults and are populated
only by valid reserved `datadog.*` bridge attributes as specified below.
An empty `ScopeSpans` produces no event because it supplies no trace ID. A non-empty
`ScopeSpans` whose spans are all rejected likewise produces no event under the parent
RFC's all-spans-rejected rule. A typed event made empty by a transform remains
representable because it retains `TraceEvent.trace_id`.

The OTLP legacy shim applies the same partitioning to every `ScopeSpans` it recovers. A
pre-flip per-span OTLP event converts one-to-one into a typed event whose `spans` holds
that span. A legacy event carrying several `ScopeSpans` groupings (for example
`use_otlp_decoding` batch encoding) may fan out further by the distinct trace IDs in
each grouping. Metadata, finalizers, and acknowledgements on the resulting sequence
follow the parent RFC's conversion contract.

| OTLP                                                               | Internal                                      |
| ------------------------------------------------------------------ | --------------------------------------------- |
| `ResourceSpans.resource.attributes["service.name"]`                | `Resource.service`                            |
| `ResourceSpans.resource.attributes["deployment.environment.name"]` | `Resource.environment` (see below)            |
| `ResourceSpans.resource.attributes["deployment.environment"]`      | `Resource.environment` (legacy fallback)      |
| `ResourceSpans.resource.attributes["host.name"]`                   | `Resource.host`                               |
| `ResourceSpans.resource.attributes` (others, see promotion rule)   | `Resource.attributes`                         |
| `ResourceSpans.resource.dropped_attributes_count`                  | `Resource.dropped_attributes_count`           |
| `ResourceSpans.schema_url`                                         | `Resource.schema_url`                         |
| `ScopeSpans.scope.{name, version, attributes}`                     | `Scope.{name, version, attributes}`           |
| `ScopeSpans.scope.dropped_attributes_count`                        | `Scope.dropped_attributes_count`              |
| `ScopeSpans.schema_url`                                            | `Scope.schema_url`                            |
| `Span.trace_id`                                                    | `TraceEvent.trace_id`                         |
| `Span.span_id`, `Span.parent_span_id`                              | same (zero-ID handling: see below)            |
| `Span.trace_state`                                                 | `Span.trace_state` (verbatim)                 |
| `Span.flags`, `Link.flags` (see flags layout)                      | `Span.flags`, `SpanLink.flags` (full u32)     |
| `Link.trace_id` (see zero-ID handling)                             | `SpanLink.trace_id` (link target)             |
| `Span.name`, `Span.kind`                                           | `Span.name`, `Span.kind`                      |
| `Span.start_time_unix_nano`, `end_time_unix_nano` (see timing)     | `Span.start_time`, `Span.duration` (ns-exact) |
| `Span.attributes`                                                  | `Span.attributes`                             |
| `Span.events`, `Span.links`                                        | `Span.events`, `Span.links`                   |
| `Span.status.{code,message}` (see status)                          | `Span.status.{code,message}`                  |
| `Span.dropped_*_count`                                             | `Span.dropped_*_count`                        |

Attributes under the reserved `datadog.` prefix are exceptions to the generic attribute
rows: resource-level keys lift into `TraceEvent.datadog`, while span-level keys lift
into `TraceEvent.datadog.chunk` or `Span.datadog`. The bridge section below defines
that reservation.

`SpanLink.trace_id` identifies the link target and may differ from the enclosing
`TraceEvent.trace_id`.

On OTLP egress, `TraceEvent`s sharing a `Resource` (including `Resource.schema_url`) and
the same resource-level Datadog bridge projection are gathered into one
`ResourceSpans`. Within it, non-empty events sharing the same `Scope` and event-level
`TraceEvent.datadog` bridge projection are re-coalesced into one `ScopeSpans`; spans retain event
order and their order within each event. Re-coalescence prevents partitioning from
multiplying scope-level state such as `Scope.dropped_attributes_count`. The emitted wire
grouping may again contain several trace IDs; the single-ID invariant applies to
Vector's internal `TraceEvent`, not to OTLP.

Every emitted OTLP `Span.trace_id` is copied from the enclosing
`TraceEvent.trace_id`; internal spans have no independent trace ID.

An empty event is not coalesced and becomes one `ScopeSpans { spans: [] }`; OTLP has no
field in that shape to carry its `TraceEvent.trace_id`. Such events arise only after
typed construction or transformation, outside the pure-relay guarantee. Two events
with identical common `Resource` content but different agent envelopes or tracer tags
therefore produce separate `ResourceSpans` messages. Datadog-native context has no
common attribute-map representation; the OTLP sink synthesizes it under reserved
resource and span keys (see "Reserved OTLP bridge keys" below) for best-effort
cross-format relay. Ordinary `_dd.*` keys have no special meaning and flow through the
generic `AttrValue` -> `AnyValue` mapping.

#### Zero-ID detection

OTLP wire IDs are raw byte arrays interpreted in big-endian order. Because they are
length-delimited rather than fixed-width, this mapping adds a structural requirement the
parent RFC's rule does not imply: a trace ID must be exactly 16 bytes and a span ID
exactly 8 bytes, and a wrong-length ID is malformed in the same way an all-zero ID is.
Rejection granularity and reporting then follow the parent RFC's identifier rule.

`Span.parent_span_id` has one explicit normalization because the model represents root
spans with `None`: an empty sequence or exactly eight zero bytes maps to `None`, and
exactly eight non-zero bytes maps to `Some(SpanId)`. Any other non-empty length is
malformed and rejects the enclosing span with the same invalid-ID telemetry. On egress,
IDs use the same fixed-width big-endian encoding and a `None` parent emits an empty
`parent_span_id`.

#### Semantic-convention promotion and the typed-slot precedence

Three resource attribute keys promote to typed `Resource` slots on ingress: `service.name`,
`deployment.environment.name` (with `deployment.environment` accepted as a legacy
fallback -- see below), and `host.name`.

Promotion to a typed `Resource` field is conditional on the attribute value being a
`string_value`. When the value is a non-empty string (the normal case), promotion is
move-not-copy: the key is removed from `Resource.attributes` and the typed slot is the
sole post-ingress owner of the value. This matches the move-not-copy pattern used for
`_dd.p.tid` consumption on Datadog ingest and for the reserved cross-format keys. An
empty `string_value` or a non-string variant is not promoted: the typed slot remains
`None` and the attribute remains under its original key, preserving the producer's
value and its presence on OTLP egress.

VRL transforms that want to change the service, environment, or host should write to the
typed slots (`.resource.service`, `.resource.environment`, `.resource.host`) rather than
to the corresponding attribute-map keys. Because promotion strips the source attribute on
ingress, the duplicate-key case arises only when (i) a transform writes to both the typed
slot and the matching attribute key, or (ii) the source attribute was empty or non-string
and the promotion rule above left it in place.

On OTLP egress specifically, the typed slot wins for the three pairs above: the canonical key is
emitted once with the typed value and any duplicate at the same key in `Resource.attributes` is
discarded, saturating-incrementing the emitted
`Resource.dropped_attributes_count`. This is required for spec conformance: OTLP
`Resource.attributes` mandates that "attribute keys MUST be unique." If the typed slot
is `None` and the attribute key is present, the attribute value is emitted unchanged (the
non-promotion rule above applies). The other typed slot/attribute-map pairs from the
parent RFC do not apply on OTLP egress: `TraceEvent.trace_id` supplies the single
16-byte wire `Span.trace_id` field (with no `_dd.p.tid` duplication), `Span.status`
egresses through OTLP's `Status.message` field with any `error.message`
attribute left in place as a regular attribute, and the chunk-state pair is the cross-format
synthesis covered under "Reserved OTLP bridge keys for Datadog-native state" below.

#### Deprecated `deployment.environment` handling

The OTLP source accepts both `deployment.environment.name` and the deprecated
`deployment.environment` as sources for `Resource.environment`. OpenTelemetry stabilized
the attribute as `deployment.environment.name` in semantic conventions
[v1.27.0](https://github.com/open-telemetry/semantic-conventions/releases/tag/v1.27.0)
([PR #3584](https://github.com/open-telemetry/semantic-conventions/pull/3584)), with
`deployment.environment` listed as "Replaced by `deployment.environment.name`."

The stable key wins when both are present; dropping the deprecated value
saturating-increments `Resource.dropped_attributes_count`. The selected
key is then subject to the promotion rule above: a non-empty string promotes, while an
empty or non-string value remains under its original key. On OTLP egress, a promoted
`Resource.environment` uses
`deployment.environment.name`; an unpromoted attribute retains its original key and
value.

The Rationale for accepting both keys is in the Rationale section.

#### Span timing

OTLP carries timing as two independent `fixed64` nanosecond values, `start_time_unix_nano`
and `end_time_unix_nano`. On ingress, `Span.duration` is computed as
`end_time_unix_nano − start_time_unix_nano`; on egress, `end_time_unix_nano` is
reconstructed as `start_time_unix_nano + duration.as_nanos()`. Both quantities are integer
nanoseconds in memory and on the wire; the round trip is bit-exact for any span where
`end_time_unix_nano >= start_time_unix_nano`.

A span with reversed timestamps (`end_time_unix_nano < start_time_unix_nano`) is clamped to zero
duration on ingress; this is one of the OTLP-side zero-loss exclusions listed above.

Egress clamping of `Span.start_time`, `SpanEvent.time`, and the reconstructed
`Span.end_time_unix_nano` follows the parent RFC's numeric boundary policy against the
OTLP timestamp domain.

#### `Span.flags` / `Link.flags` layout

OTLP defines `Span.flags` and `Link.flags` as `fixed32`, with bits 0-7 the W3C trace-flags
byte, bits 8-9 the parent-/link-target-remote tristate (`CONTEXT_HAS_IS_REMOTE`,
`CONTEXT_IS_REMOTE`), and bits 10-31 reserved. The full word is stored verbatim in the
parent RFC's `TraceFlags(u32)`, so all defined bits and any future spec additions
round-trip unchanged.

#### `Span.status.code` and `Span.status.message`

`Status.message` round-trips when `code = ERROR` (carried by `SpanStatus::Error(String)`) or when
`code` is an unrecognized future value (carried by `SpanStatus::Other(i32, String)`). For `code =
UNSET` or `OK` the message is dropped on ingest because the OpenTelemetry [Set
Status](https://opentelemetry.io/docs/specs/otel/trace/api/#set-status) rule restricts `Description`
to the `Error` code. A wire `Status.message` paired with `code = UNSET` or `OK` is non-conformant
and is dropped on ingest. See the Rationale subsection below for the
closed-enum-with-escape-hatch choice that makes future status codes round-trip unchanged.

#### Attribute encoding: `AttrValue` <-> `AnyValue`

The parent RFC's `AttrValue` mirrors OTLP `AnyValue` directly, so the mapping is 1:1
across the named variants. `AttrValue::Null` corresponds to an unset `AnyValue` oneof
(equivalently, for proto3, a `KeyValue` whose `value` field is absent), and the
mapping applies recursively into `Array` and `Map`. Conversion from VRL `Value` (for
example `Value::Timestamp` and `Value::Regex` written by transforms) happens at the
VRL-write boundary per the parent RFC's "VRL surface for `AttrValue`" rules; OTLP
egress never sees those variants in storage.

OTLP attribute fields and nested `KeyValueList` values are repeated key-value sequences,
while the typed model requires unique map keys. Ingest processes each sequence in wire
order before promotion or reserved-key lifting; when a key repeats, the last value wins.
Each overwritten earlier value saturating-increments the nearest enclosing resource,
scope, span, event, or link's `dropped_attributes_count`. OTLP egress is
unique-keyed by construction.

#### Default-valued / absent equivalence

OTLP's spec defines its "field absent" and "field default-valued" forms as semantically
equivalent for `ResourceSpans.resource`, `ScopeSpans.scope`, `Span.status`, and
`Span.kind`. The typed model relies on that equivalence rather than tracking wire
presence: it carries `Resource` and `Scope` as values rather than `Option`s, represents
an unset status as `SpanStatus::Unset`, and egress emits each field unconditionally at
its default. A round trip therefore preserves the spec-defined semantics even where the
wire bytes differ, and no exclusion is needed.

`Scope.{name, version}` and both `schema_url` fields
are non-optional proto3 strings, so decoding cannot distinguish absent from empty and they
map empty to `None`. Unknown `Span.status.code` or `Span.kind` values are carried by
the parent RFC's escape-hatch variants and egress as the same integer, so values
introduced by future OpenTelemetry versions round-trip unchanged.

#### Reserved OTLP bridge keys for Datadog-native state

The generic `AttrValue` -> `AnyValue` mapping does not carry the separate Datadog
contexts through OTLP. This mapping therefore reserves the `datadog.` attribute-key
prefix at the OTLP wire boundary: OTLP egress synthesizes the typed Datadog contexts
under reserved keys, and OTLP ingress lifts those keys back into the typed contexts
and strips them from the common attribute maps. The agent envelope and tracer tags are
event-scoped and use `Resource.attributes`; chunk state and Datadog-native span fields
use `Span.attributes`.

The contract is:

- The typed namespace is the single source of truth on egress. A common attribute at a
  reserved key is removed and replaced when the typed value requires emission,
  saturating-incrementing the corresponding emitted dropped count. Ingress lifting and
  stripping ensures OTLP egress through the same Vector emits each value once.
- A reserved key or nested member that is malformed -- wrong `AnyValue` variant,
  unrecognized member, or out-of-domain value -- is stripped without populating the
  typed slot and saturating-increments the enclosing item's
  `dropped_attributes_count`. Valid siblings continue to lift.
- Ordinary `_dd.*` resource and span attributes are not reserved, carry no
  Datadog-native meaning, and never acquire Datadog egress authority from their
  spelling.
- Recovery of Datadog state through these keys is best-effort and is explicitly outside
  the OTLP round-trip guarantee.

One consequence not derivable from the rules above:
`datadog.chunk.priority` is emitted whenever the typed priority is `Some`, including
`AutoReject` (wire `0`). Omitting `0` as a proto3 default would make an explicit reject
indistinguishable from a missing priority after an OTLP hop, and Datadog egress of a
missing priority is `AutoKeep`, so the round trip would invert the sampling decision.

The reserved key names, their per-member `AnyValue` types, the presence rules for
optional and default-valued members, and the resolution of conflicting
`datadog.chunk.*` values across spans in one trace-ID partition of a `ScopeSpans` are
implementation choices that satisfy this contract.

## Rationale

- The OTLP source accepts both `deployment.environment.name` and the deprecated
  `deployment.environment`. Accepting only the new key would silently drop the value for
  producers still on pre-stabilization conventions; accepting only the old key would
  silently drop it for producers on current conventions. Both matter because
  `Resource.environment` populates Datadog's `TracerPayload.env` on cross-format egress
  (see the Datadog sub-RFC): a route that fails to recognize the producer's chosen key
  emits an empty `TracerPayload.env` and loses environment attribution at the Datadog
  backend. The collision rule (current key wins) and egress emission (current key only)
  are documented under Implementation. A bit-exact relay alternative would require either
  adding provenance state to `Resource` or moving the typed slot to a derived view over
  `Resource.attributes`; both pay substantive cost for one transitional attribute.
- The OTLP `AnyValue.string_value` / `AnyValue.bytes_value` discriminator is preserved
  structurally because the parent RFC's `AttrValue` carries `String` and `Bytes` as
  distinct variants. OTLP egress is a 1:1 variant routing with no payload inspection,
  so the round trip is bit-exact for pure-relay pipelines.
- The `datadog.*` namespace at the OTLP wire boundary carries Datadog-native state that
  has no common attribute-map representation in the typed model. The explicit prefix
  declares authoring intent and limits collisions with user-set OTLP attributes to a
  small reserved set. Ordinary `_dd.*` resource and span attributes remain common
  attributes and never acquire Datadog egress authority merely from their spelling.
- Always-present logical Datadog contexts let VRL and typed consumers use stable paths
  without first testing the source format. Default contexts synthesize no OTLP bridge
  attributes, so purely OTLP-native traffic pays no wire-size cost.

## Drawbacks

- Partitioning a `ScopeSpans` duplicates its `Resource` and `Scope` in memory and in
  Vector's internal wire format once per distinct `trace_id`. OTLP egress re-coalesces
  compatible non-empty partitions, but the duplication remains through the topology and
  any disk buffer.
- Empty wire `ScopeSpans` values, including their resource and scope state, no longer
  produce an event because no required top-level trace ID can be derived.
- Best-effort recovery of Datadog state from reserved OTLP resource and span attributes
  is not guaranteed: a transform that drops or rewrites one of these attributes on
  OTLP-stage traffic loses the corresponding Datadog state. Operators should not use
  the reserved keys for unrelated custom attributes.

## Prior Art

- [OTLP traces protocol](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)
  -- the primary shape this RFC adopts. The container `TraceEvent` is structurally one
  trace-ID partition of a `ScopeSpans` plus its `Resource`, with explicit Datadog
  contexts projected through reserved OTLP bridge attributes when present.
- The OpenTelemetry [Collector OTLP receiver](https://github.com/open-telemetry/opentelemetry-collector/tree/main/receiver/otlpreceiver)
  is the reference implementation of the OTLP ingress semantics; Vector's OTLP source
  follows the same wire decoding.

## Alternatives

Wire-format-specific alternatives are in the parent RFC (`TraceFlags` width, `SpanStatus`
escape hatch). This sub-RFC inherits those choices.

## Outstanding Questions

- N/A.

## Plan Of Attack

The format-agnostic prerequisites (fallible proto decode boundary, temporary
`TraceEventCompat` enum, legacy-layout hint precursor, and internal `TypedTrace` proto
extension) are owned by the parent RFC's Plan of Attack and must land first. OTLP work
then proceeds through these obligations:

1. Implement `LegacyTraceEvent -> TraceEvent` conversion and unique detection of
   historical pre-hint OTLP layouts. The converter must fan out every recovered
   `ScopeSpans` by distinct `trace_id` in the stable first-seen order defined above.
2. Implement `TraceEvent -> OTLP` encoding satisfying the mapping and bridge-key
   contracts above.
3. Establish the `OTLP -> Vector -> OTLP` effective-equivalence guarantee and validate
   every declared exclusion.
4. Migrate the OTLP sink and then, after the parent RFC's compile-time consumer gate, the
   source. Typed input must pass end-to-end through OTLP export before the source emits
   typed events; typed source events retain the migration hint for the window specified
   by the parent RFC.
5. Publish the OTLP field mapping and bridge-key conventions in the user migration
   guide.

## Future Improvements

- Adopt typed support for OTLP fields as they reach `Stable` stability. The current scope
  excludes `Development` / `Alpha` -tier additions; when upstream stabilizes any of these,
  evaluate adding the corresponding typed slot and round-trip support, including a
  cross-format storage convention for fields with no Datadog wire analog.
- VRL helper for decoding raw OTLP `Span` protobuf into the typed surface
  (`decode_otlp_span`), parallel to the Datadog sub-RFC's `decode_datadog_span` helper.
