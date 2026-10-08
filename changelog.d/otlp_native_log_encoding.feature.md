The `otlp` codec now converts native Vector log events to OTLP log records, so logs from any source can
be sent with the `opentelemetry` sink without a `remap` transform that builds the `resourceLogs` structure.
The conversion is the inverse of the `opentelemetry` source decoding. Fields with no OTLP equivalent are
sent as log record attributes. Events that already have the OTLP structure are sent as they are.

authors: thomasqueirozb
