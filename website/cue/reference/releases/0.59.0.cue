package metadata

releases: "0.59.0": {
	date: "2026-10-06"
	changelog: [
		{
			type: "enhancement"
			description: #"""
				The `azure_blob` sink now supports `blob_type: append`, which writes data as Azure Append Blobs.
				Unlike the default block blob mode that creates a new uniquely-named blob per batch, append mode
				reuses a stable blob name and extends it on each flush — ideal for continuous log streaming
				where you want a single growing file per time window.

				When `blob_type` is set to `append`, `blob_append_uuid` defaults to `false` and `blob_time_format`
				defaults to `%Y-%m-%dT%H` (hourly rotation), which keeps the Azure limit of 50,000 blocks per
				append blob out of reach at realistic throughput. Both can still be overridden explicitly.
				The Azure hard limit of 4 MiB per `append_block` call is enforced at startup via `batch.max_bytes`.

				Compression is supported in append mode with `gzip`, `zstd`, or `none`. Because each batch is
				compressed independently, `snappy` and `zlib` are rejected at startup: neither format can be
				decoded as a concatenated sequence of streams.

				Because a batch is appended to whatever the blob already holds, append mode takes the same
				stream-oriented encoding defaults as the `file` sink: with `codec: json` and no explicit `framing`
				it writes newline-delimited JSON, rather than the one-array-per-batch framing used for block blobs.
				Explicitly configured `framing` is always used as given.
				"""#
			pr_numbers: [25627]
			contributors: ["danielku15"]
		},
		{
			type: "enhancement"
			description: #"""
				The `prometheus_scrape` source now supports configuring HTTP request headers.
				"""#
			pr_numbers: [26075]
			contributors: ["arfa79"]
		},
		{
			type: "enhancement"
			description: #"""
				The `loki` sink now supports the `healthcheck.uri` field to customize the healthcheck endpoint.
				"""#
			pr_numbers: [24651]
			contributors: ["simonhammes"]
		},
		{
			type: "fix"
			description: #"""
				Fix collection from systemd sockets.

				Systemd sockets are passed in blocking mode, but tokio expects them to be in non-blocking mode.
				Therefore, always set sockets from systemd to non-blocking.
				"""#
			pr_numbers: [24308]
			contributors: ["j-c-fuchs", "aagor"]
		},
		{
			type: "feat"
			description: #"""
				The `aggregate` transform now supports event-time aggregation via an optional
				`event_time` configuration block. This avoids collapsing distinct samples in
				sinks (such as Datadog Metrics) that overwrite earlier values for an identical
				timestamp.
				"""#
			pr_numbers: [24421]
			contributors: ["kaarolch"]
		},
		{
			type: "feat"
			description: #"""
				Added the `files_unwatched_bytes_unread_total` internal metric to the `file` and `kubernetes_logs` sources. It tracks known unread bytes when a file is unwatched, for example after deletion or rotation. A known count of zero means no bytes remained unread at measurement time.

				When the unread-byte count is unavailable, Vector increments `files_unwatched_with_unknown_bytes_total` instead. This counter counts unwatch events with unknown unread size, including gzipped files, skipped gzip readers, and metadata failures; it does not measure lost bytes. Gzip files are not decompressed solely to calculate telemetry. Both metrics use the existing `reached_eof` label and optional `file` label, and `files_unwatched_total` continues to count all unwatch events.
				"""#
			pr_numbers: [24676]
			contributors: ["akashvbabu91"]
		},
		{
			type: "fix"
			description: #"""
				The NATS JetStream source now automatically recovers when the pull stream terminates due to a connection close event (e.g., during NATS rolling upgrades or lame duck mode). Previously, the source would silently stop consuming messages. It now reconnects and rebuilds the pull consumer stream with exponential backoff.
				"""#
			pr_numbers: [25042]
			contributors: ["benjamin-awd"]
		},
		{
			type: "enhancement"
			description: #"""
				HTTP-based sources (`http_server`, `datadog_agent`, `splunk_hec`, `aws_kinesis_firehose`,
				`opentelemetry`, `prometheus_remote_write`, `prometheus_pushgateway`, `heroku_logs`) now
				support OS-level TCP keepalive on accepted connections via a new `keepalive.tcp_keepalive`
				configuration option. When configured, the OS will send TCP keepalive probes after the
				specified idle time, detecting and closing connections where the remote peer has disappeared
				without sending a FIN or RST packet (for example, due to an abrupt machine failure).
				"""#
			pr_numbers: [25062]
			contributors: ["erhudy"]
		},
		{
			type: "fix"
			description: #"""
				Renamed the memory enrichment table failure and TTL-expiration internal metrics to end in `_total`, matching Vector's counter naming convention:

				- `memory_enrichment_table_failed_insertions_total`
				- `memory_enrichment_table_failed_reads_total`
				- `memory_enrichment_table_ttl_expirations_total`

				This replaces the previous non-`_total` metric names. The previous metric names without the `_total` suffix are still emitted but are deprecated and will be removed in a future release.
				"""#
			pr_numbers: [26405, 25459]
			contributors: ["nanookclaw", "thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				gRPC-based sources (`vector`, `opentelemetry`) now include the underlying error when a compressed request payload fails to decompress, instead of the generic `reached impossible error during decompressor finalization` message. Decompression failures (for example, a corrupt or truncated gzip stream) are now diagnosable from the returned status and the sender's logs.
				"""#
			pr_numbers: [25796]
			contributors: ["Stunned1"]
		},
		{
			type: "fix"
			description: #"""
				Reduced memory allocated per log line in the `kubernetes_logs` source by around
				0.4–0.6 KB.
				"""#
			pr_numbers: [26490]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				`vector test` now reports ambiguous output names as configuration errors instead of panicking.
				"""#
			pr_numbers: [26504]
			contributors: ["blackmore-technology-group"]
		},
		{
			type: "fix"
			description: #"""
				Renamed the aggregate transform's `aggregate_failed_updates` internal counter to `aggregate_failed_updates_total`, matching Vector's counter naming convention. The previous `aggregate_failed_updates` name is still emitted but is deprecated and will be removed in a future release.
				"""#
			pr_numbers: [26407]
			contributors: ["thomasqueirozb"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Avro codec rejects shorthand complex type schemas"
			anchor:   "avro-strict-schema-parsing"
			description: #"""
				The `apache-avro` library has been upgraded from 0.21 to 0.22, which enforces stricter schema
				parsing per the Avro specification. Field-level attributes must now be nested inside a `"type"`
				object rather than specified as siblings of the `"type"` string:

				- **Complex types** (`array`, `map`, `enum`, `record`, `fixed`): schemas using the shorthand
				  form will **fail to parse at startup**.
				- **Logical types** (`timestamp-millis`, `date`, `uuid`, etc.): schemas using the shorthand
				  form will still parse, but the logical type is **silently ignored** and the field is treated
				  as a plain primitive.
				"""#
			pr_numbers: [26146]
			contributors: ["omwbennett"]
		},
		{
			type: "fix"
			description: #"""
				The `blackhole` sink now rejects `rate: 0` during configuration validation instead of panicking when it receives events.
				"""#
			pr_numbers: [26048]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the internal buffer usage reporter outliving the buffer it reports on,
				which would cause the reporter for the old buffer to keep publishing stale
				metrics under the same `buffer_id` as its replacement.  This also lets the
				metrics for a buffer that was removed rather than replaced age out under
				`expire_metrics_secs`, which the continuously republished values previously
				prevented.
				"""#
			pr_numbers: [25994]
			contributors: ["bruceg"]
		},
		{
			type: "security"
			description: #"""
				The `chunked_gelf` framing decoder now limits the payload buffered across incomplete messages to 128 MiB. An unauthenticated sender could previously exhaust memory by sending chunks for messages it never completed, most easily on the `socket` source in UDP mode.

				`max_length` can lower the per-message ceiling. Setting it above 128 MiB raises both the per-message ceiling and the aggregate limit to that value.
				"""#
			pr_numbers: [26302]
			contributors: ["pront"]
		},
		{
			type: "security"
			description: #"""
				The `chunked_gelf` framing decoder now applies a configurable limit to the number of incomplete messages held in memory. An unauthenticated sender could previously exhaust memory by sending unique message IDs it never completed, most easily on the `socket` source in UDP mode.

				`pending_messages_limit` now defaults to 4096. It was previously unset and therefore unbounded.
				"""#
			pr_numbers: [26301]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Fixed generated configuration schemas for flattened optional internally-tagged enums. Configs that omit the flattened block now validate: the schema encodes `None` as a missing tag field rather than JSON `null`, matching `serde`.
				"""#
			pr_numbers: [26248]
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				Templates whose literal prefix started with `http://` or `https://` were incorrectly given
				URI-specific confinement checks, even when the field was not a URI field (e.g. an
				object-store key prefix). Confinement is now selected by the field's type rather than by
				inspecting the template content, so such templates use prefix confinement instead.
				"""#
			pr_numbers: [26011]
			contributors: ["thomasqueirozb"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "`datadog_agent` source no longer accepts pre-`tracerPayloads` trace payloads"
			anchor:   "datadog-agent-legacy-trace-payload"
			description: #"""
				The `datadog_agent` source no longer accepts the pre-`tracerPayloads` Agent-to-intake trace protobuf
				(`traces` / `transactions` fields). The Datadog Agent dropped those fields in the 7.33.0 release in
				January 2022. An empty `tracerPayloads` list now produces no events and increments
				`component_errors_total` with `error_code` `empty_tracer_payloads`. Indexed `idxTracerPayloads`
				entries are recognized but not converted (`error_code` `idx_tracer_payloads`).
				"""#
			pr_numbers: [26307]
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				The `datadog_agent` source now decodes Datadog span links (`spanLinks`) and span events
				(`spanEvents`) into each span on the emitted trace event. Span-link `trace_id`, `trace_id_high`,
				and `span_id` are 16-character lowercase hexadecimal strings so the full unsigned 64-bit range is
				preserved. The `datadog_traces` sink encodes those fields back into the Agent protobuf.
				`containerDebug` and `rareSamplerEnabled` are decoded from the wire but are not copied onto events.
				"""#
			pr_numbers: [26307]
			contributors: ["bruceg"]
		},
		{
			type: "enhancement"
			description: #"""
				Add `max_payload_bytes` configuration option to the `datadog_logs` sink, allowing the
				payload size limit to be raised above the default 5 MB for endpoints that accept larger
				payloads. The batch goal is derived automatically as `max_payload_bytes - 750,000` bytes,
				keeping the same safety headroom as the previous hardcoded defaults.
				"""#
			pr_numbers: [26396]
			contributors: ["dd-sebastien-lb"]
		},
		{
			type: "enhancement"
			description: #"""
				The `datadog_logs` sink can now optionally truncate logs that exceed Datadog's
				per-log size limit. Configure `truncate_oversized_logs` to set the encoded log
				limit, mark shortened messages, and tag reduced logs. Logs whose non-message
				fields leave no room for a truncated message, or have no string message, are
				forwarded unchanged when they fit the payload limit, allowing the Datadog
				intake to truncate them.
				"""#
			pr_numbers: [26512]
			contributors: ["bruceg"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Datadog metrics series submitted to the V3 intake by default"
			anchor:   "datadog-metrics-series-v3-default"
			description: #"""
				The `datadog_metrics` sink now submits series metrics to `/api/intake/metrics/v3/series` by
				default, using Datadog's columnar protobuf format. This format uses dictionary-based string
				deduplication and delta encoding, making it more efficient than `v2` for workloads with many
				metrics that share common names or tags.

				Sketch metrics (distributions and histograms) are unaffected and continue to be submitted to
				`/api/beta/sketches`.
				"""#
			pr_numbers: [26285]
			contributors: ["stephenwakely"]
		},
		{
			type: "fix"
			description: #"""
				Decoding Vector's native protobuf format (`decoding.codec = "native"`) and disk-buffer records no longer panics when an event variant is missing or unrecognized, when a float field is `NaN`, or when an AgentDDSketch has mismatched bin lists. Those payloads are rejected, dropped, and reported through existing decode/buffer error telemetry. A `NaN` float in event data or metadata rejects the entire record rather than rewriting the value.
				"""#
			pr_numbers: [26292]
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the `file` source hanging during fingerprinting when a file is smaller than `fingerprint.ignored_header_bytes`. Incomplete files are retried when more data becomes available.
				"""#
			pr_numbers: [26481]
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: #"""
				The `host_metrics` source now exposes a `memory_oom_kill_events_total` counter
				metric on Linux, reporting the number of Out-Of-Memory kill events recorded by
				the kernel.
				"""#
			pr_numbers: [25802]
			contributors: ["simonhammes"]
		},
		{
			type: "enhancement"
			description: #"""
				Added bearer authentication strategy to HTTP server sources. The `http_server`, `heroku_logs`, and `websocket_server` components now support `strategy = "bearer"` in their `auth` configuration, allowing token-based authentication via the `Authorization: Bearer <token>` header.
				"""#
			pr_numbers: [25073]
			contributors: ["steveduan-IDME"]
		},
		{
			type: "fix"
			description: #"""
				TCP source acknowledgement and TLS connection error logs now include the remote `peer_addr`.
				"""#
			pr_numbers: [26467]
			contributors: ["pront"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Kubernetes 1.31 support removed"
			anchor:   "kubernetes-1-31-support-removed"
			description: #"""
				The `kubernetes_logs` source now uses Kubernetes v1.32 API bindings. Kubernetes
				1.31 reached end of life on 2025-11-11 and is no longer supported.
				"""#
			pr_numbers: [26506]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `lua` transform no longer panics when a metric tag contains a value that cannot be converted to a string (e.g. a boolean or a nested table). Such values now produce a Lua conversion error and the event is discarded.
				"""#
			pr_numbers: [26319]
			contributors: ["quwin"]
		},
		{
			type: "enhancement"
			description: #"""
				Add two optional configuration fields to the OpenTelemetry source: `max_concurrent_requests`
				limits concurrent requests across HTTP and gRPC, and `request_timeout_secs` limits request
				processing time. Both are disabled by default and can be enabled to help prevent out-of-memory
				errors in some deployments.
				"""#
			pr_numbers: [26457]
			contributors: ["ArunPiduguDD"]
		},
		{
			type: "fix"
			description: #"""
				Renamed the host `process_runtime` counter emitted by the `host_metrics` source to `process_runtime_total`, matching Vector's counter naming convention. The previous `process_runtime` name is still emitted but is deprecated and will be removed in a future release.
				"""#
			pr_numbers: [26406]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Ensure the `prometheus_exporter` sink emits valid text exposition by escaping newlines in label
				values and rejecting metrics whose metric names, namespaces, or label names contain carriage returns
				or newlines.
				"""#
			pr_numbers: [26354]
			contributors: ["pront"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Removed `http` source and `greptimedb` sink deprecated component aliases"
			anchor:   "removed-deprecated-component-aliases"
			description: #"""
				The deprecated `http` source and `greptimedb` sink aliases have been removed. They were deprecated in Vector 0.26.0 and 0.41.0, respectively.
				"""#
			pr_numbers: [26230]
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: #"""
				Sink `endpoint` options now require an absolute URL that includes a host.

				**Before:**

				- Endpoints without a scheme (for example `endpoint: "localhost:8080"`) were accepted at configuration load and failed only when the sink attempted to send data.
				- Empty, host-less, or non-`http(s)` endpoints (for example `endpoint: ""`, `endpoint: "http:///"`, or `endpoint: "ftp://example.com"`) were accepted at configuration load and either failed only when the sink attempted to send data or were silently completed with a default scheme and host.

				**After:**

				- Endpoints without a scheme are defaulted to `https://` (for example `endpoint: "localhost:8080"` becomes `https://localhost:8080`) and work as expected.
				- Empty, host-less, or non-`http(s)` endpoints (for example `endpoint: ""`, `endpoint: "http:///"`, or `endpoint: "ftp://example.com"`) are rejected at configuration load with a clear error, including with `vector validate --no-environment`.
				"""#
			pr_numbers: [26224, 26218, 26213]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				`vector validate --no-environment` now catches sink configurations issues that previously
				only surfaced when Vector booted.
				"""#
			pr_numbers: [26048]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				`vector validate` now resolves `SECRET[backend.key]` placeholders from the configured secret
				backends before validating the configuration, matching `vector`'s startup behavior.
				If `--no-environment` is specified then secrets aren't resolved by default. You can specify
				the new `--resolve-secrets` flag to resolve secrets as well.
				"""#
			pr_numbers: [26268]
			contributors: ["thomasqueirozb"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Boolean Vector sink compression removed"
			anchor:   "vector-compression-bool-removed"
			description: #"""
				The deprecated boolean syntax for the `vector` sink's `compression` option has
				been removed.
				"""#
			pr_numbers: [26255]
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: #"""
				The `vector` sink now rejects empty, host-less, or non-`http(s)` `address` and `routing.endpoints` values at configuration load with a clear error, including with `vector validate --no-environment`.
				"""#
			pr_numbers: [26224]
			contributors: ["thomasqueirozb"]
		},
	]

	vrl_changelog: #"""
		### [0.36.0 (2026-10-01)](https://github.com/vectordotdev/vrl/releases/tag/v0.36.0)

		#### Breaking Changes & Upgrade Guide

		- Several stdlib functions now declare element-kind constraints on array parameters, enabling the compiler to detect element-type mismatches at compile time and automatically infer call-site infallibility.

		**Before:** passing a string-literal array required `!` because the compiler assumed it could fail:
		```coffee
		join!(["sources", "transforms", "sinks"], separator: ", ")
		```

		**After:** when the compiler can prove the elements are strings, `!` is unnecessary (and `!` now triggers a warning):
		```coffee
		join(["sources", "transforms", "sinks"], separator: ", ")
		```

		Passing the wrong element type (e.g. `join([1, 2, 3])`) is now a hard compile error instead of a runtime failure.

		Affected functions: `join`, `contains_all`, `tally`, `encode_key_value`, `encode_logfmt`, `ip_cidr_contains`, `parse_groks`.

		*Thanks to [pront](https://github.com/pront) for contributing PR [#1861](https://github.com/vectordotdev/vrl/pull/1861)!*

		#### New Features

		- Add `break` statement support for early loop exit within `for_each` closures.

		*Thanks to [jimmystewpot](https://github.com/jimmystewpot) for contributing PR [#1931](https://github.com/vectordotdev/vrl/pull/1931)!*

		#### Enhancements

		- Optimize `md5` runtime performance with stack-buffered hex encoding and compile-time constant evaluation for literals.

		*Thanks to [jimmystewpot](https://github.com/jimmystewpot) for contributing PR [#1930](https://github.com/vectordotdev/vrl/pull/1930)!*
		- Improve `for_each` performance and reduce memory allocations by iterating over collections directly, binding only the closure parameters that are used, and reusing compiler variable slots across iterations. Benchmarks show a 23–41% throughput improvement for arrays and objects.

		*Thanks to [jimmystewpot](https://github.com/jimmystewpot) for contributing PR [#1932](https://github.com/vectordotdev/vrl/pull/1932)!*
		- Optimise `merge` runtime performance and memory usage with zero-clone ownership transfer, single-pass entry traversal, size-adaptive shallow merging, and compile-time constant evaluation for literals, while hardening against deep recursion stack overflows and resolving type definition unsoundness for deep merges. Benchmarks show up to a 120% throughput increase for asymmetric merges and 26% for large flat objects.

		*Thanks to [jimmystewpot](https://github.com/jimmystewpot) for contributing PR [#1953](https://github.com/vectordotdev/vrl/pull/1953)!*
		- Bump `convert_case` from 0.7.1 to 0.12.0, improving string casing function performance (`camelcase`, `snakecase`, `pascalcase`, `kebabcase`, `screamingsnakecase`) by approximately 70%.

		*Thanks to [jimmystewpot](https://github.com/jimmystewpot) for contributing PR [#1961](https://github.com/vectordotdev/vrl/pull/1961)!*
		- Optimize `sha1`, `sha2`, and `sha3` runtime performance with stack-buffered hex encoding and compile-time constant evaluation for literals.

		*Thanks to [bruceg](https://github.com/bruceg) for contributing PR [#1951](https://github.com/vectordotdev/vrl/pull/1951)!*

		#### Fixes

		- Fixed `uuid_v7` to preserve the supplied timestamp at millisecond precision.

		*Thanks to [abbit](https://github.com/abbit) for contributing PR [#1956](https://github.com/vectordotdev/vrl/pull/1956)!*

		"""#
}
