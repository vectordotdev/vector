# `internal_metrics` emits incremental counters and histograms {#internal-metrics-increments}

## Summary

The `internal_metrics` source now emits counters and histograms as `Incremental` metrics holding the
change since the previous scrape, rather than as `Absolute` metrics holding the value accumulated
since Vector started. Gauges are unaffected, as is `internal_metrics_cardinality_total`.

This fixes internal histograms being dropped entirely when all of their observations fall within a
single scrape interval. The source also scrapes once while being built, so adding it to an
already-running Vector reports the change since that point rather than the whole registry.

## Migration

Sinks are unaffected. Adjust any transform that reads the metric kind of internal counters or
histograms, such as a `remap` gating on the kind or a consumer of `metric_to_log` output reading the
`kind` field: these now see `incremental` where they previously saw `absolute`.

authors: gwenaskell
