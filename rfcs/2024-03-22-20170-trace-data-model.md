# RFC 20170 - 2024-03-22 - Establish an Internal Trace Model

As of today there is an already existing Log and Metric event model, the purpose is to establish an
internal model for describing a trace event.

## Context

The [Ingest OpenTelemetry Traces RFC] was accepted, but there was a condition that an [internal trace model] MUST be
established. Meanwhile, there's also the [Accept Datadog Traces RFC] which was established, but
that primarily only works well with the [`datadog_traces` sink]. There was also a previous RFC which
was concerned with [validating schemas], but never came attempted to define an event schema.  This leaves this RFC
to establish that data model.

## Cross cutting concerns

- This is part of the effort to support [sending OpenTelemetry traces], it would also support
  ingesting OpenTelemetry. As well as to provide a path for a canonical model which traces can be
  translated to/from ideally without loss of fidelity.

## Scope

### In scope

- Define a Vector Trace event model schema.
- Support for Links between `TraceEvent`s.
- Defining a mapping between the Vector trace event model to the Datadog trace event (one direction).
- Defining a mapping between the Vector trace event model and the OpenTelemetry trace event
  (bi-directional).

### Out of scope

- Change the `TraceEvent` type internally from a re-typing of `LogEvent`.
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

For the initial Implementation, the data model will be [v1.1.0][otel-proto-110] of the OpenTelemetry Proto.

Therefore the only necessary mapping will be how the Proto data model maps to the `TraceEvent`.

#### Mapping OpenTelemetry Tracing to Vector Data Model

This section reflects similarly to how the current Log `source` for `opentelemetry` works.

As OpenTelemetry is the primary model the this will only describe the data types. The top level
`TraceEvent` will represent a top level proto `message ResourceSpans`.
`Value::Object`.  The remaining proto to mappings can be found:

| Proto Type | Vector Type |
|------------|-------------|
| `message AnyValue` | `Value` of the inner |
| `message ArrayValue` | `Value::Array` with elements in `values` |
| `message KeyValueList` | `Value::Array` with elements in `values` |
| Any unspecified `message` types | `Value::Object` |
| All `enum` types | `Value::Integer` |
| `string` | `Value::Bytes`  |
| `fixed64` for datetime values | `Value::Timestamp` |
| `fixed64`,`fixed32`,`uint32` for generic values  | `Value::Integer` |
| `bytes` | `Value::Bytes` |
| `repeated` messages | `Value::Array` of `Value::Object` |

The keys for `message` come the field names for `Value::Object`, and MUST follow a "Snake Case" format.
As an example, instead of `traceId` and `droppedEventCount`, use `trace_id` and `dropped_event_count`.

#### Mapping Vector Tracing to the Datadog Data Model

The basis of Mapping between the Internal Trace format and the Datadog format can be based on the
[Datadog agent][datadog-agent-otlp-ingest-09].

##### Span Mapping

Vector fields shown using VRL.

| Datadog Field | Vector Field | Comments |
|---------------|--------------|----------|
| Name | X | See "Span Name Mapping" |
| TraceID | `.["scope_spans"][i]["spans"][j]["trace_id"]` |  |
| SpanID | `.["scope_spans"][i]["spans"][j]["span_id"]` |  |
| ParentID | `.["scope_spans"][i]["spans"][j]["parent_span_id"]` |  |
| Start | `.["scope_spans"][i]["spans"][j]["trace_id"]` |  |
| Duration | `.["scope_spans"][i]["spans"][j]["trace_id"]` |  |
| Service | `.["resource"]["attributes"]["service.name"]` |  |
| Resource | `.["scope_spans"][i]["spans"][j]["name"]` |  |
| Meta | X | See "Attribute Handling"  |
| Metrics | `.["scope_spans"][i]["spans"][j]["attributes"][k]` | Metrics[k] is assigned, for Integer or Double type values. |

###### Attribute Handling

For Mapping from Vector to Datadog the scope of ResourceSpan attributes -> ScopeSpan attributes and
Span attributes are merged. The reverse it will not be expanded.

To begin `.["resource"]["attributes"][*]` are placed in the `Meta` map.

After for any `.["scope_spans"][i]["spans"][j]["attributes"][k]` Meta[k] for any non-Integer and
non-Double type values. Transcoding the values to Strings.

There are a number of Datadog specific transcodings, but it generally has to do with translating
semantic conventions to [Datadog conventions][datadog-agent-otlp-ingest-09].

###### Span Name Mapping

Follow [Datadog agent][datadog-agent-otlp-ingest-09] for OTLP -> Datadog. For Datadog -> OTLP Span
Name.

###### Span Kinds and Types

| Vector Span Kind | Datadog Span Type | Comments |
|------------------|-------------------|----------|
| Server | "web" |  |
| Client | "cache" | If the "db.system" exists in any of the Attribute scopes AND value matches
"redis" or "memcached" |
| Client | "db" | If the "db.system" exists in any of the Attribute scopes |
| Client | "http" | If the above conditions are not true. |
| * | "custom" |  |

#####

##### From Vector -> Datadog

As `TraceEvent` is a representation of a OTLP `ResourceSpans` message. In Datadog the ingestion
format expects an Map of Array of Spans. The map key is the `trace_id`.

##### From Datadog -> Vector

There's not a known advantage to providing this.

## Rationale

As of right now there is no alternatives to this, and providing this will allow for interoperability
between trace formats. Not doing this will prevent the ["must have" OpenTelemetry issue][github-top-level-otel-issue] from being
able to be completed.


## Drawbacks

- TBD

## Prior Art

- [W3C Trace Context](https://www.w3.org/TR/trace-context/) used for propagating data.
- [OpenTracing Model](https://github.com/opentracing/specification/blob/master/specification.md)
  predecessor to OpenTelemetry, and used as the basis for Jaeger's internal model.
- [Zipkin Data Model](https://zipkin.io/zipkin-api/#/default/get_spans)
- [OpenTelemetry Data Model](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto)

## Alternatives

### Using OpenTelemetry-Inspired Model

Rather than needing to respecify the definitions and re-create work done by the wider OpenTelemetry
community we can re-use the language and guidelines used in the OpenTelemetry product.

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
[otel-proto-110]:https://github.com/open-telemetry/opentelemetry-proto/releases/tag/v1.1.0
[datadog-agent-otlp-ingest-09]:https://github.com/DataDog/datadog-agent/blob/v0.9.0/pkg/trace/api/otlp.go#L307
[github-top-level-otel-issue]:https://github.com/vectordotdev/vector/issues/1444
