Sinks that use `{{ field }}` references in routing templates now require a
literal prefix in the template so Vector can enforce a routing boundary. For
example, `key_prefix: "{{ host }}/"` is no longer accepted at startup because
there is no fixed leading segment.

The `file` sink additionally gains a `base_dir` config field to set the
confinement root explicitly when the `path` template has no usable literal
prefix.

Affected sinks: `aws_s3`, `azure_blob`, `gcp_cloud_storage`, `webhdfs`,
`file`, `elasticsearch`, `kafka`, `http`, `splunk_hec_logs`,
`splunk_hec_metrics`, `loki`, `clickhouse`, `redis`, `amqp`, `pulsar`,
`mqtt`, `nats`, `greptimedb_logs`, `aws_cloudwatch_logs`,
`gcp_stackdriver_logs`, `prometheus_remote_write`.

**To migrate:** please add a fixed prefix to the template, e.g. `key_prefix: "logs-{{ host }}/"`.

**To restore previous behavior:** set `dangerously_allow_unconfined_template_resolution: true` on the affected sink.

authors: pront
