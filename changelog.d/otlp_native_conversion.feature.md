The `opentelemetry` sink with `codec: otlp` now automatically converts Vector's native (flat) log format back to OTLP protobuf.

When OTLP logs are decoded into Vector's flat internal format (the default with `use_otlp_decoding: false`), re-encoding them as OTLP previously required 50+ lines of VRL to manually rebuild the nested protobuf structure. Logs from non-OTLP sources (file, syslog, socket) could not be sent to OTLP sinks at all without this VRL workaround.

The OTLP encoder now detects native log events and automatically converts them to valid OTLP protobuf. Pre-formatted OTLP events (from `use_otlp_decoding: true`) continue using the existing passthrough path unchanged.

Field mapping: `.message` → `body`, `.timestamp` → `timeUnixNano`, `.attributes.*` → `attributes[]`, `.resources.*` → `resource.attributes[]`, `.severity_text` → `severityText`, `.severity_number` → `severityNumber`, `.scope.name/version` → `scope`, `.trace_id` → `traceId`, `.span_id` → `spanId`.

Note: Native auto-conversion supports logs and traces. Metrics continue to work via the existing passthrough path (`use_otlp_decoding: true`); native metric conversion is planned for a future release.

authors: szibis
