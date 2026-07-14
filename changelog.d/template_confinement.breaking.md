Sinks that accept `{{ field }}` references in routing templates now enforce a
confinement boundary: the rendered value must stay within the literal prefix
declared in the template. Templates with no literal prefix (e.g.
`key_prefix: "{{ host }}/"`) are rejected at startup.

Affected sinks: `aws_s3`, `azure_blob`, `gcp_cloud_storage`, `webhdfs`,
`file`, `elasticsearch`, `kafka`, `http`, `splunk_hec_logs`,
`splunk_hec_metrics`, `humio_logs`, `humio_metrics`, `loki`, `clickhouse`,
`redis`, `amqp`, `pulsar`, `mqtt`, `nats`, `greptimedb_logs`,
`aws_cloudwatch_logs`, `gcp_stackdriver_logs`, `prometheus_remote_write`.

The `file` sink gains a `base_dir` config field to set the confinement root
explicitly when the `path` template has no usable literal prefix.

**URI templates:** templates for HTTP/HTTPS endpoints must not contain `?`.
Any query string in the template — static or dynamic — is rejected at startup.

**Opt-out:** set `dangerously_allow_unconfined_template_resolution: true` on
the affected sink to restore the previous behaviour. Vector logs a warning on
startup and sets `vector_security_confinement_disabled{component_type=...}` to
`1` for that sink.

**Observability:**

- `component_errors_total{error_type="confinement_failed"}` — increments on
  each violation; events that trigger it are dropped.
- `vector_security_confinement_disabled` — set to `1` while a sink is running
  with confinement disabled.

authors: pront
