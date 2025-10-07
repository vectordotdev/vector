The `prometheus_remote_write` source now has a `metadata_conflicts` option so you can determine how to handle conflicting metric metadata. By default, it continues to reject requests with conflicting metadata (HTTP 400 error) to maintain backwards compatibility. Set `metadata_conflicts` to `ignore` to align with Prometheus/Thanos behavior, which silently ignores metadata conflicts.

authors: elohmeier
