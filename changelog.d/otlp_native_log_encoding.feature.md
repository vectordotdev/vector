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

The conversion is the inverse of the `opentelemetry` source decoding. Fields with no OTLP equivalent are
sent as log record attributes. A log event with a `resourceLogs`, `resourceMetrics`, or `resourceSpans`
root field is treated as an OTLP request that is already built, and is sent as it is.

authors: thomasqueirozb
