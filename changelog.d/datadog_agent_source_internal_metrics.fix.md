The `datadog_agent` source now correctly calculates the value for the metric `component_received_event_bytes_total` before enriching the event with Vector metadata.

The source also now adheres to the Component Specification by incrementing `component_errors_total` when a request succeeded in decompression but JSON parsing failed.
