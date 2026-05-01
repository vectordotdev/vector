The `splunk_hec` source now accepts optional per-endpoint codec configuration via `event: { framing, decoding }` and `raw: { framing, decoding }`. When `decoding` is set on an endpoint, Vector applies a second decoding pass after the HEC envelope is parsed: on `/services/collector/event` the envelope's `event` field is fed through the codec, and on `/services/collector/raw` the request body is fed through the codec directly. A single payload can fan out to multiple events.

For example, to decode JSON payloads in `/event` requests while splitting `/raw` bodies on newlines:

```yaml
sources:
  hec:
    type: splunk_hec
    address: 0.0.0.0:8088
    event:
      decoding:
        codec: json
    raw:
      framing:
        method: newline_delimited
      decoding:
        codec: bytes
```

authors: thomasqueirozb
