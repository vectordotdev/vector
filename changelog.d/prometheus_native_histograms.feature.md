Added support for Prometheus native histograms (sparse/exponential histograms).

The `prometheus_remote_write` source now accepts native histograms sent via the
Prometheus remote write protocol, preserving their full resolution and sparse
bucket representation. The `prometheus_remote_write` sink emits native histograms
directly, enabling lossless pass-through of native histogram data between
Prometheus-compatible systems.

For sinks that do not natively support this format (such as the
`prometheus_exporter` text exposition format, InfluxDB, GreptimeDB, and Datadog),
native histograms are automatically converted to classic aggregated histograms.
This conversion is lossy but allows existing pipelines to continue operating.

authors: l1n
