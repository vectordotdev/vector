# Buffer Specification

This document specifies Vector's buffer behavior for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

- [Scope](#scope)
- [Instrumentation](#instrumentation)
  - [Terms And Definitions](#terms-and-definitions)
  - [Events](#events)
    - [BufferCreated](#buffercreated)
    - [BufferEventsReceived](#buffereventsreceived)
    - [BufferEventsSent](#buffereventssent)
    - [BufferError](#buffererror)
    - [BufferEventsDropped](#buffereventsdropped)

## Scope

This specification addresses direct buffer development and does not cover aspects that buffers inherit "for free". For example, this specification does not cover global context, such as component_id, that all buffers receive in their telemetry by nature of being attached to a Vector component.

## Instrumentation

**This section extends the [Instrumentation Specification], which should be read
first.**

Vector buffers MUST be instrumented for optimal observability and monitoring.

### Terms And Definitions

- `byte_size` - Refers to the byte size of events from a buffer's perspective. For memory buffers, `byte_size` represents the in-memory byte size of events. For disk buffers, `byte_size` represents the serialized byte size of events.
- `buffer_type` - One of `memory`, `disk`. Buffer metrics MUST be tagged with `buffer_type` unless otherwise specified.

### Events

#### BufferCreated

_All buffers_ MUST emit a `BufferCreated` event upon creation. To avoid stale metrics, this event MUST be regularly emitted at an interval.

- Properties
  - `max_size_bytes` - the max size of the buffer in bytes if relevant
  - `max_size_events` - the max size of the buffer in number of events if relevant
- Metric
  - MUST emit the `buffer_max_event_size` gauge (in-memory buffers) if the defined `max_size_events` value is present
  - MUST emit the `buffer_max_byte_size` gauge (disk buffers) if the defined `max_size_bytes` value is present

#### BufferEventsReceived

_All buffers_ MUST emit a `BufferEventsReceived` event:

1. upon startup if there are existing events in the buffer.
2. after receiving one or more Vector events.

- Properties
  - `count` - the number of received events
  - `byte_size` - as defined in [Terms and Definitions](#terms-and-definitions)
- Metric
  - MUST increment the `buffer_received_events_total` counter by the defined `count`
  - MUST increment the `buffer_received_bytes_total` counter by the defined `byte_size`
  - MUST increment the `buffer_events` gauge by the defined `count`
  - MUST increment the `buffer_byte_size` gauge by the defined `byte_size`

#### BufferEventsSent

_All buffers_ MUST emit a `BufferEventsSent` event after sending one or more Vector events.

- Properties
  - `count` - the number of sent events
  - `byte_size` - as defined in [Terms and Definitions](#terms-and-definitions)
- Metric
  - MUST increment the `buffer_sent_events_total` counter by the defined `count`
  - MUST increment the `buffer_sent_bytes_total` counter by the defined `byte_size`
  - MUST decrement the `buffer_events` gauge by the defined `count`
  - MUST decrement the `buffer_byte_size` gauge by the defined `byte_size`

#### BufferError

**Extends the [Error event].**

_All buffers_ MUST emit error events in accordance with the [Error event]
requirements.

This specification does not list a standard set of errors that components must
implement since errors are specific to the buffer and operation.

#### BufferEventsDropped

**Extends the [EventsDropped event].**

_All buffers_ that can drop events MUST emit a `BufferEventsDropped` event in
accordance with the [EventsDropped event] requirements.

[error event]: instrumentation.md#Error
[eventsdropped event]: instrumentation.md#EventsDropped
[instrumentation specification]: instrumentation.md
