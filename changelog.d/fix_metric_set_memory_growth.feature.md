- Add a TTL-based cache for metrics sets
- Add `expire_metrics_secs` config for Prometheus remote write sink which uses the TTL-based cache
- This fixes an issue where incremental metrics are preserved for the lifetime of Vector's runtime, which causes
  indefinite memory growth

authors: GreyLilac09