The `prometheus_remote_write` source now supports configurable handling of conflicting metric metadata via the `metadata_conflicts` option. By default, it continues to reject requests with conflicting metadata (HTTP 400 error) to maintain backwards compatibility. Set it to `ignore` to align with Prometheus/Thanos behavior, which silently ignores metadata conflicts.

authors: elohmeier
