package metadata

releases: "0.49.0": {
	date:     "2025-08-12"
	codename: ""

	whats_next: []

	known_issues: [
		"""
			The protobuf codecs do not support all telemetry types. Specifically, the following applies:
			- Decoder: supports logs.
			- Encoder: supports logs and traces.

			Metrics are not supported. Any future updates will be noted in changelogs.
			""",
	]

	description: """
		The Vector team is excited to announce version `0.49.0`!

		Please refer to the [upgrade guide](/highlights/2025-08-12-0-49-0-upgrade-guide.md) for breaking changes in this release.

		**Release highlights**:

		- A `websocket` source was introduced. A WebSocket source in Vector enables ingestion of real-time data from services that expose WebSocket APIs.
		- The `http` sink's `uri` and `request.headers` config fields now support templating, enabling dynamic construction based on event data.
		- The `--watch-config` flag now also watches for changes in enrichment table files.
		- Fixed a race condition that could cause negative values in the `vector_buffer_byte_size` and `vector_buffer_events` gauges.
		- The `prometheus_remote_write` sink now offers a `expire_metrics_secs` config option. This fixes an issue where incremental metrics were preserved for the lifetime of Vector's runtime causing indefinite memory growth.
		"""

	changelog: [
		{
			type: "enhancement"
			description: """
				Extends retry logic in `aws_s3` sink by allowing users to configure which requests to retry.
				"""
			contributors: ["jchap-pnnl", "jhbigler-pnnl"]
		},
		{
			type: "feat"
			description: """
				The `http` sink's `uri` config is now templateable, allowing for dynamic URI building based on event fields.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "enhancement"
			description: """
				The `http` sink `request.headers` configuration now supports templated values.
				"""
			contributors: ["notchairmk"]
		},
		{
			type: "fix"
			description: """
				The `utilization` metric is now properly published periodically, even when there are no events flowing through the components.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Added a `query_settings` and `async_insert_settings` option in the `clickhouse` sink, which allows users to configure asynchronous inserts.
				"""
			contributors: ["pm5"]
		},
		{
			type: "enhancement"
			description: """
				The [enrichment functions](https://vector.dev/docs/reference/vrl/functions/#enrichment-functions) now support an optional wildcard parameter where a match succeeds if the field value equals either the wildcard or the actual comparison value.
				"""
			contributors: ["nzxwang"]
		},
		{
			type: "fix"
			description: """
				Fixed a bug in the `elasticsearch` sink that caused URI credentials to be ignored. They are now correctly used.
				"""
			contributors: ["ynachi"]
		},
		{
			type: "fix"
			description: """
				Previously the `postgres` sink healthcheck was not implemented correctly. Now Vector can start when `healthcheck.enabled` is set to `false`.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "fix"
			description: """
				Secret names in the configuration can now contain hyphens. For example `"SECRET[systemd.vm-token]" is now valid.
				"""
			contributors: ["optician"]
		},
		{
			type: "chore"
			description: """
				Previously `heroku_logs` and `demo_logs` sinks could output logs, metrics and traces depending on the decoding. Now they can only output logs.
				This behavior was unintuitive and undocumented.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: """
				The `nats` sink now does not return an error when an unresolvable or unavailable URL is provided.**Note**: If `--require-healthy` is set, Vector stops on startup.
				"""
			contributors: ["rdwr-tomers"]
		},
		{
			type: "fix"
			description: """
				VRL programs can now read the `interval_ms` field. This field was previously writeable but not readable.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue where tags were not modifiable via VRL if `metrics-tag-values` was set to `full`.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: """
				The `--watch-config` flag now also watches for changes in enrichment table files.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue in the `dnstap` and `tcp` sources where throughput could drop significantly when the number of connections exceeded the number of available cores.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fixed a race condition that caused negative values to be reported by the `vector_buffer_byte_size` and `vector_buffer_events` gauges.
				"""
			contributors: ["vparfonov"]
		},
		{
			type: "feat"
			description: """
				Added `time_settings` configuration to the `dedupe` transform, allowing the `max_age` of items in the deduplication cache to be set. This helps distinguish between true duplicates and expected repetition in data over longer periods of time.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "enhancement"
			description: """
				The `nats` sink now supports message headers when publishing to JetStream.
				
				It introduces a configurable, templated Nats-Msg-Id header that ensures a unique ID for each message. This enables broker-level deduplication, resulting in stronger delivery guarantees and exactly-once semantics when combined with idempotent consumers.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue that could cause a panic when reading environment variables containing non-UTF8 data. Such variables are now handled gracefully.
				"""
			contributors: ["kurochan"]
		},
		{
			type: "fix"
			description: """
				Prevented a panic when `DD_API_KEY` value contains newline characters. Such values are now handled gracefully.
				"""
			contributors: ["kurochan"]
		},
		{
			type: "feat"
			description: """
				Added a `max_packet_size` option to set max packet size for the `mqtt` source and sink.
				"""
			contributors: ["simplepad"]
		},
		{
			type: "feat"
			description: """
				Added a new `websocket` source.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "feat"
			description: """
				Added the `max_size` configuration option for memory buffers.
				"""
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: """
				Batches encoded using newline-delimited framing now include a trailing newline at the end.
				"""
			contributors: ["jszwedko"]
		},
		{
			type: "feat"
			description: """
				The `request_retry_partial` behavior for the `elasticsearch` sink was changed. Now only the failed retriable requests in a bulk is retried (instead of all requests).
				"""
			contributors: ["Serendo"]
		},
		{
			type: "fix"
			description: """
				Fixed a potential hang in Vector core caused by the internal concurrent map. This issue could prevent Vector from shutting down cleanly.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "feat"
			description: """
				- Added a TTL-based cache for metric sets.
				- Introduced the `expire_metrics_secs` configuration to the Prometheus remote write sink, leveraging the new TTL-based cache.
				- This resolves an issue where incremental metrics were retained for the entire lifetime of Vector, leading to unbounded memory growth.
				"""
			contributors: ["GreyLilac09"]
		},
		{
			type: "fix"
			description: """
				Fixed a `log_to_metric` configuration bug where the `all_metrics` field could not be used without also specifying `metrics`. It can now be set independently.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				The `log_to_metric` transforms now emits a pair expansion error. This error was previously silently ignored.
				"""
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: """
				The `protobuf` codecs now support all telemetry data types (logs, metrics, traces).
				"""
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: """
				Adds support for Redis Sentinel in the `redis` sink.
				"""
			contributors: ["5Dev24"]
		},
		{
			type: "feat"
			description: """
				Adds support for the Redis ZADD command in the `redis` sink.
				"""
			contributors: ["5Dev24"]
		},
		{
			type: "enhancement"
			description: """
				The `UnsignedIntTemplate` now supports `strftime` formatting. For example, this `%Y%m%d%H` template evaluates timestamps to a number.
				"""
			contributors: ["5Dev24"]
		},
	]

	vrl_changelog: """
		### 0.26.0
		
		#### Breaking Changes & Upgrade Guide
		
		- The `parse_cef` now trims unnecessary whitespace around escaped values in both headers and extension fields, improving accuracy and reliability when dealing with messy input strings.
			authors: yjagdale (https://github.com/vectordotdev/vrl/pull/1430)

		- The `parse_syslog` function now treats RFC 3164 structured data items with no parameters (e.g., `[exampleSDID@32473]`) as part of the main
		message, rather than parsing them as structured data. Items with parameters (e.g., `[exampleSDID@32473 field="value"]`) continue to be
		parsed as structured data. (https://github.com/vectordotdev/vrl/pull/1435)

		- `encode_lz4`  no longer prepends the uncompressed size by default, improving compatibility with standard LZ4 tools. A new `prepend_size` flag restores the old behavior if needed. Also, `decode_lz4` now also accepts `prepend_size` and a `buf_size` option (default: 1MB).
			authors: jlambatl (https://github.com/vectordotdev/vrl/pull/1447)
		
		#### New Features
		
		- Added `haversine` function for calculating [haversine](https://en.wikipedia.org/wiki/Haversine_formula) distance and bearing.
			authors: esensar Quad9DNS (https://github.com/vectordotdev/vrl/pull/1442)

		- Add `validate_json_schema` function for validating JSON payloads against JSON schema files. An optional configuration parameter `ignore_unknown_formats` is provided to change how custom formats are handled by the validator. Unknown formats can be silently ignored by setting this to `true` and validation continues without failing due to those fields.
			authors: jlambatl (https://github.com/vectordotdev/vrl/pull/1443)
		"""

	commits: [
		{sha: "d61446d7471c5c469c77d95591fd46f7da12d33f", date: "2025-06-26 20:38:07 UTC", description: "use defaults in changes.yml", pr_number: 23273, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 11, deletions_count: 21},
		{sha: "25354f71c3df586ecc4402b8bc56841d0ad53c25", date: "2025-06-26 20:59:16 UTC", description: "it suite should run when manually evoked", pr_number: 23274, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 8, deletions_count: 37},
		{sha: "889760f4a7942136bbedd3640bf289d11bb8f906", date: "2025-06-26 22:13:36 UTC", description: "fix shebangs", pr_number: 23276, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 9, insertions_count: 9, deletions_count: 9},
		{sha: "6d484ae90547ebd51ecdee89cb268644b31a4cda", date: "2025-06-27 00:32:43 UTC", description: "Use source changes in integration.yml", pr_number: 23278, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 4, deletions_count: 3},
		{sha: "7f6c41ec69d0f256941be6842a5639e7254850cb", date: "2025-06-27 17:51:13 UTC", description: "require changes to run before downloading artifacts", pr_number: 23281, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "282dfe0d248e12e9e6b7616bdedecf9a9a43d5fa", date: "2025-06-27 18:01:23 UTC", description: "require changes to run before downloading artifacts", pr_number: 23283, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "0557e86edfa4b2deed43a8fe06b11e788e37afeb", date: "2025-06-27 21:57:01 UTC", description: "use YAML for the default config language", pr_number: 23285, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9bee1411c0d23c7a4f4571287230382aaf0c9c36", date: "2025-06-30 16:41:07 UTC", description: "add workflow_dispatch to msrv", pr_number: 23294, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "326caa5c33129f6280885f10a84073a7b39e554a", date: "2025-06-30 17:32:32 UTC", description: "add ability to specify checkout ref in msrv.yml", pr_number: 23295, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "0fef6b2f86eeb6e94c01b1f9af725605b2eee496", date: "2025-06-30 18:07:17 UTC", description: "bump cargo-msrv version", pr_number: 23296, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6ad09472e986833b7a5b48776767cfe916d03726", date: "2025-06-30 23:19:33 UTC", description: "Bump nom from 7.1.3 to 8.0.0", pr_number: 22302, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 46, deletions_count: 26},
		{sha: "a16f8301112e2292bf74f63193c8fa38b1171f58", date: "2025-07-01 02:41:18 UTC", description: "Bump bollard from 0.16.1 to 0.19.1", pr_number: 20958, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 163, deletions_count: 129},
		{sha: "833eece934379c86c0129e84dec93497faadb669", date: "2025-07-01 00:38:54 UTC", description: "improve minor release documentation", pr_number: 23298, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 14, deletions_count: 10},
		{sha: "dd205d3593e65f1e2c4613414a2b2d59ad74018a", date: "2025-07-01 04:42:53 UTC", description: "Bump governor from 0.7.0 to 0.8.1", pr_number: 22555, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 9, deletions_count: 12},
		{sha: "bf5d9d05f09b72579ae27a2204f07d1495859e3e", date: "2025-07-01 13:07:09 UTC", description: "use connect_with_local_defaults on all platforms", pr_number: 21088, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bing Wang", files_count: 2, insertions_count: 3, deletions_count: 13},
		{sha: "ed93c1deeed38fc4ef3323cf79b9423bd8d743e1", date: "2025-07-01 01:35:28 UTC", description: "prep next version and cherry pick release", pr_number: 23300, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 46, insertions_count: 328, deletions_count: 99},
		{sha: "3281b07f4f4516d408ab7c300bdc7c3cf563702f", date: "2025-07-01 17:46:28 UTC", description: "Bump proptest from 1.6.0 to 1.7.0", pr_number: 23312, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 12, deletions_count: 12},
		{sha: "b8edba0f58dda2c73a7351c2da7a12e9ef2aa2cd", date: "2025-07-01 17:54:14 UTC", description: "Bump lru from 0.14.0 to 0.15.0", pr_number: 23303, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "47cf6acb68ca347e1d48f14456087620bac5c108", date: "2025-07-01 18:05:06 UTC", description: "Bump mock_instant from 0.5.3 to 0.6.0", pr_number: 23301, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ffacb27885f5dab2e6ff3a024563d4b3f813b4b8", date: "2025-07-01 22:11:57 UTC", description: "Bump the patches group with 14 updates", pr_number: 23305, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 219, deletions_count: 129},
		{sha: "4f7195e554509bbeead4a311f4913fb1b56e9ca7", date: "2025-07-01 22:13:48 UTC", description: "Bump security-framework from 2.10.0 to 3.2.0", pr_number: 23314, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "c59788eb3cab9e92f53acb81a2ae778145032ae4", date: "2025-07-01 22:14:21 UTC", description: "Bump the aws group with 4 updates", pr_number: 23306, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 16},
		{sha: "03db0287b9452465138b89f2ddee5aacbe530cb7", date: "2025-07-01 22:20:36 UTC", description: "Bump roaring from 0.10.12 to 0.11.0", pr_number: 23311, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f491a3a5766bce28a21d00c60cb1eb1b77a98600", date: "2025-07-01 22:21:35 UTC", description: "Bump indexmap from 2.9.0 to 2.10.0", pr_number: 23308, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 29, deletions_count: 29},
		{sha: "8d4c0918740678fbe3f635c2a2be4be3effc6876", date: "2025-07-01 18:32:09 UTC", description: "install datadog-ci correctly", pr_number: 23318, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "68f7ebc31a3084ddda4c0247022a8f8cfc995801", date: "2025-07-01 20:28:54 UTC", description: "navigation overhaul - part 1", pr_number: 23320, scopes: ["website"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 34, insertions_count: 81, deletions_count: 132},
		{sha: "86b3235e33c3448f356312a7a04af4ed69e6fd74", date: "2025-07-01 20:56:01 UTC", description: "fix broken links due to // in vrl_expressions", pr_number: 23321, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "39b6277a5c2ece171a7b810b3bdbd57763a68bdb", date: "2025-07-02 01:51:52 UTC", description: "Bump docker/setup-buildx-action from 3.10.0 to 3.11.1", pr_number: 23322, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "d8f74708a2703c7f31487482a395d2f4021887ff", date: "2025-07-01 23:43:29 UTC", description: "Bump syslog_loose from 0.21.0 to 0.22.0", pr_number: 23315, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 3},
		{sha: "dff8802ddc8ea30b2929ce3bc017d7486aa12a9d", date: "2025-07-02 05:02:17 UTC", description: "Bump redis from 0.24.0 to 0.32.3", pr_number: 23307, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 22, deletions_count: 25},
		{sha: "90ba7c4efe2c1d3540ae99c3db1ec32e2e6bce07", date: "2025-07-02 01:23:08 UTC", description: "navigation overhaul part 2", pr_number: 23324, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 153, insertions_count: 346, deletions_count: 375},
		{sha: "a9104b89b6b120160f4321562a7529acc5b50f94", date: "2025-07-02 20:56:49 UTC", description: "updated multiple broken links", pr_number: 23328, scopes: ["website"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 29, insertions_count: 87, deletions_count: 157},
		{sha: "a8f54ebbb00eb7c8b8630bace36f8fe4b9f0b014", date: "2025-07-02 22:29:27 UTC", description: "remove leftover dbg statement", pr_number: 23331, scopes: ["website"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "7847e9253cbdb66550255c7fa2a274128f8b2835", date: "2025-07-03 05:16:56 UTC", description: "fix vector exiting if nats sink url fails dns resolution or is unavailable without --require-healthy", pr_number: 23287, scopes: ["nats sink"], type: "fix", breaking_change: false, author: "rdwr-tomers", files_count: 3, insertions_count: 32, deletions_count: 7},
		{sha: "c4ace5b5147b328bbf2b81984ffaf25090362d88", date: "2025-07-03 04:59:08 UTC", description: "run a separate task for utilization metric to ensure it is regularly updated", pr_number: 22070, scopes: ["metrics"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 4, insertions_count: 213, deletions_count: 72},
		{sha: "2fa1b6bf207539b931d370bbaa752ac2c79a1899", date: "2025-07-03 00:05:51 UTC", description: "Bump criterion from 0.5.1 to 0.6.0", pr_number: 23133, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 13, deletions_count: 18},
		{sha: "65d6a86a48e36bb19e8b014b888a979687dd0b27", date: "2025-07-03 20:00:25 UTC", description: "remove broken metrics table rendering", pr_number: 23335, scopes: ["website"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 41},
		{sha: "21bb9f7ed92c98649c89835669a8628bdc51f048", date: "2025-07-03 21:23:51 UTC", description: "integration remove useless setup job", pr_number: 23337, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 0, deletions_count: 15},
		{sha: "1c4472a07e479976eb061d1fbda2f44e184ec8df", date: "2025-07-04 05:09:54 UTC", description: "Bump databend-client from 0.22.2 to 0.27.1", pr_number: 22301, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 71, deletions_count: 31},
		{sha: "a2c6c442d8254b11d013bf4d2a88f84e5b08d0a6", date: "2025-07-07 19:01:11 UTC", description: "add TTL-based cache for metric sets and add expire_metrics_secs for Prometheus remote write sink", pr_number: 23286, scopes: ["metrics"], type: "feat", breaking_change: false, author: "Derek Zhang", files_count: 7, insertions_count: 242, deletions_count: 45},
		{sha: "33e44aeaff314a2bf16db8c89f8bb81a92c2d97e", date: "2025-07-08 03:12:39 UTC", description: "Bump serde_with from 3.12.0 to 3.14.0", pr_number: 23313, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 64, deletions_count: 16},
		{sha: "eb4cb46ccd4a24f06cdb2c969efe87ba734f053d", date: "2025-07-08 21:39:51 UTC", description: "Bump libz-sys for macos compatibility", pr_number: 23347, scopes: ["dev"], type: "chore", breaking_change: false, author: "bas smit", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "fc8a51de7b5c0f29a06cf6b07f1828c5ce2882aa", date: "2025-07-09 00:00:38 UTC", description: "potential hang in ConcurrentMap", pr_number: 23340, scopes: ["core"], type: "fix", breaking_change: false, author: "Jorge Hermo", files_count: 3, insertions_count: 28, deletions_count: 2},
		{sha: "a3f648f5351877102d3eb0645492de0a381e000e", date: "2025-07-08 23:15:41 UTC", description: "Bump goauth from 0.14.0 to 0.16.0", pr_number: 20688, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 26, deletions_count: 6},
		{sha: "31756e041a0652de6e7fbf06369cc502132f91a0", date: "2025-07-10 05:34:46 UTC", description: "update file websocket_server.cue", pr_number: 23348, scopes: ["external"], type: "docs", breaking_change: false, author: "Erlang Parasu", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1102f897dfea8cc14255228add5daf54191c0da2", date: "2025-07-09 18:10:54 UTC", description: "add optional wildcard search parameter", pr_number: 23074, scopes: ["enriching"], type: "enhancement", breaking_change: false, author: "Nick Wang", files_count: 14, insertions_count: 384, deletions_count: 45},
		{sha: "ca0b95121137a35466d472c741c0a897e6061e7c", date: "2025-07-10 19:47:23 UTC", description: "Rename base to generated", pr_number: 23353, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 256, insertions_count: 280, deletions_count: 280},
		{sha: "18f3055d89041bfd2ac7e4303f41a9b37cce0938", date: "2025-07-10 21:03:28 UTC", description: "Updated `replace` capture group example ", pr_number: 18785, scopes: ["external"], type: "docs", breaking_change: false, author: "pezkins", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "a9366ad55888c85258d0be206710e44a3de389df", date: "2025-07-11 18:40:13 UTC", description: "re-enable splunk ITs", pr_number: 23360, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "f83b7e0ba32167ded8d007461298a9aea11ee7d8", date: "2025-07-12 10:04:34 UTC", description: "add query_settings to clickhouse sink", pr_number: 22764, scopes: ["clickhouse sink"], type: "feat", breaking_change: false, author: "Pomin Wu", files_count: 7, insertions_count: 211, deletions_count: 12},
		{sha: "8d95e2318fd95f57b2ee76069a7a7bfea961bc99", date: "2025-07-14 17:59:48 UTC", description: "unreadable interval_ms", pr_number: 23361, scopes: ["core"], type: "fix", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 245, deletions_count: 116},
		{sha: "b63361ae2c908d1d2547d37da69d010cbf728112", date: "2025-07-14 19:45:42 UTC", description: "Ensure that batches using newline delimited framing end in a newline", pr_number: 21097, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 32, deletions_count: 7},
		{sha: "25353d2976285b29127730f27b76d01e0c537a50", date: "2025-07-15 04:22:36 UTC", description: "Bump brace-expansion from 1.1.11 to 1.1.12 in /website in the npm_and_yarn group", pr_number: 23372, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "d86dc3bd6186a283b6c10e1d242695ca5c96009a", date: "2025-07-15 01:53:51 UTC", description: "remove token permissions for changelog workflow", pr_number: 23376, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "c5a98b2497cae632df1e51a7048af039f4802a5d", date: "2025-07-15 07:36:15 UTC", description: "templateable uri", pr_number: 23288, scopes: ["http sink"], type: "feat", breaking_change: false, author: "Jorge Hermo", files_count: 12, insertions_count: 419, deletions_count: 49},
		{sha: "8c199ace3384acb71bc2337f5057976c6827d418", date: "2025-07-14 22:54:38 UTC", description: "Add ability to configure request errors to retry ", pr_number: 23206, scopes: ["aws_s3 sink"], type: "enhancement", breaking_change: false, author: "Joe Chapman", files_count: 6, insertions_count: 102, deletions_count: 11},
		{sha: "3ab75690b3b1e9ae0a58a61ac917a1aa6c78fcc6", date: "2025-07-15 17:52:29 UTC", description: "remove EOL Debian Buster 10", pr_number: 23380, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "5ff037bfb95ea62d1faa1ef6267be8ba4a2c6080", date: "2025-07-15 20:46:14 UTC", description: "rename name for publish-github job", pr_number: 23381, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "121b9da9e45e372b9f205cec7830e5a054cf0a21", date: "2025-07-15 20:49:36 UTC", description: "Add support for `max_bytes` for memory buffers", pr_number: 23330, scopes: ["sinks"], type: "enhancement", breaking_change: false, author: "Rob Blafford", files_count: 16, insertions_count: 409, deletions_count: 147},
		{sha: "00c554f87ecfaa225a9523789648ed2362070623", date: "2025-07-15 21:08:46 UTC", description: "`metrics-tag-values: full` made tags unmodifiable", pr_number: 23371, scopes: ["core"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 47, deletions_count: 13},
		{sha: "b2c6c79864b06e81c2ac73657e1a31a0c2b8d991", date: "2025-07-16 01:51:15 UTC", description: "Bump sysinfo from 0.34.2 to 0.35.1", pr_number: 23138, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 81, deletions_count: 24},
		{sha: "600f3457100bc097c68edaf6882be97c609f85ac", date: "2025-07-16 00:48:33 UTC", description: "checkout changelog script from master", pr_number: 23382, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 12, deletions_count: 2},
		{sha: "efc1c5d6da5f09bd7e2ea8f140b1a753b731794e", date: "2025-07-16 17:29:51 UTC", description: "add MIT-0 to allowed licenses", pr_number: 23386, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "e7de566922b6dc12c946829667379168fe49c0ba", date: "2025-07-17 04:05:43 UTC", description: "opensearch credentials provided in uri not used", pr_number: 23367, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Yao Noel Achi", files_count: 5, insertions_count: 185, deletions_count: 39},
		{sha: "29b323670bcc6ab1baec11cb865b927816aa954e", date: "2025-07-17 18:33:16 UTC", description: "use Debian-native bind package", pr_number: 23389, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 6},
		{sha: "e3ee69876173073f30c3a7a9634dab6e960e20de", date: "2025-07-18 08:36:19 UTC", description: "add timezone option, add performance timing into VRL Playground Website", pr_number: 23343, scopes: ["website"], type: "feat", breaking_change: false, author: "Forking Frenzy", files_count: 6, insertions_count: 137, deletions_count: 17},
		{sha: "87925298c54984812cc63799f1ccde7c74497865", date: "2025-07-17 18:18:38 UTC", description: "Allow strftime in UnsignedIntTemplate", pr_number: 23387, scopes: ["templating"], type: "enhancement", breaking_change: false, author: "Jake Halaska", files_count: 2, insertions_count: 55, deletions_count: 3},
		{sha: "74173af63a84695e29d3c3c3ad840c436f5c7832", date: "2025-07-17 23:23:21 UTC", description: "bump Rust toolchain to 1.88", pr_number: 23388, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 292, insertions_count: 917, deletions_count: 1169},
		{sha: "c011a59dae9eac97dcab103f3824c8b0e1a3c730", date: "2025-07-18 18:24:49 UTC", description: "split up test.yml", pr_number: 23383, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 160, deletions_count: 86},
		{sha: "2d4bc0b7d40f5621f99617dcb047e190650526ae", date: "2025-07-18 18:58:40 UTC", description: "pass in GITHUB_TOKEN to bypass rate limits", pr_number: 23396, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "6a6ffa591320bd6c64e6b8b1ec4a42f4ebc17fd1", date: "2025-07-18 22:32:31 UTC", description: "switch to 2024 edition", pr_number: 23395, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 60, insertions_count: 328, deletions_count: 275},
		{sha: "abe333808352eeb71b2e8e2dfe54757f3ff50a78", date: "2025-07-18 22:34:19 UTC", description: "merge test/test-component-validation and upload results", pr_number: 23397, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 11, deletions_count: 14},
		{sha: "af3ec0cfb7106b8f8615d27bfe3eb37f8d2471a0", date: "2025-07-18 23:49:33 UTC", description: "remove unused AWS env vars", pr_number: 23401, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 0, deletions_count: 8},
		{sha: "659a3c3b8afaf3543b6603a9b32db8e8da0666c2", date: "2025-07-19 00:00:52 UTC", description: "allow unused var ", pr_number: 23400, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1d699ee10269b3e35a3038cab12e98e80d3a4eb3", date: "2025-07-19 00:20:32 UTC", description: "only install datadog-ci if int test runs", pr_number: 23402, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "3abcebd28641a14a664756a3efd36f728b8e6ecd", date: "2025-07-19 00:25:01 UTC", description: "Compare correct decoded byte size in Validator tests", pr_number: 23399, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Rob Blafford", files_count: 1, insertions_count: 6, deletions_count: 3},
		{sha: "a1a75400919eb308b4895951df22c052468ea57b", date: "2025-07-19 00:42:28 UTC", description: "add workflow badges", pr_number: 23398, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 8, deletions_count: 3},
		{sha: "d654fddd33d0988d3a7272a2257e101933783f5a", date: "2025-07-19 15:12:15 UTC", description: "add documentation for validate_json_schema() function support in VRL", pr_number: 23359, scopes: ["external"], type: "docs", breaking_change: false, author: "jlambatl", files_count: 2, insertions_count: 80, deletions_count: 0},
		{sha: "144a0af4e05457e3d4434e934b763d5e09db156c", date: "2025-07-21 18:47:04 UTC", description: "add docs for `haversine` VRL function", pr_number: 23336, scopes: ["external"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 4, insertions_count: 79, deletions_count: 7},
		{sha: "dbeae5d0aa338e9c2ada94b79637663eda5cc571", date: "2025-07-21 17:54:15 UTC", description: "match only when feature is enabled", pr_number: 23404, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "91794109acade46e5482ca7dd2fc962cc88be8e9", date: "2025-07-22 02:53:27 UTC", description: "support custom partitioners for gcs and azure sinks", pr_number: 23403, scopes: ["azure_blob sink"], type: "chore", breaking_change: false, author: "Vladimir Zhuk", files_count: 2, insertions_count: 26, deletions_count: 12},
		{sha: "3778c089726c4299c292fa10ea25fd30d99e8ce1", date: "2025-07-22 17:21:35 UTC", description: "refactor changelog file fetching", pr_number: 23405, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 31, deletions_count: 25},
		{sha: "4d2472f5b1f5160c3ac9a3b5234b55030adeda99", date: "2025-07-22 19:34:41 UTC", description: "pin macos runner image version", pr_number: 23411, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 7, deletions_count: 5},
		{sha: "ea613700f650bc374d0d15a0c5961e46597e46f8", date: "2025-07-22 22:07:40 UTC", description: "add changelog fragments upper limit per PR", pr_number: 23413, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "d2a439936691c71f07d444f5ce8f80e5d46322bb", date: "2025-07-23 00:30:50 UTC", description: "clippy fixes", pr_number: 23414, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 15, insertions_count: 167, deletions_count: 162},
		{sha: "3068480f39688dac5b08273c16140b87b20badda", date: "2025-07-23 21:42:55 UTC", description: "Bump form-data from 4.0.0 to 4.0.4 in /website in the npm_and_yarn group", pr_number: 23408, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 105, deletions_count: 12},
		{sha: "fd7c30231a3ff807a49e93a496741cf006943579", date: "2025-07-23 20:27:35 UTC", description: "pin actions versions", pr_number: 23417, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 34, insertions_count: 261, deletions_count: 260},
		{sha: "f372816f0f8207e251352b512f65268684c2097c", date: "2025-07-24 18:28:23 UTC", description: "remove unnecessary traits", pr_number: 23418, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 10, insertions_count: 135, deletions_count: 106},
		{sha: "01352255c2f3641c9c492b5b9a9dbd5a822d9a51", date: "2025-07-24 18:49:14 UTC", description: "enhance prepare.sh script", pr_number: 23415, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 15, insertions_count: 171, deletions_count: 66},
		{sha: "774060bcccc11e950048652b463f119789feb4ad", date: "2025-07-24 20:27:04 UTC", description: "reduce arbitrary sleep for IT script", pr_number: 23420, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "379ee5851121c476974d955b57c8cefc7e838624", date: "2025-07-24 22:40:04 UTC", description: "allow running E2E suite manually", pr_number: 23423, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 9, deletions_count: 2},
		{sha: "614e1957d121566eb69edba25473a858afe8bb4e", date: "2025-07-25 00:06:47 UTC", description: "introduce OTEL E2E test", pr_number: 23406, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 18, insertions_count: 633, deletions_count: 4},
		{sha: "b695d2291fab68fd4619d180f06d7790ef3beb88", date: "2025-07-25 00:34:48 UTC", description: "allow running E2E suite manually", pr_number: 23425, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "e4cecb5d81544ecc3fc26734a5e02bd54092f734", date: "2025-07-25 00:40:05 UTC", description: "wait 1s before panicking in test", pr_number: 23424, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 12, deletions_count: 3},
		{sha: "49fae0916f85cd871e59c423bdc72e3eb5c76dec", date: "2025-07-25 17:03:44 UTC", description: "mark as stable", pr_number: 23431, scopes: ["remap transform"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9b3fb7b7597c34b86ace31e69fd415301258fc66", date: "2025-07-25 18:28:19 UTC", description: "switch macos to xlarge runner", pr_number: 23433, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7cf4b0fba0ee2b09e90d5c6066e2058471e6bd42", date: "2025-07-25 20:52:42 UTC", description: "scripts/int-e2e-test.sh debugging improvements", pr_number: 23434, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 29, deletions_count: 17},
		{sha: "af98d9f5afe9fc1d1906279d441cd802bfa85b64", date: "2025-07-25 21:53:05 UTC", description: "e2e otel fixes", pr_number: 23435, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 16},
		{sha: "c0c060e6bbdef860bc348122b0729278beb7fb96", date: "2025-07-25 23:37:58 UTC", description: "use cargo binstall in prepare.sh", pr_number: 23436, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 106, deletions_count: 9},
		{sha: "878708f8cd32dd4724890d51d4b9b27fe6188f55", date: "2025-07-25 23:46:37 UTC", description: "half # of tests ran by quickcheck", pr_number: 23439, scopes: ["file source"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "f136fb48edc1f8b2880a420a4f1aa8d8b457003d", date: "2025-07-28 17:38:25 UTC", description: "skip e2e otel until it's fixed", pr_number: 23445, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 7, deletions_count: 6},
		{sha: "041a3b1727137e80897cac2e2738d0ad4a8f5c8f", date: "2025-07-28 17:39:06 UTC", description: "fix trigger for DD logs E2E", pr_number: 23446, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0cf042e307c12ad297f73665b08a826dc4e61023", date: "2025-07-28 17:56:45 UTC", description: "remove orphan containers", pr_number: 23444, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "d24b04686ff2b5454b19aaf0b85c850227e3f50f", date: "2025-07-28 18:36:16 UTC", description: "remove arbitrary sleep", pr_number: 23443, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 3},
		{sha: "a0179dc3bb94f0279dc507db11801ec0765787a9", date: "2025-07-28 21:19:58 UTC", description: "workaround vdev issues by using project-name for debug logs", pr_number: 23454, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 11},
		{sha: "42b8c7966e7a8a7524aa5655c09437d3ace4acac", date: "2025-07-28 22:09:40 UTC", description: "fix vdev stop not cleaning up anything", pr_number: 23450, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "0fd22b3fe5e4974073cc81dd15c3107cd158ad4a", date: "2025-07-28 23:20:29 UTC", description: "ignore flaky diskv2 test", pr_number: 23455, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "affbcc32f75b81e2e6dc7db2e200db254b7f8184", date: "2025-07-29 17:16:06 UTC", description: "fix and renable otel logs e2e", pr_number: 23459, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 62, deletions_count: 57},
		{sha: "e326e652ec5a3f7304fc2d5eba73b782b6d5ce58", date: "2025-07-29 17:14:48 UTC", description: "Emit log when putting an object to s3-compatible storage", pr_number: 23390, scopes: ["aws_s3 sink"], type: "enhancement", breaking_change: false, author: "Jake Halaska", files_count: 2, insertions_count: 25, deletions_count: 5},
		{sha: "b77915cfb57ad39689f64773d8f09f968b8486d2", date: "2025-07-30 08:38:03 UTC", description: "vrl encode/decode_lz4 documentation update sibling PR", pr_number: 23378, scopes: ["external"], type: "docs", breaking_change: false, author: "jlambatl", files_count: 3, insertions_count: 43, deletions_count: 7},
		{sha: "182a309445b083b223291d64fe4f1c48507f8fb0", date: "2025-07-30 10:57:21 UTC", description: "improve readability", pr_number: 23460, scopes: ["external"], type: "docs", breaking_change: false, author: "Benjamin Dornel", files_count: 76, insertions_count: 293, deletions_count: 293},
		{sha: "7e488ffaaf3bc68855581fdb8dd71dc6f2fbe92b", date: "2025-07-29 22:59:13 UTC", description: "add Redis Sentinel support for redis sink", pr_number: 23355, scopes: ["redis sink"], type: "feat", breaking_change: false, author: "Jake Halaska", files_count: 13, insertions_count: 719, deletions_count: 45},
		{sha: "f4c1ac570badfeb00564ebd02146b86a6f5bb370", date: "2025-07-30 22:09:02 UTC", description: "bump indicatif to 0.18.0", pr_number: 23470, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e6bd05955d4418e4beda13e001dd0612de7b42f7", date: "2025-07-30 22:24:55 UTC", description: "improve Vector support page", pr_number: 23471, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 23, deletions_count: 3},
		{sha: "2ae37a7ee4e8ee9da2d16e097ca60ecc049c48d6", date: "2025-07-30 23:45:15 UTC", description: "disable splunk ITs", pr_number: 23473, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "48caeaebc78a3d56eff509141d4a87d9fa5a3759", date: "2025-07-31 00:30:25 UTC", description: "remove unused scripts", pr_number: 23462, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 7, insertions_count: 0, deletions_count: 555},
		{sha: "cc78555a413f2cc6506d56b10c89686545aabae5", date: "2025-08-01 00:46:00 UTC", description: "properly disable postgres sink's healthcheck", pr_number: 23441, scopes: ["postgres sink"], type: "fix", breaking_change: false, author: "Jorge Hermo", files_count: 6, insertions_count: 43, deletions_count: 14},
		{sha: "735b0ee4702067f7d08664cadad72278743a2bc6", date: "2025-08-01 00:50:07 UTC", description: "Update to_unix_timestamp function fallibility status", pr_number: 23466, scopes: ["external"], type: "docs", breaking_change: false, author: "Antoine Sauzeau", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "aad876a4bb4828c5de2ff9ff99df59399f4127e5", date: "2025-07-31 19:04:30 UTC", description: "bump toml to v9", pr_number: 23475, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 10, insertions_count: 93, deletions_count: 34},
		{sha: "862524746cf3e2d7049aaf812f60f6845c97e67b", date: "2025-07-31 16:11:22 UTC", description: "support for template headers", pr_number: 23422, scopes: ["http sink"], type: "enhancement", breaking_change: false, author: "Taylor Chaparro", files_count: 13, insertions_count: 280, deletions_count: 50},
		{sha: "c7eaf6529079fb1070f913aaf2a3d5145c5f4001", date: "2025-07-31 19:45:16 UTC", description: "correctly set HOSTNAME in docker environment", pr_number: 23452, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 17, deletions_count: 3},
		{sha: "f64089a198291de31f7c15affe423737600a6dc3", date: "2025-08-01 04:41:52 UTC", description: "Prevent negative buffer size and event gauges", pr_number: 23453, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Vitalii Parfonov", files_count: 6, insertions_count: 259, deletions_count: 16},
		{sha: "af893011aea9fa39817deaf5c26b1c7bab890d1b", date: "2025-08-02 02:04:00 UTC", description: "allowed symbols in secrets #23220", pr_number: 23465, scopes: ["config"], type: "fix", breaking_change: false, author: "Danila Matveev", files_count: 2, insertions_count: 7, deletions_count: 2},
		{sha: "46689f41145826611b82ad14b50ccc3fa2bfcc83", date: "2025-08-01 18:29:10 UTC", description: "Bump crc32fast from 1.4.2 to 1.5.0", pr_number: 23496, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8e33bb236b3fe8492b7a8cbe621db57e229bdba8", date: "2025-08-01 18:29:13 UTC", description: "update deps and add some workspace deps", pr_number: 23476, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 9, deletions_count: 7},
		{sha: "64e26937616e5f71321a97d1e61beb297a96ac97", date: "2025-08-01 22:32:56 UTC", description: "Bump proptest-derive from 0.5.1 to 0.6.0", pr_number: 23494, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 15},
		{sha: "81e238e0fc5dab87af618574bc363fcb5fb6a7cf", date: "2025-08-01 22:35:42 UTC", description: "Bump lru from 0.15.0 to 0.16.0", pr_number: 23495, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "10795e9d7fd080cd49976eecb8654a69a2d0c05a", date: "2025-08-01 22:39:21 UTC", description: "Bump rstest from 0.25.0 to 0.26.1", pr_number: 23492, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 7},
		{sha: "9bd70fddcba34e4f8f65c961a17a179349fa0c64", date: "2025-08-01 23:50:04 UTC", description: "Bump opendal from 0.53.3 to 0.54.0", pr_number: 23490, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 31, deletions_count: 8},
		{sha: "6a17a5423d886d98e96d8a4bc24bbfb03a6785d3", date: "2025-08-02 07:55:48 UTC", description: "add initial `websocket` source", pr_number: 23449, scopes: ["new source"], type: "feat", breaking_change: false, author: "Benjamin Dornel", files_count: 27, insertions_count: 2578, deletions_count: 408},
		{sha: "963219f6449ae6504bcd053cbf16a87847df1f32", date: "2025-08-02 00:02:25 UTC", description: "Bump criterion from 0.6.0 to 0.7.0", pr_number: 23493, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 11, deletions_count: 11},
		{sha: "c4db7972cdfcc94dc8bba13b3ccae34de890d8c8", date: "2025-08-01 21:13:32 UTC", description: "Bump databend-client from 0.27.1 to 0.28.0", pr_number: 23491, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 5},
		{sha: "d1ecc0499c6a89aeed25cbdefffe7524ee7abc4f", date: "2025-08-02 01:19:31 UTC", description: "Bump github/codeql-action from 3.29.4 to 3.29.5", pr_number: 23505, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "81b05d1869cfb7f6a8b97fbc41d01a69b2e28edd", date: "2025-08-02 09:20:44 UTC", description: "treat websocket source tests as unit", pr_number: 23503, scopes: ["websocket source"], type: "fix", breaking_change: false, author: "Benjamin Dornel", files_count: 3, insertions_count: 2, deletions_count: 4},
		{sha: "9f95c21f5644f6a109c0439bce90835824623f84", date: "2025-08-02 01:38:01 UTC", description: "Bump docker/metadata-action from 5.7.0 to 5.8.0", pr_number: 23506, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2003d617c7befdc7aa7f73a37263d272cd4a3718", date: "2025-08-02 01:38:44 UTC", description: "Bump notify from 8.0.0 to 8.1.0", pr_number: 23489, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 29},
		{sha: "ab36caa0f85f4d22358f0a31621b341311eb61d6", date: "2025-08-04 17:41:51 UTC", description: "Bump the patches group with 20 updates", pr_number: 23484, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 110, deletions_count: 125},
		{sha: "51f39d67e56e5d998172b3d5749e23857de01ee7", date: "2025-08-05 09:21:48 UTC", description: "Fix instances of removed to_timestamp in documentation", pr_number: 23512, scopes: ["external"], type: "docs", breaking_change: false, author: "岡崎", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1a07ec4ea801ee778b128abf4fd8410052ca6ac7", date: "2025-08-05 19:26:34 UTC", description: "fix config parsing", pr_number: 23526, scopes: ["log_to_metric transform"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 67, deletions_count: 83},
		{sha: "71b378aedb993f78d5dc6febbaffb1c37c1f88fd", date: "2025-08-05 20:20:37 UTC", description: "rename 'common' to 'minimal'", pr_number: 23527, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 22, deletions_count: 22},
		{sha: "06727eb0f1dd1fb3b4c6229ea5017a7afdc376a0", date: "2025-08-05 20:07:39 UTC", description: "add Redis ZADD support for redis sink", pr_number: 23464, scopes: ["redis sink"], type: "feat", breaking_change: false, author: "Jake Halaska", files_count: 10, insertions_count: 302, deletions_count: 45},
		{sha: "1be6a9b6650a505fc36e119dd60f7a97678ec6d0", date: "2025-08-06 04:54:08 UTC", description: "add time-based settings to dedupe transform", pr_number: 23480, scopes: ["dedupe transform"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 8, insertions_count: 337, deletions_count: 112},
		{sha: "a1aeae826a4386ced63bcd6ec190cc7b597b0fe1", date: "2025-08-06 23:20:50 UTC", description: "buffer_id handling in buffer usage metrics reporting", pr_number: 23507, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Vitalii Parfonov", files_count: 2, insertions_count: 131, deletions_count: 50},
		{sha: "7d81cb69c03f284022c1e035bfce44b429b1c672", date: "2025-08-07 06:56:09 UTC", description: "add datadog_common sink to semantic.yml", pr_number: 23533, scopes: ["ci"], type: "chore", breaking_change: false, author: "Kurochan", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "1d646dad6019ff161c885a6e174eea1e8f1c35dd", date: "2025-08-06 17:36:41 UTC", description: "update test config", pr_number: 23529, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "e7a3126b3d427a6627deb7eb87a9e76b34724c30", date: "2025-08-07 00:11:16 UTC", description: "increase request limit for TCP dnstap source", pr_number: 23448, scopes: ["dnstap source"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 7, deletions_count: 2},
		{sha: "2485565b3773e3b6451ef6c47727934bd8f30201", date: "2025-08-07 07:19:38 UTC", description: "prevent panic on non-UTF8 environment variables", pr_number: 23513, scopes: ["config"], type: "fix", breaking_change: false, author: "Kurochan", files_count: 2, insertions_count: 10, deletions_count: 1},
		{sha: "72ea46b0dfeb0e89f0a59ac0d6de45051ac3d93f", date: "2025-08-07 07:21:07 UTC", description: "prevent panic on invalid api key", pr_number: 23514, scopes: ["datadog_common sink"], type: "fix", breaking_change: false, author: "Kurochan", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "7c30315b287c29aaffabb286ab4df77d17b24425", date: "2025-08-07 03:07:30 UTC", description: "change type of `max_frame_handling_tasks` to `usize`", pr_number: 23537, scopes: ["dnstap source"], type: "chore", breaking_change: false, author: "Ensar Sarajčić", files_count: 4, insertions_count: 24, deletions_count: 24},
		{sha: "cce428651c7be6c11e2be2392de125c7b8b5f86f", date: "2025-08-06 23:22:06 UTC", description: "Bump tmp from 0.2.1 to 0.2.4 in /website in the npm_and_yarn group", pr_number: 23539, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 13},
		{sha: "3b8b744ccecf78e2ed184a14565e1d27e09858c8", date: "2025-08-06 23:53:34 UTC", description: "remove invalid URL and add info section", pr_number: 23541, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "0a485f5de0caf86909706e1e581cd74301e81f84", date: "2025-08-07 00:01:19 UTC", description: "preview_site_trigger.yml skip instead of failing if the website name is invalid", pr_number: 23540, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 24, deletions_count: 42},
		{sha: "4c9ed1ea7cfcfb688b243a9448db6c7d1ac38ba1", date: "2025-08-07 00:05:08 UTC", description: "emit PairExpansion error", pr_number: 23538, scopes: ["log_to_metric transform"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 80, deletions_count: 6},
		{sha: "e8e19ccbc293d0e189282f5341a7d75e61940bac", date: "2025-08-07 07:05:47 UTC", description: "allow setting max packet size for mqtt source and sink", pr_number: 23515, scopes: ["config"], type: "feat", breaking_change: false, author: "simplepad", files_count: 6, insertions_count: 24, deletions_count: 0},
		{sha: "a9c4c1666012b082cbb83ffd86a04f2a6ee79634", date: "2025-08-07 12:24:08 UTC", description: "add support for JetStream message headers", pr_number: 23510, scopes: ["nats sink"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 10, insertions_count: 401, deletions_count: 238},
		{sha: "d9bef7e49987313afa064b52dc55762e98d1c7d7", date: "2025-08-08 05:27:00 UTC", description: "fix partial retry logic", pr_number: 22431, scopes: ["elasticsearch sink"], type: "feat", breaking_change: false, author: "Serendo", files_count: 6, insertions_count: 131, deletions_count: 26},
		{sha: "ab519c5bb17177b9d4b7f44b283727cfad2ae7f5", date: "2025-08-07 23:30:58 UTC", description: "watch enrichment tables with `--watch-config` CLI option", pr_number: 23442, scopes: ["cli"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 10, insertions_count: 161, deletions_count: 33},
		{sha: "317f151ea649199ec0aa1c78f9fabc90191d3d30", date: "2025-08-07 18:37:13 UTC", description: "fail when jobs are cancelled", pr_number: 23545, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 28, deletions_count: 34},
		{sha: "7cf8457a0a6eafaef5c41a46a4da0a8728a6371b", date: "2025-08-07 19:04:44 UTC", description: "fix typo", pr_number: 23549, scopes: ["internal"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f9d2800d873d2e5a15b1920ad49c7e357702a890", date: "2025-08-07 19:31:19 UTC", description: "allow all telemetry types", pr_number: 23550, scopes: ["codecs protobuf"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 6, deletions_count: 2},
		{sha: "92b19362265a6140d9f816e556171be80eeed2d5", date: "2025-08-07 20:51:05 UTC", description: "cargo update -p vrl", pr_number: 23553, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 26, deletions_count: 13},
		{sha: "a1c1fc3493f33093e844b0fda9569e10acdfef13", date: "2025-08-07 20:54:46 UTC", description: "improve build.rs", pr_number: 23551, scopes: ["opentelemetry lib"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 24, deletions_count: 15},
		{sha: "0b3c886d6c64b461ce2d56e6e9405b2d7f5ffc37", date: "2025-08-07 22:27:01 UTC", description: "remove downcast and trait in sink retry code", pr_number: 23543, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Thomas", files_count: 36, insertions_count: 183, deletions_count: 136},
		{sha: "3f8cc10bde1baa631332e67b457cc9719365b24d", date: "2025-08-08 21:12:13 UTC", description: "re-add deleted utils", pr_number: 23562, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 543, deletions_count: 0},
	]
}
