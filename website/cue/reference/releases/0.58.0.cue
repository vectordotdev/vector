package metadata

releases: "0.58.0": {
	date: "2026-08-26"

	whats_next: []

	changelog: [
		{
			type: "enhancement"
			description: #"""
				Added common internal HTTP metrics to the connector used by AWS sinks.
				"""#
			pr_numbers: [25508]
			contributors: ["gwenaskell"]
		},
		{
			type: "fix"
			description: #"""
				Propagate FIPS endpoint setting to STS AssumeRole clients. When `AWS_USE_FIPS_ENDPOINT=true` is configured, Vector now correctly uses FIPS endpoints for STS operations (e.g., `sts-fips.<region>.amazonaws.com`) in addition to the primary service client.
				"""#
			pr_numbers: [25232]
			contributors: ["hligit"]
		},
		{
			type: "fix"
			description: #"""
				Allow explicit null values in JSON and YAML configuration files to load without being converted
				through TOML.
				"""#
			pr_numbers: [25981]
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: #"""
				The `azure_blob` sink now supports a `tags` option, which sets [blob index tags](https://learn.microsoft.com/azure/storage/blobs/storage-blob-index-how-to) (`x-ms-tags`) on every created blob (parity with the `tags` option on the `aws_s3` sink).
				
				The `azure_blob` sink now supports a `metadata` option, which sets [custom blob metadata](https://learn.microsoft.com/rest/api/storageservices/set-blob-metadata) (`x-ms-meta-*`) on every created blob (parity with the `metadata` option on the `gcp_cloud_storage` sink).
				"""#
			pr_numbers: [25545]
			contributors: ["danielku15"]
		},
		{
			type: "fix"
			description: #"""
				The `redis` source configured with `data_type = "channel"` now automatically reconnects and re-subscribes after the Redis connection drops (for example, on a Redis restart or a transient network blip), instead of silently stopping until Vector is restarted. Reconnect attempts use exponential backoff (capped at 30s) and emit `component_errors_total` on failures and `connection_established_total` on recovery.
				"""#
			pr_numbers: [25892]
			contributors: ["gibranbadrul"]
		},
		{
			type: "enhancement"
			description: #"""
				The `prometheus_exporter` sink's `flush_period_secs` option now accepts `0` to disable metric
				expiration. Previously, metrics with sparse or bursty updates (for example, high
				cardinality counters produced by `log_to_metric`) could be expired and re-added as a "new"
				series, causing gaps and apparent counter resets in downstream Prometheus queries even with a
				large `flush_period_secs` configured. Setting `flush_period_secs: 0` keeps all previously seen
				metrics for the lifetime of the sink; be aware this can result in unbounded memory growth if
				metric series cardinality is unbounded.
				"""#
			pr_numbers: [26041]
			contributors: ["valerypetrov"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a deadlock in metric sinks that use a disk buffer where the sink would permanently
				stall after 10-15 minutes of operation. Affected sinks include `prometheus_remote_write`,
				`datadog_metrics`, `influxdb_metrics`, `aws_cloudwatch_metrics`, `gcp_stackdriver_metrics`,
				`appsignal`, `sematext`, `statsd`, and `greptimedb`.
				
				Additionally fixed a panic when `expire_metrics_secs: 0` was set.
				"""#
			pr_numbers: [25214]
			contributors: ["GreyLilac09"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the OTLP decoder adding an extra `timestamp` field to trace events when `log_namespace` was set to `legacy`. Trace events don't have a `timestamp` field in their schema, so this field showed up unexpectedly:
				
				```json
				// Before
				{
				  "trace_id": "...",
				  "span_id": "...",
				  "name": "test_span",
				  "timestamp": "2026-08-06T12:00:00Z"
				}
				
				// After
				{
				  "trace_id": "...",
				  "span_id": "...",
				  "name": "test_span"
				}
				```
				"""#
			pr_numbers: [25404]
			contributors: ["kimjune01"]
		},
		{
			type: "feat"
			description: #"""
				Added cuckoo filter support for the `memory` enrichment table to provide an efficient way to store and check for the presence of keys with a low memory footprint, at the cost of false positives.
				"""#
			pr_numbers: [25143]
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: #"""
				Added bloom filter support for `memory` enrichment table, similar to cuckoo filter, providing a simple and efficient way to store and check the presence of keys with a low memory footprint at the cost of false positives, but with fewer features than cuckoo filter.
				"""#
			pr_numbers: [25154]
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: #"""
				The `datadog_agent` source now accepts LLM Observability (LLMObs) telemetry at `/api/v2/llmobs`. When `multiple_outputs` is enabled, LLMObs span events are available as log events on the `llmobs` output port.
				"""#
			pr_numbers: [25636]
			contributors: ["ronitanilkumar"]
		},
		{
			type: "enhancement"
			description: #"""
				The `character_delimited` and `newline_delimited` decoders now support truncating oversized frames.
				A new `oversized_action`configuration option allows choosing between `drop` (default, existing behavior) and `truncate`.
				When `oversized_action` is set to `truncate`, frames that exceed the configured `max_length` are truncated to the maximum
				allowed size, and the remainder of the oversized frame is discarded up to the next delimiter.
				"""#
			pr_numbers: [25567]
			contributors: ["vparfonov"]
		},
		{
			type: "enhancement"
			description: #"""
				The `kubernetes_logs` source now supports truncating oversized merged log lines instead of
				dropping them. A new `max_merged_line_action` configuration option allows choosing between
				`drop` (default, existing behavior) and `truncate`. When truncation is enabled, lines exceeding
				`max_merged_line_bytes` are truncated to the limit with a `..TRUNCATED` suffix appended.
				
				In `drop` mode, `max_line_bytes` is capped to `max_merged_line_bytes` to avoid wasted I/O.
				In `truncate` mode, individual lines up to `max_line_bytes` are allowed through so the merger
				can truncate the combined result. Note that `max_line_bytes` still applies at the file level
				and always drops individual lines exceeding it; file-level truncation is not yet supported.
				"""#
			pr_numbers: [25567]
			contributors: ["vparfonov"]
		},
		{
			type: "fix"
			description: #"""
				The `mqtt` sink and source now honor the configured `tls.alpn_protocols` option instead of always advertising the hardcoded `mqtt` ALPN protocol. This allows connecting to endpoints that require a specific ALPN protocol name, such as AWS IoT Core over port 443 which requires `x-amzn-mqtt-ca`. When `tls.alpn_protocols` is not set, the previous `mqtt` default is preserved.
				"""#
			pr_numbers: [25807]
			contributors: ["frank-hivewatch"]
		},
		{
			type: "fix"
			description: #"""
				Fixed generated configuration schemas for overlapping untagged enum variants, allowing values accepted by `serde` to validate correctly.
				"""#
			pr_numbers: [25828]
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				`vector validate --no-environment` now catches sink confinement issues that previously
				only surfaced when Vector booted.
				
				For example, a Kafka sink with an unconfined topic template:
				
				```yaml
				sinks:
				  kafka_out:
				    type: kafka
				    inputs: [logs]
				    bootstrap_servers: "localhost:9092"
				    topic: "{{ topic }}"
				    encoding:
				      codec: json
				```
				
				This configuration previously passed `vector validate --no-environment` and only failed with a full `vector validate` or when running the config. It now fails validation with a confinement error due to `topic` having no confinement base.
				
				```yaml
				sinks:
				  kafka_out:
				    type: kafka
				    inputs: [logs]
				    bootstrap_servers: "localhost:9092"
				    topic: "events-{{ topic }}"
				    encoding:
				      codec: json
				```
				"""#
			pr_numbers: [26177]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Allows hyphens in the `<backend name>` portion of the `SECRET[<backend name>.<secret name>]` collector regex. Before, a backend name containing a hyphen (e.g., `my-backend`) would fail to match, leaving the literal `SECRET[...]` string in the resolved config instead of the secret value.
				"""#
			pr_numbers: [25896]
			contributors: ["maklean"]
		},
		{
			type: "fix"
			description: #"""
				Added missing "httpProtocol" field to dnstap source events.
				"""#
			pr_numbers: [25887]
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "enhancement"
			description: #"""
				Dynamic `loki` sink label and structured metadata values no longer require a literal prefix or
				`dangerously_allow_unconfined_template_resolution`. Template keys and tenant IDs remain confined.
				"""#
			pr_numbers: [26176]
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: #"""
				Dynamic `aws_cloudwatch_logs` sink `stream_name` templates no longer require a literal prefix or
				`dangerously_allow_unconfined_template_resolution`. `group_name` remains confined.
				"""#
			pr_numbers: [26188]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Reduce transforms using the `sum` merge strategy now return an error instead of panicking when
				floating-point addition would produce NaN.
				"""#
			pr_numbers: [26022]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				The `sematext_logs` sink no longer panics when the `token` cannot be parsed as a template (e.g., `{{ }}`). It now fails configuration validation with a clear error instead of crashing at startup.
				"""#
			pr_numbers: [26180]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Custom auth VRL enrichment (`%field` writes) is now supported by all HTTP-based sources
				(`http_server`, `heroku_logs`, `prometheus_pushgateway`, `prometheus_remote_write`), not just
				a subset. Enrichment fields are inserted into event metadata under `http_server.<field>` in the
				Vector namespace, or into the event body in the legacy namespace, without overwriting existing
				fields.
				"""#
			pr_numbers: [25935]
			contributors: ["petere-datadog"]
		},
		{
			type: "enhancement"
			description: #"""
				The `aws_s3` source can now retrieve objects from S3 Requester Pays buckets by setting
				`request_payer: requester`.
				"""#
			pr_numbers: [26028]
			contributors: ["vibe"]
		},
		{
			type: "enhancement"
			description: #"""
				The `ssekms_key_id` option in the `aws_s3` sink now respects the configured timezone when the
				value is a template containing time components, matching the existing behavior of `key_prefix`.
				"""#
			pr_numbers: [26141]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fix Azure Blob Storage uploads larger than 4 MiB when using an account-key connection string. These uploads could send all data blocks successfully but fail with a 403 while completing the upload because the final request's body length was missing during Shared Key signing. Vector now sets the body length before signing that request.
				"""#
			pr_numbers: [25978]
			contributors: ["ArunPiduguDD"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "`azure_monitor_logs` sink removed"
			anchor:   "azure-monitor-logs-sink-removed"
			description: #"""
				The deprecated `azure_monitor_logs` sink has been removed. Configurations using it now fail
				validation. Microsoft ends support for the sink's underlying Data Collector API in September
				2026.
				"""#
			pr_numbers: [26152]
			contributors: ["pront"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Legacy buffer metrics removed"
			anchor:   "legacy-buffer-metrics-removed"
			description: #"""
				The deprecated `buffer_byte_size` and `buffer_events` gauge metrics have been removed.
				"""#
			pr_numbers: [26153]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Fixed two problems with the `chunked_gelf` framing decoder's limits. `pending_messages_limit` was applied to every chunk rather than only to new messages, so once the limit was reached even chunks of messages already pending were rejected and those messages could never complete. Separately, dropping a message for exceeding `max_length` left its timeout task running, so `pending_messages_limit` bounded the pending message count but not the number of live tasks.
				"""#
			pr_numbers: [26162]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a panic in the `chunked_gelf` framing decoder when a one-byte message arrived with trace-level logging enabled, which caused the source to fail. Such a message is now passed on for the decoder to reject, as it would any other malformed payload.
				"""#
			pr_numbers: [26162]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Improve configuration error messages by including the affected field path. For example, consider the following invalid configuration:
				
				```yaml
				sources:
				  broken:
				    type: demo_logs
				    interval: not-a-number
				```
				
				This configuration now reports `sources.broken: invalid type: string "not-a-number", expected f64`.
				"""#
			pr_numbers: [25981]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Encode `resource.<type>` metric tags as resources when using a V2 `datadog_metrics`
				sink, preserving Datadog Agent resources such as `database_instance`.
				"""#
			pr_numbers: [25973]
			contributors: ["tessneau"]
		},
		{
			type: "fix"
			description: #"""
				After a crash, affected `disk_v2` buffers could incorrectly appear full, block new events, and stall recovery. Vector now restores buffer usage correctly on restart so the pipeline can continue processing.
				"""#
			pr_numbers: [25845]
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a `disk_v2` buffer bug where a record too large to write (one whose encoded size exceeds the buffer's maximum record size) caused the buffer writer to return an unrecoverable error, which tore down the entire Vector topology and stopped the process. The writer now drops just that record and continues: the record's finalizers are resolved with the default `Dropped` status (which sources with end-to-end acknowledgement treat as `Delivered`, so they ack or checkpoint rather than redelivering the un-writable record), an error is logged, and the drop is counted via the `buffer_discarded_events_total` and `buffer_discarded_bytes_total` metrics (with `intentional="false"`). Every other record and the buffer itself are unaffected.
				"""#
			pr_numbers: [25795]
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: #"""
				Prevent disk buffers from stalling by publishing flushed writer progress before notifying readers.
				"""#
			pr_numbers: [25872]
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: #"""
				Fix deserialization of `aggregated_histogram` bucket `upper_limit` when the value is provided as
				an integer instead of a float.
				"""#
			pr_numbers: [26031]
			contributors: ["dd-sebastien-lb"]
		},
		{
			type: "enhancement"
			description: #"""
				`gcp_stackdriver_logs` label templates are now unconfined; they were previously overly constrained.
				"""#
			pr_numbers: [25976]
			contributors: ["pront"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "HTTP server `encoding` option removed"
			anchor:   "http-server-encoding-removed"
			description: #"""
				The deprecated `encoding` option has been removed from the `http_server` source
				and its deprecated `http` alias. Configurations using it now fail validation.
				"""#
			pr_numbers: [26082]
			contributors: ["pront"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "`influxdb_logs` sink `namespace` option removed"
			anchor:   "influxdb-logs-namespace-removed"
			description: #"""
				The deprecated `namespace` option has been removed from the `influxdb_logs` sink. It has been
				deprecated since v0.24.0 in favor of `measurement`. Configurations using it now fail validation.
				"""#
			pr_numbers: [26108]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				The `influxdb_logs` and `influxdb_metrics` sinks now accept a `version` field to select the
				InfluxDB API version whose settings are used. When unset, the version is inferred from the
				configured settings, matching the previous behavior. The `version` field will be required in a
				future release.
				"""#
			pr_numbers: [26093]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: #"""
				The `kafka` source now supports a `decompression` option for decompressing message payloads that
				were compressed by the producer at the application level (as opposed to Kafka protocol-level
				compression, which is handled transparently by the client library). Supported algorithms are
				`gzip`, `zlib`, and `zstd`, and zstd decompression supports custom dictionaries via
				`dictionary_path`. Payloads are decompressed before framing and decoding are applied.
				"""#
			pr_numbers: [26024]
			contributors: ["cjford"]
		},
		{
			type: "fix"
			description: #"""
				The `kubernetes_logs` source now falls back to extracting pod metadata from the log file path when the pod is not found in the Kubernetes API store. Previously, if the pod was deleted before Vector could look it up, the event was sent downstream with no kubernetes metadata at all, causing errors in downstream transforms that expect fields like `pod_namespace` to be present.
				
				On this fallback path, Vector still populates `pod_name`, `pod_namespace`, and `container_name`. The path segment that is usually a Pod UID is exposed as `pod_log_directory_id` (not `pod_uid`), because for static pods it can be a config hash instead of the API Pod UID. Users who want UID semantics can remap the field.
				"""#
			pr_numbers: [25834]
			contributors: ["vparfonov"]
		},
		{
			type: "enhancement"
			description: #"""
				Explicitly log when components are gracefully shut down.
				"""#
			pr_numbers: [25974]
			contributors: ["clementd-dd"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "`logdna` sink alias removed"
			anchor:   "logdna-sink-alias-removed"
			description: #"""
				The deprecated `logdna` sink alias has been removed. It was renamed to `mezmo` in v0.29.0.
				Configurations using `type: logdna` now fail validation.
				"""#
			pr_numbers: [26114]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "security"
			description: #"""
				The `logstash` source no longer runs out of memory and crashes on rare problematic compressed frames, such as one containing an extremely large number of events. Additionally, events received before a malformed frame inside a compressed payload are now delivered instead of being silently discarded along with the malformed frame.
				"""#
			pr_numbers: [26129]
			contributors: ["pront"]
		},
		{
			type: "security"
			description: #"""
				The `logstash` source now rejects frames larger than the configured maximum instead of buffering them indefinitely; previously, a sender could declare an extremely large frame and force the source to hold its bytes in memory until it ran out of memory. The limit is controlled by the existing `--max-decompressed-size-bytes` option (default 100 MiB).
				"""#
			pr_numbers: [26129]
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: #"""
				The `metric_tag_values` option now accepts an `auto` value that exposes single-value tags
				as strings and multi-value tags as arrays, preserving the underlying shape of each tag
				instead of forcing every tag into one form. The `lua` transform continues to support only
				`single` and `full`.
				"""#
			pr_numbers: [25376]
			contributors: ["kaarolch"]
		},
		{
			type: "feat"
			description: #"""
				The OTLP codec's serializer now supports native Vector `Metric` events for `Counter`, `Gauge`, `AggregatedHistogram`, and `AggregatedSummary` values, converting them into the OTLP `Sum`, `Gauge`, `Histogram`, and `Summary` protobuf types respectively. Previously, encoding a native metric event with the OTLP serializer always failed.
				"""#
			pr_numbers: [25738]
			contributors: ["petere-datadog"]
		},
		{
			type: "fix"
			description: #"""
				Fixed an issue where unusually deeply nested event data or metadata could make disk buffers unreadable or cause vector-to-vector pipelines to retry indefinitely. Vector now detects affected events before buffering or sending while leaving safely nested events unchanged. When when_full = "overflow" is configured, the original event is routed intact to the overflow stage regardless of buffer occupancy; otherwise, only the affected event is dropped.
				"""#
			pr_numbers: [26099]
			contributors: ["connoryy", "ganelo", "EricaJ6", "jonodera97"]
		},
		{
			type: "security"
			description: #"""
				Prevent configured HTTP proxy credentials from being sent in the `Authorization` header to origin servers for plaintext `http://` requests; they are now sent only in `Proxy-Authorization`.
				"""#
			pr_numbers: [26095]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the `vector_security_confinement_disabled` internal metric disappearing after the metric idle timeout (300 seconds by default) while a sink was still running with `dangerously_allow_unconfined_template_resolution` enabled. The gauge is now owned by the topology and held for the lifetime of each sink, and refreshed on configuration reload, so alerts watching this metric no longer silently stop firing.
				"""#
			pr_numbers: [25910]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Sink `endpoint` options now require an absolute URL that includes a host. Endpoints without a scheme are defaulted to `https://` (for example `endpoint: "localhost:8080"` becomes `https://localhost:8080`). Previously, partial or empty endpoints (for example `endpoint: ""` or `endpoint: "localhost:8080"` without a scheme) were accepted at configuration load and only failed when the sink attempted to send data, or were silently completed with a default scheme and host. Empty, host-less, or non-`http(s)` endpoints (for example `endpoint: ""`, `endpoint: "/path"`, or `endpoint: "ftp://example.com"`) are now rejected at configuration load with a clear error, including with `vector validate --no-environment`. This affects the `appsignal`, `azure_logs_ingestion`, `datadog_events`, `datadog_logs`, `datadog_metrics`, `datadog_traces`, `elasticsearch`, `gcp_cloud_storage`, `gcp_pubsub`, `gcp_stackdriver_logs`, `gcp_stackdriver_metrics`, `honeycomb`, `humio`, `influxdb`, `loki`, `prometheus_remote_write`, `sematext`, `splunk_hec`, and `webhdfs` sinks.
				"""#
			pr_numbers: [26195, 26177, 26180, 26152, 26130, 26116]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: #"""
				Added an optional `tls_handshake_timeout_secs` setting to the `socket` (TCP mode), `syslog` (TCP mode), `logstash`, `fluent`, and `statsd` (TCP mode) sources. When set, a TLS-enabled connection that does not complete its TLS handshake within the configured number of seconds is closed.
				
				Previously, TLS handshakes on these sources had no timeout: a client that opened a TCP connection and never completed (or never started) the TLS handshake would hold its slot against `connection_limit` indefinitely, since neither TCP keepalive nor `max_connection_duration_secs` are evaluated until after the handshake succeeds. This could let misbehaving or unresponsive clients gradually exhaust the connection limit and block legitimate traffic. The new setting is unset by default, preserving prior behavior.
				"""#
			pr_numbers: [26126]
			contributors: ["vladimir-dd"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the `tls.server_name` option so that it is used for certificate hostname verification in addition to SNI. Previously, on the OpenSSL path (used by HTTP-based sinks such as `datadog_logs`), the certificate was still verified against the connection URL host, causing a "hostname mismatch" verification failure when `server_name` differed from the endpoint host. The override applies only to the upstream destination, so an HTTPS forward proxy's own certificate is verified against the proxy host.
				"""#
			pr_numbers: [25899, 25881]
			contributors: ["gwenaskell"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "URI template field references inside the authority are rejected"
			anchor:   "uri-template-partial-authority"
			description: #"""
				Vector now refuses to build configs where a `{{ field }}` reference lands inside
				the hostname (or immediately adjacent to it without a path separator). Previously,
				such templates built successfully but silently dropped every event at render time.
				"""#
			pr_numbers: [25886]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fix a panic in Vector when a sink endpoint or URI contains a non-numeric port (e.g. `http://localhost:notaport`). Malformed URIs now produce a validation error instead of crashing Vector.
				"""#
			pr_numbers: [26083]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a framing bug in the `varint_length_delimited` decoder that corrupted streams whenever a
				frame was split across two reads from the underlying source. The decoder consumed the varint length
				prefix before confirming that the whole frame had been buffered, so a partial frame permanently
				desynchronized the stream: the first byte of the payload was then misread as the next length
				prefix. Any source using `framing.method = "varint_length_delimited"` was affected once the stream
				exceeded the 8 KiB read buffer, unless the frame size happened to tile that buffer exactly.
				"""#
			pr_numbers: [26169]
			contributors: ["meirdev"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "`webhdfs` sink defaults endpoints to `https://`"
			anchor:   "webhdfs-sink-defaults-endpoints-to-https"
			description: #"""
				The `webhdfs` sink's `endpoint` option now defaults a missing scheme to `https://` instead of
				`http://`. A scheme-less endpoint (for example `endpoint: "127.0.0.1:9870"`) still loads and
				remains valid, but it now resolves to `https://127.0.0.1:9870`; previously the underlying
				WebHDFS client resolved to `http://127.0.0.1:9870`
				"""#
			pr_numbers: [26177]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				The `databricks_zerobus` sink now has OTel v2 compatibility.
				"""#
			pr_numbers: [25831]
			contributors: ["yorickvanzweeden"]
		},
	]

	vrl_changelog: #"""
		### [0.35.0 (2026-08-20)](https://github.com/vectordotdev/vrl/releases/tag/v0.35.0)
		
		#### Enhancements
		
		- The `parse_aws_vpc_flow_log` function now recognizes all fields introduced in AWS VPC Flow Logs versions 7 through 11, including the v7 ECS metadata fields, v8 `reject_reason`, v9 `resource_id`, v10 `encryption_status`, and the v11 tag/interface/next-hop fields.
		
		Thanks to [avestuk](https://github.com/avestuk) for contributing PR [#1879](https://github.com/vectordotdev/vrl/pull/1879)!
		
		#### Fixes
		
		- Fixed `round`'s type definition, which previously always claimed to return an integer even though it returns a float for float inputs.
		
		Thanks to [thomasqueirozb](https://github.com/thomasqueirozb) for contributing PR [#1862](https://github.com/vectordotdev/vrl/pull/1862)!
		- Prevent float arithmetic that produces NaN from panicking or silently returning zero. Such operations now return a runtime error.
		
		Thanks to [pront](https://github.com/pront) for contributing PR [#1890](https://github.com/vectordotdev/vrl/pull/1890)!
		
		"""#
}
