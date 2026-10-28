---
what: "`internal_metrics_cardinality_total` counter metric"
deprecated_since: "0.24.0"
---

The `internal_metrics_cardinality_total` counter emitted by the `internal_metrics` source is
deprecated. Use the `internal_metrics_cardinality` gauge instead.

The registry can expire metrics, so a counter cannot report the current number of metrics in it.
The value of `internal_metrics_cardinality_total` is inaccurate when `expire_metrics_secs` is set.

Migrate any dashboards or alerts that reference:

- `internal_metrics_cardinality_total`

to:

- `internal_metrics_cardinality`
