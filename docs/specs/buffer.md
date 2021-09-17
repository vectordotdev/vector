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
  * MUST update the `buffer_usage_percentage` gauge which measures the current buffer space utilization (number of events/bytes) over total space available (max number of events/bytes)

#### `EventsFlushed`

*All buffers* MUST emit an `EventsFlushed` event immediately after flushing one or more Vector events.

* Properties
  * `count` - the number of flushed events
  * `byte_size` - the byte size of flushed events
* Metric
  * MUST increment the `buffer_flushed_events_total` counter by the defined `count`
  * MUST increment the `buffer_flushed_bytes_total` counter by the defined `byte_size`
  * MUST decrement the `buffer_events` gauge by the defined `count`
  * MUST decrement the `buffer_byte_size` gauge by the defined `byte_size`
  * MUST update the `buffer_usage_percentage` gauge

#### `EventsDropped`

*All buffers* MUST emit an `EventsDropped` event immediately after dropping one or more Vector events.

* Properties
  * `count` - the number of dropped events
* Metric
  * MUST increment the `buffer_discarded_events_total` counter by the defined `count`

[Instrumentation Specification]: instrumentation.md
