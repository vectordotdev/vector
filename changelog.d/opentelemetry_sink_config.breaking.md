Changed the `opentelemetry` sink config fields to remove `protocol.*`. `protocol.type` was replaced
by `protocol` and all fields previously nested under `protocol` now can be placed in the top level
configuration.

The legacy nested `protocol.*` format is still accepted temporarily but is deprecated and logs a
warning on startup. Migrate to the flat format before the fallback is removed in a future release.

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
