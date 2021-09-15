# Buffer Specification

This document specifies Vector's buffer behavior for the development of Vector.

The key words “MUST”, “MUST NOT”, “REQUIRED”, “SHALL”, “SHALL NOT”, “SHOULD”,
“SHOULD NOT”, “RECOMMENDED”, “MAY”, and “OPTIONAL” in this document are to be
interpreted as described in [RFC 2119].

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Instrumentation](#instrumentation)

<!-- /MarkdownTOC -->

## Instrumentation

### When a Vector event is stored in the buffer
* Metric
  * MUST increment the `buffer_event_count` gauge by one
  * MUST increment the `buffer_byte_size` gauge by size of the event
  * MUST update the `buffer_usage_percentage` gauge which measures the current buffer space utilization (number of events/bytes) over total space available (max number of events/bytes).

### When a Vector event is removed from the buffer
* Metric
  * MUST decrement the `buffer_event_count` gauge by one
  * MUST decrement the `buffer_byte_size` gauge by size of the event
  * MUST update the `buffer_usage_percentage` gauge 

### When a Vector event is dropped
* Metric
  * MUST increment the `buffer_discarded_events_total` counter by one