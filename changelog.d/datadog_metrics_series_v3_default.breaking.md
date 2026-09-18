# Datadog metrics series submitted to the V3 intake by default {#datadog-metrics-series-v3-default}

## Summary

The `datadog_metrics` sink now submits series metrics to `/api/intake/metrics/v3/series` by
default, using Datadog's columnar protobuf format. This format uses dictionary-based string
deduplication and delta encoding, making it more efficient than `v2` for workloads with many
metrics that share common names or tags.

Sketch metrics (distributions and histograms) are unaffected and continue to be submitted to
`/api/beta/sketches`.

## Migration

No configuration change is required for Datadog's managed intake. To keep submitting series to
the previous endpoint — for example when sending to a proxy or agent that doesn't accept the V3
intake route — set `series_api_version` explicitly:

```yaml
sinks:
  my_sink:
    type: datadog_metrics
    series_api_version: v2
```

authors: stephenwakely
