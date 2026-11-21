Fix increasing CPU usage and throughput degradation in the `prometheus_remote_write` sink when receiving incremental histogram distributions from `log_to_metric`. Histogram samples are accumulated without retaining an ever-growing per-series history.

authors: phpont
