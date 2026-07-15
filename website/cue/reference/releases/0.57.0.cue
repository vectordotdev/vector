package metadata

releases: "0.57.0": {
	date: "2026-07-14"

	whats_next: []

	description: """
		## Breaking Changes

		See the [0.57 upgrade guide](/highlights/2026-07-14-0-57-0-upgrade-guide/) for full details
		and migration steps. At a glance, you are affected if you:

		- rely on `${VAR}` interpolation in Vector configuration files: environment variable interpolation
		  is now disabled by default. Pass `--dangerously-allow-env-var-interpolation` (or set
		  `VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`) to restore the previous behavior.
		- use `{{ field }}` references in sink routing templates (object keys, file paths, HTTP headers,
		  table or stream names): sinks now enforce a confinement boundary and reject templates with no
		  literal prefix at startup. Set a static prefix in the template, or use
		  `dangerously_allow_unconfined_template_resolution: true` to opt out.
		"""

	changelog: [
		{
			type: "fix"
			description: #"""
				Fixed `vector validate --no-environment` so it reports VRL and condition compilation errors for transforms without requiring full environment-dependent component initialization.
				"""#
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: #"""
				The `host_metrics` source can now collect hardware temperature readings via a
				new `temperature` collector. When enabled, it emits `temperature_celsius`,
				`temperature_max_celsius`, and `temperature_critical_celsius` gauges, each
				tagged with the `component` label of the sensor it was read from.
				
				The collector is opt-in: add `temperature` to the `collectors` list to enable
				it. Components that do not report a given value (for example a missing critical
				threshold) are skipped, and environments without temperature sensors simply
				produce no metrics.
				"""#
			contributors: ["somaz94"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the `logstash` source to close the connection on a malformed frame instead of attempting to continue. A failed JSON decode or decompression previously left the decoder desynchronized but still running, which could busy-loop and emit ACKs for bogus sequence numbers (surfacing as `invalid sequence number received` on the client). The source now treats any decode error as fatal and closes the connection — matching the upstream `logstash-input-beats` server — so the client reconnects and retransmits the unacknowledged window.
				"""#
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the `file` source silently dropping all but the first member of concatenated (multi-stream) gzip files. This regression was introduced in v0.50.0.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Adds support for including attributes from the `X-Amz-Firehose-Common-Attributes` header in the log events for the `aws_kinesis_firehose` source.
				"""#
			contributors: ["tchanturia"]
		},
		{
			type: "fix"
			description: #"""
				Added support for DOQ socket protocol in dnstap source. This will prevent error messages when DOQ
				traffic is encountered.
				"""#
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the `logstash` source to preserve writer window boundaries when generating ACKs. This prevents batched reads from producing ACK sequences that advance past the current window, which could lead to "invalid sequence number received" errors and duplicate retransmits under load.
				"""#
			contributors: ["bruceg"]
		},
		{
			type: "feat"
			description: #"""
				Added a way to keep memory enrichment table state between configuration reloads, using the new `reload_behavior` option.
				"""#
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "enhancement"
			description: #"""
				The `databricks_zerobus` sink now supports a `user_agent` option whose value is appended to the `user-agent` header sent to Databricks. The header always identifies Vector (`Vector/<version>`); when set, the configured value is appended after it.
				"""#
			contributors: ["flaviocruz"]
		},
		{
			type: "fix"
			description: #"""
				Fixed SQL injection via identifier names in the `clickhouse` sink. The `database` and `table` config values are now passed as ClickHouse query parameters with the `Identifier` type (`{database:Identifier}.{table:Identifier}`), letting the server handle quoting rather than relying on client-side string escaping.
				"""#
			contributors: ["pront", "thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed `vector top` freezes when using a high number of components.
				"""#
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a bug in the `datadog_metrics` sink where the metric type name was compared against itself (instead of the peer metric) when sorting metrics before encoding. The sort key is `(type_name, metric_name, timestamp)`, but the type comparison was a no-op, making `metric_name` the effective primary key. The fix restores the intended ordering.
				"""#
			contributors: ["gwenaskell"]
		},
		{
			type: "feat"
			description: #"""
				Added support for configuring gRPC source maximum connection age, allowing
				long-lived client connections to be gracefully recycled for better load balancer
				distribution.
				"""#
			contributors: ["fpytloun"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a config reload bug that could silently stop event delivery. If a reload changes a component's kind while keeping the same name (for example, replacing an enrichment table's derived source named `X` with a regular source named `X`, or replacing a transform named `X` with a source named `X`), any downstream sink or transform that still reads from `X` now correctly reconnects to the new component instead of going silent until the next restart.
				"""#
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a `reduce` transform bug where a timestamp field with a name that requires quoting in a VRL path (e.g. `"created.at"` or `"event-time"`) would have its `_end` companion silently dropped from the reduced event. The companion path is now built structurally and correctly lands next to the base field.
				"""#
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the `logstash` source to ACK only completed writer windows rather than
				sometimes emitting a partial ACK before the window is complete. While partial
				ACKs are permitted by the official protocol spec and cause no problems for the
				reference `go-lumber` client in Beats, they appears to confuse proxies that
				assume there will only be one ACK per window, causing errors on subsequent
				batches.
				"""#
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				The `logstash` source now rejects a `WindowSize` frame that arrives before the
				current window has received all of its advertised events, closing the connection
				with a fatal decode error instead of making any attempt to continue. While this
				is allowed by the protocol spec, no known client makes use of this and the
				reference server in `go-lumber` treats it as a protocol violation.
				"""#
			contributors: ["bruceg"]
		},
		{
			type: "feat"
			description: #"""
				Added a new counter metric `component_cpu_usage_ns_total` counting the CPU
				time consumed by a transform in nanoseconds.
				
				The metric is opt-in: set `measure_cpu_usage: true` on individual transform
				configurations to enable it. When disabled (the default), no counter is
				registered and no per-poll clock sampling takes place.
				"""#
			contributors: ["gwenaskell"]
		},
		{
			type: "fix"
			description: #"""
				The `aws_kinesis_firehose` source no longer falls back to forwarding raw, undecoded bytes for a record that hits the decompressed-size cap under `Compression::Auto`; such records are now rejected instead of forwarded.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Updated the zlib backend (`zlib-rs`) from 0.6.0 to 0.6.6, pulling in an upstream fix for an aarch64 NEON Adler-32 bug that could produce incorrect checksums. On arm64, valid zlib-format (RFC 1950) payloads, such as `deflate` content-encoding handled by HTTP-based sources, could previously be rejected as corrupt with an `incorrect data check` error.
				"""#
			contributors: ["Stunned1"]
		},
		{
			type: "enhancement"
			description: #"""
				Optional Arrow IPC compression for Flight payloads. Defaults to no compression.
				"""#
			contributors: ["flaviofcruz"]
		},
		{
			type: "enhancement"
			description: #"""
				Improved the warning log emitted by the `datadog_logs` sink when a field with a Datadog reserved attribute semantic meaning needs to be relocated but the destination path already exists. The log now includes `source_path`, `destination_path`, and `renamed_existing_to` fields to make the conflict easier to diagnose;
				additionally, it will now also increment a new counter `datadog_logs_reserved_attribute_conflicts_total`.
				"""#
			contributors: ["gwenaskell"]
		},
		{
			type:     "chore"
			breaking: true
			description: #"""
				Environment variable interpolation in configuration files is now disabled by default. Previously, Vector interpolated `${VAR}` references in config files automatically. To restore the previous behavior, pass `--dangerously-allow-env-var-interpolation` (or set `VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`). The `--disable-env-var-interpolation` flag and `VECTOR_DISABLE_ENV_VAR_INTERPOLATION` environment variable have been removed.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the syslog codec silently ignoring short-form severity keywords (`crit`, `emerg`, `err`, `info`, `warn`) and falling back to the default `informational`. The encoder now accepts both short-form and full-form severity names, matching the values used by VRL's `to_syslog_severity` and `to_syslog_level` functions.
				"""#
			contributors: ["vparfonov"]
		},
		{
			type: "fix"
			description: #"""
				The `fluent` source now caps how large a single msgpack frame may grow while being buffered, using the same limit, so a peer can no longer stream an oversized array/map/string without ever completing a message. Frames that exceed the limit before a complete message is decoded are now rejected and the connection is closed.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `vector` source and the `opentelemetry` source's gRPC mode now bound gRPC message size at the header read and cap `gzip`/`zstd` decompression mid-stream, matching the decompressed-size limit enforced elsewhere, so a compressed gRPC message can no longer expand past the cap during decompression before being rejected.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fix programmatic defaults for endpoint health configuration to match the documented deserialization defaults.
				"""#
			contributors: ["fpytloun"]
		},
		{
			type: "fix"
			description: #"""
				HTTP-based sources (`http_server`, `prometheus_pushgateway`, `prometheus_remote_write`, `heroku_logs`, `opentelemetry`) now cap decompressed request bodies at 100 MiB. Previously, a single unauthenticated request carrying a compressed payload (e.g. a gzip bomb) could allocate unbounded memory and OOM-kill the Vector process. Decompressed payloads exceeding the cap are rejected with HTTP 413, as are requests whose declared `Content-Length` exceeds the same limit. The cap can be raised or lowered via `--max-decompressed-size-bytes` (or `VECTOR_MAX_DECOMPRESSED_SIZE_BYTES`).
				"""#
			contributors: ["pront", "thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Add support for optionally applying rate limiting to the `internal_logs` source controlled by the
				`--internal-logs-source-rate-limit` CLI option and `VECTOR_INTERNAL_LOGS_SOURCE_RATE_LIMIT`
				environment variable. This provides the same rate limiting functionality as was available before
				version 0.51.1 but with a rate limit window separate from the console one.
				"""#
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				The `logstash` source now caps the size of compressed frame payloads. Previously, the 32-bit payload size field of a compressed (`C`) frame was read straight from the wire and used to reserve a buffer, so a 6-byte malformed frame advertising a multi-gigabyte size could trigger an allocation large enough to abort the process. The size of the decompressed output is now 100MiB by default and can be configured by `--max-decompressed-size-bytes` or `VECTOR_MAX_DECOMPRESSED_SIZE_BYTES`. Oversized frames are rejected as a decode error.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `logstash` source now rejects compressed frames that are nested inside other compressed frames. Previously, a malicious sender could nest compressed (`C`) frames arbitrarily deep, driving unbounded recursion in the decoder until the process exhausted its stack and aborted. Compressed payloads may now contain only a single layer of compression; no known Lumberjack/Beats client (e.g. Filebeat) ever emits more than one.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed an integer underflow in the octet-counting framer (used by TCP `syslog` sources) that occurred when an over-length, length-prefixed message was split across multiple reads. Previously the decoder could panic in debug builds, or in release builds wrap the remaining-bytes counter to a huge value, wedging the decoder and silently dropping all subsequent input on that connection.
				"""#
			contributors: ["hhh6593"]
		},
		{
			type: "fix"
			description: #"""
				A new `--raise-fd-limit` CLI flag (or `VECTOR_RAISE_FD_LIMIT` environment variable)
				raises the file descriptor soft limit to the hard limit at startup. This prevents
				"Too many open files" errors when Vector monitors large numbers of log files. On
				macOS, Vector falls back to the kernel-enforced per-process file limit if the hard
				limit is too high.
				"""#
			contributors: ["vparfonov"]
		},
		{
			type: "enhancement"
			description: #"""
				The `socket` source (UDP mode) now supports a `multicast_interface` option that controls which local network interface is used when joining multicast groups. This is useful on hosts with multiple interfaces and on macOS, where specifying `0.0.0.0` only joins on the default interface (unlike Linux, which joins on all interfaces).
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: #"""
				Add `--chunk-size-events` / `VECTOR_CHUNK_SIZE_EVENTS` to configure the source sender batch size and source output buffer base capacity (defaults to 1000 events).
				"""#
			contributors: ["sakateka"]
		},
		{
			type: "fix"
			description: #"""
				Sources that can decompress potentially untrusted input now cap compressed and decompressed payload sizes, preventing a small compressed payload (e.g. a gzip/zlib/zstd bomb) from allocating unbounded memory and OOM-killing the Vector process. The decompressed-output cap defaults to 100MiB and can be configured via `--max-decompressed-size-bytes` or `VECTOR_MAX_DECOMPRESSED_SIZE_BYTES`. Affected sources: `http_server`, `heroku_logs`, `prometheus_pushgateway`, `prometheus_remote_write`, `datadog_agent`, `splunk_hec`, `aws_kinesis_firehose`, `fluent`, `logstash`, `vector`, and `opentelemetry` (both its HTTP and gRPC modes). The `datadog_agent`, `splunk_hec`, and `aws_kinesis_firehose` sources additionally now cap the size of the raw (compressed) request body they buffer in memory before decompression, matching `http_server` and `opentelemetry`; oversized requests are rejected with `413 Payload Too Large` instead of being read into memory unbounded. Relatedly, zstd decoders across all affected sources now also bound the decoder's internal window allocation, derived from the decompressed-size cap (and for HTTP-based sources, additionally clamped to the 8 MB ceiling suggested by RFC 9659), so a crafted frame can no longer declare a large `Window_Size` and drive a big allocation before the cap has a chance to trip.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Updated the `source send cancelled` error message to point towards possible causes.
				This error usually happens either because a pipeline is shutting down or because
				of backpressure.
				"""#
			contributors: ["clementd-dd"]
		},
		{
			type: "fix"
			description: #"""
				Internal telemetry (metrics and logs) emitted from work that Vector runs on spawned `tokio` tasks now correctly inherits the owning component's tags (`component_id`, `component_kind`, `component_type`). Previously, several components spawned background tasks without propagating the tracing span, so some internal events emitted from those tasks were missing their component tags. Affected emissions include the `datadog_logs` sink's `component_discarded_events_total` (events too large to encode), the `gcp_pubsub` source's `component_errors_total`/`component_discarded_events_total` from its per-stream tasks, and the `splunk_hec` sinks' acknowledgement-handling `component_errors_total`.
				"""#
			contributors: ["gwenaskell"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a potential panic in the `statsd` source when a gauge metric value begins with a multi-byte UTF-8 character. The invalid value now returns a parse error instead.
				"""#
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: #"""
				The `tag_cardinality_limit` transform now supports `mode: exact_fingerprint`, a new storage
				mode that can reduce memory usage for high-cardinality tag values compared to
				`mode: exact`. Instead of storing the full tag-value strings, only a 64 bit fingerprint hash of
				each value is kept. The trade-off is that throughput is slightly impacted due to extra hashing
				operations, and there is technically a (unlikely) chance of collisions at very high cardinalities
				"""#
			contributors: ["ArunPiduguDD"]
		},
		{
			type: "enhancement"
			description: #"""
				Adds a per-tag `cache_size_per_key` option to configuration options in probabilistic mode. Previously, per-tag overrides always inherited the bloom filter cache size from the enclosing config, which could cause a higher false positive rate when the per-tag `value_limit` is higher than the global or per-metric `value_limit`. When omitted, the cache size value from the enclosing config is used. Only valid in `probabilistic` mode — using it in `exact` mode will cause a configuration error.
				"""#
			contributors: ["ArunPiduguDD"]
		},
		{
			type:     "chore"
			breaking: true
			description: #"""
				Sinks that accept `{{ field }}` references in routing templates now enforce a
				confinement boundary: the rendered value must stay within the literal prefix
				declared in the template. Templates with no literal prefix (e.g.
				`key_prefix: "{{ host }}/"`) are rejected at startup. The `file` sink is the
				only exception: its `base_dir` config field can provide an explicit
				confinement root for `path` templates with no usable literal prefix.

				Any sink that includes a templated config field can be affected.

				The `file` sink gains a `base_dir` config field to set the confinement root
				explicitly when the `path` template has no usable literal prefix.

				**HTTP-family templates:** HTTP/HTTPS URI templates that use `{{ field }}`
				references must not contain `?` or `#`. A field-rendered value could smuggle
				additional query parameters or fragments into the rendered URI. Fully static URI
				templates (no `{{ }}`) with a query string or fragment are still accepted.
				Dynamic query or fragment segments (e.g.
				`https://api.internal/ingest?tenant={{ tenant }}`) are rejected at startup.
				Templated `request.headers` values are also confined for HTTP-family sinks.

				**Opt-out:** set `dangerously_allow_unconfined_template_resolution: true` on
				the affected sink to disable all confinement checks for that sink — both at
				startup and at runtime. Vector logs a warning per template on startup and sets
				`vector_security_confinement_disabled{component_type=...}` to `1`.

				**Observability:**

				- `component_errors_total{error_type="confinement_failed"}` — increments on
				  each violation; events that trigger it are dropped.
				- `vector_security_confinement_disabled` — set to `1` while a sink is running
				  with confinement disabled.
				"""#
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: #"""
				The `vector` sink now supports optional HTTP/2 keepalive on its pooled gRPC connections, configured via a new `keepalive` block (`interval_secs` and `timeout_secs`). When enabled, the sink periodically sends HTTP/2 PING frames so that a connection to a downstream Vector instance that has gone away is detected and evicted rather than reused indefinitely (which could otherwise stall delivery until the connection was replaced). Keepalive is disabled by default; when enabled, `interval_secs` defaults to 60 (aligned with gRPC keepalive guidance to avoid tripping `too_many_pings` policies) and `timeout_secs` defaults to 20. PINGs are sent on idle connections so an idle-but-dead connection is still detected.
				"""#
			contributors: ["graphcareful"]
		},
		{
			type: "feat"
			description: #"""
				Add support for configuring multiple endpoints in the `vector` sink via the new `routing.endpoints` option, enabling built-in `load_balance`, `failover`, and `failover_primary` endpoint strategies across downstream Vector instances. The previous `address` option is now deprecated in favor of `routing.endpoints`.
				"""#
			contributors: ["fpytloun"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the VRL Playground truncating large integers (such as `xxhash` and `seahash` results) to their least-significant digits. Results are now serialized with full precision instead of being coerced to JavaScript floating-point numbers.
				"""#
			contributors: ["stigglor"]
		},
		{
			type: "enhancement"
			description: #"""
				Improved the VRL playground UX:
				
				- Added resizable panels via draggable gutters.
				- Added a dark/light/system theme toggle that syncs with the OS preference.
				- Added run history persisted in localStorage, with a clear button.
				- Added a Shift+Enter shortcut to run the program.
				- The Share button now copies the shareable URL to the clipboard.
				- Various visual polish (unified header, purple accents, tightened spacing).
				"""#
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a typo in the `WebSocketMessageReceived` internal event emitted by the `websocket` source: the `protocol` field was previously misspelled as `protcol`. Users filtering on this field in trace-level logs should update their queries accordingly.
				"""#
			contributors: ["pront"]
		},
	]

	vrl_changelog: #"""
		### [0.34.0 (2026-07-13)](https://github.com/vectordotdev/vrl/releases/tag/v0.34.0)
		
		#### New Features
		
		- Added support for dynamic regex patterns in `parse_regex`, allowing variables and runtime expressions to be passed as the `pattern` argument.
		
		[PR #1809](https://github.com/vectordotdev/vrl/pull/1809) by [@thomasqueirozb](https://github.com/thomasqueirozb)
		- Add `strict` parameter to `parse_cef` (default: `true`). When set to `false`, the function performs best-effort parsing of non-compliant CEF input, treating unescaped `=` characters within field values as literals rather than field delimiters. This improves compatibility with vendors such as Infoblox and Palo Alto Networks whose CEF output does not fully conform to the spec.
		
		[PR #1821](https://github.com/vectordotdev/vrl/pull/1821) by [@jacklongsd](https://github.com/jacklongsd)
		
		#### Enhancements
		
		- Improved performance of `parse_regex` and `parse_regex_all` by pre-computing capture group names and indices at compile time, replacing name-based hash lookups with direct index-based access at runtime.
		
		[PR #1811](https://github.com/vectordotdev/vrl/pull/1811) by [@thomasqueirozb](https://github.com/thomasqueirozb)
		- Improved performance of `truncate` function if suffix parameter is provided.
		
		[PR #1784](https://github.com/vectordotdev/vrl/pull/1784) by [@JakubOnderka](https://github.com/JakubOnderka)
		- Improved performance of `parse_regex_all` when using a literal regex pattern in concurrent workloads.
		
		[PR #1798](https://github.com/vectordotdev/vrl/pull/1798) by [@thomasqueirozb](https://github.com/thomasqueirozb)
		
		#### Fixes
		
		- Fixed a panic in `parse_key_value`, `parse_cef`, `decode_mime_q`, and `parse_ruby_hash` on inputs with lines ≥ 65,535 bytes. This is a workaround until [rust-bakery/nom#1867](https://github.com/rust-bakery/nom/issues/1867) is fixed.
		
		[PR #1848](https://github.com/vectordotdev/vrl/pull/1848) by [@pront](https://github.com/pront)
		- Fixed `find`’s type definition and documentation. Previously, the function advertised its return type as an integer, but never as nullable. Now it correctly states that the function returns `null` when `value` doesn’t match `pattern`.
		
		[PR #1812](https://github.com/vectordotdev/vrl/pull/1812) by [@JakubOnderka](https://github.com/JakubOnderka)
		
		"""#

	commits: [
		{sha: "b017893944effcc9c2904c1d5e4cdfb871c8f41b", date: "2026-06-01 17:34:03 UTC", description: "preserve writer windows when generating ACKs", pr_number: 25531, type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 340, deletions_count: 38},
		{sha: "067ef75e6474e885203885826f0d1f8ca2f93897", date: "2026-06-01 18:45:28 UTC", description: "emit breaking: true for .breaking.md changelog fragments", pr_number: 25556, type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 11, deletions_count: 0},
		{sha: "29b089daf44056173bf3f4467df257217fdbfb2a", date: "2026-06-02 17:30:11 UTC", description: "propagate component span into spawned tasks", pr_number: 25521, type: "fix", breaking_change: false, author: "Yoenn Burban", files_count: 42, insertions_count: 228, deletions_count: 127},
		{sha: "5072d8b86de09962c1ef1aed9cd36db3ae9c5dd4", date: "2026-06-02 19:31:35 UTC", description: "improve log_namespace docs and warn on log_schema", pr_number: 25560, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 18, deletions_count: 2},
		{sha: "c0806d3b31323b1419ae39242b341ff008e0ac87", date: "2026-06-03 14:24:14 UTC", description: "correct CUE metric catalog discrepancies", pr_number: 25559, type: "fix", breaking_change: false, author: "Yoenn Burban", files_count: 9, insertions_count: 150, deletions_count: 141},
		{sha: "06af024431c44fdb3c55b09bd4deb1a3bcd70dd1", date: "2026-06-03 20:01:49 UTC", description: "v0.56.0 release", pr_number: 25568, type: "chore", breaking_change: false, author: "Thomas", files_count: 62, insertions_count: 699, deletions_count: 260},
		{sha: "0260a58141fd365e26d1a46cf8f553f8bc0c3a9a", date: "2026-06-05 15:12:57 UTC", description: "encode batches with Arrow Flight", pr_number: 25519, type: "feat", breaking_change: false, author: "Flavio Cruz", files_count: 16, insertions_count: 435, deletions_count: 1046},
		{sha: "b697f53fbb02145f93b30ef3dbc1d73ba907993f", date: "2026-06-05 16:07:09 UTC", description: "document verbatim substitution of structural characters in env var interpolation", pr_number: 25581, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "cbc44892bc90a4bfcf198bb0d4500d2b8137d120", date: "2026-06-05 17:48:51 UTC", description: "clean up and improve examples with links to the official documentation", pr_number: 25569, type: "docs", breaking_change: false, author: "Flavio Cruz", files_count: 2, insertions_count: 60, deletions_count: 15},
		{sha: "6b0201ae7edabbba216b43748dbbe8543d8fe5f9", date: "2026-06-05 22:27:50 UTC", description: "update SMP CLI to 0.27.0", pr_number: 25588, type: "chore", breaking_change: false, author: "George Hahn", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b9c2b7cf95ecc768a76c4ef37bfeb5b884cce034", date: "2026-06-08 16:46:00 UTC", description: "add Git Conventions section", pr_number: 25592, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "ef57b2316e544c4b05b69a7493344fabffec02e2", date: "2026-06-08 16:52:20 UTC", description: "prevent panic on multi-byte UTF-8 gauge value prefix", pr_number: 25582, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 15, deletions_count: 1},
		{sha: "637cfb748fb47e8983e5d7b45b78bd2ff3908a97", date: "2026-06-08 18:00:49 UTC", description: "bump vrl to latest main and mlua from 0.10.5 to 0.11.6", pr_number: 25594, type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 36, deletions_count: 32},
		{sha: "2db0d41ca98d519dacab4e56e3e77e52b73e5c4c", date: "2026-06-08 19:20:12 UTC", description: "Update OTLP experiments' lading configs", pr_number: 25595, type: "chore", breaking_change: false, author: "George Hahn", files_count: 2, insertions_count: 6, deletions_count: 2},
		{sha: "c78b67f64961acd2eb3437edfa499f19c0a62016", date: "2026-06-08 19:29:43 UTC", description: "Fix typo in aggregator documentation", pr_number: 25574, type: "docs", breaking_change: false, author: "Tuer·maimaitiaili Ba", files_count: 2, insertions_count: 2, deletions_count: 1},
		{sha: "85fba18494f6c631bf25831d4f4a8cc16464d60f", date: "2026-06-08 19:34:13 UTC", description: "enhance note on `path` templates", pr_number: 25583, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "0a55506e0b73f571ec0afbddfb2ea4bea81e1887", date: "2026-06-08 19:35:24 UTC", description: "replace semantic PR title action with custom script", pr_number: 25593, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 40, deletions_count: 306},
		{sha: "b1467add17ac84e1acd93f3bd22b00465535b7ba", date: "2026-06-10 13:49:35 UTC", description: "ignore RUSTSEC-2026-0173 (proc-macro-error2 unmaintained via mlua_derive)", pr_number: 25605, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "3fb79f6c365a89a514f828935f98f8908f674562", date: "2026-06-10 14:01:27 UTC", description: "replace check-spelling with typos", pr_number: 25597, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 91, insertions_count: 230, deletions_count: 2453},
		{sha: "1d28ae80652eb8c2fe52b75c8e8fb281cf9109ec", date: "2026-06-10 15:25:59 UTC", description: "split docs review label workflow to fix fork PR permissions", pr_number: 25596, type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 78, deletions_count: 29},
		{sha: "6a75fb272bad5768f0276260859be5fea041766e", date: "2026-06-10 15:41:04 UTC", description: "Add per-component CPU usage metric (poll-hook)", pr_number: 25317, type: "feat", breaking_change: false, author: "Yoenn Burban", files_count: 14, insertions_count: 565, deletions_count: 39},
		{sha: "cdd6ebb174034b7fcb939fb1249911fad5b3006c", date: "2026-06-10 15:45:33 UTC", description: "add Arrow IPC compression option", pr_number: 25586, type: "feat", breaking_change: false, author: "Flavio Cruz", files_count: 5, insertions_count: 123, deletions_count: 2},
		{sha: "169b5a9d1fd49cf14d18b193df4944eb96a6b536", date: "2026-06-10 17:11:13 UTC", description: "use ClickHouse Identifier query parameters for safe database/table names", pr_number: 25591, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 82, deletions_count: 11},
		{sha: "9acf0f838eda502ab6174428ac83b78c6cfa609a", date: "2026-06-10 17:20:32 UTC", description: "add user_agent config option", pr_number: 25561, type: "feat", breaking_change: false, author: "Flavio Cruz", files_count: 4, insertions_count: 79, deletions_count: 1},
		{sha: "16df149c95f9ad572d44eb3a0734c1ec0e888de5", date: "2026-06-10 18:16:54 UTC", description: "add debug cfg gate to test for debug builds", pr_number: 25601, type: "fix", breaking_change: false, author: "Leon White", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "dcd4f03898393bf6482b9fe15aee473f096bfec9", date: "2026-06-10 18:18:02 UTC", description: "prevent error message when DOQ is encountered", pr_number: 25495, type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 9, deletions_count: 18},
		{sha: "984aa76b77a0c9fb22fcfb161b78247847912452", date: "2026-06-10 22:12:46 UTC", description: "bump rust toolchain to 1.95 and fix clippy lints", pr_number: 25606, type: "chore", breaking_change: false, author: "Thomas", files_count: 22, insertions_count: 54, deletions_count: 63},
		{sha: "2c9ce107942f98254596b42056b3b9c5988f9f9a", date: "2026-06-11 13:05:38 UTC", description: "add --raise-fd-limit CLI flag to raise file descriptor soft limit", pr_number: 25251, type: "feat", breaking_change: false, author: "Vitalii Parfonov", files_count: 4, insertions_count: 194, deletions_count: 1},
		{sha: "ae344accec636e1649c6403c9fc68e92bfbfa2e3", date: "2026-06-11 15:24:20 UTC", description: "bump the serde group across 1 directory with 2 updates", pr_number: 25584, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "9c6e197e098b917754ba435d8074f0a716195bdf", date: "2026-06-11 18:56:29 UTC", description: "bump the artifact group across 1 directory with 2 updates", pr_number: 25469, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "212651b08e066b775a8e5e253c1d37c3715e3476", date: "2026-06-11 19:26:50 UTC", description: "remove codename field from release definitions and template", pr_number: 25554, type: "chore", breaking_change: false, author: "Thomas", files_count: 92, insertions_count: 98, deletions_count: 201},
		{sha: "bcaebb2922f860e4ac55368d06daa8551837331c", date: "2026-06-12 14:19:30 UTC", description: "parse-first config interpolation", pr_number: 25603, type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 323, deletions_count: 1},
		{sha: "d93f177db4bbc0f1270abf9df657aa1be471d4a6", date: "2026-06-12 14:47:59 UTC", description: "Avoid using unnecessary LazyLock", pr_number: 25609, type: "chore", breaking_change: false, author: "Jakub Onderka", files_count: 3, insertions_count: 62, deletions_count: 66},
		{sha: "744931a304779d9d6a7f5ea3e3ad7333783cd264", date: "2026-06-12 14:50:28 UTC", description: "simplify RFC template and process", pr_number: 25611, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 13, deletions_count: 29},
		{sha: "8819af6b411a6877ac5596e3ed80714f696be9f3", date: "2026-06-12 19:32:43 UTC", description: "add AI-assisted component maturity evaluation skill", pr_number: 25518, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 391, deletions_count: 0},
		{sha: "6f3a5d8d2acd3f27061721cc77bf677be40368cd", date: "2026-06-12 19:39:39 UTC", description: "handle UI events separately to prevent UI freeze", pr_number: 25604, type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 5, insertions_count: 176, deletions_count: 184},
		{sha: "ae8cc915d66a4a3028bd846527d58765dfbfe6fc", date: "2026-06-12 20:48:33 UTC", description: "fix Integration/E2E badge link in README", pr_number: 25618, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0e8cddd49792f6fe35c972e9067c04f368e3ee9d", date: "2026-06-15 15:12:37 UTC", description: "bump tokio-postgres to 0.7.18 (RUSTSEC-2026-0178)", pr_number: 25623, type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 218, deletions_count: 83},
		{sha: "55a85ee9d9781f5e8f92a77400371d42c3602a84", date: "2026-06-15 16:19:34 UTC", description: "sync AI_POLICY.md", pr_number: 25626, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b9c0755e1ce547605096d3e28a2942845162ffe2", date: "2026-06-15 16:38:57 UTC", description: "serialize enrichment table registry loads", pr_number: 25615, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "816563e10ea51c9221a8c5b10ec8577acb6405d1", date: "2026-06-15 18:22:04 UTC", description: "bump tmp from 0.2.5 to 0.2.7 in /website", pr_number: 25617, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "052bdfe8869c4428cdd2bb299e00984862307dfe", date: "2026-06-16 00:47:45 UTC", description: "bump VRL and remove sha-1 dep", pr_number: 25634, type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 5, deletions_count: 27},
		{sha: "5563109696f38766a9df5b44ca77cae2c94b209b", date: "2026-06-16 14:22:26 UTC", description: "release tooling fixes from #25442", pr_number: 25629, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 62, deletions_count: 43},
		{sha: "0bcb4c6ff9442804abf92746a57b74c70d857619", date: "2026-06-16 15:45:20 UTC", description: "Add option to rate limit `internal_logs` sources", pr_number: 25635, type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 213, deletions_count: 17},
		{sha: "d0d55b021c34f665cebe775b5db9eeb686fdada2", date: "2026-06-16 20:08:49 UTC", description: "add deprecation fragment commands", pr_number: 25638, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 11, insertions_count: 1223, deletions_count: 19},
		{sha: "182129080ab7c5fe80461560ac19569082fb97e2", date: "2026-06-17 13:17:28 UTC", description: "antithesis harness, durability harness scenario", pr_number: 25562, type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 29, insertions_count: 1534, deletions_count: 4},
		{sha: "ad5eff1e979ba057fa5fba4c7697696b4eb6905d", date: "2026-06-17 15:00:34 UTC", description: "install vdev via --manifest-path; drop VDEV_VERSION pin", pr_number: 25456, type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 45, deletions_count: 13},
		{sha: "3dbc4f447e7e5dc30f0332fc0d90554b0ff2cddc", date: "2026-06-17 16:55:49 UTC", description: "Antithesis Vector end-to-end ack scenario", pr_number: 25571, type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 9, insertions_count: 344, deletions_count: 10},
		{sha: "2932e8466db4f65359a96cf56304a4845bd37174", date: "2026-06-17 17:14:48 UTC", description: "only save cargo registry cache from master", pr_number: 25643, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 16, deletions_count: 1},
		{sha: "07cbe62cf4717c6702989c9940ddc4126b83c0ba", date: "2026-06-17 18:11:34 UTC", description: "bump version to 0.3.4", pr_number: 25645, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "a86cf36456ab48fa2bc12078e0d125049f83f58f", date: "2026-06-17 19:06:13 UTC", description: "use multi-stage Dockerfile to skip source copy for integration runner", pr_number: 25646, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 14, deletions_count: 8},
		{sha: "2d0d0fbb2ddf1530e028864fb23a8e5af91b6e7d", date: "2026-06-17 19:27:04 UTC", description: "Remove component aliases except for mezmo and greptimedb", pr_number: 25648, type: "chore", breaking_change: false, author: "Thomas", files_count: 7, insertions_count: 0, deletions_count: 9},
		{sha: "5d41252a9f307e1c2774a42e2f6abb375b2fea47", date: "2026-06-17 20:33:00 UTC", description: "handle concatenated gzip streams", pr_number: 25614, type: "fix", breaking_change: false, author: "Thomas", files_count: 13, insertions_count: 72, deletions_count: 13},
		{sha: "8a58f90704be9e289d4b2ac8fb16618ffa27b1e3", date: "2026-06-22 13:30:36 UTC", description: "bump cmake to 0.1.58 for VS 2026 support on Windows CI", pr_number: 25653, type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "5fc9ef86f8800746a3c02a12ffe309df6d11746f", date: "2026-06-22 14:19:35 UTC", description: "post-processing in SourceSender", pr_number: 25563, type: "feat", breaking_change: false, author: "Josué AGBEKODO", files_count: 5, insertions_count: 340, deletions_count: 7},
		{sha: "0e13503056a6f5203daa5060c825509208588fd0", date: "2026-06-22 14:20:47 UTC", description: "wrap apt-get calls with timeout 30m to prevent CI hangs", pr_number: 25661, type: "chore", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 8, deletions_count: 8},
		{sha: "127ea00dd37859cebb84a87f5b7a486681414cf4", date: "2026-06-22 14:29:03 UTC", description: "add deprecation.d fragment system", pr_number: 25442, type: "feat", breaking_change: false, author: "Thomas", files_count: 24, insertions_count: 533, deletions_count: 126},
		{sha: "882c6b868be3131eadf68be2467f64b12f268f33", date: "2026-06-22 14:36:54 UTC", description: "bump form-data from 4.0.5 to 4.0.6 in /website", pr_number: 25631, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 14, deletions_count: 7},
		{sha: "7546667bd4008a2a0fe82fd91e6d8d71bcf9fa73", date: "2026-06-22 15:09:06 UTC", description: "improve semantic meaning warning", pr_number: 25608, type: "chore", breaking_change: false, author: "Yoenn Burban", files_count: 6, insertions_count: 70, deletions_count: 6},
		{sha: "56174251ef4134462d094c9b745952260d15b6df", date: "2026-06-22 15:10:22 UTC", description: "fix metric type comparison using wrong operand in sort", pr_number: 25621, type: "fix", breaking_change: false, author: "Yoenn Burban", files_count: 2, insertions_count: 22, deletions_count: 1},
		{sha: "f60d4659e009d133637e8f2c131805ba7beb1f7b", date: "2026-06-22 16:39:01 UTC", description: "bump undici from 7.24.1 to 7.28.0 in /website", pr_number: 25658, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "044d4f8616751db7aee37a7807746ce78b02a9b8", date: "2026-06-22 16:54:19 UTC", description: "auto-tag vdev on version bump in Cargo.toml", pr_number: 24061, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 54, deletions_count: 0},
		{sha: "b6d00bfc083244981180585ccab4625beedea117", date: "2026-06-22 17:17:07 UTC", description: "bump tsx from 4.21.0 to 4.22.4", pr_number: 25663, type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 165, deletions_count: 178},
		{sha: "62895868b9cac9010f41c6ac8773c34569b49758", date: "2026-06-22 18:33:12 UTC", description: "fix duplicated word in Avro decoder doc comment", pr_number: 25651, type: "docs", breaking_change: false, author: "SEONGHYUN HONG", files_count: 24, insertions_count: 25, deletions_count: 25},
		{sha: "c0cc69c3d2348ddf1d75ab411bf1a9999b55c8dc", date: "2026-06-23 13:56:06 UTC", description: "Change liveness check for eventually_conservation", pr_number: 25667, type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 6, insertions_count: 10, deletions_count: 84},
		{sha: "2567199d8d2a5c312faff057f632da7352bd7344", date: "2026-06-23 14:54:33 UTC", description: "close the connection on a malformed frame", pr_number: 25664, type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 79, deletions_count: 9},
		{sha: "9347576a3e2673f9510267d4ba6163c5c83ea844", date: "2026-06-24 13:40:01 UTC", description: "cap decompressed body to prevent OOM", pr_number: 25488, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 406, deletions_count: 28},
		{sha: "54de9e1f757861b4bf9bdc124227f45c8c9ef713", date: "2026-06-24 16:51:55 UTC", description: "sync docs-review-workflows", pr_number: 25675, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 4, deletions_count: 1},
		{sha: "4adabd2ff4927e40f3b178e892e6a24d7c314b28", date: "2026-06-24 17:41:28 UTC", description: "fix stale references and add FIPS disclaimer", pr_number: 25674, type: "docs", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 8, deletions_count: 2},
		{sha: "3ee87dbc7975d86b6f3c87c92bdd28b0a24bd520", date: "2026-06-24 17:59:28 UTC", description: "fix race between labeler and docs-review-on-hold workflow", pr_number: 25676, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 8, deletions_count: 0},
		{sha: "16ef254a3c0874b3a108a04475944ae3c85cef53", date: "2026-06-24 19:07:59 UTC", description: "Add exact_fingerprint mode for lower memory usage", pr_number: 25640, type: "feat", breaking_change: false, author: "ArunPiduguDD", files_count: 6, insertions_count: 268, deletions_count: 4},
		{sha: "7d8c120546389c2b47efa74ab7d86808ef05503d", date: "2026-06-24 20:30:11 UTC", description: "convert test config strings from TOML to YAML", pr_number: 25673, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 49, insertions_count: 2407, deletions_count: 2654},
		{sha: "24bd0604cabb40d3b1b8c8d87042729ff0efa07a", date: "2026-06-24 20:47:13 UTC", description: "remove timberio/chronicle-emulator", pr_number: 25670, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 83, deletions_count: 145},
		{sha: "5397fe88639a2661f73668ec41e5a3fb29ae6428", date: "2026-06-25 14:31:56 UTC", description: "sync docs-review-workflows", pr_number: 25682, type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 112, deletions_count: 45},
		{sha: "5befbcc5293452b5c09634df2a2a9f39196c7059", date: "2026-06-25 18:07:31 UTC", description: "add workspace lints string_slice, await_holding_lock, let_underscore_must_use", pr_number: 25669, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 98, insertions_count: 409, deletions_count: 202},
		{sha: "2f4ba864e8f16250e6f8c9890e82bccf38745e60", date: "2026-06-25 18:51:48 UTC", description: "note known restart bugs in disk buffer docs", pr_number: 25679, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "a1d74ad1fc0d6493b2b23375255ef4df1e1ab101", date: "2026-06-25 19:49:38 UTC", description: "fix two failures in cleanup-ghcr-images workflow", pr_number: 25684, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 17},
		{sha: "0141894cb40218aff7fcb308dd32847dd875bf74", date: "2026-06-25 20:07:17 UTC", description: "update stable component criteria", pr_number: 25681, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 7},
		{sha: "0f80fafec6190ed0cbde1d0e39f961d782ccbd0d", date: "2026-06-26 23:41:46 UTC", description: "refactor Blob code out of azure_common", pr_number: 25689, type: "chore", breaking_change: false, author: "Jed Laundry", files_count: 8, insertions_count: 274, deletions_count: 284},
		{sha: "31f80dfc3e4183f416408d3151f7747b4cca9263", date: "2026-06-29 15:24:44 UTC", description: "add dedicated section for security and critical bug exceptions", pr_number: 25694, type: "docs", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 10, deletions_count: 5},
		{sha: "9be7d86c2d0acfe2eadca69b5937104fb7ca36d4", date: "2026-06-29 15:29:51 UTC", description: "make azure_blob modules public", pr_number: 25695, type: "chore", breaking_change: false, author: "Yoenn Burban", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "be2b50c0d79fb92fe485120028ff9c8d9fa57ca0", date: "2026-06-29 17:07:28 UTC", description: "Add per-tag cache_size_per_key override in probabilistic mode", pr_number: 25650, type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 5, insertions_count: 378, deletions_count: 28},
		{sha: "5485a9ad58e0692fc066c8159e2d1591ce523ef2", date: "2026-06-29 17:07:39 UTC", description: "bump databricks-zerobus-ingest-sdk to 2.3.0", pr_number: 25696, type: "chore", breaking_change: false, author: "Flavio Cruz", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "d69e0c496752ba609b3d216d501680a3facec107", date: "2026-06-29 18:19:31 UTC", description: "replace timing-based sleep+collect_ready with collect_n in firehose tests", pr_number: 25701, type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 9, deletions_count: 12},
		{sha: "50eca0ca451b5d6946a68c4eca99c677cc7b3bcb", date: "2026-06-29 18:21:55 UTC", description: "add gRPC max connection age", pr_number: 25660, type: "feat", breaking_change: false, author: "Filip Pytloun", files_count: 9, insertions_count: 734, deletions_count: 18},
		{sha: "c96234d1d6c9ecc1b78765fbb0e2c9bb322d567c", date: "2026-06-29 19:31:06 UTC", description: "improve clarity of the `source send cancelled` error message", pr_number: 25686, type: "enhancement", breaking_change: false, author: "Clément Delafargue", files_count: 2, insertions_count: 6, deletions_count: 1},
		{sha: "7302a261a170d3745d76f2a0d5f16519a2f6aab1", date: "2026-06-29 19:31:46 UTC", description: "bump linkify-it from 5.0.0 to 5.0.1 in /scripts/environment/npm-tools", pr_number: 25702, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 3},
		{sha: "a7a6282a9aa66bf1ed32c9665ad257b7cad8f0c8", date: "2026-06-29 19:42:58 UTC", description: "convert test fixture and doc TOML configs to YAML", pr_number: 25703, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 126, insertions_count: 3271, deletions_count: 5822},
		{sha: "97ae51e37d9e63d1782d9e1c7069f8011ea8e080", date: "2026-06-29 20:55:57 UTC", description: "replace default-msvc with default on Windows", pr_number: 25690, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 24, insertions_count: 61, deletions_count: 98},
		{sha: "2f497a3dabe474771eb09161713e641e61eb92d8", date: "2026-06-29 23:05:59 UTC", description: "add temperature metrics collector", pr_number: 25607, type: "feat", breaking_change: false, author: "somaz", files_count: 5, insertions_count: 141, deletions_count: 13},
		{sha: "b4d73500bd1efd294f6d0852f73ddcbeb4870256", date: "2026-06-30 01:34:27 UTC", description: "add --features flag and FEATURES env var support to build/check/test commands", pr_number: 25688, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 89, deletions_count: 45},
		{sha: "727ec9f7deb6ca27445626d83dcfd645e623d5e5", date: "2026-06-30 13:47:44 UTC", description: "make chunk size configurable", pr_number: 25637, type: "feat", breaking_change: false, author: "Sergey Kacheev", files_count: 10, insertions_count: 149, deletions_count: 25},
		{sha: "3a899b67d7d5f2968b40cbfe412cf4d3b0b31934", date: "2026-06-30 14:34:36 UTC", description: "upgrade react-use to 17.6.1", pr_number: 25707, type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 53, deletions_count: 48},
		{sha: "e050273358cb17469c67b6ca17be7c2af06173fa", date: "2026-06-30 14:48:58 UTC", description: "upgrade @babel/core to 7.29.7", pr_number: 25708, type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 788, deletions_count: 731},
		{sha: "e62995a0b185f9411c9013c4e8638a5895835b85", date: "2026-06-30 15:53:16 UTC", description: "render deprecated enum variants with yellow deprecation box", pr_number: 25706, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 89, deletions_count: 18},
		{sha: "4a4cff5e53e857c334d1bb00962005fbe55abd0f", date: "2026-06-30 15:55:29 UTC", description: "add an option to preserve memory enrichment table state on reload", pr_number: 25547, type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 17, insertions_count: 428, deletions_count: 44},
		{sha: "33dca2f761aae7c6a3342928a71c9d9502b0d595", date: "2026-07-01 13:29:55 UTC", description: "split component diffs", pr_number: 25712, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 465, deletions_count: 204},
		{sha: "006b5d6ff89f14ec4fcbfa72e02fcfd9e14ad128", date: "2026-07-01 13:50:27 UTC", description: "bump alpine from 3.23 to 3.24 in /distribution/docker/distroless-static in the docker-images group", pr_number: 25721, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "02a01e5f84e015361ce17b34a9b9952b00daf052", date: "2026-07-01 18:05:55 UTC", description: "bump aws-actions/amazon-ecr-login from 2.0.1 to 2.1.6", pr_number: 25730, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "5c1cf77c380e56978703a2b9643ac724c58f113a", date: "2026-07-01 18:07:27 UTC", description: "bump softprops/action-gh-release from 2.5.0 to 3.0.1", pr_number: 25729, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "8d155368d369ccdbc33dd0635c8f6d9d78c9bf34", date: "2026-07-01 18:16:46 UTC", description: "bump debian from `b6e2a15` to `28de087` in /distribution/docker/debian in the docker-images group", pr_number: 25718, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "5ca0c52d204d6bb649e4554403c36085f7351b64", date: "2026-07-01 18:16:59 UTC", description: "bump alpine from 3.23 to 3.24 in /distribution/docker/alpine in the docker-images group", pr_number: 25719, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "a5c43350db0b8595c727ad01a15672d79792e933", date: "2026-07-01 18:17:10 UTC", description: "bump the docker-images group in /distribution/docker/distroless-libc with 2 updates", pr_number: 25720, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "bb2a481a8f10b88859b3aabd409599184999e0d7", date: "2026-07-01 18:43:07 UTC", description: "validate VRL with --no-environment", pr_number: 25161, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 20, insertions_count: 543, deletions_count: 68},
		{sha: "fa342dc88aa1fa2e1a873ada5ccae639ff9b4df1", date: "2026-07-01 19:18:50 UTC", description: "bump tracing-subscriber from 0.3.22 to 0.3.23 in the tracing group across 1 directory", pr_number: 25010, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "3a8ff2a5fbc16f1f79ceb02a3b631464d3275062", date: "2026-07-01 19:29:26 UTC", description: "accept short-form syslog severity keywords in encoder", pr_number: 25731, type: "fix", breaking_change: false, author: "Vitalii Parfonov", files_count: 2, insertions_count: 42, deletions_count: 0},
		{sha: "ab7fee3d83f866417924d8b3cdd673e23c75bd20", date: "2026-07-01 19:42:50 UTC", description: "bump docker/setup-qemu-action from 4.0.0 to 4.2.0", pr_number: 25728, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "cf85073e17d8142461c90be64f46368d33da45a2", date: "2026-07-01 20:08:49 UTC", description: "consolidate docker dependabot entries using directories", pr_number: 25733, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 46},
		{sha: "0849d3c33253b12ca15473eb448d03c2268bcb8d", date: "2026-07-01 20:32:27 UTC", description: "correct operand order in octet-counting framer underflow", pr_number: 25657, type: "fix", breaking_change: false, author: "hhh6593", files_count: 2, insertions_count: 23, deletions_count: 1},
		{sha: "b055b181f5869e237c3fa1680135eb9255c311a1", date: "2026-07-02 13:52:56 UTC", description: "reattach fanout on cross-type producer key swap", pr_number: 25725, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 31, deletions_count: 25},
		{sha: "5416bbd5df6064c7249ab11409f73bdc9d4b7b76", date: "2026-07-02 15:34:28 UTC", description: "bump clap_complete from 4.6.5 to 4.6.7 in the clap group across 1 directory", pr_number: 25714, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 7, deletions_count: 7},
		{sha: "0babf509b7c823122b5b1ffc57c668ce05c7f36b", date: "2026-07-02 15:39:26 UTC", description: "bump actions/labeler from 6.0.1 to 6.1.0", pr_number: 25735, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "ac50b4cf9ab7b9acc8906041f17dab953ef42fa9", date: "2026-07-02 16:56:16 UTC", description: "use documented retry defaults", pr_number: 25743, type: "fix", breaking_change: false, author: "Filip Pytloun", files_count: 2, insertions_count: 27, deletions_count: 1},
		{sha: "be6367f5d90d6fcc3d7d98b30a2402d1582dba2e", date: "2026-07-02 17:10:38 UTC", description: "bump github/codeql-action/upload-sarif from 4.35.2 to 4.36.2", pr_number: 25736, type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "57450f28f5bd86da234b6d09b27ffc07d76e9118", date: "2026-07-02 19:58:58 UTC", description: "add multicast_interface option for UDP multicast group joining", pr_number: 25711, type: "enhancement", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 64, deletions_count: 12},
		{sha: "8b04f85053900cd8160895d9549b1b14a0b56ce8", date: "2026-07-06 14:28:45 UTC", description: "add a regression test for component cpu metric", pr_number: 25672, type: "chore", breaking_change: false, author: "Yoenn Burban", files_count: 3, insertions_count: 195, deletions_count: 0},
		{sha: "3743f3b8b17094b1b52807b0c5308b0198c0ef53", date: "2026-07-06 14:33:23 UTC", description: "remove blanket allows", pr_number: 25748, type: "chore", breaking_change: false, author: "claire", files_count: 4, insertions_count: 17, deletions_count: 5},
		{sha: "0f7f98c118918def351c759c92aa378d366fa129", date: "2026-07-06 19:19:08 UTC", description: "Make `HttpClient` generic over connector", pr_number: 25742, type: "chore", breaking_change: false, author: "Adrien Guillo", files_count: 4, insertions_count: 88, deletions_count: 34},
		{sha: "953a4490531fb9c242a7fa7ca98bebeb4a21027d", date: "2026-07-07 14:21:09 UTC", description: "guarantee measurable CPU usage in component CPU metric regression tests", pr_number: 25755, type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 71, deletions_count: 11},
		{sha: "2282ccfb55d7113c27fca87c86099df05ce5e72d", date: "2026-07-07 15:00:13 UTC", description: "bump anyhow, memmap2, quick-junit to fix RUSTSEC advisories", pr_number: 25753, type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 45, deletions_count: 43},
		{sha: "cfa71fbefc01e982f8362226b22375c25d2e152a", date: "2026-07-07 15:20:17 UTC", description: "UX overhaul", pr_number: 25759, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 570, deletions_count: 118},
		{sha: "cb5b7f25d328046e579d7889b62dd30fdd7eebba", date: "2026-07-07 18:59:43 UTC", description: "add highlights blog post for July 2026", pr_number: 25744, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 184, deletions_count: 0},
		{sha: "7ba91d165f2d498a36d5d0ed95643296d0869bd7", date: "2026-07-07 22:31:11 UTC", description: "bump wasm-pack to 0.15.0 and fix playground LICENSE", pr_number: 25769, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 365, deletions_count: 2},
		{sha: "f867c6352190ddadb8e13ca915e7ca4e44f29fc9", date: "2026-07-08 13:46:00 UTC", description: "add scale regression cases", pr_number: 25762, type: "chore", breaking_change: false, author: "Armand Thibaudon", files_count: 30, insertions_count: 1264, deletions_count: 0},
		{sha: "960218ef66c155770b7f9738e65e3ea5940169f6", date: "2026-07-08 14:14:08 UTC", description: "include attributes from X-Amz-Firehose-Common-Attributes header in the log event", pr_number: 24914, type: "feat", breaking_change: false, author: "tchanturia", files_count: 6, insertions_count: 601, deletions_count: 13},
		{sha: "2de6735b32f57ffcd5e7addbc87e7bd860dd9003", date: "2026-07-08 17:38:22 UTC", description: "consolidate preview site links into a single sticky comment", pr_number: 25777, type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 112, deletions_count: 30},
		{sha: "b2ab8fea48ac5baa31bd789b75bafe37f6cb2df5", date: "2026-07-08 18:58:04 UTC", description: "trigger preview site deploys via labels instead of branch naming", pr_number: 25780, type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 66, deletions_count: 8},
		{sha: "20350a0488530fa227e87d18ac16783653dc915e", date: "2026-07-08 20:05:32 UTC", description: "stop rejecting branch names with slashes or dots in preview trigger", pr_number: 25783, type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 8, deletions_count: 25},
		{sha: "b725f6e0fae556f5dae5c120e1682e5a5ddbab85", date: "2026-07-09 13:28:31 UTC", description: "bump crossbeam-epoch to 0.9.20 (RUSTSEC-2026-0204)", pr_number: 25781, type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 6},
		{sha: "ff3719d2f23e76a94d5781fd0a844830bf38bb09", date: "2026-07-09 14:57:15 UTC", description: "preserve large integer precision in VRL Playground", pr_number: 25745, type: "fix", breaking_change: false, author: "Jared Patterson", files_count: 5, insertions_count: 76, deletions_count: 5},
		{sha: "014a5878ceffc77a92112880f67a440f10c46b4a", date: "2026-07-09 14:15:40 UTC", description: "add multiple endpoint strategies", pr_number: 25662, type: "feat", breaking_change: false, author: "Filip Pytloun", files_count: 4, insertions_count: 1930, deletions_count: 28},
		{sha: "31327628bba329102a31e338b11cadcf987d7ba1", date: "2026-07-09 14:42:24 UTC", description: "restore zero-copy fast path for HTTP body collection", pr_number: 25760, type: "fix", breaking_change: false, author: "Armand Thibaudon", files_count: 1, insertions_count: 41, deletions_count: 4},
		{sha: "7fbcc6ab42aef71878c600f799bd3b2c0cb11af1", date: "2026-07-09 18:52:15 UTC", description: "pin datadog-metrics e2e agent_version away from v3 series API", pr_number: 25789, type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 7, deletions_count: 2},
		{sha: "4a4b644811868f14b5e9c309391ff327d3781d6d", date: "2026-07-09 20:10:47 UTC", description: "add configurable HTTP/2 keepalive for gRPC connections", pr_number: 25765, type: "enhancement", breaking_change: false, author: "Rob Blafford", files_count: 5, insertions_count: 111, deletions_count: 2},
		{sha: "57f90b6138fbdeba650ec52faf1f500bfb289430", date: "2026-07-09 20:37:05 UTC", description: "replace bare string paths with explicit path macros", pr_number: 25778, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 107, insertions_count: 1770, deletions_count: 1146},
		{sha: "1192edbe80a6d0262b5a90846ef165f4fa033c02", date: "2026-07-09 23:59:33 UTC", description: "Bump zlib-rs from 0.6.0 to 0.6.6", pr_number: 25797, type: "chore", breaking_change: false, author: "Aidan Nguyen", files_count: 2, insertions_count: 5, deletions_count: 2},
		{sha: "0bce35e61a85a34e718c4f4f23808af05ab8a337", date: "2026-07-10 00:05:30 UTC", description: "replace unsafe NonZeroUsize::new_unchecked with safe const block", pr_number: 25798, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 5, deletions_count: 2},
		{sha: "410da89a0ed42c523143da89fffeb7f6402833e0", date: "2026-07-10 14:01:40 UTC", description: "never emit partial ACKs", pr_number: 25700, type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 284, deletions_count: 41},
		{sha: "30f0978d8941debf1edff5bc16ed6cc539b76e20", date: "2026-07-11 15:34:22 UTC", description: "add a runtime-swappable TLS acceptor to TCP sources", pr_number: 25800, type: "enhancement", breaking_change: false, author: "Adrien Guillo", files_count: 13, insertions_count: 361, deletions_count: 34},
		{sha: "37ae696a411d52fde58154c891dacc916ed1c4a6", date: "2026-07-13 14:42:06 UTC", description: "add missing sources-http_server feature dependency", pr_number: 25814, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5d4bbb17e9ea10a095b4cf0585cb0bbf23000c18", date: "2026-07-13 14:57:06 UTC", description: "add website pages for renumbered error codes 112, 113, 114", pr_number: 25816, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 64, deletions_count: 0},
		{sha: "780f009e966582a6da06446be6dec98afeb99df5", date: "2026-07-13 17:09:13 UTC", description: "cap payload size to prevent OOM", pr_number: 25818, type: "fix", breaking_change: false, author: "Thomas", files_count: 8, insertions_count: 99, deletions_count: 30},
		{sha: "3162ed1a2e5e8d3f210134607518a26aa01e1a37", date: "2026-07-13 19:29:40 UTC", description: "cap compressed and decompressed payload size across network sources to prevent OOM", pr_number: 25819, type: "fix", breaking_change: false, author: "Thomas", files_count: 41, insertions_count: 925, deletions_count: 451},
		{sha: "9c87890b6b0bfbeb15e364bc3e706d5f75637c3c", date: "2026-07-13 19:32:37 UTC", description: "announce structural interpolation removal", pr_number: 25822, type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 27, deletions_count: 0},
		{sha: "57fed991083c9a43d5fda2d5b3bf4718aedf6526", date: "2026-07-13 20:28:39 UTC", description: "reject nested compressed frames to prevent stack exhaustion", pr_number: 25825, type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 57, deletions_count: 1},
		{sha: "e835dcb022a1b207d65baa92d67cead393404f73", date: "2026-07-13 20:47:21 UTC", description: "add `upgrade` method to `WeakTlsAcceptorReloader`", pr_number: 25823, type: "chore", breaking_change: false, author: "Adrien Guillo", files_count: 1, insertions_count: 12, deletions_count: 25},
		{sha: "fdfd3a0cc1ebcdabfa8eb78c7f6f13d496c4e5ba", date: "2026-07-13 21:01:20 UTC", description: "map capped-body rejections to client errors in firehose and splunk_hec sources", pr_number: 25826, type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 43, deletions_count: 0},
		{sha: "6951064ed1d295fbff67a5ba7da5971fc6f58c46", date: "2026-07-13 22:29:42 UTC", description: "confine routing-field templates to prevent injection", pr_number: 25820, type: "fix", breaking_change: true, author: "Pavlos Rontidis", files_count: 89, insertions_count: 3578, deletions_count: 194},
		{sha: "cc72a8a926feb4ec6d1568970b58e8761b2aa479", date: "2026-07-14 14:37:06 UTC", description: "disable env var interpolation by default, add `--dangerously-allow-env-var-interpolation`", pr_number: 25699, type: "feat", breaking_change: true, author: "Thomas", files_count: 18, insertions_count: 181, deletions_count: 90},
		{sha: "8832452f57afb536ea0de53a093f9fd1b669ccec", date: "2026-07-14 20:12:13 UTC", description: "confinement follow-ups from #25820 codex review", pr_number: 25830, type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 66, insertions_count: 1368, deletions_count: 428},
		{sha: "a3693209ce1a9ef743770733219524392eb575fd", date: "2026-07-14 20:33:10 UTC", description: "Pinned VRL version to 0.34.0", pr_number: null, type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 9, deletions_count: 8},
	]
}
