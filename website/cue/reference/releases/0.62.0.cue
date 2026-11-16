package metadata

releases: "0.62.0": {
	date: "2026-11-16"

	description: #"""
		The [COSE team](https://opensource.datadoghq.com/about/#the-community-open-source-engineering-team) is happy to announce Vector `0.62.0`! 🚀

		Thanks to the 18 contributors who made the 40 changes in this release:

		- [6 breaking changes](/releases/0.62.0/#breaking-changes)
		- [3 new features](/releases/0.62.0/#new-features)
		- [8 enhancements](/releases/0.62.0/#enhancements)
		- [23 bug fixes](/releases/0.62.0/#bug-fixes)

		Upgrading to 0.62? Read the [upgrade guide](/releases/0.62.0/#upgrade-guide).
		"""#

	changelog: [
		{
			type: "fix"
			description: #"""
				Fixed mutual TLS (mTLS) authentication by properly passing client certificates in the `amqp` source and sink.
				
				A configuration that specifies both `tls.crt_file` and a PKCS#12 archive in `tls.key_file` now fails,
				because that setup contradicts the documented TLS configuration.
				For more information, see the [`amqp` source docs](https://vector.dev/docs/reference/configuration/sources/amqp/#tls) and [`amqp` sink docs](https://vector.dev/docs/reference/configuration/sinks/amqp/#tls).
				"""#
			pr_numbers: [27130]
			contributors: ["Captain1653"]
		},
		{
			type: "fix"
			description: #"""
				Fixed an issue in the `amqp` sink where configuring a DER or inline PEM certificate in `tls.ca_file` caused initialization errors.
				"""#
			pr_numbers: [27100]
			contributors: ["Captain1653"]
		},
		{
			type: "fix"
			description: #"""
				The `http_client` source now sends `Accept: application/json` when `decoding.codec` is `json`, `native_json` or `gelf` and the default `bytes` framing is used. Previously it sent `Accept: text/plain`.
				"""#
			pr_numbers: [27147]
			contributors: ["anpawo"]
		},
		{
			type: "fix"
			description: #"""
				Messages that are not valid UTF-8 no longer fail `octet_counting`, the framing the `syslog` source uses, in either its octet-counted or newline-delimited mode. UTF-8 handling is now done by the configured deserializer. `syslog` decoding replaces invalid sequences with U+FFFD by default (`lossy`). Because framing no longer errors, the `syslog` source no longer closes a connection that carried a non UTF-8 message. Previously the framing error ended the connection, losing every message after it on that connection.
				"""#
			pr_numbers: [26993]
			contributors: ["yackushevas"]
		},
		{
			type: "fix"
			description: #"""
				The `log_to_metric` transform with `all_metrics: true` now accepts integer values wherever it expects a float (for example `counter.value`, `gauge.value`, histogram `upper_limit` and `sum`, summary `quantile`, `value` and `sum`). Previously a value such as `42` was rejected with a "Failed to parse field as float" error and the event was dropped, while `42.0` worked.
				"""#
			pr_numbers: [27142]
			contributors: ["anpawo"]
		},
		{
			type: "fix"
			description: #"""
				`vector tap` now exits cleanly when its output pipe is closed, for example `vector tap | head -1`, instead of panicking with "failed printing to stdout: Broken pipe".
				"""#
			pr_numbers: [27146]
			contributors: ["anpawo"]
		},
		{
			type: "feat"
			description: #"""
				Added a `GetCapabilities` RPC to the Vector observability gRPC API. Clients such as `vector top` can call this once at connection time to discover whether allocation tracing is active and the full set of metric names (counters, gauges, histograms) available on the running instance.
				"""#
			pr_numbers: [25427]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed `sum`/`avg` reported by the DDSketch generated when converting an `AggregatedHistogram` (used by the `datadog_metrics` sink, among others). Both were previously reconstructed purely from bucket-boundary interpolation, which breaks down badly when a bucket's true values sit far from its edges. `sum`/`avg` are now taken directly from the histogram's own exact running totals instead (skipped if the buckets don't fully account for the histogram's true count — e.g. Prometheus dropping its cumulative `+Inf` bucket — or if that exact sum is not a finite number).
				"""#
			pr_numbers: [25752]
			contributors: ["vladimir-dd", "gwenaskell"]
		},
		{
			type: "fix"
			description: #"""
				The sketch conversion used by the `datadog_metrics` sink no longer reports a sum and average of zero for a histogram that reports no sum, keeping its bucket-derived estimate instead. Previously such a histogram arrived with a fabricated sum of zero, which the conversion then trusted as exact.
				"""#
			pr_numbers: [25752]
			contributors: ["gwenaskell"]
		},
		{
			type: "fix"
			description: #"""
				The `remap` transform's "Event mapping aborted." debug log now includes the message given to `abort` in its `error` field, so the reason for a dropped event is no longer lost when `reroute_dropped` is not enabled.
				"""#
			pr_numbers: [27148]
			contributors: ["anpawo"]
		},
		{
			type: "fix"
			description: #"""
				The `mqtt` source now subscribes to its topics again after reconnecting when the broker reports that the session is no longer present (for example after a broker restart, an expired session, or a deleted subscription queue). Previously the source stayed connected but silently stopped receiving messages until Vector was restarted.
				"""#
			pr_numbers: [26731]
			contributors: ["florissmit10"]
		},
		{
			type: "fix"
			description: #"""
				An HTTP 200 stream that ends without a `grpc-status` trailer now reports `Unknown` instead of
				looking like normal stream completion. This affects handling of truncated responses, including
				proxy or load-balancer resets.
				"""#
			pr_numbers: [27044]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				The `elasticsearch` and `opensearch` sinks now accept data-stream components without
				their own literal prefix when the complete data-stream name supplies a confinement
				boundary. For example, `type: logs`, `dataset: system`, and
				`namespace: "{{ service }}"` use `logs-system-` as the boundary, including for
				auto-routed destinations. Existing component prefix restrictions and routing
				behavior for previously supported configurations are preserved.
				"""#
			pr_numbers: [27166]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				AWS components now preserve request-body size hints, end-of-stream state, and trailers. This avoids unnecessary HTTP/1.1 chunked encoding for known-length bodies without an explicit `Content-Length` header and removes redundant empty HTTP/2 DATA frames, saving nine bytes per affected HTTP/2 request.
				"""#
			pr_numbers: [27093]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Reduced CPU usage of metric batching in the `aws_cloudwatch_metrics`, `sematext_metrics` and `influxdb_metrics` sinks. Since 0.50.0, these sinks did LRU cache bookkeeping on every metric, but they never evicted metrics from the cache.
				"""#
			pr_numbers: [27128]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				The `aggregate` transform now supports an `event_time.late_events` option to control metrics that arrive after their bucket's cutoff. Setting `strategy` to `forward` sends them downstream unchanged instead of dropping them, and the optional `metric_types` list restricts forwarding to incremental metrics of the listed types (for example `distribution` and `sketch`). The default strategy, `drop`, keeps the existing behavior.
				"""#
			pr_numbers: [27045]
			contributors: ["dd-sebastien-lb"]
		},
		{
			type: "enhancement"
			description: #"""
				Adds multi-value tags support for `log_to_metric` transform when using `all_metrics: true`.
				"""#
			pr_numbers: [27103]
			contributors: ["tchanturia"]
		},
		{
			type: "fix"
			description: #"""
				The `aws_cloudwatch_logs` sink skips the log-group healthcheck for templated
				`group_name` values, avoiding invalid CloudWatch requests during startup and
				`vector validate`.
				"""#
			pr_numbers: [25128]
			contributors: ["iandelahorne"]
		},
		{
			type: "fix"
			description: #"""
				AWS components now send the configured proxy credentials to an HTTP forward proxy for plain `http://` AWS endpoints, for example a custom `endpoint` or the EC2 instance metadata service. Before, these requests reached the proxy without a `Proxy-Authorization` header. HTTPS requests through a `CONNECT` proxy are unchanged.
				"""#
			pr_numbers: [26998]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a delay in sinks that batch events up to `batch.max_bytes`. When an event did not fit in the current batch and filled the next batch by itself, that next batch was sent only after `batch.timeout_secs` if no more events arrived. The batch is now sent immediately.
				
				This affects the `appsignal`, `aws_kinesis_firehose`, `aws_kinesis_streams`, `azure_logs_ingestion`, `databend`, `databricks_zerobus`, `elasticsearch`, `gcp_stackdriver_logs`, `gcp_stackdriver_metrics`, `greptimedb_metrics`, `honeycomb`, `keep`, `new_relic`, `postgres`, `redis`, `statsd`, and `vector` sinks.
				"""#
			pr_numbers: [26958]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: #"""
				The `datadog_agent` source now accepts `apm_stats` (alpha) in `enabled_data_types`, which emits the APM stats sent by the Datadog Agent unmodified on a new `apm_stats` output instead of dropping them. It requires `multiple_outputs` and is not enabled when `enabled_data_types` is omitted.
				"""#
			pr_numbers: [26940]
			contributors: ["antoninferrand"]
		},
		{
			type: "enhancement"
			description: #"""
				The `datadog_agent` source now supports an `enabled_data_types` option that lists the data types (`logs`, `metrics`, `traces`, `llmobs`) the source accepts.
				This replaces the (now deprecated) `disable_logs`, `disable_metrics`, `disable_traces`, and `disable_llmobs` options.
				Please see the [dedicated migration notice](https://vector.dev/deprecations/#datadog_agent-disable_options).
				"""#
			pr_numbers: [27000]
			contributors: ["antoninferrand"]
		},
		{
			type: "enhancement"
			description: #"""
				The `datadog_traces` sink now supports a `disable_apm_stats` option, which stops the sink from computing APM stats from the traces it receives, for example when the Datadog Agent's stats are forwarded separately.
				"""#
			pr_numbers: [26939]
			contributors: ["antoninferrand"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Absent histogram sums are no longer reported as zero"
			anchor:   "histogram-absent-sum"
			description: #"""
				A histogram's sum is now optional, so a source that reports no sum no longer has one fabricated as
				zero. OTLP makes `sum` optional and OpenMetrics only recommends the `_sum` series, so the
				`opentelemetry` and `prometheus_*` sources can produce histograms without one. Downstream the sum is
				omitted rather than reported as zero: the `prometheus_exporter` and `prometheus_remote_write` sinks
				skip the `<name>_sum` series, `influxdb_metrics` omits the `sum` field, `greptimedb_metrics` omits
				the `sum` column, and a `lua` transform sees `aggregated_histogram.sum` as `nil`.
				"""#
			pr_numbers: [25752]
			contributors: ["gwenaskell"]
		},
		{
			type: "fix"
			description: #"""
				The `http_client` and `prometheus_scrape` sources now apply the configured timeout to the whole request, including reading the response body. Before, the timeout only covered sending the request and receiving the response headers, so an endpoint that sent headers and then stalled the body kept the request open indefinitely. Such requests now fail with a timeout error.
				"""#
			pr_numbers: [27139]
			contributors: ["JinchengCao"]
		},
		{
			type: "fix"
			description: #"""
				The `clickhouse`, `gcp_stackdriver_logs`, `gcp_stackdriver_metrics`, `greptimedb_logs`, `honeycomb`, and `keep` sinks again send `Accept-Encoding: zstd,gzip,deflate,br` with their event requests and decompress compressed responses. Healthcheck requests are unchanged and still send `Accept-Encoding: identity`.
				
				`Accept-Encoding` sent with event requests:
				
				| 0.58.0 and earlier | 0.59.0 until this release | This release |
				| --- | --- | --- |
				| `zstd,gzip,deflate,br` | `identity` | `zstd,gzip,deflate,br` |
				
				Response-body transport failures remain retriable for both compressed and uncompressed responses. Malformed compressed responses are not retried.
				"""#
			pr_numbers: [27094]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				HTTP-based sinks now accept UTF-8 characters in the path or query of their endpoint URI, for example `https://example.com/café?tag=é`. Before, such an endpoint failed with `invalid uri character`. Hosts must still be ASCII, so use the punycode form of an internationalized domain name.
				
				The same change applies to the endpoints of the `http_client`, `prometheus_scrape`, `apache_metrics`, `nginx_metrics`, `eventstoredb_metrics` and `aws_ecs_metrics` sources, and to the `endpoint` and `tags` options of the `aws_ec2_metadata` transform. The `http_client` and `prometheus_scrape` sources percent-encode the query string before they send the request.
				
				This does not apply to the `http` sink's `uri`, the `opentelemetry` sink's `protocol.uri`, or the `axiom` sink's `url`. Vector treats these as templates, and they must still contain only ASCII characters, both as written in the config and after event fields are filled in.
				"""#
			pr_numbers: [27012, 26977]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `influxdb_metrics` sink now reports the result of each write to end-to-end acknowledgements. Before, metrics were acknowledged as delivered even when InfluxDB rejected the write or all retries failed, so sources with acknowledgements enabled could drop metrics that InfluxDB never stored. Rejected writes now mark their events as rejected, and writes that fail after retries mark them as errored.
				"""#
			pr_numbers: [26952]
			contributors: ["thomasqueirozb"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "`internal_metrics_cardinality_total` counter removed"
			anchor:   "internal-metrics-cardinality-total-removed"
			description: #"""
				The `internal_metrics` source no longer emits the `internal_metrics_cardinality_total` counter. It
				was deprecated in 0.24.0 in favor of the `internal_metrics_cardinality` gauge, which reports the
				same value: the current number of metrics in the registry.
				"""#
			pr_numbers: [27074]
			contributors: ["JinchengCao"]
		},
		{
			type: "enhancement"
			description: #"""
				The `internal_metrics` source has a new `emit_incremental` option that emits counters and histograms
				as increments since the previous scrape instead of absolute totals. This keeps sinks like
				`datadog_metrics` from dropping histograms observed within a single scrape interval and the first
				scrape of each counter.
				"""#
			pr_numbers: [26080]
			contributors: ["gwenaskell"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Legacy buffer maximum-size metrics removed"
			anchor:   "legacy-buffer-max-size-metrics-removed"
			description: #"""
				The deprecated `buffer_max_byte_size`, `buffer_max_event_size`, `source_buffer_max_byte_size`,
				`source_buffer_max_event_size`, `transform_buffer_max_byte_size`, and
				`transform_buffer_max_event_size` gauge metrics have been removed. They were deprecated in 0.53.0
				in favor of the `*_max_size_bytes` and `*_max_size_events` gauges, which report the same values.
				"""#
			pr_numbers: [27076]
			contributors: ["JinchengCao"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Legacy `process_runtime` counter removed"
			anchor:   "legacy-process-runtime-counter-removed"
			description: #"""
				The `host_metrics` source no longer emits the `process_runtime` counter. It was deprecated in
				0.59.0 in favor of `process_runtime_total`, which reports the same value with the same tags.
				"""#
			pr_numbers: [27073]
			contributors: ["JinchengCao"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "The `mongodb_metrics` source requires MongoDB 4.4 or newer"
			anchor:   "mongodb-metrics-min-server-4-4"
			description: #"""
				The `mongodb` driver is updated to 3.9.1, which removes support for MongoDB 4.2. The
				`mongodb_metrics` source now requires MongoDB server 4.4 or newer (wire version 9 or newer).
				Connections to MongoDB 4.2 or older servers now fail with an incompatible server error. MongoDB 4.2
				reached end of life on April 30, 2023.
				"""#
			pr_numbers: [26878]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: #"""
				Added MessagePack decoder codec `message_pack`.
				
				Stream-based `socket` sources (`tcp` and `unix_stream` modes) require an explicit `framing` method
				when using `message_pack`.
				"""#
			pr_numbers: [27119]
			contributors: ["Keuin", "byronwolfman"]
		},
		{
			type: "enhancement"
			description: #"""
				Retry logs now include the retry attempt number. Sustained retryable sink failures are logged at
				error level every 10 attempts so persistent destination outages can trigger monitoring.
				"""#
			pr_numbers: [27081]
			contributors: ["20agbekodo"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Configuration is parsed before interpolation"
			anchor:   "configuration-is-parsed-before-interpolation"
			description: #"""
				Vector now parses configuration files before substituting environment variables and
				`SECRET[backend.key]` references. Substitution applies only to string values, followed
				by conversion to the field's declared type. Quotes, newlines, and other configuration
				syntax in substituted values cannot add keys, components, or array elements.
				
				This removes structural interpolation, deprecated in 0.57.0. Environment-variable
				interpolation remains disabled by default and still requires
				`--dangerously-allow-env-var-interpolation` or
				`VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`.
				"""#
			pr_numbers: [26475]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				The `prometheus_exporter` sink now sends `Content-Length` on uncompressed responses instead of `Transfer-Encoding: chunked`. It also sends `Vary: Accept-Encoding` on responses that can be compressed, and it compresses responses for clients that send `Accept-Encoding: x-gzip`. Status codes and response bodies are unchanged.
				"""#
			pr_numbers: [27034]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				The `remap` transform can now change a metric's type between `counter` and `gauge` by assigning to `.type`, for example `.type = "counter"`. The metric's value and kind are kept. This is useful when a source can't tell a metric's type, such as Prometheus remote write samples that arrive without type metadata and default to gauges.
				
				Like other disallowed metric changes, assigning any other value to `.type`, or changing the type of a set, distribution, histogram, summary, or sketch metric, is ignored and leaves the metric unchanged.
				"""#
			pr_numbers: [27080]
			contributors: ["sghall"]
		},
		{
			type: "fix"
			description: #"""
				The `webhdfs` sink now retries temporary write failures instead of immediately
				dropping the affected batch. You can configure retry behavior and other
				outbound request settings through the `request` options.
				"""#
			pr_numbers: [27159]
			contributors: ["zaugust4891"]
		},
		{
			type: "fix"
			description: #"""
				The `websocket` source and `websocket` sink now fail the handshake when the server returns a `Sec-WebSocket-Protocol` header, because these components do not request a subprotocol (RFC 6455). Before, the header was ignored. The `websocket_server` sink now rejects clients that send a `Sec-WebSocket-Key` that is not a base64-encoded 16-byte value.
				"""#
			pr_numbers: [27028]
			contributors: ["thomasqueirozb"]
		},
	]

	vrl_changelog: "### [0.39.0 (2026-11-16)](https://github.com/vectordotdev/vrl/releases/tag/v0.39.0)\n\n#### New Features\n\n- Added a new `log10` function that computes the base 10 logarithm of a number, for example to convert milliwatts to dBm with `10 * log10!(mw)`.\n\n  *Thanks to [lchabert](https://github.com/lchabert) for contributing PR [#2012](https://github.com/vectordotdev/vrl/pull/2012)!*\n- Added a new `pow` function that raises a number to a power, for example to convert dBm to milliwatts with `pow!(10, dbm / 10)`.\n\n  *Thanks to [lchabert](https://github.com/lchabert) for contributing PR [#2012](https://github.com/vectordotdev/vrl/pull/2012)!*\n\n#### Fixes\n\n- VRL now rejects field paths with array indices outside the `isize` range instead of silently wrapping to a different index.\n\n  *Thanks to [pront](https://github.com/pront) for contributing PR [#2045](https://github.com/vectordotdev/vrl/pull/2045)!*\n"
}
