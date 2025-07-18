# Component Specification

This document specifies Vector Component behavior (source, transforms, and
sinks) for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

- [Component Specification](#component-specification)
  - [Introduction](#introduction)
  - [Scope](#scope)
  - [Naming](#naming)
    - [Source and sink naming](#source-and-sink-naming)
    - [Transform naming](#transform-naming)
  - [Configuration](#configuration)
    - [Options](#options)
      - [`endpoint(s)`](#endpoints)
  - [Instrumentation](#instrumentation)
    - [Events](#events)
      - [ComponentEventsReceived](#componenteventsreceived)
      - [ComponentEventsSent](#componenteventssent)
      - [ComponentError](#componenterror)
      - [ComponentEventsDropped](#componenteventsdropped)
      - [SinkNetworkBytesSent](#sinknetworkbytessent)
      - [SourceNetworkBytesReceived](#sourcenetworkbytesreceived)
  - [Sink Operational Requirements](#sink-operational-requirements)
    - [Health checks](#health-checks)
    - [Finalization](#finalization)
  - [Acknowledgements](#acknowledgements)

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

Finally, this document is written from the broad perspective of a Vector
component. Unless otherwise stated, a section applies to all component types
(sources, transforms, and sinks).

## Naming

To align with the [logical boundaries of components], component naming MUST
follow the following guidelines.

### Source and sink naming

- MUST only contain ASCII alphanumeric, lowercase, and underscores.
- MUST be a noun named after the protocol or service that the component
  integrates with.
- MAY be suffixed with the event type only if the component is specific to
  that type, `logs`, `metrics`, or `traces` (e.g., `kubernetes_logs`,
  `apache_metrics`).

### Transform naming

- MUST only contain ASCII alphanumeric, lowercase, and underscores.
- MUST be a verb describing the broad purpose of the transform (e.g., `route`,
  `sample`, `delegate`).

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

#### `listen`

When a component listens for incoming connections, it SHOULD expose a `listen` configuration option that takes
a `string` representing an address with `<protocol>:<address>`.

Options for `protocol` are:

- `unix+stream`, where `address` should be a file path
- `unix+datagram`, where `address` should be a file path
- `unix`, same as `unix+stream`
- `tcp`, where `address` should be `<host>:<port>`
- `udp`, where `address` should be `<host>:<port>`

Components MAY have a default protocol. For example, a `statsd` component may default the protocol
to `udp` and only require the `<host>:<port>` to bind to.

## Instrumentation

**Extends the [Instrumentation Specification].**

Vector components MUST be instrumented for optimal observability and monitoring.

### Events

This section lists all required events that a component MUST emit. Additional
events are listed that a component is RECOMMENDED to emit, but remain OPTIONAL.
It is expected that components will emit custom events beyond those listed here
that reflect component specific behavior. There is leeway in the implementation
of these events:

- Events MAY be augmented with additional component-specific context. For
  example, the `socket` source adds a `mode` attribute as additional context.
- The naming of the events MAY deviate to satisfy implementation. For example,
  the `socket` source may rename the `EventReceived` event to
  `SocketEventReceived` to add additional socket specific context.
- Components MAY emit events for batches of Vector events for performance
  reasons, but the resulting telemetry state MUST be equivalent to emitting
  individual events. For example, emitting the `EventsReceived` event for 10
  events MUST increment the `component_received_events_total` counter by 10.

#### ComponentEventsReceived

**Note**: Will be deprecated once `SourceNetworkBytesReceived` exists.

_All components_ MUST emit a `ComponentEventsReceived` event that represents
the reception of Vector events from an upstream component.

- Emission
  - MUST emit immediately after creating or receiving Vector events, before modification or metadata
    is added.
- Properties
  - `count` - The count of Vector events.
  - `byte_size` - The estimated JSON byte size of all events received.
- Metrics
  - MUST increment the `component_received_events_total` counter by the defined
    `quantity` property with the other properties as metric tags.
  - MUST increment the `component_received_event_bytes_total` counter by the
    defined `byte_size` property with the other properties as metric tags.
- Logs
  - MUST log a `Events received.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

#### ComponentBytesReceived

**Note**: Will be deprecated once `SourceNetworkBytesSent` exists.

*Sources* MUST emit a `ComponentBytesReceived` event that represent the reception of bytes.

- Emission
  - MUST emit immediately after receiving, decompressing and filtering bytes from the upstream
    source and before the creation of a Vector event.
- Properties
  - `byte_size`
    - For UDP, TCP, and Unix protocols, the total number of bytes received from
      the socket excluding the delimiter.
    - For HTTP-based protocols, the total number of bytes in the HTTP body, after decompression
    - For files, the total number of bytes read from the file excluding the
      delimiter.
  - `protocol` - The protocol used to send the bytes (i.e., `tcp`, `udp`,
    `unix`, `http`, `https`, `file`, etc.).
  - `http_path` - If relevant, the HTTP path, excluding query strings.
- Metrics
  - MUST increment the `component_received_bytes_total` counter by the defined value with
    the defined properties as metric tags.
- Logs
  - MUST log a `Bytes received.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

#### ComponentBytesSent

*Sinks* MUST emit a `ComponentBytesSent` event that represent the transmission of bytes.

- Emission
  - MUST emit a `ComponentBytesSent` event immediately after sending bytes to the downstream target,
    if the transmission was successful. The reported bytes MUST be before compression.
  - Note that sinks that simply expose data, but don't delete the data after sending it, like the
    `prometheus_exporter` sink, SHOULD NOT emit this metric.
- Properties
  - `byte_size`
    - For UDP, TCP, and Unix protocols, the total number of bytes placed on the
      socket excluding the delimiter.
    - For HTTP-based protocols, the total number of bytes in the HTTP body before compression
    - For files, the total number of bytes written to the file excluding the
      delimiter.
  - `protocol` - The protocol used to send the bytes (i.e., `tcp`, `udp`,
    `unix`, `http`, `https`, `file`, etc.).
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

#### ComponentEventsSent

_All components_ MUST emit an `ComponentEventsSent` event that represents the
emission of Vector events to the next downstream component(s).

- Emission
  - MUST emit immediately after _successful_ transmission of Vector events.
    MUST NOT emit if the transmission was unsuccessful.
  - MUST NOT emit for pull-based sinks since they do not send events. For
    example, the `prometheus_exporter` sink MUST NOT emit this event.
- Properties
  - `count` - The count of Vector events.
  - `byte_size` - The estimated JSON byte size of all events sent.
  - `output` - OPTIONAL, for components that can use multiple outputs, the name
    of the output that events were sent to. For events sent to the default
    output, this value MUST be `_default`.
- Metrics
  - MUST increment the `component_sent_events_total` counter by the defined
    `quantity` property with the other properties as metric tags.
  - MUST increment the `component_sent_event_bytes_total` counter by the
    defined `byte_size` property with the other properties as metric tags.
- Logs
  - MUST log a `Events sent.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

#### ComponentError

**Extends the [Error event].**

_All components_ MUST emit error events in accordance with the [Error event]
requirements.

This specification does not list a standard set of errors that components must
implement since errors are specific to the component.

#### ComponentEventsDropped

**Extends the [EventsDropped event].**

_All components_ that can drop events MUST emit a `ComponentEventsDropped`
event in accordance with the [EventsDropped event] requirements.

#### SinkNetworkBytesSent

(to be implemented)

_Sinks_ MUST emit a `SinkNetworkBytesSent` that represents the egress of
_raw network bytes_.

- Emission
  - MUST emit immediately after egress of raw network bytes regardless
    of whether the transmission was successful or not.
    - This includes pull-based sinks, such as the `prometheus_exporter` sink,
      and SHOULD reflect the bytes sent to the client when requested (pulled).
  - MUST emit _after_ processing of the bytes (encryption, compression,
    filtering, etc.)
- Properties
  - `byte_size` - The number of raw network bytes sent after processing.
    - SHOULD be the closest representation possible of raw network bytes based
      on the sink's capabilities. For example, if the sink uses an HTTP
      client that does not provide access to the total request byte size, then
      the sink should use the byte size of the payload/body.
- Metrics
  - MUST increment the `component_sent_network_bytes_total` counter by the
    defined value with the defined properties as metric tags.
- Logs
  - MUST log a `Network bytes sent.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

#### SourceNetworkBytesReceived

(to be implemented)

_Sources_ MUST emit a `SourceNetworkBytesReceived` event that represents the
ingress of _raw network bytes_.

- Emission
  - MUST emit immediately after ingress of raw network bytes.
  - MUST emit _before_ processing of the bytes (decryption, decompression,
    filtering, etc.).
    - This includes pull-based sources that issue requests to ingest bytes.
- Properties
  - `byte_size` - The number of raw network bytes received before
    processing (decryption, decompression, filtering, etc.).
    - SHOULD be the closest representation possible of raw network bytes based
      on the source's capabilities. For example, if the source uses an HTTP
      client that only provides access to the request body, then the raw
      request body bytes should be used.
- Metrics
  - MUST increment the `component_received_network_bytes_total` counter by the
    defined value with the defined properties as metric tags.
- Logs
  - MUST log a `Network bytes received.` message at the `trace` level with the
    defined properties as key-value pairs.
  - MUST NOT be rate limited.

## Sink Operational Requirements

### Health checks

All sink components SHOULD define a health check. These checks are executed at
boot and as part of `vector validate`. This health check SHOULD, as closely as
possible, emulate the sink's normal operation to give the best possible signal
that Vector is configured correctly.

These checks SHOULD NOT query the health of external systems, but MAY fail due
to external system being unhealthy. For example, a health check for the `aws_s3`
sink might fail if AWS is unhealthy, but the check itself should not query for
AWS's status.

See the [development documentation][health checks] for more context guidance.

### Finalization

All sink components MUST defer finalization of events until after those events have been
delivered. This finalization controls when the events are removed from any source disk buffer. To do
this, the sink must extract the finalizers from events before they are delivered and ensure they are
not dropped until after delivery is completed.

## Acknowledgements

Further to the above, all sink components MUST support acknowledgements. This requires both a
configuration option named `acknowledgements` conforming to the `AcknowledgementsConfig` type, as
well as updating the status of all finalizers deferred above after delivery of the events is
completed. This update is automatically handled for all sinks that use the newer `StreamSink`
framework. Additionally, unit tests for the sink SHOULD ensure through unit tests that delivered
batches have their status updated properly for both normal delivery and delivery errors.

[configuration specification]: configuration.md
[error event]: instrumentation.md#Error
[eventsdropped event]: instrumentation.md#EventsDropped
[high user experience expectations]: https://github.com/vectordotdev/vector/blob/master/docs/USER_EXPERIENCE_DESIGN.md
[health checks]: ../DEVELOPING.md#sink-healthchecks
[instrumentation specification]: instrumentation.md
[logical boundaries of components]: ../USER_EXPERIENCE_DESIGN.md#logical-boundaries
[rfc 2119]: https://datatracker.ietf.org/doc/html/rfc2119
