The `otlp` codec now converts native trace events from the `opentelemetry` source (decoded without
`use_otlp_decoding`) back to OTLP spans, so they can be sent with the `opentelemetry` sink. Fields that
cannot be converted are reported with a rate-limited warning. Datadog traces are still rejected.

authors: thomasqueirozb
