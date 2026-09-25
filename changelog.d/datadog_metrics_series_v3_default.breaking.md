# Datadog metrics series submitted to the V3 intake by default {#datadog-metrics-series-v3-default}

## Summary

The `datadog_metrics` sink now submits series metrics to `/api/intake/metrics/v3/series` by
default, using Datadog's columnar protobuf format. This format uses dictionary-based string
deduplication and delta encoding, making it more efficient than `v2` for workloads with many
metrics that share common names or tags.

Sketch metrics (distributions and histograms) are unaffected and continue to be submitted to
`/api/beta/sketches`.

## Migration

No configuration change is required when sending directly to Datadog's managed intake.

If your `datadog_metrics` sink forwards to another Vector instance's `datadog_agent`
source, the receiving instance must support V3 before the sender switches to the new
default. Upgrading the sender first causes series metric requests to fail.

Upgrade receiving instances first, or explicitly configure sending sinks to continue
using V2 until all receivers support V3. The same workaround applies to proxies or
other endpoints that do not accept the V3 intake route:

```yaml
sinks:
  my_sink:
    type: datadog_metrics
    series_api_version: v2
```

authors: stephenwakely
