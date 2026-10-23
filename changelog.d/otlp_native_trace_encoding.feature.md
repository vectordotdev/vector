The `otlp` codec now converts native trace events from the `opentelemetry` source (decoded without
`use_otlp_decoding`) back to OTLP spans, so they can be sent with the `opentelemetry` sink. Spans and
fields that cannot be converted are omitted and reported as errors. A trace whose spans are all
rejected is left out of the request instead of failing the batch. Datadog traces are still rejected.

authors: thomasqueirozb
