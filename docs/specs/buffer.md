# Buffer Specification

This document specifies Vector's buffer behavior for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Instrumentation](#instrumentation)

<!-- /MarkdownTOC -->

## Instrumentation

Vector buffers MUST be instrumented for optimal observability and monitoring. This is required to drive various interfaces that Vector users depend on to manage Vector installations in mission critical production environments. This section extends the [Instrumentation Specification].

### Events

#### `BufferCreated`

*All buffers* MUST emit a `BufferCreated` event immediately upon creation.

* Properties
  * `max_size` - the maximum number of events or byte size of the buffer
  * `initial_events_size` - the number of events in the buffer at creation
  * `initial_bytes_size` - the byte size of the buffer at creation
  * `component_id` - the ID of the component associated with the buffer
* Metric
  * MUST emit the `buffer_max_event_size` gauge (in-memory buffers) or `buffer_max_byte_size` gauge (disk buffers) with the defined `max_size` value and `component_id` as tag.
  * MUST emit the `buffer_received_events_total` counter with the defined `initial_events_size` value with `component_id` as tag.
  * MUST emit the `buffer_received_bytes_total` counter with the defined `initial_bytes_size` value with `component_id` as tag.

#### `EventsReceived`

*All buffers* MUST emit an `EventsReceived` event immediately after receiving one or more Vector events.

* Properties
  * `count` - the number of received events
  * `byte_size` - the byte size of received events
* Metric
  * MUST increment the `buffer_received_events_total` counter by the defined `count`
  * MUST increment the `buffer_received_bytes_total` counter by the defined `byte_size`
  * MUST increment the `buffer_events` gauge by the defined `count`
  * MUST increment the `buffer_byte_size` gauge by the defined `byte_size`

#### `EventsSent`

*All buffers* MUST emit an `EventsSent` event immediately after sending one or more Vector events.

* Properties
  * `count` - the number of sent events
  * `byte_size` - the byte size of sent events
* Metric
  * MUST increment the `buffer_sent_events_total` counter by the defined `count`
  * MUST increment the `buffer_sent_bytes_total` counter by the defined `byte_size`
  * MUST decrement the `buffer_events` gauge by the defined `count`
  * MUST decrement the `buffer_byte_size` gauge by the defined `byte_size`

#### `EventsDropped`

*All buffers* MUST emit an `EventsDropped` event immediately after dropping one or more Vector events.

* Properties
  * `count` - the number of dropped events
* Metric
  * MUST increment the `buffer_discarded_events_total` counter by the defined `count`

[Instrumentation Specification]: instrumentation.md
