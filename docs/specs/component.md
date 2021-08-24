# Component Specification

This document specifies Vector Component behavior (source, transforms, and
sinks) for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Introduction](#introduction)
1. [How to read this document](#how-to-read-this-document)
1. [Instrumentation](#instrumentation)
   1. [Events](#events)
      1. [BytesReceived](#bytesreceived)
      1. [EventRecevied](#eventrecevied)

<!-- /MarkdownTOC -->

## Introduction

Vector is a highly flexible observability data pipeline due to its directed
acyclic graph processing model. Each node in the graph is a Vector Component,
and in order to meet our [high user experience expectations] each Component must
adhere to a common set of behaviorial rules. This document aims to clearly
outline these rules to guide new component development and ongoing maintenance.

## How to read this document

This document is written from the broad perspective of a Vector component.
Unless otherwise stated, a section applies to all component types, although,
most sections will be broken along component lines for easy adherence.

## Instrumentation

### Events

Vector implements an event driven pattern ([RFC 2064]) for internal
instrumentation. This section lists all required and optional events that a
component MUST emit.

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
* Logging
   * MUST log a `Bytes received.` message at the `trace` level with no rate
     limiting.

#### EventRecevied

*Components* MUST emit an `EventReceived` event immediately after receiving or
creating a Vector event.

* Metrics
   * MUST increment the `events_in_total` counter by 1.
   * MUST increment the `event_bytes_in_total` counter by the event's byte
     size in JSON representation.
* Logging
   * MUST log a `Event received.` message at the `trace` level with no rate
     limiting.

[high user experience expectations]: https://github.com/timberio/vector/blob/master/docs/USER_EXPERIENCE_DESIGN.md
[RFC 2064]: https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
[RFC 2119]: https://datatracker.ietf.org/doc/html/rfc2119
