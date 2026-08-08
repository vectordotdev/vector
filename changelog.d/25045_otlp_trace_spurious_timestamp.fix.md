Fixed the OTLP decoder adding an extra `timestamp` field to trace events when `log_namespace` was set to `legacy`. Trace events don't have a `timestamp` field in their schema, so this field showed up unexpectedly:

```json
// Before
{
  "trace_id": "...",
  "span_id": "...",
  "name": "test_span",
  "timestamp": "2026-08-06T12:00:00Z"
}

// After
{
  "trace_id": "...",
  "span_id": "...",
  "name": "test_span"
}
```

authors: kimjune01
