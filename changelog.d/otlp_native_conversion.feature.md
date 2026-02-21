The `opentelemetry` sink with `codec: otlp` now automatically converts Vector's native log format to OTLP (OpenTelemetry Protocol) format.

Previously, events required manual VRL transformation to build the nested OTLP structure (`resourceLogs` -> `scopeLogs` -> `logRecords`). Now, native Vector logs with standard fields are automatically converted to proper OTLP protobuf.

Supported sources include OTLP receiver with `use_otlp_decoding: false` (flat decoded OTLP), file source with JSON/syslog logs, and any other Vector source (socket, kafka, exec, etc.).

Field mapping: `.message`/`.body`/`.msg` maps to `logRecords[].body`, `.timestamp` to `timeUnixNano`, `.attributes.*` to `logRecords[].attributes[]`, `.resources.*` to `resource.attributes[]`, `.severity_text` to `severityText`, and `.scope.name/version` to `scopeLogs[].scope`.

Invalid fields are handled gracefully with warnings and sensible defaults rather than errors. Events already in OTLP format (containing `resourceLogs`) continue to work unchanged.

authors: szibis
