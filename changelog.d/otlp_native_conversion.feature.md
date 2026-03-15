The `opentelemetry` sink with `codec: otlp` now automatically converts Vector's native (flat) log and trace formats back to OTLP protobuf.

When OTLP data is decoded into Vector's flat internal format (the default with `use_otlp_decoding: false`), re-encoding as OTLP previously required complex VRL to manually rebuild the nested protobuf structure. Logs and traces from non-OTLP sources could not be sent to OTLP sinks at all without this VRL workaround.

The OTLP encoder now detects native events and automatically converts them to valid OTLP protobuf. Pre-formatted OTLP events (from `use_otlp_decoding: true`) continue using the existing passthrough path unchanged.

Log field mapping: `.message` → `body`, `.timestamp` → `timeUnixNano`, `.attributes.*` → `attributes[]`, `.resources.*` → `resource.attributes[]`, `.severity_text` → `severityText`, `.severity_number` → `severityNumber`, `.scope.name/version` → `scope`, `.trace_id` → `traceId`, `.span_id` → `spanId`.

Trace field mapping: `.trace_id` → `traceId`, `.span_id` → `spanId`, `.parent_span_id` → `parentSpanId`, `.name` → `name`, `.kind` → `kind`, `.start_time_unix_nano` → `startTimeUnixNano`, `.end_time_unix_nano` → `endTimeUnixNano`, `.attributes.*` → `attributes[]`, `.resources.*` → `resource.attributes[]`, `.events` → `events[]`, `.links` → `links[]`, `.status` → `status`.

Note: Native auto-conversion supports logs and traces. Metrics continue to work via the existing passthrough path (`use_otlp_decoding: true`); native metric conversion is planned for a future release.

authors: szibis
