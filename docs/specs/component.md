# Component Specification

This document specifies Vector Component behavior (source, transforms, and
sinks) for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

- [Introduction](#introduction)
- [Scope](#scope)
- [How to read this document](#how-to-read-this-document)
- [Naming](#naming)
  - [Source and sink naming](#source-and-sink-naming)
  - [Transform naming](#transform-naming)
- [Configuration](#configuration)
  - [Options](#options)
    - [`endpoint(s)`](#endpoints)
- [Instrumentation](#instrumentation)
  - [Events](#events)
    - [ComponentBytesReceived](#componentbytesreceived)
    - [ComponentEventsReceived](#componenteventsreceived)
    - [ComponentEventsSent](#componenteventssent)
    - [ComponentBytesSent](#componentbytessent)
    - [ComponentError](#componenterror)
    - [ComponentEventsDropped](#componenteventsdropped)
- [Health checks](#health-checks)

## Introduction

Vector is a highly flexible observability data pipeline due to its directed
acyclic graph processing model. Each node in the graph is a Vector Component,
and in order to meet our [high user experience expectations] each Component must
adhere to a common set of behavioral rules. This document aims to clearly
outline these rules to guide new component development and ongoing maintenance.

## Scope

This specification addresses _direct_ component development and does not cover
aspects that components inherit "for free". For example, this specification does
not cover global context, such as `component_id`, that all components receive in
their telemetry by nature of being a Vector component.

## How to read this document

This document is written from the broad perspective of a Vector component.
Unless otherwise stated, a section applies to all component types (sources,
transforms, and sinks).

## Naming

To align with the [logical boundaries of components], component naming MUST
follow the following guidelines.

### Source and sink naming

- MUST only contain ASCII alphanumeric, lowercase, and underscores.
- MUST be a noun named after the protocol or service that the component integrates with.
- MAY be suffixed with the event type, `logs`, `metrics`, or `traces` (e.g., `kubernetes_logs`, `apache_metrics`).

### Transform naming

- MUST only contain ASCII alphanumeric, lowercase, and underscores.
- MUST be a verb describing the broad purpose of the transform (e.g., `route`, `sample`, `delegate`).

## Configuration

This section extends the [Configuration Specification] for component specific
configuration.

### Options

#### `endpoint(s)`

When a component makes a connection to a downstream target, it SHOULD
expose either an `endpoint` option that takes a `string` representing a
single endpoint, or an `endpoints` option that takes an array of strings
representing multiple endpoints. If a component uses multiple options to
automatically build the endpoint, then the `endpoint(s)` option MUST
override that process.

## Instrumentation

**Extends the [Instrumentation Specification].**

Vector components MUST be instrumented for optimal observability and monitoring.

### Events

This section lists all required events that a component MUST emit. Additional events
are listed that a component is RECOMMENDED to emit, but remain OPTIONAL. It is
expected that components will emit custom events beyond those listed here that
reflect component specific behavior. There is leeway in the implementation of these
events:

- Events MAY be augmented with additional component-specific context. For
  example, the `socket` source adds a `mode` attribute as additional context.
- The naming of the events MAY deviate to satisfy implementation. For example,
  the `socket` source may rename the `EventReceived` event to
  `SocketEventReceived` to add additional socket specific context.
- Components MAY emit events for batches of Vector events for performance
  reasons, but the resulting telemetry state MUST be equivalent to emitting
  individual events. For example, emitting the `EventsReceived` event for 10
  events MUST increment the `component_received_events_total` counter by 10.

#### ComponentBytesReceived

*Sources- MUST emit a `ComponentBytesReceived` event immediately after receiving, decompressing
and filtering bytes from the upstream source and before the creation of a Vector event.

- Properties
  - `byte_size`
    - For UDP, TCP, and Unix protocols, the total number of bytes received from
      the socket excluding the delimiter.
    - For HTTP-based protocols, the total number of bytes in the HTTP body, as
      represented by the `Content-Length` header.
    - For files, the total number of bytes read from the file excluding the
      delimiter.
  - `protocol` - The protocol used to send the bytes (i.e., `tcp`, `udp`,
    `unix`, `http`, `https`, `file`, etc.)
  - `http_path` - If relevant, the HTTP path, excluding query strings.
  - `socket` - If relevant, the socket number that bytes were received from.
- Metrics
  - MUST increment the `component_received_bytes_total` counter by the defined value with
    the defined properties as metric tags.
- Logs
  - MUST log a `Bytes received.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

#### ComponentEventsReceived

*All components- MUST emit an `ComponentEventsReceived` event immediately after creating
or receiving one or more Vector events.

- Properties
  - `count` - The count of Vector events.
  - `byte_size` - The cumulative in-memory byte size of all events received.
- Metrics
  - MUST increment the `component_received_events_total` counter by the defined `quantity`
    property with the other properties as metric tags.
  - MUST increment the `component_received_event_bytes_total` counter by the defined
    `byte_size` property with the other properties as metric tags.
- Logs
  - MUST log a `Events received.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

#### ComponentEventsSent

*All components- that send events down stream, and delete them in Vector, MUST
emit an `ComponentEventsSent` event immediately after sending, if the transmission was
successful.

Note that for sinks that simply expose data, but don't delete the data after
sending it, like the `prometheus_exporter` sink, SHOULD NOT publish this metric.

- Properties
  - `count` - The count of Vector events.
  - `byte_size` - The cumulative in-memory byte size of all events sent.
  - `output` - For components that can use multiple outputs, the name of the
    output that events were sent to. For events sent to the default output, this
    value MUST be `_default`.
- Metrics
  - MUST increment the `component_sent_events_total` counter by the defined value with the
    defined properties as metric tags.
  - MUST increment the `component_sent_event_bytes_total` counter by the event's byte size
    in JSON representation.
- Logs
  - MUST log a `Events sent.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

#### ComponentBytesSent

*Sinks- that send events down stream, and delete them in Vector, MUST emit
a `ComponentBytesSent` event immediately after sending bytes to the downstream target, if
the transmission was successful. The reported bytes MUST be before compression.

Note that for sinks that simply expose data, but don't delete the data after
sending it, like the `prometheus_exporter` sink, SHOULD NOT publish this metric.

- Properties
  - `byte_size`
    - For UDP, TCP, and Unix protocols, the total number of bytes placed on the
      socket excluding the delimiter.
    - For HTTP-based protocols, the total number of bytes in the HTTP body, as
      represented by the `Content-Length` header.
    - For files, the total number of bytes written to the file excluding the
      delimiter.
  - `protocol` - The protocol used to send the bytes (i.e., `tcp`, `udp`,
    `unix`, `http`, `https`, `file`, etc.)
  - `endpoint` - If relevant, the endpoint that the bytes were sent to. For
    HTTP, this MUST be the host and path only, excluding the query string.
  - `file` - If relevant, the absolute path of the file.
- Metrics
  - MUST increment the `component_sent_bytes_total` counter by the defined value with the
    defined properties as metric tags.
- Logs
  - MUST log a `Bytes sent.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

#### ComponentError

**Extends the [Error event].**

*All components- MUST emit error events in accordance with the [Error event]
requirements.

This specification does not list a standard set of errors that components must
implement since errors are specific to the component.

#### ComponentEventsDropped

**Extends the [EventsDropped event].**

*All components- that can drop events MUST emit a `ComponentEventsDropped`
event in accordance with the [EventsDropped event] requirements.

## Health checks

All sink components SHOULD define a health check. These checks are executed at
boot and as part of `vector validate`. This health check SHOULD, as closely as
possible, emulate the sink's normal operation to give the best possible signal
that Vector is configured correctly.

These checks SHOULD NOT query the health of external systems, but MAY fail due
to external system being unhealthy. For example, a health check for the `aws_s3`
sink might fail if AWS is unhealthy, but the check itself should not query for
AWS's status.

See the [development documentation][health checks] for more context guidance.

[Configuration Specification]: configuration.md
[Error event]: instrumentation.md#Error
[EventsDropped event]: instrumentation.md#EventsDropped
[high user experience expectations]: https://github.com/vectordotdev/vector/blob/master/docs/USER_EXPERIENCE_DESIGN.md
[health checks]: ../DEVELOPING.md#sink-healthchecks
[Instrumentation Specification]: instrumentation.md
[logical boundaries of components]: ../USER_EXPERIENCE_DESIGN.md#logical-boundaries
[RFC 2119]: https://datatracker.ietf.org/doc/html/rfc2119
