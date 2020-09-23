# RFC #4063 - Tracing: building blocks.

This RFC covers the introduction of traces to Vector using the [OpenTelemetry specification](https://github.com/open-telemetry/opentelemetry-specification) as a base for that introduction.

## Motivation

Vector intends to be the best tool for working with logs, metrics, and traces. Logs and metrics are first-class citizens in Vector already and now we need to add traces to compliment them.

## Background

A distributed trace is a set of events, linked by a single logical operation, for example clicking the `Buy` button on a website. This operation would trigger a trace. The trace would combine all of the events represented in the operation, for example in this case it might be: button click, request to backend service, credit card auth to payments service, update to inventory, generation of shipping label, return of CC auth, return of result to end user site. 

These events cross process, network, and service boundaries. The start and end of each event is recorded and potentially metadata or other information is attached to it.  Then, when combined as a trace, this allows an engineer to track the operation across boundaries and identify issues, performance bottlenecks or latency, and even potential security exposures by examining the end-to-end flow.

Inside a trace, the primary component is called [a `span`](https://opentracing.io/docs/overview/spans/). A span represents an individual unit of work done in a distributed system. Spans have a start and an end timestamp, they measure the latency of a particular operation. A trace contains a single `root` span, which represents the latency of the entire request operation, and one or more `child` spans, which represent operations taking place as part of the request. 

The spans in a trace are linked together via a reference, for example a trace ID. OpenTelemetry defines a trace like so:

> Traces in OpenTracing are defined implicitly by their Spans. In particular, a Trace can be thought of as a directed acyclic graph (DAG) of Spans, where the edges between Spans are called References.

The reference, or unique ID is contained in a field called the "span context". The span context is immutable and can't be changed after creation. This allows you to build a trace across process boundaries and allows the engineer to correlate and track everything involved in a specific request or transaction.

Each span contains metadata about the operation, such as its name, start and end timestamps, attributes, events, and status.

The metadata of a span could include [tags](https://opentracing.io/docs/overview/tags-logs-baggage/), to allow you to add metadata to the span to provide assistance in understanding where the trace is from and the context in which it was generated. 

Spans can also [carry logs](https://opentracing.io/docs/overview/tags-logs-baggage/) in the form of `key:value pairs`, useful for informational output from the application that sets some context or documents some specific aspect of the event.

OpenTelemetry has adopted [some semantic conventions](https://github.com/open-telemetry/opentelemetry-specification/blob/master/specification/overview.md#semantic-conventions) for spans that we should ensure we follow.

Here is an example of a span from [the OpenTelemetry docs](https://opentracing.io/docs/overview/spans/):

```text
   t=0            operation name: db_query               t=x 

     +-----------------------------------------------------+
     | · · · · · · · · · ·    Span     · · · · · · · · · · |
     +-----------------------------------------------------+

Tags:
- db.instance:"jdbc:mysql://127.0.0.1:3306/customers
- db.statement: "SELECT * FROM mytable WHERE foo='bar';"

Logs:
- message:"Can't connect to mysql server on '127.0.0.1'(10061)"

SpanContext:
- trace_id:"abc123"
- span_id:"xyz789"
- Baggage Items:
  - special_id:"vsid1738"
```

## Out of Scope

- Adding tracing support to existing sinks that could receive them.

## Proposal

We want Vector to receive incoming traces from applications. In order to do this Vector needs to understand the concept of tracing. This RFC will define the basic internal objects and building blocks for tracing in Vector in a similar manner to how Event includes logs and metrics.

To implement those building blocks this RFC will propose an OpenTelemetry source and sink.

### Source

Source modelled on [the OTLP Receiver](https://github.com/open-telemetry/opentelemetry-collector/blob/master/receiver/otlpreceiver/README.md).

### Sink

Sink modelled on [the OTLP exporter](https://github.com/open-telemetry/opentelemetry-collector/blob/master/exporter/otlpexporter/otlp.go) that supports outputting traces, metrics, and logs.

## Prior Art

* [Sample exporters to other platforms](https://github.com/open-telemetry/opentelemetry-collector/tree/master/exporter)
* [Rust sample app for OpenTelemetry](https://github.com/open-telemetry/opentelemetry-rust/tree/0fa4e7d506cb52520607fa5da70d0efa15e1f6cb/examples/basic)
* [Jaeger](https://www.jaegertracing.io/).
* There are currently two Rust client language implementations for OpenTelemetry:
  * https://github.com/open-telemetry/opentelemetry-rust / https://crates.io/crates/opentelemetry
  * https://github.com/jtescher/opentelemetry-rust / https://crates.io/crates/tracing-opentelemetry


## Drawbacks


## Rationale


## Plan of Attack

## Decisions

* OpenTelmetry supports traces, metrics, and potentially logs. Do we want to support all incoming? Does this mean three sources?

* Do we want to add [an exporter](https://github.com/open-telemetry/opentelemetry-collector/tree/master/exporter) to OpenTelemetry?

## Follow up work

### Traces

* Jaeger Source
* OpenCensus Source
* Zipkin Source
* Kafka Source

### Metrics

* OpenCensus Metrics Source
* OpenTelemetry Metrics Source