The `otlp` codec now converts native Vector log events to OTLP log records, so logs from any source can
be sent with the `opentelemetry` sink without a `remap` transform that builds the `resourceLogs` structure.
For example, this log event from the `file` source:

```yaml
message: disk full
host: web-1
timestamp: 2026-10-09T12:00:00Z
source_type: file
```

is sent as this OTLP log record (shown as YAML, without default fields):

```yaml
resourceLogs:
  - scopeLogs:
      - logRecords:
          - timeUnixNano: "1791547200000000000"
            body:
              stringValue: disk full
            attributes:
              - key: host
                value:
                  stringValue: web-1
```

The conversion is the inverse of the `opentelemetry` source decoding. A log event with a `resourceLogs`,
`resourceMetrics`, or `resourceSpans` root field is treated as an OTLP request that is already built,
and is sent as it is.

The source type marker identifies the Vector source component type that produced the event, for example
`source_type: file`.

With `log_namespace: false` (Legacy namespace), fields with no OTLP equivalent are sent as log record
attributes. A timestamp inside the configured message field still sets `timeUnixNano` and remains in
the body. The marker's location is configured with `log_schema.source_type_key` (default:
`.source_type`). The codec omits this internal field. If the configured path points to metadata and
that field exists (for example, `%source_type`), the matching payload field (`.source_type`) is kept as
an attribute. Otherwise, the matching event field is omitted, because some Legacy sources still write
the marker there.

With `log_namespace: true` (Vector namespace), the entire event payload becomes the OTLP `body`.
A payload field named `.source_type` is preserved in the body, not sent as an attribute. The internal
marker is stored separately in `%vector.source_type` metadata and is not sent.

authors: thomasqueirozb
