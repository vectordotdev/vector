Fixed a deadlock in metric sinks that use a disk buffer where the sink would permanently
stall after 10-15 minutes of operation. Affected sinks include `prometheus_remote_write`,
`datadog_metrics`, `influxdb_metrics`, `aws_cloudwatch_metrics`, `gcp_stackdriver_metrics`,
`appsignal`, `sematext`, `statsd`, and `greptimedb`.

Additionally fixed a panic when `expire_metrics_secs: 0` was set.

authors: GreyLilac09
