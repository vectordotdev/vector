The `prometheus_remote_write` source now defaults to ignoring conflicting metric metadata to align with Prometheus/Thanos behavior, whereas it previously returned an HTTP 400 error. This is now configurable via the `metadata_conflicts` option. Set it to `reject` to restore the previous behavior.

authors: elohmeier
