Added `path` configuration option to `prometheus_remote_write` source to allow accepting metrics on custom URL paths instead of only the root path. This enables configuration of endpoints like `/api/v1/write` to match standard Prometheus remote write conventions.

authors: elohmeier
