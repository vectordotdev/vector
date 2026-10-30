---
what: "`internal_metrics_cardinality_total` counter metric"
deprecated_since: "0.24.0"
---

The `internal_metrics_cardinality_total` counter emitted by the `internal_metrics` source is
deprecated. Use the `internal_metrics_cardinality` gauge instead.

Both metrics report the same value: the current number of metrics in the registry. When
`expire_metrics_secs` is set, the registry expires metrics and this value can decrease. A counter
must not decrease, so consumers that expect counter semantics can misinterpret
`internal_metrics_cardinality_total`.

Migrate any dashboards or alerts that reference:

- `internal_metrics_cardinality_total`

to:

- `internal_metrics_cardinality`
