# RFC 20170 - 2024-03-22 - Establish an Internal Trace Model

As of today there is an already existing Log and Metric event model, the purpose is to establish an
internal model for describing a trace event.

## Context

The [Ingest OpenTelemetry Traces RFC] was accepted, but there was a condition that an [internal trace model] MUST be
established. Meanwhile, there's also the [Accept Datadog Traces RFC] which was established, but
that primarily only works well with the [`datadog_traces` sink]. There was also a previous RFC which
was concerned with [validating schemas], but never came attempted to define an event schema.  This leaves this RFC
to establish that data model.

[Ingest OpenTelemetry Traces RFC]:https://github.com/vectordotdev/vector/blob/master/rfcs/2022-03-15-11851-ingest-opentelemetry-traces.md
[internal trace model]:https://github.com/vectordotdev/vector/pull/11802#pullrequestreview-933957932
[Accept Datadog Traces RFC]:https://github.com/vectordotdev/vector/blob/master/rfcs/2021-10-15-9572-accept-datadog-traces.md
[`datadog_traces` sink]:https://vector.dev/docs/reference/configuration/sinks/datadog_traces/
[validating schemas]:https://github.com/vectordotdev/vector/pull/9388

## Cross cutting concerns

- This is part of the effort to support [sending OpenTelemetry traces], it would also support
  ingesting OpenTelemetry. As well as to provide a path for a canonical model which traces can be
  translated to/from ideally without loss of fidelity.

[sending OpenTelemetry traces]:https://github.com/vectordotdev/vector/issues/17308

## Scope

### In scope

- Define a Vector Trace event model schema.
- Support for Links between TraceEvents.
- Defining a mapping between the Vector trace event model and the Datadog trace event.
- Defining a mapping between the Vector trace event model and the OpenTelemetry trace event.

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

For the initial Implementation, the data model will be [v1.1.0] of the OpenTelemetry Proto.

Therefore the only necessary mapping will be how the Proto data model maps to the `TraceEvent`.

[v1.1.0]:https://github.com/open-telemetry/opentelemetry-proto/releases/tag/v1.1.0

#### Mapping OpenTelemetry Tracing to Vector Data Model

As OpenTelemetry is the primary model the this will only describe the data types. The top level
`TraceEvent` will represent a top level proto `message ResourceSpans`. This will be a
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

The keys for `message` come the field names for `Value::Object`, and MUST follow a "Lower Camel Case" format.
As an example: `traceId` and `droppedEventCount`, NOT `trace_id` and `dropped_event_count`.

## Rationale

As of right now there is no alternatives to this, and providing this will allow for interoperability
between trace formats. Not doing this will prevent the ["must have" OpenTelemetry issue] from being
able to be completed.

["must have" OpenTelemetry issue]:https://github.com/vectordotdev/vector/issues/1444

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
