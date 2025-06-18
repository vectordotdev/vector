# RFC 20170 - 2024-03-22 - Establish an Internal Trace Model

As of today there is an already existing Log and Metric event model, I propose to establish a
canonical internal model for describing a trace event in Vector.

## Context

The [Ingest OpenTelemetry Traces RFC] was accepted, but there was a condition that an [internal trace model] MUST be
established. Meanwhile, there's also the [Accept Datadog Traces RFC] which was established, but
that primarily only works well with the [`datadog_traces` sink]. There was also a previous RFC which
was concerned with [validating schemas], but never came attempted to define an event schema.  This leaves this RFC
to establish that data model.

## Cross cutting concerns

This is part of the effort to support [sending and receiving Opentelemetry signals][otel-signals-pr]

## Scope

### In scope

- Define a Vector Trace event model schema.
- Support for Links between `TraceEvent`s.
- Defining a mapping between the Vector trace event model to the Datadog trace event.
- Defining a mapping between the Vector trace event model and the OpenTelemetry trace event.

### Out of scope

- Define the settings and configuration for sinks and sources using the data model.

## Pain

- Without an internal trace model it makes it more difficult to manage adding `sinks`, `sources`,
  and `transforms` to Vector allowing for translations functions between the various components.

## Proposal

### User Experience

- As a Vector user I would like to be able to ingest and propagate traces using various protocols.
  This will allow for the greatest flexibility when managing and migrating services of developed by
  a set of different sources, including closed and open source software.

### Implementation

For the initial Implementation, the data model will be [v1.5.0][otel-proto-150] of the OpenTelemetry Proto.
Specifically, the `opentelemetry/proto/trace/v1/trace.proto`

The below data models are meant to represent close to the final internal structures, but there may
be some variations to improve performance made in the final implementation; `Inner`, `Arc`, etc.

#### TraceEvent

```rust
pub struct TraceEvent {
  pub resource: Resource,
  pub scope_spans: Vec<ScopeSpan>,
  pub schema_url: Option<String>,
}
```

#### ScopeSpan

Leaving the scope optional since not everything is guaranteed to implement this.

```rust
pub struct ScopeSpans {
  pub scope: Option<InstrumentationScope>,
  pub spans: Vec<Span>,
  pub schema_url: Option<String>,
}
```

#### Resource

```rust
pub struct Resource {
  pub attributes: ObjectMap,
  pub dropped_attributes_count: u32,
}
```

#### InstrumentationScope

```rust
pub struct InstrumentationScope {
  pub name: String,
  pub version: String,
  pub attributes: ObjectMap,
  pub dropped_attributes_count: u32,
}
```

#### Span

```rust
pub struct Span {
  pub name: String,
  pub is_remote: bool,
  pub trace_id: TraceId(u128),
  pub span_id: SpanId(u64),
  pub trace_state: TraceState,
  pub span_kind: SpanKind,
  pub flags: TraceFlags(u8),
  pub start_time: u64,
  pub end_time: u64,
  pub dropped_attributes_count: u32,
  pub dropped_events_count: u32,
  pub dropped_links_count: u32,
  pub attributes: ObjectMap,
  pub events: Vec<SpanEvent>,
  pub links: Vec<TraceLink>,
  pub parent_span_id: Option<SpanId>,
  pub status: Option<SpanStatus>,
}
```

#### SpanStatus

The message MUST be ignored for `SpanStatusCode::Unset` and `SpanStatusCode::Ok`, since
it is intended to be used as an error message.

```rust
pub struct SpanStatus {
  pub code: SpanStatusCode,
  pub message: Option<String>,
}
```

#### SpanStatusCode

Based on the [OTel semantics][otel-span-status-semantics]

```rust
pub enum SpanStatusCode {
  Unset,
  Ok,
  Error,
}
```

#### SpanKind

```rust
pub enum SpanKind{
  Unspecified,
  Internal,
  Server,
  Client,
  Producer,
  Consumer,
}
```

#### SpanEvent

```rust
pub struct SpanEvent {
  pub name: String,
  pub time: u64,
  pub attributes: ObjectMap,
  pub dropped_attributes_count: u32,
}
```

#### TraceLink

```rust
pub struct TraceLink {
  pub trace_id: TraceId(u128),
  pub span_id: SpanId(u64),
  pub trace_state: TraceState,
  pub dropped_attributes_count: u32,
  pub attributes: ObjectMap,
  pub flags: TraceFlags(u8),
}
```

#### TraceFlags

Follows the [Trace Flags W3C Spec][w3c-trace-context-trace-flags].

#### TraceState

Follows the [TraceState Header W3C Spec][w3c-trace-context-tracestate-header]

```rust
pub struct TraceState(Option<VecDeque<(String,String)>>)
```

#### Mapping OpenTelemetry Tracing to Vector Data Model

This section reflects similarly to how the current Log `source` for `opentelemetry` works.

As `TraceEvent` is a representation of an OTLP `ResourceSpans` message. In Datadog the ingestion
format expects an Map of Array of Spans. The map key is the `trace_id`.

As OpenTelemetry is the primary model the this will only describe the data types. The top level
`TraceEvent` will represent a top level proto `message ResourceSpans`.
`Value::Object`.  The remaining proto to mappings can be found:

| Proto Type                      | Vector Type                              |
|---------------------------------|------------------------------------------|
| `message AnyValue`              | `Value` of the inner                     |
| `message ArrayValue`            | `Value::Array` with elements in `values` |
| `message KeyValueList`          | `Value::Array` with elements in `values` |
| Any unspecified `message` types | `Value::Object`                          |
| All `enum` types                | `Value::Integer`                         |
| `string`                        | `Value::Bytes`                           |
| `fixed64` for datetime values   | `Value::Timestamp`                       |
| `fixed64`,`fixed32`,`uint32` \  | `Value::Integer`                         |
| for generic values              |                                          |
| `bytes`                         | `Value::Bytes`                           |
| `repeated` messages             | `Value::Array` of `Value::Object`        |

The keys for `message` come the field names for `Value::Object`, and MUST follow a "snake case" format.
As an example, instead of `traceId` and `droppedEventCount`, use `trace_id` and `dropped_event_count`.

#### Mapping Vector Tracing to the Datadog Data Model

The basis of Mapping between the Internal Trace format and the Datadog format can be based on the
[Datadog agent][datadog-agent-otlp-ingest-7601].

##### Span Mapping

Vector fields are shown using VRL path notation.

| Datadog Field | Vector Field | Comments |
|---------------|---------------------------------------------|--------------------------------|
| Name          | See [Span Name Mapping][#span-name-mapping] |                                |
| TraceID       | `.scope_spans[i].spans[j].trace_id`         |                                |
| SpanID        | `.scope_spans[i].spans[j].span_id`          |                                |
| ParentID      | `.scope_spans[i].spans[j].parent_span_id`   |                                |
| Start         | `.scope_spans[i].spans[j].start_time`       |                                |
| Duration      | `.scope_spans[i].spans[j].duration`         |                                |
| Service       | `.resource.attributes["service.name"]`      |                                |
| Resource      | `.scope_spans[i].spans[j].name`             |                                |
| Meta          | X                                           | See "Attribute Handling"       |
| Metrics       | `.scope_spans[i].spans[j].attributes[k]`    | Metrics[k] is assigned, for \  |
|               |                                             | Integer or Double type values. |

###### Attribute Handling

For Mapping from Vector to Datadog the scope of ResourceSpan attributes -> ScopeSpan attributes and
Span attributes are merged. The reverse it will not be expanded.

To begin `.resource["attributes"][*]` are placed in the `Meta` map.

After for any `.scope_spans[i].spans"][j]["attributes"][k]` Meta[k] for any non-Integer and
non-Double type values. Transcoding the values to Strings.

There are a number of Datadog specific transcodings, but it generally has to do with translating
semantic conventions to [Datadog conventions][datadog-agent-otlp-ingest-7601].

###### Span Name Mapping

Follow [Datadog agent][datadog-agent-otlp-ingest-7601] for OTLP -> Datadog. For Datadog -> OTLP Span
Name.

###### Span Kinds and Types

| Vector Span Kind | Datadog Span Type | Comments |
|------------------|-------------------|----------|
| Server           | "web"             |          |
| Client           | "cache"           | If the "db.system" exists in any of the Attribute scopes AND value matches "redis" or "memcached" |
| Client           | "db"              | If the "db.system" exists in any of the Attribute scopes |
| Client           | "http"            | If the above conditions are not true. |
| *                | "custom"          |          |

## Rationale

As of right now there is no alternatives to this, and providing this will allow for interoperability
between trace formats. Not doing this will prevent the ["must have" OpenTelemetry issue][github-top-level-otel-issue] from being
able to be completed.

## Drawbacks

None found so far.

## Prior Art

- [W3C Trace Context](https://www.w3.org/TR/trace-context/) used for propagating data.
- [OpenTracing Model](https://github.com/opentracing/specification/blob/master/specification.md)
  predecessor to OpenTelemetry, and used as the basis for Jaeger's internal model.
- [Zipkin Data Model](https://zipkin.io/zipkin-api/#/default/get_spans)
- [OpenTelemetry Data Model](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)

## Alternatives

### Continue with existing log-based model

This requires users to navigate an imprecise model for traces which essentially works as a btree
which can be defined be every input and output. Transforms could be provided, but this would still
require a standard model so that 1-1 transforms would not need to be provided for each input/output.

Furthermore, it was already determined at an earlier date that an [internal trace model] was
required.

### Using OpenTelemetry-inspired model on the existing log model

A logical model built on top of the existing log model would allow for a quick implementation, but
lacks some of the benefits of having a more regular typing. It still has advantage of rather than
needing to respecify the definitions and re-create work done by the wider OpenTelemetry
community we can re-use the language and guidelines used in the OpenTelemetry product. However, it
lacks opportunities for optimization of data processing. Additionally, refactoring to mappings
in the model in the future is more difficult as with will require reliance on convention.

## Outstanding Questions

- What Drawbacks exist?

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Document the Traces data model for reference in the "under the hood" section.
- [ ] Document the mapping for Datadog and Internal Trace Model.
- [ ] Document the mapping for OpenTelemetry and Internal Trace Model.
- [ ] Implement VRL function for OpenTelemetry to Internal model and the reverse. (Necessary?)
- [ ] Implement VRL function for Datadog to Internal model and the reverse.

## Future Improvements

- Define the settings and configuration for sinks and sources using the data model. OpenTelemetry
  `source` and `sink` as an example.
- Determine whether sources and sinks should accept the standard trace model or if they should rely
  on transforms. (Case for relying on `transforms` is that in the case of acting as a relay and
  impart less overhead on the overall system)

[Ingest OpenTelemetry Traces RFC]:https://github.com/vectordotdev/vector/blob/master/rfcs/2022-03-15-11851-ingest-opentelemetry-traces.md
[internal trace model]:https://github.com/vectordotdev/vector/pull/11802#pullrequestreview-933957932
[Accept Datadog Traces RFC]:https://github.com/vectordotdev/vector/blob/master/rfcs/2021-10-15-9572-accept-datadog-traces.md
[`datadog_traces` sink]:https://vector.dev/docs/reference/configuration/sinks/datadog_traces/
[validating schemas]:https://github.com/vectordotdev/vector/pull/9388
[sending OpenTelemetry traces]:https://github.com/vectordotdev/vector/issues/17308
[otel-proto-150]:https://github.com/open-telemetry/opentelemetry-proto/releases/tag/v1.5.0
[datadog-agent-otlp-ingest-7601]:https://github.com/DataDog/datadog-agent/blob/7.60.1/pkg/trace/api/otlp.go#L489
[github-top-level-otel-issue]:https://github.com/vectordotdev/vector/issues/1444
[w3c-trace-context-tracestate-header]:https://www.w3.org/TR/trace-context/#tracestate-header
[w3c-trace-context-trace-flags]: https://www.w3.org/TR/trace-context/#trace-flags
[otel-span-status-semantics]:https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/trace/api.md#set-status
[otel-signals-pr]:https://github.com/vectordotdev/vector/issues/1444

