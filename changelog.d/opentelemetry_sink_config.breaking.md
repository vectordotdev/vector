Changed the `opentelemetry` sink config fields to remove `protocol.*`. `protocol.type` was replaced
by `protocol` and all fields previously nested under `protocol` now can be placed in the top level
configuration.

Before:

```yaml
sinks:
  otel_sink:
    inputs:
      - in
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

authors: thomasqueirozb
