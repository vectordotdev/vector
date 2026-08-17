Histograms from sources that report no sum are no longer treated as having a sum of zero. OTLP makes the field optional, and OpenMetrics only recommends the `_sum` series, so this affects the `opentelemetry` and `prometheus_*` sources. Sinks now omit the sum rather than publishing a zero, and the sketch conversion used by `datadog_metrics` keeps its bucket-derived estimate.

Two behavior changes to note: the `prometheus_exporter` and `prometheus_remote_write` sinks no longer emit a `<name>_sum` series for these histograms, and a `lua` transform sees `aggregated_histogram.sum` as `nil`.

authors: gwenaskell
