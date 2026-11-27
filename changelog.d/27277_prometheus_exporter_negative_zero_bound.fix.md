The `prometheus_exporter` sink no longer exposes the same histogram or summary twice when its buckets or quantiles arrive once as `-0.0` and once as `0.0`. Both are now treated as one series.

authors: VanshajPoonia
