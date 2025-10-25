The `prometheus_remote_write` sink now supports custom HTTP headers via the `request.headers` configuration option. This allows users to add custom headers to outgoing requests, which is useful for authentication, routing, or other integration requirements with Prometheus-compatible backends.

authors: elohmeier
