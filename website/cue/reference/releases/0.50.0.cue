package metadata

releases: "0.50.0": {
	date:     "2025-09-23"
	codename: ""

	whats_next: []

	description: """
		The Vector team is excited to announce version `0.50.0`!

		## Release highlights

		- The `opentelemetry` source can now decode data according to the standard [OpenTelemetry protocol ](https://opentelemetry.io/docs/specs/otel/protocol)
		 for all telemetry data types (logs, metrics and traces). This eliminates the need for complex event remapping. It
		greatly simplifies configuration for OTEL -> Vector -> OTEL use cases or when forwarding data to any system that expects OTLP-formatted telemetry.
		- A new `varint_length_delimited` framing option is now available which enables compatibility with standard protobuf streaming implementations and tools like ClickHouse.
		- Introduced a new `incremental_to_absolute` transform, useful when metric data might be lost in transit or for creating a historical record of the metric.
		- A new `okta` source for consuming [Okta system logs](https://developer.okta.com/docs/api/openapi/okta-management/management/tag/SystemLog/) is now available.
		- The `exec` secrets option now supports protocol version `v1.1` which can be used with the [Datadog Secret Backend](https://github.com/DataDog/datadog-secret-backend/blob/v1/README.md).

		## Breaking Changes

		- The `azure_blob` sink now requires a `connection_string`. This is the only supported authentication method for now. For more details, see this [pull request](https://github.com/vectordotdev/vector/pull/23351).
		"""

	changelog: [
		{
			type: "enhancement"
			description: """
				The `gelf` encoding format now supports [chunking](https://go2docs.graylog.org/current/getting_in_log_data/gelf.html#chunking) when used with the `socket` sink in `udp` mode. The maximum chunk size can be configured using `encoding.gelf.max_chunk_size`.
				"""
			contributors: ["aramperes"]
		},
		{
			type: "enhancement"
			description: """
				The `nats` source now drains subscriptions during shutdown, ensuring that in-flight and pending messages are processed.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "enhancement"
			description: """
				Added JetStream support to the `nats` source.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "feat"
			description: """
				Introduced a new `okta` source for consuming [Okta system logs](https://developer.okta.com/docs/api/openapi/okta-management/management/tag/SystemLog/)
				"""
			contributors: ["sonnens"]
		},
		{
			type: "enhancement"
			description: """
				Added `insert_namespace_fields` config option which can be used to disable listing Kubernetes namespaces, reducing resource usage in clusters with many namespaces.
				"""
			contributors: ["imbstack"]
		},
		{
			type: "fix"
			description: """
				Fixed the `splunk_hec` sink to not use compression on indexer acknowledgement queries.
				"""
			contributors: ["sbalmos"]
		},
		{
			type: "feat"
			description: """
				Added an optional `ttl_field` configuration option to the memory enrichment table, to override the global memory table TTL on a per event basis.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fixed a bug where certain floating-point values such as `f64::NAN`, `f64::INFINITY`, and similar would cause Vector to panic when sorting more than 20 items in some internal functions.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: """
				Prevent panic in `file` source during timing stats reporting.
				"""
			contributors: ["mayuransw"]
		},
		{
			type: "feat"
			description: """
				The `request_retry_partial` behavior for the `aws_kinesis_streams` sink was changed. Now only the failed records in a batch will be retried (instead of all records in the batch).
				"""
			contributors: ["lht"]
		},
		{
			type: "feat"
			description: """
				Secrets options now support the protocol version 1.1 and can be used with the [datadog-secret-backend](https://github.com/DataDog/datadog-secret-backend/blob/v1/README.md).

				Sample config:
				```yaml
				secret:
					exec_backend:
						type: "exec"
						command: [/usr/bin/datadog-secret-backend]
						protocol:
							version: v1_1
							backend_type: file.json
							backend_config:
								file_path: ~/secrets.json
				```
				"""
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: """
				Fixed the default `aws_s3` sink retry strategy.
				The default configuration now correctly retries common transient errors instead of requiring manual configuration.
				"""
			contributors: ["pront"]
		},
		{
			type: "chore"
			description: """
				The `azure_blob` sink now requires a `connection_string`. This simplifies configuration and ensures predictable behavior in production.
				Other authentication methods will be not supported at least until `azure_*` crates mature.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				Fix disk buffer panics when both reader and writer are on the last data file and it is corrupted. This scenario typically occurs when a node shuts down improperly, leaving the final data file in a corrupted state.
				"""
			contributors: ["anil-db"]
		},
		{
			type: "fix"
			description: """
				When there is an error encoding `udp` and `unix` socket datagrams, the event status is now updated correctly to indicate an error.
				"""
			contributors: ["aramperes"]
		},
		{
			type: "feat"
			description: """
				Add a new `incremental_to_absolute` transform which converts incremental metrics to absolute metrics. This is useful for
				use cases when sending metrics to a sink is lossy or you want to get a historical record of metrics, in which case
				incremental metrics may be inaccurate since any gaps in metrics sent will result in an inaccurate reading of the ending
				value.
				"""
			contributors: ["GreyLilac09"]
		},
		{
			type: "feat"
			description: """
				When config reload is aborted due to `GlobalOptions` changes, the specific top-level fields that differ are now logged to help debugging.
				"""
			contributors: ["suikammd"]
		},
		{
			type: "enhancement"
			description: """
				The `opentelemetry` source now supports a new decoding mode which can be enabled by setting `use_otlp_decoding` to `true`. In this mode,
				all events preserve the [OTLP](https://opentelemetry.io/docs/specs/otel/protocol/) format. These events can be forwarded directly to
				the `opentelemetry` sink without modifications.
				
				**Note:** The OTLP metric format and the Vector metric format differ, so the `opentelemetry` source emits OTLP formatted metrics as Vector log
				events. These events cannot be used with existing metrics transforms. However, they can be ingested by the OTEL collectors as metrics.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				Added validation to ensure a test that expects no output from a source, does not perform operations on said source.
				"""
			contributors: ["kalopsian-tz"]
		},
		{
			type: "feat"
			description: """
				The `prometheus_remote_write` source now supports optional NaN value filtering via the `skip_nan_values` configuration option.
				
				When enabled, metric samples with NaN values are discarded during parsing, preventing downstream processing of invalid metrics. For counters and gauges, individual samples with NaN values are filtered. For histograms and summaries, the entire metric is filtered if any component contains NaN values (sum, bucket limits, or quantile values).
				
				This feature defaults to `false` to maintain backward compatibility.
				"""
			contributors: ["elohmeier"]
		},
		{
			type: "enhancement"
			description: """
				Added support for varint length delimited framing for protobuf, which is compatible with standard protobuf streaming implementations and tools like ClickHouse.
				
				Users can now opt-in to varint framing by explicitly specifying `framing.method: varint_length_delimited` in their configuration. The default remains length-delimited framing for backward compatibility.
				"""
			contributors: ["modev2301"]
		},
	]

	vrl_changelog: """
		### 0.27.0 (2025-09-18)
		
		#### Breaking Changes & Upgrade Guide
		
		- The `validate_json_schema` functionality has been enhanced to collect and return validation error(s) in the error message return value, in addition to the existing primary Boolean `true / false` return value. (https://github.com/vectordotdev/vrl/pull/1483)

		Using JSON schema `test-schema.json` below:
		```json
		{
		"$schema": "https://json-schema.org/draft/2020-12/schema",
		"type": "object",
		"properties": {
		"test": {
		"type": "boolean"
		},
		"id": {
		"type": "integer"
		}
		},
		"required": ["test"],
		"additionalProperties": false
		}
		```
		
		Before:
		```text
		$ invalid_object = { "id": "123" }
		{ "id": "123" }
		
		$ valid, err = validate_json_schema(encode_json(invalid_object), "test-schema.json")
		false
		
		$ valid
		false
		
		$ err
		null
		```
		
		After:
		```text
		$ invalid_object = { "id": "123" }
		{ "id": "123" }
		
		$ valid, err = validate_json_schema(encode_json(invalid_object), "test-schema.json")
		"function call error for \"validate_json_schema\" at (13:82): JSON schema validation failed: \"123\" is not of type \"integer\" at /id, \"test\" is a required property at /"
		
		$ valid
		false
		
		$ err
		"function call error for \"validate_json_schema\" at (13:82): JSON schema validation failed: \"123\" is not of type \"integer\" at /id, \"test\" is a required property at /"
		```
		
		#### New Features
		
		- Added a new `xxhash` function implementing `xxh32/xxh64/xxh3_64/xxh3_128` hashing algorithms.
		
		authors: stigglor (https://github.com/vectordotdev/vrl/pull/1473)
		- Added an optional `strict_mode` parameter to `parse_aws_alb_log`. When set to `false`, the parser ignores any newly added/trailing fields in AWS ALB logs instead of failing. Defaults to `true` to preserve current behavior.
		
		authors: anas-aso (https://github.com/vectordotdev/vrl/pull/1482)
		- Added a new array function `pop` that removes the last item from an array.
		
		authors: jlambatl (https://github.com/vectordotdev/vrl/pull/1501)
		- Added two new cryptographic functions `encrypt_ip` and `decrypt_ip` for IP address encryption
		
		These functions use the IPCrypt specification and support both IPv4 and IPv6 addresses with two encryption modes: `aes128` (IPCrypt deterministic, 16-byte key) and `pfx` (IPCryptPfx, 32-byte key). Both algorithms are format-preserving (output is a valid IP address) and deterministic. (https://github.com/vectordotdev/vrl/pull/1506)
		
		#### Enhancements
		
		- Added an optional `body` parameter to `http_request`. Best used when sending a POST or PUT request.
		
		This does not perform automatic setting of `Content-Type` or `Content-Length` header(s). The caller should add these headers using the `headers` map parameter. (https://github.com/vectordotdev/vrl/pull/1502)
		
		#### Fixes
		
		- The `validate_json_schema` function no longer panics if the JSON schema file cannot be accessed or is invalid. (https://github.com/vectordotdev/vrl/pull/1476)
		- Fixed the `http_request` function's ability to run from the VRL CLI, no longer panics.
		
		authors: sbalmos (https://github.com/vectordotdev/vrl/pull/1510)
		"""

	commits: [
		{sha: "a8cbfb76461a6c3d3e3a82bff158c65c42e2d2fc", date: "2025-08-12 23:17:19 UTC", description: "v0.49.0 release", pr_number: 23579, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 60, insertions_count: 595, deletions_count: 134},
		{sha: "94d56e79c20a49a090b52318ad60e89ee5d7f72a", date: "2025-08-12 23:50:33 UTC", description: "playground fix build when `path` vrl dep is used", pr_number: 23577, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 28, deletions_count: 26},
		{sha: "f4c7b08cb436127ad6df916c447cf8d5ed70ef79", date: "2025-08-13 00:20:43 UTC", description: "0.49.0 release - address docs review", pr_number: 23581, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 8, deletions_count: 9},
		{sha: "dcfbbde7f7e92e30d7e7be11e2b2b8ac4545dc40", date: "2025-08-13 00:34:06 UTC", description: "config module and reduce duplication", pr_number: 23580, scopes: ["opentelemetry source"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 414, deletions_count: 450},
		{sha: "4dc7c7701e0bda1a4c4238c360c708ccf6620474", date: "2025-08-13 12:35:44 UTC", description: "bump async-nats from 0.33.0 to 0.42.0", pr_number: 23564, scopes: ["deps"], type: "chore", breaking_change: false, author: "Benjamin Dornel", files_count: 5, insertions_count: 55, deletions_count: 36},
		{sha: "6417a85adb0aa805763b093631fd0f458012ded5", date: "2025-08-13 01:06:51 UTC", description: "use sha instead of ref in changes.yml", pr_number: 23582, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "d2e3bd177448f6a6f6b086cbe9a25d97b9982a6a", date: "2025-08-13 18:41:08 UTC", description: "add one more highlight", pr_number: 23586, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "8dfd2a07f0b668afe1289d422e110836d0fa605b", date: "2025-08-13 18:57:26 UTC", description: "sort semantic scopes", pr_number: 23587, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 16, deletions_count: 10},
		{sha: "9228f326c2091f76a35ef48579f161be76d42834", date: "2025-08-13 21:26:17 UTC", description: "improve minor release issue template", pr_number: 23590, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 16, deletions_count: 9},
		{sha: "44731453e53611fdeda2355b80e76b2e75339389", date: "2025-08-13 21:56:49 UTC", description: "multiline env var interpolation", pr_number: 23588, scopes: ["deprecations"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "07296113ad46cfc9c8ff6bd1490084dd6ed46b03", date: "2025-08-14 08:03:33 UTC", description: "document Vector 0.49.0 behavior change", pr_number: 23594, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 17, deletions_count: 2},
		{sha: "ba4390af918ce576439b8dde21389fb4bd526b98", date: "2025-08-14 16:20:04 UTC", description: "Expose Datadog search matching directly", pr_number: 23578, scopes: ["transforms"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 22, deletions_count: 6},
		{sha: "0c16d840ef2809cee875f5c06e355fdde51ddc50", date: "2025-08-14 20:45:41 UTC", description: "upgrade cargo-deb to 3.4.1", pr_number: 23591, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c5f394b1afb5583a2a355f0c42e71695384f5f82", date: "2025-08-16 00:22:59 UTC", description: "run k8s tests when k8s library files change", pr_number: 23600, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "1294a8d75006f7d67a9be947d1863e6d3c3a95d7", date: "2025-08-18 19:09:45 UTC", description: "use workspace whenever possible", pr_number: 23599, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 19, insertions_count: 108, deletions_count: 146},
		{sha: "d035b63a9ac9748a2b261006c10855b51033d2e3", date: "2025-08-18 18:38:05 UTC", description: "Handle `serde(untagged)` in enum variants", pr_number: 23575, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 113, deletions_count: 6},
		{sha: "efe2b0c86d0b9f285da35f80a5c19f590c8fcc18", date: "2025-08-18 20:05:50 UTC", description: "Rework gauge metric handling", pr_number: 23561, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 218, deletions_count: 330},
		{sha: "c5fa0500941cb6d011fb899a10cd3d8a5ce0de5b", date: "2025-08-18 22:07:29 UTC", description: "refactor file source into common module", pr_number: 23607, scopes: ["file source"], type: "chore", breaking_change: false, author: "Thomas", files_count: 26, insertions_count: 195, deletions_count: 123},
		{sha: "4e90e05869c8723585cd186770387998103ffd07", date: "2025-08-19 00:11:50 UTC", description: "update Typesense connection string [WEB-6971]", pr_number: 23611, scopes: ["website"], type: "chore", breaking_change: false, author: "Nick Sollecito", files_count: 1, insertions_count: 17, deletions_count: 2},
		{sha: "bf948f7434ada293f0a8dcc2b43aaaf763504b4f", date: "2025-08-19 00:41:09 UTC", description: "fix protobuf codecs supported data types", pr_number: 23610, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 12, deletions_count: 3},
		{sha: "3f7f6ebceed18d34f81afd79761b3e8a654b7b00", date: "2025-08-19 17:01:30 UTC", description: "add varint length delimited framing for protobuf", pr_number: 23352, scopes: ["codecs"], type: "feat", breaking_change: false, author: "MoSecureSyntax", files_count: 41, insertions_count: 942, deletions_count: 5},
		{sha: "fffcea5b3868c5c0e1e499fba0bba6915b6199c3", date: "2025-08-19 22:31:32 UTC", description: "upgrade to Rust 2024 edition", pr_number: 23522, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 583, insertions_count: 3229, deletions_count: 2820},
		{sha: "e0e8463be767a212c1e4116c3f7edfe94c6aa0f2", date: "2025-08-20 18:38:16 UTC", description: "update libs to 2024 edition", pr_number: 23620, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 240, insertions_count: 882, deletions_count: 756},
		{sha: "c2f0d4efdcb61ebc85cd4e57bbbf005dddc7bd6e", date: "2025-08-21 09:51:34 UTC", description: "add support for NATS JetStream", pr_number: 23554, scopes: ["nats source"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 10, insertions_count: 1176, deletions_count: 909},
		{sha: "d3bb3d24feb7aba03a5b8ef49f5dc7aa0b39f638", date: "2025-08-20 21:55:17 UTC", description: "more debugging statements for vdev", pr_number: 23621, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 13, deletions_count: 5},
		{sha: "e4552eaf6ac7a4bb19aff7e14862a203e7ccf68a", date: "2025-08-20 22:22:59 UTC", description: "better e2e out dir permissions", pr_number: 23622, scopes: ["tests"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 7, deletions_count: 1},
		{sha: "103ed488ee8f7a3d5a9e7d321b0602b95b3e6aca", date: "2025-08-21 00:58:41 UTC", description: "integration script runner should run all environments", pr_number: 23623, scopes: ["tests"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 259, deletions_count: 142},
		{sha: "0f154d501201ca1d4cfa67585a6d0a9b47bb3ffc", date: "2025-08-21 23:23:49 UTC", description: "properly set detached cancel button's height", pr_number: 23627, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b3d264117ba473fdec4e9a8d423aedee26c79b41", date: "2025-08-22 17:55:56 UTC", description: "msrv increase timeout", pr_number: 23634, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b884ee7d38482632f3d44f8c7b4010be8f681160", date: "2025-08-23 07:30:24 UTC", description: "Add documentation and tests about OpenSearch", pr_number: 23603, scopes: ["elasticsearch sink"], type: "chore", breaking_change: false, author: "Hiroki Sakamoto", files_count: 2, insertions_count: 101, deletions_count: 1},
		{sha: "f86a9d472a2bdaaf4ab48deb3cbb92dea4bed9f4", date: "2025-08-23 06:34:24 UTC", description: "make shared nats module public", pr_number: 23625, scopes: ["nats source"], type: "chore", breaking_change: false, author: "Benjamin Dornel", files_count: 2, insertions_count: 5, deletions_count: 1},
		{sha: "a99d096a9368db221070b3e664c4c41de8e6f47e", date: "2025-08-25 21:04:01 UTC", description: "deprecate x86_64 macOS", pr_number: 23644, scopes: ["deprecations"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "e4ee26d913f1dc70a2f50b8884a13536103b8bde", date: "2025-08-25 21:16:14 UTC", description: "refactor support page", pr_number: 23642, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 25, deletions_count: 54},
		{sha: "95caa649cd3bd9affc8d730aec9e8ec6b7030ee6", date: "2025-08-26 11:06:39 UTC", description: "terminology consistency", pr_number: 23640, scopes: ["website"], type: "fix", breaking_change: false, author: "Junwon Lee", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "abcb5a73f1cea1d0101d7639fc2699b9611c8878", date: "2025-08-26 06:24:18 UTC", description: "rust clippy enhancements", pr_number: 23647, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 48, deletions_count: 19},
		{sha: "1601ff2a3c4d791b13f9c912466d352f8d2a3a57", date: "2025-08-26 06:45:51 UTC", description: "volume path resolution", pr_number: 23637, scopes: ["vdev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 146, deletions_count: 61},
		{sha: "dcf951e8f9f6e37ffed43225b5986b0b094b0901", date: "2025-08-26 20:38:07 UTC", description: "fix default features to all-features", pr_number: 23654, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 85, deletions_count: 80},
		{sha: "4636c76610fb1a9ea9518cf8aabe3392f705a3a2", date: "2025-08-27 15:54:15 UTC", description: "Add xxhash vrl function documentation", pr_number: 23576, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jared Patterson", files_count: 5, insertions_count: 79, deletions_count: 4},
		{sha: "84dd39a9ea33fba496dfbfae22b147cfff57b4a0", date: "2025-08-28 07:09:58 UTC", description: "call `drain()` during shutdown", pr_number: 23635, scopes: ["nats source"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 3, insertions_count: 119, deletions_count: 19},
		{sha: "904ed1df90cb8191df8bbcec09c2e329207ed598", date: "2025-08-27 22:23:11 UTC", description: "Add new incremental_to_absolute transform and change to MetricSet to an LRU cache with optional capacity policies", pr_number: 23374, scopes: ["new transform"], type: "feat", breaking_change: false, author: "Derek Zhang", files_count: 11, insertions_count: 898, deletions_count: 108},
		{sha: "badeb4a2e34568ac35ba1cbd755eb0a3085fe4a7", date: "2025-08-28 01:45:38 UTC", description: "bump Rust to 1.89", pr_number: 23650, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 151, insertions_count: 1074, deletions_count: 1147},
		{sha: "ecb1b4175584673d383a70b988f9b0e214e6f6c2", date: "2025-08-28 18:56:15 UTC", description: "fixed environment parsing for local IT runs", pr_number: 23664, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 14, deletions_count: 10},
		{sha: "6e26596875d5f229ef59296b1715a966a35367d0", date: "2025-08-28 19:49:10 UTC", description: "features check", pr_number: 23663, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 5, deletions_count: 0},
		{sha: "44551f5b99fa15d1734aead708a3c8725d7113fe", date: "2025-08-28 23:01:12 UTC", description: "prevent panic in debug+IDE builds", pr_number: 23668, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 22, deletions_count: 12},
		{sha: "563688f43ee4742048b4401330d9ad8bcb4a8aa3", date: "2025-08-28 23:09:01 UTC", description: "update VRL", pr_number: 23667, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 141, deletions_count: 41},
		{sha: "590c14b8a6d618588246a6603242fb3bfa873e0b", date: "2025-08-29 00:36:34 UTC", description: "Update SMP CLI", pr_number: 23669, scopes: ["ci"], type: "chore", breaking_change: false, author: "Caleb Metz", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "e071b23438a0d22f9aa25fc09d4456911e92ef07", date: "2025-08-29 00:42:32 UTC", description: "Make tests pass with `cargo test`", pr_number: 23520, scopes: ["tests"], type: "fix", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 169, deletions_count: 81},
		{sha: "86d4f0a652d0da1cdec7bc8a8fca16113e29fb33", date: "2025-08-30 07:42:08 UTC", description: "log changed global fields when reload is rejected", pr_number: 23662, scopes: ["config"], type: "feat", breaking_change: false, author: "Suika", files_count: 3, insertions_count: 66, deletions_count: 0},
		{sha: "56eee56c60356eda67b1a4767c799c1c0f94cf7d", date: "2025-08-29 18:51:43 UTC", description: "okta", pr_number: 22968, scopes: ["new source"], type: "feat", breaking_change: false, author: "John Sonnenschein", files_count: 11, insertions_count: 1019, deletions_count: 0},
		{sha: "b4a78c81882aefdf81664f3e72a4e948f9a21659", date: "2025-08-30 01:37:49 UTC", description: "fix sources-okta component features check", pr_number: 23673, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 9, deletions_count: 1},
		{sha: "995a5b5022c3fc9221f252058e75027b15f75998", date: "2025-08-30 06:26:44 UTC", description: "Bump tracing-subscriber from 0.3.19 to 0.3.20 in the cargo group", pr_number: 23674, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 27, deletions_count: 45},
		{sha: "84c28ec3c0b40640ff6cb1896398c01b282a2b14", date: "2025-09-02 20:00:30 UTC", description: "update windows runner to 2025", pr_number: 23701, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "188a154fc241384d740a2f1a29ef5bcf0b61b554", date: "2025-09-03 01:49:33 UTC", description: "Bump tempfile from 3.20.0 to 3.21.0", pr_number: 23687, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "17b1a507e59533b5f23f39ad9046ba02f43dcc3e", date: "2025-09-02 23:22:07 UTC", description: "increase test-misc timeout", pr_number: 23704, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "d62fec3fd5a3bcc68e2a192a91f2823f002f603e", date: "2025-09-03 01:52:34 UTC", description: "Bump uuid from 1.17.0 to 1.18.0", pr_number: 23685, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "76553c12e651eb2a866c5456dcdf0da3f2edfbff", date: "2025-09-03 02:48:01 UTC", description: "Bump indexmap from 2.10.0 to 2.11.0", pr_number: 23682, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 30, deletions_count: 30},
		{sha: "49ac8f862b4a00a17dda6078b2034ce3fbd13543", date: "2025-09-03 02:49:20 UTC", description: "Bump actions/cache from 4.2.3 to 4.2.4", pr_number: 23697, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 6, deletions_count: 6},
		{sha: "9423eba38537d1fd44a0c24652fef6f879190565", date: "2025-09-03 00:07:41 UTC", description: "prepare for crates.io", pr_number: 23705, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 10, deletions_count: 1},
		{sha: "717331bd319a05df2fba5c8b12d674093cbf21c1", date: "2025-09-03 03:31:18 UTC", description: "Bump notify from 8.1.0 to 8.2.0", pr_number: 23680, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "cf211d10b8688d8a36b6c505a84739d238c2425a", date: "2025-09-03 01:13:27 UTC", description: "support OTLP logs, metrics & traces", pr_number: 23524, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 17, insertions_count: 394, deletions_count: 144},
		{sha: "5ea9f72f619bd94c91505e2e19833c8bd48be451", date: "2025-09-03 01:42:30 UTC", description: "Bump syslog_loose from 0.22.0 to 0.23.0", pr_number: 23688, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 3},
		{sha: "031c2794dc96a21abedf0addf351f0ae59f7fda3", date: "2025-09-03 01:48:55 UTC", description: "introduce release workflow", pr_number: 23706, scopes: ["vdev"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 60, deletions_count: 0},
		{sha: "7b28d5b902cc41647d2cf9f0c18d5fa197869c93", date: "2025-09-03 19:13:45 UTC", description: "vdev publish remove sudo", pr_number: 23712, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f646fb428d67b006097471f1ab208b184f55a527", date: "2025-09-03 22:56:50 UTC", description: "Bump docker/login-action from 3.4.0 to 3.5.0", pr_number: 23710, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "d161bd3bda9f4b3ae06a81838eb6f625ee3c90df", date: "2025-09-03 22:57:56 UTC", description: "Bump github/codeql-action from 3.29.7 to 3.30.0", pr_number: 23699, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "92df6454d86b5c826341e9114dceb06317b4b774", date: "2025-09-03 20:07:34 UTC", description: "rename e2e vector config", pr_number: 23713, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 0},
		{sha: "8345693af6f7dbe982c76af6e18fbf5d186bc28e", date: "2025-09-04 00:49:19 UTC", description: "Bump netlink-packet-core from 0.7.0 to 0.8.0", pr_number: 23691, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 17, deletions_count: 2},
		{sha: "0fdfaeadffc813c677adf3e0ab0d43d7d8fc6572", date: "2025-09-03 21:07:43 UTC", description: "simplify bootstrap-macos.sh", pr_number: 23714, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 11},
		{sha: "8450f85f40830755ad9049e3e6503e47a5d5d88e", date: "2025-09-04 01:18:28 UTC", description: "Bump actions/checkout from 4.2.2 to 5.0.0", pr_number: 23698, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 25, insertions_count: 85, deletions_count: 85},
		{sha: "1885f094497a15fa4cc40ea435ab83ce56bd960f", date: "2025-09-03 22:00:23 UTC", description: "Bump sysinfo from 0.36.1 to 0.37.0", pr_number: 23686, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b6f8e354c0e568cc083821790dfcb9dd0c3a321f", date: "2025-09-04 02:01:16 UTC", description: "Bump security-framework from 3.2.0 to 3.3.0", pr_number: 23684, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "06f2f59c2dd836c3de98ba22181cd2eb1c0a70bd", date: "2025-09-03 23:40:53 UTC", description: "vdev publish binary location fix", pr_number: 23716, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 13, deletions_count: 22},
		{sha: "606c896197aacb09de4f5f05478b37400081dc0e", date: "2025-09-04 21:47:44 UTC", description: "Bump the patches group across 1 directory with 33 updates", pr_number: 23721, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 336, deletions_count: 330},
		{sha: "515dd9fe441755da0dff7d8ba1e61349d6047de7", date: "2025-09-04 20:53:17 UTC", description: "each SMP experiment now requests 6 cpus instead of 7", pr_number: 23722, scopes: ["ci"], type: "chore", breaking_change: false, author: "Scott Opell", files_count: 26, insertions_count: 26, deletions_count: 26},
		{sha: "cb08d026c7718fe079d37408e505db83c2d66bcc", date: "2025-09-05 01:50:48 UTC", description: "Add support for v1.1 protocol of secrets exec backend ", pr_number: 23655, scopes: ["config"], type: "feat", breaking_change: false, author: "Rob Blafford", files_count: 5, insertions_count: 257, deletions_count: 10},
		{sha: "9c0cba6e6929176ba80b659c08a0868dcccfeded", date: "2025-09-06 07:19:38 UTC", description: "Panic caused by edge case during unit test build", pr_number: 23628, scopes: ["unit tests"], type: "fix", breaking_change: false, author: "Kal Brydson", files_count: 2, insertions_count: 8, deletions_count: 1},
		{sha: "09461c963d4f0aeb0d6c100c17b175cc33965cd9", date: "2025-09-05 18:00:28 UTC", description: "Update azure (0.25) and azure storage (0.21)", pr_number: 23351, scopes: ["azure_blob sink"], type: "chore", breaking_change: true, author: "Thomas", files_count: 10, insertions_count: 193, deletions_count: 238},
		{sha: "adb51fb633df05b67ba29e908cfa3addd61d4ce9", date: "2025-09-08 17:41:15 UTC", description: "update backtrace to remove deprecated adler", pr_number: 23731, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 14, deletions_count: 31},
		{sha: "1f9b9d77a6e80bf26ee8c50a0cb88e0b2ff7dfdc", date: "2025-09-09 00:14:06 UTC", description: "Update debian image usages to trixie", pr_number: 23720, scopes: ["deps"], type: "chore", breaking_change: false, author: "Denise Ratasich", files_count: 8, insertions_count: 9, deletions_count: 8},
		{sha: "697b64dec74167313050b3af8f0d969d533b4e22", date: "2025-09-08 18:36:31 UTC", description: "format/merge imports with nightly rustfmt options", pr_number: 23730, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 757, insertions_count: 4384, deletions_count: 3776},
		{sha: "6a5eecc23e212f07ffe0cf62a70bea3c92190e52", date: "2025-09-08 20:45:26 UTC", description: "better formatting with nightly features", pr_number: 23734, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 32, deletions_count: 16},
		{sha: "d427a862cc322eb218ef88b1cd5829b65e32e85b", date: "2025-09-08 21:50:30 UTC", description: "rengen component docs", pr_number: 23737, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 34, deletions_count: 0},
		{sha: "e68c9501a5ae07b248e6028e5ac9f121c554cf24", date: "2025-09-08 21:43:11 UTC", description: "add vdev to bininstall targets", pr_number: 23735, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 9, deletions_count: 1},
		{sha: "aef3e60a608b5dab8ba0b70015c70fb3f78f3e6f", date: "2025-09-08 23:02:35 UTC", description: "remove `components` filter component docs check", pr_number: 23738, scopes: ["vdev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 5, deletions_count: 2},
		{sha: "710fa5d953df13f810db1f5be9648d8c2d1ece40", date: "2025-09-09 00:18:04 UTC", description: "add how it works section for OTLP decoding", pr_number: 23736, scopes: ["opentelemetry source"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 72, deletions_count: 0},
		{sha: "6031b3e32b6bbf0de8b58f746fb28e3171ec0688", date: "2025-09-10 00:18:16 UTC", description: "make file server async", pr_number: 23612, scopes: ["file source"], type: "feat", breaking_change: false, author: "Thomas", files_count: 16, insertions_count: 619, deletions_count: 515},
		{sha: "b8a34577bcc90a9aa9c0c2310da1069852a5be42", date: "2025-09-10 18:12:10 UTC", description: "add 100 file source regression test", pr_number: 23742, scopes: ["performance"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 65, deletions_count: 0},
		{sha: "9d9542d29032fa8d6b0e4f300a08d8a6bff6e215", date: "2025-09-11 00:30:07 UTC", description: "add per-event ttl for memory enrichment table", pr_number: 23666, scopes: ["enrichment tables"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 5, insertions_count: 99, deletions_count: 15},
		{sha: "56776c96914c0505b3ccb4bac0873d9b783df4f1", date: "2025-09-11 00:52:14 UTC", description: "various fixes and enhancements", pr_number: 23755, scopes: ["vrl playground"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 217, deletions_count: 164},
		{sha: "9840a13a6ca37755e0bc44d1b1014e7cd0d873a1", date: "2025-09-11 19:03:00 UTC", description: "remove explicit libstdc++ dep", pr_number: 23758, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 1, deletions_count: 2},
		{sha: "f70b3d893de3cbd63b388149e79dcb239baa112b", date: "2025-09-11 20:01:28 UTC", description: "ci-integration-review.yml update-pr-status bug", pr_number: 23760, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6de4ad1d284728c9e6ca801626eaf613cdd7bfe9", date: "2025-09-11 20:36:11 UTC", description: "'encoding' field in 'http_server' source", pr_number: 23759, scopes: ["deprecations"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 18, deletions_count: 5},
		{sha: "5f49fa060c8e5aa7e0b7b154873cd56b0b046c53", date: "2025-09-11 21:51:37 UTC", description: "remove unused powerpc arch", pr_number: 23761, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 0, deletions_count: 28},
		{sha: "2b4923f6d676441dd855156ba160d59a60cde8c8", date: "2025-09-11 22:21:53 UTC", description: "update security policy", pr_number: 23715, scopes: ["security"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 16, deletions_count: 12},
		{sha: "1a8e3fe6e70da575712e4142e8acfc38bd3bdc1d", date: "2025-09-11 22:45:00 UTC", description: "introduce base cross dockerfile", pr_number: 23763, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 26, deletions_count: 49},
		{sha: "4c2d9caeaf29a12353178eeb3ddb44dfd65f86d5", date: "2025-09-12 17:35:25 UTC", description: "Bump aws-actions/configure-aws-credentials from 4.2.1 to 4.3.1", pr_number: 23696, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "814833931cb2028001a41ef5520a94d1552a791a", date: "2025-09-12 18:32:04 UTC", description: "libc workspace dependency", pr_number: 23767, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 4, deletions_count: 3},
		{sha: "2d94959181f18feaef8bb2980698ec96f6d6dfab", date: "2025-09-12 19:05:17 UTC", description: "Bump axios from 1.8.2 to 1.12.0 in /website in the npm_and_yarn group across 1 directory", pr_number: 23768, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 15},
		{sha: "5671d44fa8db2bf0684fcaaf5894606b9cf28d3c", date: "2025-09-12 22:02:36 UTC", description: "proper rustup and toolchain installation in 'prepare.sh'", pr_number: 23770, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 38, deletions_count: 12},
		{sha: "eee6e669965edc25cfbb73e6c2e8c9beead113ca", date: "2025-09-12 23:04:27 UTC", description: "disable compression in HEC indexer ack queries", pr_number: 23560, scopes: ["splunk_hec sink"], type: "fix", breaking_change: false, author: "Scott Balmos", files_count: 3, insertions_count: 11, deletions_count: 0},
		{sha: "f705759ac638d7faf406d5b7f725f2a94c91438d", date: "2025-09-15 18:00:46 UTC", description: "Bump actions/download-artifact from 4.3.0 to 5.0.0 in the artifact group", pr_number: 23700, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 52, deletions_count: 52},
		{sha: "dfa09ce7151c998c041852deefd2735d5f1439ab", date: "2025-09-15 22:03:31 UTC", description: "homebrew.rs should use GITHUB_TOKEN for CI", pr_number: 23779, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2bbc10886dbbb54723940de6cf95cddf811608a7", date: "2025-09-16 00:42:54 UTC", description: "improve output", pr_number: 23782, scopes: ["vrl playground"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 73, deletions_count: 57},
		{sha: "582fd3cfd55bf5dc27c62b3da9b0efcffdf55bce", date: "2025-09-16 17:33:32 UTC", description: "partial_cmp used in sort functions causing panics", pr_number: 23780, scopes: ["sinks", "sources"], type: "fix", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 95, deletions_count: 11},
		{sha: "f86b2d66e8bda044483362c98f1c769d737241ac", date: "2025-09-16 16:25:25 UTC", description: "Allow disabling listwatching of namespaces", pr_number: 23601, scopes: ["kubernetes_logs source"], type: "enhancement", breaking_change: false, author: "Brian Stack", files_count: 4, insertions_count: 119, deletions_count: 25},
		{sha: "77615b9f5354a3dd44e82e3dc1d913091a9997a2", date: "2025-09-17 09:05:45 UTC", description: "fix `parse_nginx_log` docs rendering", pr_number: 23792, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "fc84b8639f1ceed3e166410bfefebfe5b75d822a", date: "2025-09-16 20:14:57 UTC", description: "fix panic in disk buffer when dealing with corrupted file", pr_number: 23617, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Anil Gupta", files_count: 4, insertions_count: 178, deletions_count: 5},
		{sha: "7495f1e6ced9691e9263deb530f732bd0ac63d7f", date: "2025-09-16 23:27:07 UTC", description: "fix duration subtraction overflow in TimingStats::report", pr_number: 23791, scopes: ["file source"], type: "fix", breaking_change: false, author: "mayuransw", files_count: 2, insertions_count: 7, deletions_count: 1},
		{sha: "0613d2089e65f8ea99db5dfb077e36074eda869d", date: "2025-09-17 06:49:05 UTC", description: "Add optional NaN value filtering", pr_number: 23774, scopes: ["prometheus_remote_write source"], type: "feat", breaking_change: false, author: "elohmeier", files_count: 4, insertions_count: 366, deletions_count: 7},
		{sha: "3a27665fc620be46b2f9f37967e1a15ef75319b0", date: "2025-09-17 22:10:35 UTC", description: "use legacy mongodb image", pr_number: 23797, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "92781bda98ee4155fade1428fb27818148f41782", date: "2025-09-18 04:47:02 UTC", description: "add documentation for IPCrypt functions", pr_number: 23771, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Alter Step", files_count: 5, insertions_count: 191, deletions_count: 6},
		{sha: "8db96061725342d0a43df617472326d2c3caeb82", date: "2025-09-17 22:47:16 UTC", description: "fix default retry strategy", pr_number: 23795, scopes: ["aws_s3 sink"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 53, deletions_count: 5},
		{sha: "dbe0d63ad7c13390b83dee1f496454bc3f736d7d", date: "2025-09-18 13:56:31 UTC", description: "Retry failed records on partial failures", pr_number: 23733, scopes: ["aws_kinesis_streams sink"], type: "feat", breaking_change: false, author: "Haitao Li", files_count: 8, insertions_count: 281, deletions_count: 5},
		{sha: "dfd1d659b880f4960f5c2715f70e62ca67563fa2", date: "2025-09-17 23:58:56 UTC", description: "publish dev environment weekly", pr_number: 23800, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 9},
		{sha: "a98693c6259eca59c65dd6b96d7c8b95f352c516", date: "2025-09-18 17:47:32 UTC", description: "Support GELF chunking for encoding", pr_number: 23728, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Aram Peres", files_count: 40, insertions_count: 763, deletions_count: 70},
		{sha: "a0814088d5b0d0e0138006397cb71b843244cd19", date: "2025-09-18 19:34:19 UTC", description: "added aws feature gate", pr_number: 23802, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 0},
		{sha: "c29c71026a9e714654cb29f1120d3a687a4a36d1", date: "2025-09-18 22:25:50 UTC", description: "release prep skip vdev tags", pr_number: 23805, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 20, deletions_count: 7},
		{sha: "d0dbbacecd111b2008f3ade666e137a46bf331bc", date: "2025-09-18 23:01:13 UTC", description: "remove nightly fmt options", pr_number: 23806, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 4, deletions_count: 5},
		{sha: "1ec7191706af9eed396b5031f932094e1e03d542", date: "2025-09-18 23:48:02 UTC", description: "add revert to known commit types", pr_number: 23807, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
	]
}
