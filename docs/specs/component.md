# Component Specification

This document specifies Vector Component behavior (source, transforms, and
sinks) for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Introduction](#introduction)
1. [Scope](#scope)
1. [How to read this document](#how-to-read-this-document)
1. [Instrumentation](#instrumentation)
   1. [Batching](#batching)
   1. [Events](#events)
      1. [BytesReceived](#bytesreceived)
      1. [EventsRecevied](#eventsrecevied)
      1. [EventsProcessed](#eventsprocessed)
      1. [EventsSent](#eventssent)
      1. [Error](#error)

<!-- /MarkdownTOC -->

## Introduction

Vector is a highly flexible observability data pipeline due to its directed
acyclic graph processing model. Each node in the graph is a Vector Component,
and in order to meet our [high user experience expectations] each Component must
adhere to a common set of behaviorial rules. This document aims to clearly
outline these rules to guide new component development and ongoing maintenance.

## Scope

This specification addresses direct component concerns

TODO: limit this document to direct component-level code and not supporting
infrastructure.

## How to read this document

This document is written from the broad perspective of a Vector component.
Unless otherwise stated, a section applies to all component types (sources,
transforms, and sinks).

## Instrumentation

Vector components MUST be instrumented for optimal observability and monitoring.
This is required to drive various interfaces that Vector users depend on to
manage Vector installations in mission critical production environments.

### Batching

For performance reasons, components SHOULD instrument batches of Vector events
as opposed to individual Vector events. [Pull request #8383] demonstrated
meaningful performance improvements as a result of this strategy.

### Events

Vector implements an event driven pattern ([RFC 2064]) for internal
instrumentation. This section lists all required and optional events that a
component MUST emit. It is expected that components will emit custom events
beyond those listed here that reflect component specific behavior.

There is leeway in the implementation of these events:

* Events MAY be augmented with additional component-specific context. For
  example, the `socket` source adds a `mode` attribute as additional context.
* The naming of the events MAY deviate to satisfy implementation. For example,
  the `socket` source may rename the `EventRecevied` event to
  `SocketEventReceived` to add additional socket specific context.
* Components MAY emit events for batches of Vector events for performance
  reasons, but the resulting telemetry state MUST be equivalent to emitting
  individual events. For example, emitting the `EventsReceived` event for 10
  events MUST increment the `events_in_total` by 10.

#### BytesReceived

*Sources* MUST emit a `BytesReceived` event immediately after receiving bytes
from the upstream source, before the creation of a Vector event. The following
telemetry MUST be included:

* Metrics
   * MUST increment the `bytes_in_total` counter by the number of bytes
     received.
     * If received over the HTTP then the `http_path` tag must be set.
* Logging
   * MUST log a `Bytes received.` message at the `trace` level with no rate
     limiting.

#### EventsRecevied

*All components* MUST emit an `EventsReceived` event immediately after creating
or receiving one or more Vector events.

* Metrics
   * MUST increment the `events_in_total` counter by the number of events
     received.
   * MUST increment the `event_bytes_in_total` counter by the cumulative byte
     size of the events in JSON representation.
* Logging
   * MUST log a `{count} events received.` message at the `trace` level with no
     rate limiting.

#### EventsProcessed

*All components* MUST emit an `EventsProcessed` event processing an event,
before the event is encoded and sent downstream.

* Metrics
   * MUST increment the `events_in_total` counter by 1.
   * MUST increment the `event_bytes_in_total` counter by the event's byte
     size in JSON representation.
* Logging
   * MUST log a `Event received.` message at the `trace` level with no rate
     limiting.

#### EventsSent

*All components* MUST emit an `EventsSent` event processing an event,
before the event is encoded and sent downstream.


#### Error



[high user experience expectations]: https://github.com/timberio/vector/blob/master/docs/USER_EXPERIENCE_DESIGN.md
[Pull request #8383]: https://github.com/timberio/vector/pull/8383/
[RFC 2064]: https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
[RFC 2119]: https://datatracker.ietf.org/doc/html/rfc2119
