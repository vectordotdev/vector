# `internal_metrics_cardinality_total` counter removed {#internal-metrics-cardinality-total-removed}

## Summary

The `internal_metrics` source no longer emits the `internal_metrics_cardinality_total` counter. It
was deprecated in 0.24.0 in favor of the `internal_metrics_cardinality` gauge, which reports the
same value: the current number of metrics in the registry.

## Migration

Update dashboards and alerts that reference `internal_metrics_cardinality_total` to use
`internal_metrics_cardinality`.

authors: JinchengCao
