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
  scope clauses, and User Experience as background.
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
- Effective-equivalence round-trip through a single Vector instance for
  `OTLP -> Vector -> OTLP` when the pipeline does not otherwise mutate the data: byte-for-byte
  identity is not required, but the output must be ingested by an OTLP backend as the same data
  as the original. Details the backend does not observe (e.g. attribute iteration order within
  `Span.attributes` when the producer-side ordering was non-canonical) may differ.
- The promotion rule for the three semantic-convention attributes (`service.name`,
  `deployment.environment.name`, `host.name`) into typed `Resource` slots, including the
  legacy-key acceptance for `deployment.environment`.
- The 1:1 mapping between `AttrValue` variants and OTLP's `string_value` / `bytes_value` /
  `int_value` / `bool_value` / `double_value` / `array_value` / `kvlist_value` (with unset
  oneof representing `AttrValue::Null`).
- Synthesis on OTLP egress (and lift on OTLP ingress) of typed-only Datadog state into
  reserved span-attribute keys (`datadog.chunk.priority`, `datadog.chunk.origin`,
  `datadog.chunk.dropped`, `datadog.chunk.tags`, `datadog.span.resource`,
  `datadog.span.type`) for best-effort cross-format relay; the keys are defined here and
  their contents are specified by the Datadog sub-RFC.
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

- **Deprecated `deployment.environment` key** is rewritten to `deployment.environment.name`
  on OTLP egress. If both keys are present on ingress with different values, the deprecated
  value is dropped (the stable key wins). See the deprecated-environment paragraph under
  Implementation.
- **Reserved cross-format OTLP span-attribute keys** (`datadog.chunk.priority`,
  `datadog.chunk.origin`, `datadog.chunk.dropped`, `datadog.chunk.tags`,
  `datadog.span.resource`, `datadog.span.type`): lifted from `Span.attributes` into
  typed `TraceEvent.chunk` / `Span.resource_name` / `Span.span_type` slots on OTLP
  ingress and stripped; synthesized into `Span.attributes` from the typed slots on OTLP
  egress, overwriting any pre-existing attribute at the same key. The Datadog sub-RFC
  specifies the contents these keys carry; this sub-RFC reserves the keys.
- **OTLP fields at `Development` or `Alpha` stability tier** are dropped on OTLP ingress.
- **Empty `string_value` on a promoted resource attribute** (`service.name`,
  `deployment.environment.name`, the deprecated `deployment.environment`, or
  `host.name`): consumed from `Resource.attributes` on ingress and normalised to typed
  slot `None` per the parent RFC's "Empty-string invariant for `Option<KeyString>`
  slots". Egress emits the typed `None` as field-absent rather than as the original
  empty-string attribute.
- **`Span.end_time_unix_nano < start_time_unix_nano`** is clamped to zero duration on
  ingress; the source increments a counter and emits a warning log
  identifying the affected span. The egress reconstruction emits `end_time_unix_nano = start_time_unix_nano`,
  byte-different from the original input.

The OTLP-side consequences of model-level exclusions defined in the parent RFC (zero-ID
rejection and pre-epoch timestamps via the internal proto's `fixed64` encoding) also
apply. The OTLP-specific duration-overflow clamp is documented under "Span timing"
below.

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

Each OTLP `ScopeSpans` is one `TraceEvent`. The containing `ResourceSpans.resource`
populates `TraceEvent.resource`; the `ScopeSpans.scope` populates `TraceEvent.scope`; the
spans inside populate `TraceEvent.spans`; `TraceEvent.chunk` is default-empty. A
`ScopeSpans` with an empty `spans` repeated field produces a `TraceEvent` with
`spans = []`, forwarded per the parent RFC's empty-spans guideline; on OTLP egress an
empty-spans `TraceEvent` becomes one `ScopeSpans { spans: [] }`.

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
| `Span.trace_id`, `Span.span_id`, `Span.parent_span_id`             | same (zero-ID handling: see below)            |
| `Span.trace_state`                                                 | `Span.trace_state` (verbatim)                 |
| `Span.flags`, `Link.flags` (see flags layout)                      | `Span.flags`, `SpanLink.flags` (full u32)     |
| `Span.name`, `Span.kind`                                           | `Span.name`, `Span.kind`                      |
| `Span.start_time_unix_nano`, `end_time_unix_nano` (see timing)     | `Span.start_time`, `Span.duration` (ns-exact) |
| `Span.attributes`                                                  | `Span.attributes`                             |
| `Span.events`, `Span.links`                                        | `Span.events`, `Span.links`                   |
| `Span.status.{code,message}` (see status)                          | `Span.status.{code,message}`                  |
| `Span.dropped_*_count`                                             | `Span.dropped_*_count`                        |

On OTLP egress, `TraceEvent`s sharing a `Resource` (including `Resource.schema_url`) are
gathered into one `ResourceSpans`; each event becomes one `ScopeSpans`. Two events with
identical `Resource` content but different `schema_url` values produce separate
`ResourceSpans` messages; the grouping key includes `schema_url`. The `_dd.*` reserved
keys in `Resource.attributes` (`_dd.payload`, `_dd.tracer`) and `Span.attributes`
(`_dd.meta_struct`) egress through the generic `AttrValue` -> `AnyValue` mapping under
their model-level keys. Typed-only Datadog state (`TraceEvent.chunk.*`,
`Span.resource_name`, `Span.span_type`) has no attribute-map representation; the OTLP
sink synthesizes it into `Span.attributes` under reserved keys (see "Reserved OTLP-egress
keys" below) for best-effort cross-format relay.

#### Zero-ID detection

OTLP wire IDs are raw byte arrays; an all-zero `Span.trace_id`, `Span.span_id`,
`Link.trace_id`, or `Link.span_id` is invalid per the OTLP specification and triggers the
parent RFC's drop rule (per-span or per-link, with the corresponding
`invalid_trace_id` / `invalid_span_id` `component_errors_total` increment). An absent
`Span.parent_span_id` deserializes to an empty byte sequence, which the ingest treats
identically to all-zero and maps to `Span.parent_span_id = None`; this is not a zero-ID
failure. On egress, a `None` parent emits an empty `parent_span_id`.

#### Semantic-convention promotion and the typed-slot precedence

Three resource attribute keys promote to typed `Resource` slots on ingress: `service.name`,
`deployment.environment.name` (with `deployment.environment` accepted as a legacy
fallback -- see below), and `host.name`.

Promotion to a typed `Resource` field is conditional on the attribute value being a
`string_value`. When the value is a non-empty string (the normal case), promotion is
move-not-copy: the key is removed from `Resource.attributes` and the typed slot is the
sole post-ingress owner of the value. This matches the move-not-copy pattern used for
`_dd.p.tid` consumption on Datadog ingest and for the reserved cross-format keys. A
`string_value` whose contents are empty is consumed identically -- the key is stripped
from `Resource.attributes` -- but the typed slot stays `None` per the parent RFC's
"Empty-string invariant for `Option<KeyString>` slots"; this is the OTLP-side
application of the invariant and is one of the OTLP-side zero-loss exclusions listed
above. If the `AnyValue` for any of these three keys is a non-string variant (e.g.
`int_value`, `bytes_value`, `bool_value`, `array_value`, `kvlist_value`, or an unset
oneof), the key is not promoted and remains in `Resource.attributes` under its original
key, so OTLP egress emits it unchanged. A non-string `service.name`,
`deployment.environment.name`, or `host.name` therefore round-trips exactly; the typed
`Resource` slot is left empty (`None`). Producers that violate the semantic-convention
string typing for these keys are uncommon but produce valid OTLP wire data, and this
rule ensures they are not silently truncated.

VRL transforms that want to change the service, environment, or host should write to the
typed slots (`.resource.service`, `.resource.environment`, `.resource.host`) rather than
to the corresponding attribute-map keys. Because promotion strips the source attribute on
ingress, the duplicate-key case arises only when (i) a transform writes to both the typed
slot and the matching attribute key, or (ii) the source attribute was a non-string
variant that the promotion rule above left in place.

On OTLP egress specifically, the typed slot wins for the three pairs above: the canonical key is
emitted once with the typed value and any duplicate at the same key in `Resource.attributes` is
dropped; a counter is incremented and a warning log is emitted for visibility. This is required for
spec conformance: OTLP `Resource.attributes` mandates that "attribute keys MUST be unique." If the
typed slot is `None` and the attribute key is present, the attribute value is emitted unchanged (the
non-string-promotion rule above applies). The other typed slot/attribute-map pairs from the parent
RFC do not apply on OTLP egress: `Span.trace_id` is a single 16-byte wire field (no `_dd.p.tid`
duplication), `Span.status` egresses through OTLP's `Status.message` field with any `error.message`
attribute left in place as a regular attribute, and the chunk-state pair is the cross-format
synthesis covered under "Reserved OTLP-egress keys for typed-only Datadog state" below.

#### Deprecated `deployment.environment` handling

The OTLP source accepts both `deployment.environment.name` and the deprecated
`deployment.environment` as sources for `Resource.environment`. OpenTelemetry stabilized
the attribute as `deployment.environment.name` in semantic conventions
[v1.27.0](https://github.com/open-telemetry/semantic-conventions/releases/tag/v1.27.0)
([PR #3584](https://github.com/open-telemetry/semantic-conventions/pull/3584)), with
`deployment.environment` listed as "Replaced by `deployment.environment.name`."

The OTLP source promotes whichever of the two keys is present; if both are present,
`deployment.environment.name` wins and the duplicate value at `deployment.environment` is
dropped. On OTLP egress, `Resource.environment` is emitted only as
`deployment.environment.name`. The relay-path consequences (key rewrite from deprecated to
stable, and divergent-value drop when both keys are present) are the two declared
OTLP-side partial-exclusion cases above.

The Rationale for accepting both keys is in the Rationale section.

#### Span timing

OTLP carries timing as two independent `fixed64` nanosecond values, `start_time_unix_nano`
and `end_time_unix_nano`. On ingress, `Span.duration` is computed as
`end_time_unix_nano − start_time_unix_nano`; on egress, `end_time_unix_nano` is
reconstructed as `start_time_unix_nano + duration.as_nanos()`. Both quantities are integer
nanoseconds in memory and on the wire; the round trip is bit-exact for any span where
`end_time_unix_nano >= start_time_unix_nano`.

A span with reversed timestamps (`end_time_unix_nano < start_time_unix_nano`) is clamped to zero
duration on ingress, a counter is incremented, and a warning log is emitted; this is one of the
OTLP-side zero-loss exclusions listed above.

A `Duration` exceeding `u64::MAX` nanoseconds (~584 years) is clamped to `u64::MAX` on
encode through the OTLP `fixed64` `start_time_unix_nano` and `end_time_unix_nano` wire
fields per the parent RFC's guideline; no additional OTLP-specific behaviour.

A pre-epoch `DateTime<Utc>` value in `Span.start_time` or `SpanEvent.time` (writable via
the Rust API or VRL but never produced by OTLP ingress, since `fixed64` is unsigned) is
clamped to epoch-zero on OTLP encode; a counter is incremented and a warning log
identifies the affected span. This mirrors the parent RFC's internal-proto clamp.

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
and is dropped on ingest; a counter is incremented and a warning log is emitted for visibility. See
the Rationale subsection below for the closed-enum-with-escape-hatch choice that makes future status
codes round-trip unchanged.

#### Attribute encoding: `AttrValue` <-> `AnyValue`

The parent RFC's `AttrValue` mirrors OTLP `AnyValue` directly, so the mapping is 1:1
across the named variants. `AttrValue::Null` corresponds to an unset `AnyValue` oneof
(equivalently, for proto3, a `KeyValue` whose `value` field is absent), and the
mapping applies recursively into `Array` and `Map`. Conversion from VRL `Value` (for
example `Value::Timestamp` and `Value::Regex` written by transforms) happens at the
VRL-write boundary per the parent RFC's "VRL surface for `AttrValue`" rules; OTLP
egress never sees those variants in storage.

#### Default-valued / absent equivalence

OTLP defines several "field absent" / "field default-valued" pairs as semantically
equivalent at the spec level, in which case the model represents both forms as the default
value and the round-trip preserves spec-defined semantic equivalence even when the wire
bytes differ:

- `ResourceSpans.resource` (proto comment: "If this field is not set then no resource info
  is known") and `ScopeSpans.scope` (proto comment: "Semantically when InstrumentationScope
  isn't set, it is equivalent with an empty instrumentation scope name (unknown)") -- absent
  on the wire is spec-equivalent to a default-valued message. The model carries
  `TraceEvent.resource` and `TraceEvent.scope` as values rather than `Option`, and egress
  emits the field unconditionally. Within `Scope`, `name` and `version` are
  `Option<KeyString>`: an absent or empty-string wire value normalises to `None` on
  ingress (OTLP treats empty and absent as equivalent); on egress `None` is emitted as an
  absent (zero-length proto3 string), which is spec-equivalent to the original.
- `Span.status` (proto comment: "Semantically when Status isn't set, it means span's
  status code is unset, i.e. assume STATUS_CODE_UNSET (code = 0)") -- the model represents
  this as `SpanStatus::Unset` and egress emits the corresponding zero-coded `Status`. A
  status code outside the three known values (`UNSET = 0`, `OK = 1`, `ERROR = 2`) ingests
  as `SpanStatus::Other(code, message)` and egresses as the same code and message, so
  unknown status codes introduced by future OpenTelemetry versions round-trip unchanged.
- `Span.kind` -- `SPAN_KIND_UNSPECIFIED = 0` is the proto3 default; absent and zero-valued
  are byte-identical anyway. A value outside the six known enum numbers ingests as
  `SpanKind::Other(n)` and egresses as the same integer, so unknown kind values introduced
  by future OpenTelemetry versions round-trip unchanged.

#### Reserved OTLP-egress keys for typed-only Datadog state

`TraceEvent.chunk.*`, `Span.resource_name`, and `Span.span_type` are typed-only fields
with no attribute-map representation, so the generic `AttrValue` -> `AnyValue` mapping
does not carry them through OTLP. The OTLP sink synthesizes them into `Span.attributes`
under the following reserved keys, and OTLP ingress lifts the same keys back into the
typed slots and strips them from `Span.attributes`. The Datadog sub-RFC specifies the
contents these keys carry; this sub-RFC reserves the keys at OTLP-wire-level:

- `Span.attributes."datadog.chunk.priority"` -- carries `TraceEvent.chunk.priority` as
  `int_value` (the integer-form `SamplingPriority` wire value).
- `Span.attributes."datadog.chunk.origin"` -- carries `TraceEvent.chunk.origin` as
  `string_value`.
- `Span.attributes."datadog.chunk.dropped"` -- carries `TraceEvent.chunk.dropped` as
  `bool_value`; omitted when `false`.
- `Span.attributes."datadog.chunk.tags"` -- carries `TraceEvent.chunk.tags` as a
  `kvlist_value`; omitted when empty.
- `Span.attributes."datadog.span.resource"` -- carries `Span.resource_name` as
  `string_value`.
- `Span.attributes."datadog.span.type"` -- carries `Span.span_type` as `string_value`.

Each reserved key is omitted when the typed source is `None` or default-valued, so an
OTLP-sourced span with default chunk state carries no `datadog.chunk.*` attributes. On
OTLP ingress, an absent reserved key restores the typed slot to its default
(`None` / `false` / `{}`), so the egress-omit-on-default pairs with an
ingress-default-on-absent rule and the round trip is preserved.

Reserved-key semantics: any pre-existing attribute at one of the reserved keys above on
OTLP egress is overwritten by the synthesized value (the typed slot is the single source
of truth). On OTLP ingress, the keys are lifted into their typed slots and stripped from
`Span.attributes` so OTLP egress through the same Vector emits them once under the typed
path, not twice. Cross-format recovery via these keys is best-effort and is explicitly
outside the OTLP round-trip guarantee (see "Out of scope" above).

For per-span chunk-context recovery on OTLP ingress: spans within the same `ScopeSpans` typically
share chunk-context values (the wire shape mirrors the original `TraceChunk` on the Datadog side of
the relay), but in the degenerate case where spans within a `ScopeSpans` carry conflicting values
for the same `datadog.chunk.*` key, the wire-order- first value wins, a counter is incremented, and
a warning log is emitted; if the keys are absent on all spans, `ChunkContext` is default-empty.

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
- The `datadog.*` namespace at the OTLP wire boundary carries typed-only Datadog state
  that has no attribute-map representation in the typed model (chunk-scoped state and the
  Datadog-native `Span` slots). The namespace prefix limits the collision class with
  user-set OTLP attributes to a small, declared set rather than relying on bare key names
  that could plausibly appear in unrelated user data. Items already in the typed attribute
  maps (`_dd.payload`, `_dd.tracer`, `_dd.meta_struct`) egress through the generic
  `AttrValue` -> `AnyValue` mapping under their model-level keys and need no separate
  wire-level reservation.

## Drawbacks

- Best-effort recovery of Datadog state from reserved OTLP span-attribute keys
  (`datadog.chunk.*`, `datadog.span.resource`, `datadog.span.type`) on cross-format relay
  is not guaranteed: a transform that drops or rewrites one of these attributes on
  OTLP-stage traffic silently loses the corresponding Datadog state. Operators are
  advised not to set these reserved keys outside cross-format pipelines.
- Additional parent-RFC-level drawbacks (VRL-config breakage on typed-path migration,
  per-span operations requiring `.spans` iteration, etc.) apply to OTLP-sourced and
  OTLP-bound events as well.

## Prior Art

- [OTLP traces protocol](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)
  -- the primary shape this RFC adopts. The container `TraceEvent` is structurally one
  `ScopeSpans` plus its `Resource` and the Datadog-only `ChunkContext`.
- The OpenTelemetry [Collector OTLP receiver](https://github.com/open-telemetry/opentelemetry-collector/tree/main/receiver/otlpreceiver)
  is the reference implementation of the OTLP ingress semantics; Vector's OTLP source
  follows the same wire decoding.

## Alternatives

### `Span.flags` as `u8`

OTLP defines `Span.flags` as `fixed32` and Vector stores the full word in `TraceFlags(u32)`.
A narrower `u8` storage matching only the W3C trace-flags byte would discard OTLP's bits
8-9 (the parent-remote tristate) and bits 10-31 (reserved for future spec additions). This
would round-trip incorrectly for any span carrying the OTLP tristate or any future flag the
spec defines. The wider `u32` storage is the parent RFC's choice; this sub-RFC inherits it.

### Status as a closed enum without escape hatch

Defining `SpanStatus` without an escape hatch would silently coerce any unrecognized status
code introduced by a future OpenTelemetry version to `Unset` (the proto3 default),
breaking the `OTLP -> Vector -> OTLP` relay guarantee for those spans. The
`Other(i32, String)` variant stores the raw code and message verbatim and egresses them
unchanged, preserving relay fidelity by the same mechanism the parent RFC uses for
`SpanKind`. Rationale and VRL surface are in the parent RFC; the OTLP-side consequence is
that the relay guarantee holds for spec-future codes.

## Outstanding Questions

- N/A.

## Plan Of Attack

The OTLP-mapping PRs sequence as follows. The format-agnostic prerequisites (fallible
proto decode boundary, migration enum, legacy-layout hint precursor, internal `TypedTrace`
proto extension) are owned by the parent RFC's Plan of Attack and must land first.

- [ ] OTLP `Legacy -> Typed` shim in `lib/opentelemetry-proto`. Registers under the OTLP
  legacy-layout hint emitted by the precursor step in the parent. Consumed by any OTLP-aware
  downstream component once the shim ships.
- [ ] `Typed -> OTLP-wire` encoder in `lib/opentelemetry-proto`. Implements the egress
  mapping table, the typed-slot-precedence rule, the `AttrValue` -> `AnyValue` mapping,
  the reserved-key emission for Datadog state, and the empty-`ScopeSpans` rule.
- [ ] Property-based round-trip unit tests for `OTLP -> Vector -> OTLP` asserting
  effective equivalence per Scope: typed-slot promotion (both string and non-string
  cases), `deployment.environment` legacy-key handling, reversed-timestamp clamping,
  oversized duration clamping, default-valued / absent equivalence for `Resource` /
  `Scope` / `Status`, and unknown enum-number forward compatibility for `SpanKind` and
  `SpanStatus`.
- [ ] Migrate the `opentelemetry` sink to consume `Typed` natively; wire the
  `lib/opentelemetry-proto` encoder into the sink's trace path and cover the end-to-end
  HTTP export flow with typed-input tests.
- [ ] Migrate the `opentelemetry` source to produce `Typed` natively. Lands after the
  parent's "remove untyped forwarding methods" step so the build catches any unmigrated
  consumer.
- [ ] Document the OTLP mapping in the trace migration guide section the parent RFC's
  POA owns: which OTLP wire fields land in which typed slots, how to write VRL against
  the typed surface, and the reserved-key conventions for cross-format relay.

## Future Improvements

- Adopt typed support for OTLP fields as they reach `Stable` stability. The current scope
  excludes `Development` / `Alpha` -tier additions; when upstream stabilizes any of these,
  evaluate adding the corresponding typed slot and round-trip support, including a
  cross-format storage convention for fields with no Datadog wire analog.
- VRL helper for decoding raw OTLP `Span` protobuf into the typed surface
  (`decode_otlp_span`), parallel to the Datadog sub-RFC's `decode_datadog_span` helper.
