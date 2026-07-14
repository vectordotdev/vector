Sinks that use `{{ field }}` references in routing templates now require a
literal prefix so Vector can enforce a routing boundary. For example,
`key_prefix: "{{ host }}/"` is no longer accepted at startup because there
is no fixed leading segment. Events whose rendered value falls outside that
boundary are dropped.

The `file` sink additionally gains a `base_dir` config field to set the
confinement root explicitly when the `path` template has no usable literal
prefix.

Affected sinks: `aws_s3`, `azure_blob`, `gcp_cloud_storage`, `webhdfs`,
`file`, `elasticsearch`, `kafka`, `http`, `splunk_hec_logs`,
`splunk_hec_metrics`, `humio_logs`, `humio_metrics`, `loki`, `clickhouse`,
`redis`, `amqp`, `pulsar`, `mqtt`, `nats`, `greptimedb_logs`,
`aws_cloudwatch_logs`, `gcp_stackdriver_logs`, `prometheus_remote_write`.

Two new metrics track enforcement:

- `component_errors_total{error_type="condition_failed"}` — increments on every confinement violation; use this to alert on routing injection attempts.
- `vector_security_confinement_disabled` — set to `1` for any sink running with confinement disabled; use this to alert when a sink's boundary check is bypassed.

An `ERROR` log line accompanies each violation with sink-specific detail.

**To preserve previous behavior (opt-out):** set `dangerously_allow_unconfined_template_resolution: true` on the affected sink. Vector will route using unvalidated event values and log a warning on startup. `vector_security_confinement_disabled` will be set to `1` for that sink.

**To migrate (recommended):** please add a fixed prefix to the template, e.g. `key_prefix: "logs-{{ host }}/"`. Any event whose rendered value falls outside that prefix is dropped and counted in `component_errors_total{error_type="condition_failed"}`.

authors: pront
