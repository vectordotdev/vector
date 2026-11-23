Added a `prometheus_text` encoding codec that encodes metrics in the [Prometheus text exposition format](https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format). It lets sinks such as `kafka`, `file`, `http` and `socket` write metrics in a format that Prometheus-compatible tools can read, which the `text` codec could not do because it only resembles that format.

Every event is encoded on its own, as a `# TYPE` line followed by one line per series. Only absolute counters, gauges, sets, aggregated histograms and aggregated summaries are supported. Other metrics, such as incremental metrics (use the `incremental_to_absolute` transform first), distributions and sketches, are rejected with an error.

authors: valerypetrov
