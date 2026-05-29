package metadata

releases: "0.56.0": {
	date:     "2026-05-15"
	codename: ""

	whats_next: []

	deprecation_announcements: [
		{
			what:             "Boolean syntax for the `compression` field in the `vector` sink"
			deprecated_since: "0.56.0"
			description: #"""
				The boolean syntax (`compression: true` / `compression: false`) is deprecated.
				Use the string syntax instead: `compression: "gzip"`, `compression: "zstd"`, or `compression: "none"`.

				The `bool_or_vector_compression` deserializer will be removed once the boolean syntax is no longer supported.
				"""#
		},
		{
			what:             "GreptimeDB v0.x support in `greptimedb_metrics` and `greptimedb_logs` sinks"
			deprecated_since: "0.56.0"
			description: #"""
				The `greptimedb_metrics` and `greptimedb_logs` sinks drop support for GreptimeDB v0.x.
				Users must upgrade their GreptimeDB instance to v1.x before upgrading Vector.
				"""#
		},
		{
			what:             "`azure_monitor_logs` sink"
			deprecated_since: "0.58.0"
			description: #"""
				The `azure_monitor_logs` sink is deprecated in favor of the new `azure_logs_ingestion` sink,
				which uses the Azure Monitor Logs Ingestion API.

				Users should migrate before Microsoft ends support for the old Data Collector API (scheduled
				for September 2026).
				"""#
		},
		{
			what:             "`buffer_byte_size` and `buffer_events` gauge metrics"
			deprecated_since: "0.53.0"
			description: #"""
				The `buffer_byte_size` and `buffer_events` gauges are deprecated in favor of the
				`buffer_size_bytes` and `buffer_size_events` metrics described in `docs/specs/buffer.md`.
				"""#
		},
		{
			what:             "`series_api_version: v1` option on the `datadog_metrics` sink"
			deprecated_since: "0.58.0"
			description: #"""
				The `series_api_version: v1` option is deprecated in favor of `v2` (the default).
				The v1 series endpoint (`/api/v1/series`) is a legacy endpoint.

				Users should remove `series_api_version: v1` from their configuration or set it to `v2`.
				"""#
		},
		{
			what:             "`encoding` field on HTTP server sources"
			deprecated_since: "0.50.0"
			description: #"""
				The `encoding` field will be removed. Use `decoding` and `framing` instead.
				"""#
		},
		{
			what:             "FAKE EXAMPLE — `fake_option` on the `fake_sink` sink [remove before merging]"
			deprecated_since: "0.56.0"
			description: #"""
				This is a fake deprecation entry used to test the Deprecation Announcements section on the release page. Remove this file before merging.
				"""#
		},
	]

	planned_deprecations: []

	changelog: [
		{
			type: "feat"
			description: #"""
				HTTP-based sinks that use the shared retry helpers now support a `retry_strategy` configuration
				option to control which HTTP response codes are retried. The `http` sink also includes a new
				example showing how to retry only specific transient status codes.
				
				Issue: https://github.com/vectordotdev/vector/issues/10870
				"""#
			contributors: ["ndrsg"]
		},
		{
			type: "enhancement"
			description: #"""
				HTTP-based sinks using the shared retry logic now classify transport-layer failures with
				`HttpError::is_retriable`: connection and TLS connector issues may be retried, while failures
				such as invalid HTTP request construction or an invalid proxy URI are not. Setting
				`retry_strategy` to `none` disables retries for these transport errors and for request
				timeouts, in addition to status-code-based retries.
				
				Issue: https://github.com/vectordotdev/vector/issues/10870
				"""#
			contributors: ["ndrsg"]
		},
		{
			type: "fix"
			description: #"""
				The default `/etc/vector/vector.yaml` config file is no longer installed by the Debian, RPM, Alpine, and distroless-static Docker packages. The previous default ran a `demo_logs` source and printed synthesized syslog lines to stdout, which then surfaced in journald or `/var/log/` on hosts running Vector as a service and was a common source of confusion.
				
				New installs will now have no active config on disk. Provide your own configuration at `/etc/vector/vector.yaml` (or pass `--config <path>`) before starting Vector. A reference example is shipped at `/usr/share/vector/examples/vector.yaml`, and more sample configs remain at `/etc/vector/examples/`.
				
				Existing installs are unaffected on upgrade: package managers preserve the on-disk `/etc/vector/vector.yaml` if you already had one.
				"""#
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: #"""
				Unit tests now support an optional `expected_event_count` field on test outputs, allowing assertions on the number of events emitted by a transform.
				"""#
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: #"""
				The `vector` sink now supports `zstd` compression in addition to `gzip`. This provides better
				compression ratios and performance for Vector-to-Vector communication.
				
				The compression configuration has been enhanced to support multiple algorithms while maintaining
				full backward compatibility:
				
				## Legacy boolean syntax (still supported)
				
				```yaml
				sinks:
				  my_vector:
				    type: vector
				    address: "localhost:6000"
				    compression: true   # Uses gzip (default)
				    # or
				    compression: false  # No compression
				```
				
				## New string syntax
				
				```yaml
				sinks:
				  my_vector:
				    type: vector
				    address: "localhost:6000"
				    compression: "zstd"  # Use zstd compression
				    # Supported values: "none", "gzip", "zstd"
				```
				
				The Vector source automatically accepts both gzip and zstd compressed data, enabling seamless
				communication between Vector instances using different compression algorithms.
				"""#
			contributors: ["jpds"]
		},
		{
			type: "feat"
			description: #"""
				Add a new `databricks_zerobus` sink that streams log data to Databricks Unity Catalog tables via the Zerobus ingestion service. Supports OAuth 2.0 authentication, automatic schema fetching from Unity Catalog, and protobuf batch encoding.
				"""#
			contributors: ["flaviocruz"]
		},
		{
			type: "fix"
			description: #"""
				The shared gRPC decompression layer now rejects request frames that set the
				compressed flag without a negotiated `grpc-encoding` (e.g. `identity` or a
				missing header). Previously such malformed frames were silently decoded as
				gzip, which could mask client/server compression-negotiation bugs.
				"""#
			contributors: ["jpds"]
		},
		{
			type: "enhancement"
			description: #"""
				The `opentelemetry` source's gRPC OTLP receiver now accepts `zstd`-compressed
				requests in addition to `gzip`, matching the compression schemes advertised via
				the `grpc-accept-encoding` response header. No configuration change is required;
				clients can send OTLP payloads with `grpc-encoding: zstd` and they will be
				transparently decompressed.
				"""#
			contributors: ["jpds"]
		},
		{
			type: "fix"
			description: #"""
				Fixed issue during in place reload of a sink with a disk buffer configured, where
				the component would stall for batch.timeout_sec before gracefully reloading.
				This fix also resolves issues Vector had where it would ignore SIGINT during
				cases where the pipeline stall had occurred.
				"""#
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: #"""
				The `windows_event_log` source no longer freezes after periods of inactivity.
				"""#
			contributors: ["tot19"]
		},
		{
			type: "fix"
			description: #"""
				Sinks using batch encoding (Parquet, Arrow IPC) now consistently emit `ComponentEventsDropped` for every encode failure path. Previously some `build_record_batch` failures (notably type mismatches) dropped events silently. A new `EncoderRecordBatchError` internal event also reports `component_errors_total` with `error_code="arrow_json_decode"` or `"arrow_record_batch_creation"` at `stage="sending"` for granular alerting.
				"""#
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				The error log + metric that `splunk_hec` source emit on missing/invalid auth header now specifies "authentication_failed" as error_type.
				"""#
			contributors: ["20agbekodo"]
		},
		{
			type: "fix"
			description: #"""
				Restored support for installing Vector on RHEL 8, Rocky Linux 8, AlmaLinux 8, and CentOS Stream 8, which had been broken since v0.55.0 due to an inadvertent glibc requirement bump.
				"""#
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Restored the full VRL stdlib, including `get_env_var`, in the standalone VRL CLI and web playground by default.
				"""#
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Parquet encoding in the `aws_s3` sink (`batch_encoding`) now works out of the box in the official release binaries. Previously it required compiling Vector from source with the `codecs-parquet` feature.
				"""#
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				The `windows_event_log` source now adds standard source metadata, including `source_type`, to emitted log events.
				"""#
			contributors: ["tot19"]
		},
		{
			type: "fix"
			description: #"""
				The `aws_s3` and `clickhouse` sinks now correctly advertise only the `batch_encoding.codec` values they actually support: `parquet` for `aws_s3` and `arrow_stream` for `clickhouse`. Previously the documentation and configuration schema listed both codecs for both sinks, even though picking the wrong one produced a startup error.
				"""#
			contributors: ["flaviofcruz"]
		},
		{
			type: "fix"
			description: #"""
				The text content generated by the `demo_logs` source has changed: the
				pool of fake usernames and the pool of fake domain TLDs are now both
				defined inside Vector rather than pulled from an external crate. The
				line formats (`apache_common`, `apache_error`, `json`, `syslog`,
				`bsd_syslog`) are unchanged. If any of your tests or downstream
				pipelines assert on specific generated usernames or TLDs, please
				update those expectations.
				"""#
			contributors: ["pront"]
		},
		{
			type: "chore"
			description: #"""
				The `greptimedb_metrics` and `greptimedb_logs` sinks now require GreptimeDB v1.x. Users running GreptimeDB v0.x must upgrade their GreptimeDB instance before upgrading Vector.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a bug in the `mqtt` source where user-provided TLS client certificates (`crt_file` / `key_file`) were being silently ignored, breaking mTLS connections to strict brokers like AWS IoT Core.
				"""#
			contributors: ["mr-"]
		},
		{
			type: "feat"
			description: #"""
				Added `ratio_field` and `rate_field` options to the `sample` transform to support dynamic per-event sampling, while requiring static `rate` or `ratio` fallback configuration and disallowing `ratio_field` and `rate_field` together.
				"""#
			contributors: ["jhammer"]
		},
		{
			type: "enhancement"
			description: #"""
				Bumped `serde_json` to `1.0.149` and `serde_with` to `3.18.0`. `serde_json` switched its float-to-string formatter from Ryū to Żmij in `1.0.147`, so floats serialized via the `native_json` codec may render with slightly different textual form (for example `1e+16` instead of `1e16`). The change is purely cosmetic: parsed `f32`/`f64` values round-trip identically, and Vector-to-Vector communication between old and new versions is unaffected.
				"""#
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: #"""
				The `splunk_hec` source now accepts optional per-endpoint codec configuration via `event: { framing, decoding }` and `raw: { framing, decoding }`. When `decoding` is set on an endpoint, Vector applies a second decoding pass after the HEC envelope is parsed: on `/services/collector/event` the envelope's `event` field is fed through the codec, and on `/services/collector/raw` the request body is fed through the codec directly. A single payload can fan out to multiple events.
				
				For example, to decode JSON payloads in `/event` requests while splitting `/raw` bodies on newlines:
				
				```yaml
				sources:
				  hec:
				    type: splunk_hec
				    address: 0.0.0.0:8088
				    event:
				      decoding:
				        codec: json
				    raw:
				      framing:
				        method: newline_delimited
				      decoding:
				        codec: bytes
				```
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				The `tag_cardinality_limit` transform gained two new configuration capabilities:
				
				- **Per-tag overrides** (`per_tag_limits`): configure cardinality limits per tag key within a metric, or exclude individual tags from tracking.
				- **Metric exclusion**: opt entire metrics out of cardinality tracking via `mode: excluded` in `per_metric_limits`.
				"""#
			contributors: ["ArunPiduguDD"]
		},
		{
			type: "enhancement"
			description: #"""
				The `tag_cardinality_limit` transform gained two new settings:
				
				- **`tracking_scope`**: isolate tag tracking per metric (`per_metric`) instead of sharing a single bucket across all metrics (`global`, the default).
				- **`max_tracked_keys`**: cap the total number of tag keys tracked to bound memory usage.
				"""#
			contributors: ["ArunPiduguDD"]
		},
	]

	vrl_changelog: """
		### [0.32.0 (2026-04-16)]
		
		#### New Features
		
		- Added a new `encode_csv` function that encodes an array of values into a CSV-formatted string. This is the inverse of the existing `parse_csv` function and supports an optional single-byte delimiter (defaults to `,`).
		
		authors: armleth (https://github.com/vectordotdev/vrl/pull/1649)
		- Added `to_entries` and `from_entries` with jq-compatible behavior: `to_entries` supports both objects and arrays, and `from_entries` accepts `key`/`Key`/`name`/`Name` and `value`/`Value` aliases.
		
		authors: close2code-palm (https://github.com/vectordotdev/vrl/pull/1653)
		
		#### Enhancements
		
		- Added `except` parameter to `flatten` function to exclude specific keys from being flattened.
		
		authors: benjamin-awd (https://github.com/vectordotdev/vrl/pull/1682)
		
		#### Fixes
		
		- Fixed a bug where the REPL input validator was executing programs instead of only compiling them, causing functions with side effects (e.g. `http_request`) to run twice per submission.
		
		authors: prontidis (https://github.com/vectordotdev/vrl/pull/1701)
		
		"""

	commits: [
		{sha: "1c70988b54156abf8d031538f0f81f28e7c0a0e4", date: "2026-04-21 17:00:33 UTC", description: "restore HTTP GET /health endpoint", pr_number: 25234, scopes: ["api"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 162, deletions_count: 11},
		{sha: "aafd4cb44f5649e692722b97d82973fea5509a41", date: "2026-04-21 18:21:39 UTC", description: "drop fakedata_generator, fix broken fake domains", pr_number: 25236, scopes: ["demo_logs source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 61, deletions_count: 26},
		{sha: "8cf773df0e3b219e40537119aa2b95b916342df2", date: "2026-04-22 18:31:21 UTC", description: "start 0.56.0 development cycle", pr_number: 25242, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 59, insertions_count: 786, deletions_count: 233},
		{sha: "f36ed0e2cebf36efdc7392aed0ed52ae28a29101", date: "2026-04-22 19:54:48 UTC", description: "render release date on per-version release page", pr_number: 25244, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 9, deletions_count: 0},
		{sha: "29b17aadc63fdd9a9bdc3b64bbd8f9c647844cdb", date: "2026-04-22 20:30:30 UTC", description: "skip re-adding WIP label on synchronize if already approved", pr_number: 25246, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 29, deletions_count: 1},
		{sha: "d99899e2f9cb94c8d421270ada8688243402b5a1", date: "2026-04-22 20:41:08 UTC", description: "fix docs sidebar expand/collapse navigation", pr_number: 25238, scopes: ["website"], type: "fix", breaking_change: false, author: "shalk(xiao kun)", files_count: 1, insertions_count: 5, deletions_count: 2},
		{sha: "ecdaa50ac15e6a26e811bf21e3a6e33729bebb33", date: "2026-04-22 23:23:12 UTC", description: "verify choco package install after 5xx from feed", pr_number: 25116, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 4, deletions_count: 1},
		{sha: "f1557d68c8e7f36feb8e2d0c7a04286817c6bf9f", date: "2026-04-23 14:16:39 UTC", description: "add `expected_event_count` field to test outputs", pr_number: 25186, scopes: ["unit tests"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 323, deletions_count: 11},
		{sha: "cfb942bf7d1983b9eeaeb93ba2f843a816d1db5d", date: "2026-04-23 17:39:22 UTC", description: "bump openssl from 0.10.75 to 0.10.78", pr_number: 25250, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "c0cdf2068f79176fd8af4dca9b27cabae5be3091", date: "2026-04-23 17:52:07 UTC", description: "extract unit tests into reusable workflow", pr_number: 25247, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 125, deletions_count: 24},
		{sha: "f56f4fa7234440345d790a2b81c7aa880be831d8", date: "2026-04-23 20:29:50 UTC", description: "remove Chocolatey from Windows bootstrap", pr_number: 25254, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 39, deletions_count: 40},
		{sha: "7428dc3f4b23c56f1f2eecf44b3f6b2532c28dac", date: "2026-04-24 18:40:53 UTC", description: "use dd-sts instead of DD_API_KEY", pr_number: 25235, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 11, insertions_count: 82, deletions_count: 12},
		{sha: "7815f26ef4f357c7e3e90048e4287728c9506423", date: "2026-04-24 18:43:56 UTC", description: "add Docker support for local development", pr_number: 25237, scopes: ["website"], type: "chore", breaking_change: false, author: "shalk(xiao kun)", files_count: 3, insertions_count: 83, deletions_count: 0},
		{sha: "74574a360112a4bb212915b896471320fb157b96", date: "2026-04-24 18:50:53 UTC", description: "add linux arm64 to publish matrix and bump to 0.3.2", pr_number: 25260, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 5, deletions_count: 3},
		{sha: "3b043d33b95670ef1cbd2f5af274fa25ebb1db84", date: "2026-04-24 18:55:51 UTC", description: "add support for GreptimeDB v1.0.0", pr_number: 25178, scopes: ["greptimedb sink"], type: "feat", breaking_change: true, author: "Thomas", files_count: 12, insertions_count: 623, deletions_count: 273},
		{sha: "61cc4d84d2966e7552a308d5df5a0f03305d7eed", date: "2026-04-27 13:40:18 UTC", description: "make publish-s3 wait for generate-sha256sum", pr_number: 25265, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "454b096abcfd609b4e317b83fa4ec1ec79af5da8", date: "2026-04-27 14:14:28 UTC", description: "add datadog_metrics series v1 deprecation entry", pr_number: 25271, scopes: ["deprecations"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "aa9265d6026f7d5998cada6a50e9c66790fbf054", date: "2026-04-27 14:47:04 UTC", description: "add dependabot config for scripts/environment/npm-tools", pr_number: 25175, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 16, deletions_count: 0},
		{sha: "69fd3b6f7e544a3f4bbba1d6d7e882a67387b081", date: "2026-04-27 14:56:51 UTC", description: "bump github/codeql-action from 4.32.4 to 4.35.2", pr_number: 25280, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "413caac10f64475b509687000317e06e55de55e0", date: "2026-04-27 15:18:47 UTC", description: "allow dd-token federation to fail on fork PRs", pr_number: 25284, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "ba84da5d8965aef1d5eb5455dc9f4d723177e914", date: "2026-04-27 15:01:24 UTC", description: "bump actions/github-script from 7 to 9", pr_number: 25278, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 10, deletions_count: 10},
		{sha: "a4ea6de53b829e6ec66fadaddd78317f099207b9", date: "2026-04-27 15:34:31 UTC", description: "bump actions/upload-artifact from 7.0.0 to 7.0.1 in the artifact group across 1 directory", pr_number: 25276, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 15, deletions_count: 15},
		{sha: "1d4b6c2122b82997b808bba0527ec60dbb37a3ba", date: "2026-04-27 15:36:38 UTC", description: "Emit warn on unauthenticated request", pr_number: 25230, scopes: ["splunk_hec source"], type: "fix", breaking_change: false, author: "Josué AGBEKODO", files_count: 3, insertions_count: 35, deletions_count: 12},
		{sha: "156b832637c2f18b4d32fbb347e0e5f0919407d3", date: "2026-04-27 18:33:20 UTC", description: "include page title in docs search query fields", pr_number: 25255, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a843435f9677fe45f482ff4604bfe92b8a3d57c7", date: "2026-04-27 20:16:32 UTC", description: "Fix for issue causing stalling on shutdown for sinks configured w/ disk buffers", pr_number: 24949, scopes: ["topology"], type: "fix", breaking_change: false, author: "Rob Blafford", files_count: 7, insertions_count: 396, deletions_count: 111},
		{sha: "ff4754ab10da5838f68cc143e507087090518f91", date: "2026-04-28 14:04:28 UTC", description: "make nightly S3 verify resilient to CDN staleness", pr_number: 25259, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 17, deletions_count: 10},
		{sha: "6f3857906d3a54d9c05aa5d3aaa8b132ac8f5e6e", date: "2026-04-28 15:12:07 UTC", description: "restore stdlib functions in CLI and playground", pr_number: 25310, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 10, deletions_count: 0},
		{sha: "23016adf7d87f41a3d887d587b21d3409e71e3fe", date: "2026-04-28 15:52:16 UTC", description: "fix release issue templates", pr_number: 25318, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "96ad9edc5bd894029af95961ea205e0d89b17bf0", date: "2026-04-28 18:17:38 UTC", description: "improve docs search ranking for component pages", pr_number: 25319, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 5, deletions_count: 2},
		{sha: "ce6ca439caf80bd2af10864a84e949ef244e2163", date: "2026-04-28 19:40:29 UTC", description: "centralize `events_dropped` emission for batch encoding errors", pr_number: 25199, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 153, deletions_count: 61},
		{sha: "d6cdf031d16a382a38046127fbc7ff30c2457709", date: "2026-04-28 21:40:45 UTC", description: "enable codecs-parquet in all release feature sets", pr_number: 25321, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 8, insertions_count: 186, deletions_count: 23},
		{sha: "233a35c47eab1a0691b39e4af06991dfe4b0f571", date: "2026-04-28 23:33:23 UTC", description: "correct cross-build artifact name and path", pr_number: 25282, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "8b465f6406fd088302d42f7e879297937bb8783f", date: "2026-04-29 17:44:41 UTC", description: "bump the patches group across 1 directory with 13 updates", pr_number: 25283, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 293, deletions_count: 155},
		{sha: "75b9d07a8231e7c321652e4ff031d8ce0757d9ab", date: "2026-04-29 19:57:15 UTC", description: "improve search ranking for component reference pages", pr_number: 25327, scopes: ["website"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 44, deletions_count: 6},
		{sha: "625c1d3a57a59784da7b1e42b2318d4c372649d8", date: "2026-04-29 20:06:31 UTC", description: "bump the serde group across 1 directory with 2 updates", pr_number: 25227, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 16, insertions_count: 71, deletions_count: 26},
		{sha: "b5fb618eb24bade72d5277be6d23836810d24fa8", date: "2026-04-30 21:53:00 UTC", description: "remove unused update_counter macro", pr_number: 25333, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 0, deletions_count: 44},
		{sha: "bd8ab1a245a439134b2bd9f2cb540ca550db860c", date: "2026-05-01 13:30:24 UTC", description: "introduce RetryStrategy / Config for http based sinks", pr_number: 25057, scopes: ["sinks"], type: "feat", breaking_change: false, author: "Andy", files_count: 32, insertions_count: 957, deletions_count: 59},
		{sha: "9c617a7b766dc95ea919384ff16da2654595f6a4", date: "2026-05-01 14:04:29 UTC", description: "grant issues:write to remove_wip_label workflow", pr_number: 25339, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "308d2469b2e70153d3eafd20a647bf33a9d69d73", date: "2026-05-01 14:43:39 UTC", description: "prevent windows_event_log permanent freeze from signal-event lost wakeup", pr_number: 25195, scopes: ["sources"], type: "fix", breaking_change: false, author: "tot19", files_count: 3, insertions_count: 546, deletions_count: 112},
		{sha: "59a53b138d127fdca68d260628d1dc0035b3f711", date: "2026-05-01 17:43:39 UTC", description: "remove type-level default on StatusCode", pr_number: 25345, scopes: ["internal docs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 13, insertions_count: 12, deletions_count: 13},
		{sha: "f1b2c3a3f14f59ab9615829852da6e65a7d2c512", date: "2026-05-01 17:06:42 UTC", description: "add windows_event_log source metadata", pr_number: 25337, scopes: ["sources"], type: "fix", breaking_change: false, author: "tot19", files_count: 2, insertions_count: 18, deletions_count: 2},
		{sha: "9f15e23943d4347a6f2171eaa97a921a5e58d457", date: "2026-05-01 17:56:58 UTC", description: "bump cue and add cue-build step to Check Cue docs", pr_number: 25346, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 9, deletions_count: 7},
		{sha: "7923556313d66be69e638022e10fe3fd13f468ac", date: "2026-05-01 18:57:31 UTC", description: "use dedicated batch_encoding types", pr_number: 25340, scopes: ["clickhouse sink", "aws_s3 sink"], type: "fix", breaking_change: false, author: "Flavio Cruz", files_count: 8, insertions_count: 147, deletions_count: 212},
		{sha: "e1c6139b9717f36027b0ac9fe4d20276da4da128", date: "2026-05-02 20:51:00 UTC", description: "introduce enums for metric names", pr_number: 25342, scopes: ["metrics"], type: "chore", breaking_change: false, author: "Thomas", files_count: 106, insertions_count: 1254, deletions_count: 711},
		{sha: "48be543ff3b84dfbd56a49cc6a0a0aac450bbceb", date: "2026-05-04 14:01:46 UTC", description: "scope HistogramName import to s3 module", pr_number: 25353, scopes: ["aws_sqs source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "96db40ef65b0d1246f06581987e5b6428468edf0", date: "2026-05-04 17:47:01 UTC", description: "Make transform-related functions in aggregate & tag cardinality transforms public", pr_number: 25358, scopes: ["metrics"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8073e93b48352f781aace209f816ac55280a8935", date: "2026-05-04 19:13:10 UTC", description: "rename WIP label workflows to docs review label", pr_number: 25355, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 23, deletions_count: 10},
		{sha: "4524b52c921447dfca228338add190f0363b582e", date: "2026-05-04 23:08:38 UTC", description: "upgrade hickory-proto to 0.26.1, ignore RUSTSEC-2026-0119", pr_number: 25354, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 226, deletions_count: 240},
		{sha: "6c3116a6e0a1d44113e5e2ce0b7e5aeeef3db785", date: "2026-05-05 13:33:38 UTC", description: "retry apt fetches in deb-verify to reduce flakes", pr_number: 25367, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "95756d72356406d1a71625dbcc8f83e49f43947d", date: "2026-05-05 15:34:08 UTC", description: "dynamic rate for sample", pr_number: 25035, scopes: ["transforms"], type: "enhancement", breaking_change: false, author: "jh7459-gh", files_count: 5, insertions_count: 749, deletions_count: 61},
		{sha: "f7cc83e980e5af429ee040c07aab7f96dd70cb15", date: "2026-05-05 17:37:26 UTC", description: "adjust tocbot content tracking", pr_number: 25359, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Aaron Zheng", files_count: 2, insertions_count: 6, deletions_count: 3},
		{sha: "bec3290d7806fbefc1923fd676bb865c95e7f115", date: "2026-05-05 17:39:34 UTC", description: "bump axios from 1.15.0 to 1.16.0 in /website", pr_number: 25369, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "202c974c18a8bfdb8102f6f0a8c3580bd3e9e96a", date: "2026-05-05 18:00:29 UTC", description: "bump postcss from 8.5.6 to 8.5.14 in /website", pr_number: 25368, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "92ee2b26923ea0eaf8c0b4c1bc0398c679a4b44d", date: "2026-05-06 03:49:05 UTC", description: "Add remove tag function for metrics which returns entire tag set", pr_number: 25361, scopes: ["metrics"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 1, insertions_count: 33, deletions_count: 0},
		{sha: "249d064d75e5337efd1cd4fd2949de20e16d801e", date: "2026-05-06 12:36:13 UTC", description: "rewrite scripts/generate-component-docs.rb in Rust (#22350)", pr_number: 24781, scopes: ["dev"], type: "feat", breaking_change: false, author: "Swaraj Patil", files_count: 11, insertions_count: 1990, deletions_count: 1930},
		{sha: "e109afcff7b4d0d58bf710c797b5028c2d067250", date: "2026-05-06 15:19:22 UTC", description: "add docker run example in distribution README", pr_number: 25268, scopes: ["external"], type: "docs", breaking_change: false, author: "st-omarkhalid", files_count: 1, insertions_count: 12, deletions_count: 3},
		{sha: "66e25a90bec1e3b3def56f0c00ae49d8e71260e5", date: "2026-05-06 15:23:25 UTC", description: "use single agent to fix e2e datadog-metrics histogram flakiness", pr_number: 25363, scopes: ["tests"], type: "fix", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 26, deletions_count: 64},
		{sha: "e6c0e3f6c46dd38b3c601c473149e07f5b71eb9e", date: "2026-05-06 15:38:07 UTC", description: "add new databricks_zerobus for Databricks ingestion", pr_number: 24840, scopes: ["sinks"], type: "feat", breaking_change: false, author: "Flavio Cruz", files_count: 26, insertions_count: 3187, deletions_count: 32},
		{sha: "5112e0ae334120465b333c1777da38593a4b8c60", date: "2026-05-06 18:00:21 UTC", description: "bump openssl from 0.10.78 to 0.10.79", pr_number: 25380, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 9, deletions_count: 10},
		{sha: "27e74de2d2daa8368e4b073fa90d443eb7974ba2", date: "2026-05-06 20:34:22 UTC", description: "bump docker/login-action from 4.0.0 to 4.1.0", pr_number: 25349, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "8105f31eef65e4e3d823f382d05d63e433394777", date: "2026-05-06 21:26:46 UTC", description: "add code coverage collection for integration and e2e test suites", pr_number: 25088, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 15, insertions_count: 353, deletions_count: 46},
		{sha: "bfeb76986acc74ad7c28acd2c60053d3cbdeb2bd", date: "2026-05-07 15:26:32 UTC", description: "support second-stage framing and decoding", pr_number: 25312, scopes: ["splunk_hec source"], type: "feat", breaking_change: false, author: "Thomas", files_count: 7, insertions_count: 2683, deletions_count: 219},
		{sha: "17a720cc90ebbbab9051eb5a2479a51eddf2760d", date: "2026-05-07 16:23:08 UTC", description: "remove release-flags.sh", pr_number: 24828, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 7, insertions_count: 4, deletions_count: 24},
		{sha: "256b8fa98627f37f26d6e11a84bfeda44981fc21", date: "2026-05-07 17:52:19 UTC", description: "skip Windows UDP-excluded ports in next_addr_for_ip", pr_number: 25386, scopes: ["tests"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 10, deletions_count: 0},
		{sha: "8d83e1790cf3337f8db56c0920e629866e7a1362", date: "2026-05-07 18:03:02 UTC", description: "bump hickory-net from 0.26.0 to 0.26.1", pr_number: 25389, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 12, deletions_count: 12},
		{sha: "bbde98b342fff4d69ccc49d537feda9572c9df79", date: "2026-05-08 14:20:34 UTC", description: "fix LTO settings after release-flags.sh removal", pr_number: 25393, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 3, deletions_count: 2},
		{sha: "cdb27e859b95fda129cfbed5e2b078af0f3c42c9", date: "2026-05-08 14:36:09 UTC", description: "kill Ruby and port release scripts to native vdev", pr_number: 25379, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 36, insertions_count: 2616, deletions_count: 1806},
		{sha: "5cedf015de0c07d9f7e6f0023264279fcf986a40", date: "2026-05-08 15:06:43 UTC", description: "fix wording in decoder and framing doc strings", pr_number: 25382, scopes: ["codecs"], type: "docs", breaking_change: false, author: "Thomas", files_count: 32, insertions_count: 639, deletions_count: 712},
		{sha: "d9b06937242d4e5f484362fab6c0506b3676347f", date: "2026-05-08 15:09:03 UTC", description: "Add zstd compression support", pr_number: 24917, scopes: ["vector sink"], type: "feat", breaking_change: false, author: "Jonathan Davies", files_count: 14, insertions_count: 528, deletions_count: 72},
		{sha: "e59ac5715d4c8cc63b2e9076d19c4ef0429a2b4e", date: "2026-05-08 15:12:02 UTC", description: "Clarify acknowledgement guarantees with disk buffers", pr_number: 25388, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 7, deletions_count: 2},
		{sha: "39138e0e7b73a3b1d4f9dd7f0617343c3afd42ca", date: "2026-05-08 15:27:00 UTC", description: "add /ci-run-regression trigger + accept refs in inputs", pr_number: 25245, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 85, deletions_count: 43},
		{sha: "4ff1874b38008c15afeb61902298b36593a727bb", date: "2026-05-08 16:43:29 UTC", description: "emit channel-suffixed version from publish-metadata", pr_number: 25395, scopes: ["vdev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 3},
		{sha: "41bc804eab5590ccb44084512eb05b4a5612e2b7", date: "2026-05-08 16:49:29 UTC", description: "replace bootstrap scripts with setup action in publish workflows", pr_number: 25311, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 56, deletions_count: 21},
		{sha: "0d9111807bc7abb24ff53ef8590b8ca3ba4d8834", date: "2026-05-08 17:41:34 UTC", description: "bump vrl to latest main", pr_number: 25398, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 19, deletions_count: 15},
		{sha: "63b29fb2d8d80b5be57a897e14c8d29ed23d5663", date: "2026-05-08 18:38:00 UTC", description: "skip dd-sts federation on fork PRs via ACTIONS_ID_TOKEN_REQUEST_URL guard", pr_number: 25399, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 14, deletions_count: 2},
		{sha: "81f013820839be8ab0cc28069ab943c44f0f28cb", date: "2026-05-08 19:03:05 UTC", description: "use debug profile for k8s e2e build", pr_number: 25397, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 12, deletions_count: 10},
		{sha: "f3346607ba25645a682b4b5bbc56d95a7f4bb4dd", date: "2026-05-08 19:11:30 UTC", description: "make -D warnings the default via .cargo/config.toml", pr_number: 25400, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 2, deletions_count: 20},
		{sha: "f2f19212bb5cee15615c8d86658447a4ac096729", date: "2026-05-08 19:18:40 UTC", description: "pass client certificates to rumqttc for mTLS", pr_number: 24929, scopes: ["mqtt source"], type: "fix", breaking_change: false, author: "Martin Ruderer", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "eda5b866e83cf5e0e926cae8ecf89c00a3109dc8", date: "2026-05-08 22:39:20 UTC", description: "fix flaky `initial_size_correct_with_multievents` test", pr_number: 25239, scopes: ["tests"], type: "fix", breaking_change: false, author: "Vitalii Parfonov", files_count: 1, insertions_count: 32, deletions_count: 7},
		{sha: "c3676b9fa8aef8088bd341775416e4ecd91095b2", date: "2026-05-08 23:11:57 UTC", description: "centralize CARGO_INCREMENTAL=0 in setup action", pr_number: 25401, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 2, deletions_count: 7},
		{sha: "4ddc412437c69cfa6d3b5184ff3abab7e3d75008", date: "2026-05-11 14:52:29 UTC", description: "Add setting for per-metric vs global tag cardinality tracking", pr_number: 25372, scopes: ["tag_cardinality_limit transform"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 7, insertions_count: 385, deletions_count: 45},
		{sha: "11f1dff407534822b90d4690e446d718867d6baf", date: "2026-05-11 18:55:00 UTC", description: "include lib/ workspace crates in coverage reports", pr_number: 25402, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "7efd6d7b53185a2997b81b2ef8fff4162d6c6506", date: "2026-05-11 20:19:41 UTC", description: "remove cargo vdev test --container runner", pr_number: 25410, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 6, deletions_count: 43},
		{sha: "e38ef0cab69d8dbd02deaadbd63870aafd77081b", date: "2026-05-12 13:53:04 UTC", description: "warn about log namespace with disk buffers", pr_number: 25413, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 39, deletions_count: 1},
		{sha: "fc6a2b567f08a9c229e8a4b7584d6f30e1da1c9f", date: "2026-05-12 13:58:10 UTC", description: "make regression/Dockerfile self-contained", pr_number: 25411, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 29, deletions_count: 1},
		{sha: "d06def7be6c4ac1e29e46c73f94ea3d9f99b9997", date: "2026-05-12 20:06:08 UTC", description: "bump @babel/plugin-transform-modules-systemjs from 7.28.5 to 7.29.4 in /website", pr_number: 25403, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 85, deletions_count: 6},
		{sha: "10de0ea6c6871d14d06b741d457d3cfab3676d67", date: "2026-05-12 20:32:17 UTC", description: "publish to crates.io on tag push", pr_number: 25420, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 21, deletions_count: 0},
		{sha: "dce3678640998bdbaedd67a4f5c4ddea588dd3af", date: "2026-05-12 20:43:00 UTC", description: "bump version to 0.3.3", pr_number: 25419, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "fe3871fd15e8cc07a132aba2413f4c8efa69f983", date: "2026-05-13 01:42:02 UTC", description: "bump markdownlint-cli2 to 0.22.1 and remove smol-toml override", pr_number: 25416, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 10, deletions_count: 15},
		{sha: "c60bc0a68c30eecdf8698eddec3ee7e3c7393fbb", date: "2026-05-13 16:46:40 UTC", description: "install vdev via binstall", pr_number: 25418, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 42, deletions_count: 90},
		{sha: "c3720d863abc606cef5c117486b6ced03f8cf134", date: "2026-05-13 17:18:52 UTC", description: "remove default OS package config", pr_number: 25425, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 28, deletions_count: 15},
		{sha: "904f69f81c68826a982d351cc08335cea9afab5b", date: "2026-05-13 20:35:46 UTC", description: "warn on invalid json batching", pr_number: 25423, scopes: ["opentelemetry sink"], type: "enhancement", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 29, deletions_count: 2},
		{sha: "338fc3805f131ca9281baadccb05e4ff8ac26111", date: "2026-05-13 21:06:03 UTC", description: "restore Vector RPM/DEB install on EL8 family", pr_number: 25387, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 116, deletions_count: 18},
		{sha: "989f2ae0af0aa3da2691cfc78f675601178edadf", date: "2026-05-13 21:38:33 UTC", description: "pass GITHUB_TOKEN to prepare.sh for authenticated binstall requests", pr_number: 25428, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "e1772733754c4d069928169675791b2a484375a2", date: "2026-05-14 14:28:23 UTC", description: "retire the unused Docker dev environment", pr_number: 25429, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 102, deletions_count: 623},
		{sha: "45b6d2b8faa9d89005abefd5d6311db9d730cd0a", date: "2026-05-14 16:08:30 UTC", description: "Add more fine grained controls tag cardinality", pr_number: 25360, scopes: ["tag_cardinality_limit transform"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 6, insertions_count: 747, deletions_count: 122},
		{sha: "11ad4e7df2f16203357c875557230905a94cfe0d", date: "2026-05-14 17:33:09 UTC", description: "clarify JEMALLOC_SYS_WITH_LG_PAGE comment", pr_number: 25435, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "45c4010e78a8470177061b8257648499ba1f61fe", date: "2026-05-14 19:33:52 UTC", description: "add \"View open issues\" link to component pages", pr_number: 25437, scopes: ["external docs"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 14, deletions_count: 0},
		{sha: "a1a56fe123829ab224361648165973d6ba9a5292", date: "2026-05-14 20:04:48 UTC", description: "drop gssapi from default cargo feature", pr_number: 25256, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 35, deletions_count: 15},
		{sha: "ec27f907e50dacdc7d1124720f1b29dfe967aac7", date: "2026-05-14 20:20:46 UTC", description: "unify Dockerfile apt deps into one script", pr_number: 25436, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 27, deletions_count: 33},
		{sha: "f06f820a9f66a6518e01f0d7a3d43b5a2826bd9b", date: "2026-05-14 20:32:54 UTC", description: "make libz.so.1 a consistent dynamic runtime dependency in distroless-libc", pr_number: 25434, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 10, deletions_count: 3},
		{sha: "f5643b687151a31c5494b46646068f1dece4df7b", date: "2026-05-15 13:36:52 UTC", description: "add deprecation.d fragment system with vdev check/show commands and release integration", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 18, insertions_count: 670, deletions_count: 27},
		{sha: "6d0e0c07a40a234ecb37b21a4b8d65b3f578621c", date: "2026-05-15 14:02:32 UTC", description: "wire deprecation check into changelog.yaml via bash script", pr_number: null, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 116, deletions_count: 40},
		{sha: "411a95799077734160b680181a7ac919bc9823a7", date: "2026-05-15 14:14:57 UTC", description: "chore(ci): wire deprecation check into changelog.yaml via bash script", pr_number: null, scopes: [], type: "revert", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 40, deletions_count: 116},
		{sha: "f2b55c226e1b67dd502a3c0cb092f52d2bc95773", date: "2026-05-15 14:15:43 UTC", description: "use changes.yml and setup action for deprecation fragment check", pr_number: null, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 55, deletions_count: 26},
		{sha: "a42c0a4fc73a7bd881fa17d1f69a9d1af7333f88", date: "2026-05-15 14:16:47 UTC", description: "remove stale comment from deprecation workflow", pr_number: null, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 0, deletions_count: 9},
		{sha: "9e5a2eb22c0e8f4b3c1962fb63877e7a210db178", date: "2026-05-15 14:27:03 UTC", description: "add deprecations and planned_deprecations to release CUE schema", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 9, deletions_count: 0},
		{sha: "1196d69e0aaad8e4ffbe7d226aedbe2a715e22b7", date: "2026-05-15 14:29:43 UTC", description: "fix deprecation fragments to use deprecation_version not announcement_version", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 5, deletions_count: 11},
		{sha: "d7b55a21986031413aec814390d47b39de944c0e", date: "2026-05-15 14:32:59 UTC", description: "restore announcement_version to deprecation fragments", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 6, deletions_count: 0},
		{sha: "a0b1f0ff2ae17bd79063e6476d60472f15c93147", date: "2026-05-15 15:11:22 UTC", description: "make announcement_version required, add next version keyword, rewrite next on release", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 12, insertions_count: 171, deletions_count: 45},
		{sha: "37eba350d6ba591419c1e30a90099db0afbf0e73", date: "2026-05-15 15:15:48 UTC", description: "update deprecation docs and release template for new fragment fields", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 10, deletions_count: 7},
		{sha: "9659a52582a8cc94e110f9dc2dce45c82dc3d100", date: "2026-05-15 15:16:57 UTC", description: "sort deprecation show output by version", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 22, deletions_count: 1},
		{sha: "5ae8acdb5b2bd93d3d0504d6fbe01d9d99303bea", date: "2026-05-15 15:36:31 UTC", description: "fix announcement versions for azure_monitor_logs and datadog_metrics series v1", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "0b535c6ec1ae2881bed5d9c9ce8cf69eedc18a9e", date: "2026-05-15 15:41:49 UTC", description: "refine deprecation show output with grouped sections and next-release detection", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 86, deletions_count: 33},
		{sha: "d1eff3ad6d4854ffb53f968204277bc00e5c6d98", date: "2026-05-15 15:43:14 UTC", description: "add color and styling to deprecation show output", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 46, deletions_count: 14},
		{sha: "a2928a79209be19854cfb1d9e63fb8137536768d", date: "2026-05-15 15:44:08 UTC", description: "fix label alignment in deprecation show output", pr_number: null, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b575eb9dbd10001627f27914154cf96802ac924d", date: "2026-05-15 15:45:00 UTC", description: "colour concrete next-minor version red same as next keyword", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 25, deletions_count: 31},
		{sha: "0c6d24703d0e7e8602520ba9e9ed15ef85aab6f9", date: "2026-05-15 15:46:49 UTC", description: "correct deprecation versions in fragments", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "986bc3222b4ba0337b66084e34a587976e9bed70", date: "2026-05-15 15:48:40 UTC", description: "expose generate-cue as standalone release subcommand", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 62, deletions_count: 0},
		{sha: "4ccc8cf603448f0e2590499a2169a4492aba8abc", date: "2026-05-15 16:30:34 UTC", description: "render deprecations and planned_deprecations on release pages", pr_number: null, scopes: ["external docs"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 52, deletions_count: 0},
		{sha: "0723a0dac1cc9bd5d9992dea29f54cc6616c8ca6", date: "2026-05-15 16:45:59 UTC", description: "split deprecations into three buckets: enacted, announcing, planned", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 79, deletions_count: 26},
		{sha: "8643b9ee251132c22e3668cd23e8eb3d0a0911b3", date: "2026-05-15 16:47:59 UTC", description: "fix clippy warnings in deprecation utils", pr_number: null, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 53, deletions_count: 35},
		{sha: "55fc56dce9625822817d6981bf16ad876537cf79", date: "2026-05-15 17:07:32 UTC", description: "in dry-run mode, create branches from current branch instead of master", pr_number: null, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 7, deletions_count: 3},
		{sha: "99f0cf1face6d6d88d21539f623d043b8d464154", date: "2026-05-15 17:13:41 UTC", description: "resolve next to concrete version before rendering CUE and partitioning", pr_number: null, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 14, deletions_count: 0},
		{sha: "53367982cf50bc5deb233713210bf106d8757432", date: "2026-05-15 17:14:07 UTC", description: "rename VersionOrTbd->DeprecationVersion", pr_number: null, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 39, deletions_count: 37},
		{sha: "923269d197176275cc518a1be5f27fa4540758d1", date: "2026-05-15 17:19:52 UTC", description: "hide TBD removal date on release deprecation sections", pr_number: null, scopes: ["external docs"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "9bc543d11370b551176c78481a014dcc3fc07a67", date: "2026-05-15 17:23:23 UTC", description: "remove leading 'The' from deprecation what fields", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 4, deletions_count: 4},
		{sha: "a0d26067e61f4456909ebe1b2a9b330d3118029c", date: "2026-05-15 17:28:47 UTC", description: "Add fake deprecation + generated cue file", pr_number: null, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 487, deletions_count: 0},
		{sha: "12c78d37bdc8d7fcb5314dadd05d79b243b21749", date: "2026-05-15 17:36:46 UTC", description: "fix nested code fences in deprecation.d README", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "fd52722532d1c715f49bb04e00796137498088e6", date: "2026-05-15 17:39:25 UTC", description: "delete generated files", pr_number: null, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 0, deletions_count: 480},
		{sha: "8eb5b4279e4b630548137f6edbcc502a2d1ce282", date: "2026-05-15 17:46:27 UTC", description: "fix insert_block_after_changelog to target the changelog array not the first ]", pr_number: null, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 7, deletions_count: 2},
		{sha: "9dabc01cea2e3eee0eb4720c23338b58360bf2f9", date: "2026-05-15 17:54:08 UTC", description: "fix markdown table separator spacing in deprecation.d README", pr_number: null, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3b2a6f05b071dafb216fb8738e01412f0ba87560", date: "2026-05-15 17:59:10 UTC", description: "use gh CLI to fetch VRL release notes instead of unauthenticated GitHub API", pr_number: null, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 41, deletions_count: 39},
		{sha: "35fbe89db6dbb2a4d4318496acdc7f45231871d0", date: "2026-05-15 18:01:44 UTC", description: "Pinned VRL version to 0.32.0", pr_number: null, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 7, deletions_count: 6},
	]
}
