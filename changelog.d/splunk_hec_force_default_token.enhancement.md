The `splunk_hec_logs` and `splunk_hec_metrics` sinks now support a `force_default_token` option. When set to `true`, the configured `default_token` is always used for outgoing requests, ignoring any per-event `splunk_hec_token` stored in event metadata (e.g. forwarded by an upstream `splunk_hec` source). This prevents passthrough tokens from overriding a custom output token.

```yaml
sinks:
  splunk_out:
    type: splunk_hec_logs
    endpoint: https://hec.example.com:8088
    default_token: "${SPLUNK_OUTPUT_TOKEN}"
    force_default_token: true
    encoding:
      codec: json
```

authors: taylorchandleryoung