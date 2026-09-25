---
what: "Nested `protocol.*` configuration format for the `opentelemetry` sink"
deprecated_since: "0.59.0"
---

The nested `protocol.*` configuration format for the `opentelemetry` sink is deprecated. The legacy
format is still accepted temporarily but logs a warning on startup and will be removed in a future
release.

Migrate to the flat format by moving all fields from `protocol.*` to the top level and replacing
`protocol.type` with `protocol`.

The HTTP `retry_strategy` option moves from `protocol.retry_strategy` to `retry_strategy`.

Before:

```yaml
sinks:
  otel_sink:
    inputs:
      - in
    type: opentelemetry
    protocol:
      type: http
      uri: http://otel-collector-sink:5318/v1/logs
      method: post
      encoding:
        codec: json
      framing:
        method: newline_delimited
      batch:
        max_events: 1
      request:
        headers:
          content-type: application/json
```

After:

```yaml
sinks:
  otel_sink:
    inputs:
      - in
    type: opentelemetry
    protocol: http
    uri: http://otel-collector-sink:5318/v1/logs
    method: post
    encoding:
      codec: json
    framing:
      method: newline_delimited
    batch:
      max_events: 1
    request:
      headers:
        content-type: application/json
```
