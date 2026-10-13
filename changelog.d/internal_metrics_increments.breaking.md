# `internal_metrics` emits incremental counters and histograms {#internal-metrics-increments}

## Summary

The `internal_metrics` source now emits counters and histograms as `Incremental` metrics holding the
change since the previous scrape, instead of `Absolute` totals since Vector started. Gauges and
`internal_metrics_cardinality_total` are unchanged.

This fixes data loss in sinks that need incremental metrics, such as `datadog_metrics`: histograms
observed within a single scrape interval were never sent, and counters dropped everything counted
before their first scrape.

## Migration

Transforms that read the metric kind of internal counters or histograms, such as a `remap` or the
`kind` field of `metric_to_log` output, now see `incremental` instead of `absolute`.

With `prometheus_exporter`, internal counters now count from when the `internal_metrics` source
starts, and reset to zero when the sink is rebuilt on reload.

authors: gwenaskell
