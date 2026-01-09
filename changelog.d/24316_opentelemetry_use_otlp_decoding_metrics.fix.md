The `opentelemetry` source now correctly emits the `component_received_events_total` metric when `use_otlp_decoding` is enabled for HTTP requests. Previously, this metric would show 0 despite events being received and processed.

authors: thomasqueirozb
