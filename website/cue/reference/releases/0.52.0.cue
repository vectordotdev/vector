package metadata

releases: "0.52.0": {
	date:     "2025-12-16"
	codename: ""

	whats_next: []

	description: """
		The Vector team is excited to announce version `0.52.0`!

		## Release highlights

		- Enhanced Vector's observability with new buffer utilization metrics for sources and
		  transforms
		  ([source_buffer_*](\(urls.vector_internal_metrics)/#source_buffer_max_byte_size)
		  and [transform_buffer_*](\(urls.vector_internal_metrics)/#transform_buffer_max_byte_size) metrics), providing visibility into
		  buffer capacity, usage and historical usage levels.
		- Introduced `trace_to_log` transform that allows converting traces to logs.
		- The blackhole sink now implements end-to-end acknowledgements.
		- The GELF decoder now supports a `validation` option with two modes: `strict` (default)
		  and `relaxed`. When set to `relaxed`, the decoder will parse GELF messages from sources
		  that don't strictly follow the GELF specification.
		- The `docker_logs` source now retries Docker daemon communication failures with exponential backoff.


		## Breaking Changes

		- The `mongodb_metrics` source now requires MongoDB Server 4.2 or later. MongoDB Server 4.0, the previously supported minimum version, reached end-of-life on April 30, 2022.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				The `syslog` source in UDP mode now emits the standard "received" metrics, aligning behavior with TCP and the Component Specification:

				- `component_received_events_total`
				- `component_received_event_bytes_total`
				- `component_received_bytes_total`

				This makes internal telemetry consistent and restores compliance checks for UDP syslog.
				"""
			contributors: ["sghall"]
		},
		{
			type: "fix"
			description: """
				The `journald` source now correctly respects the `current_boot_only: false` setting on systemd versions >= 258.

				Compatibility notes:

				- **systemd < 250**: Both `current_boot_only: true` and `false` work correctly
				- **systemd 250-257**: Due to systemd limitations, `current_boot_only: false` will not work. An error will be raised on startup.
				- **systemd >= 258**: Both settings work correctly
				"""
			contributors: ["bachorp"]
		},
		{
			type: "feat"
			description: """
				Added a new `prefetch_count` option to the AMQP source configuration. This allows limiting the number of in-flight (unacknowledged) messages per consumer using RabbitMQ's prefetch mechanism (`basic.qos`). Setting this value helps control memory usage and load when processing messages slowly.
				"""
			contributors: ["elkh510"]
		},
		{
			type: "enhancement"
			description: """
				Vector's TLS implementation now stores credentials in PEM format internally instead of PKCS12, enabling FIPS-compliant operation in
				environments with strict cryptographic requirements. This change is transparent to users - both PEM and PKCS12 certificate files continue to
				be supported as configuration inputs, with PKCS12 files automatically converted at load time.
				"""
			contributors: ["rf-ben"]
		},
		{
			type: "enhancement"
			description: """
				The `http_client` source now supports the `body` parameter. VRL is also supported in the body which allows a dynamic request body to be generated.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "enhancement"
			description: """
				The GELF decoder now supports a `validation` option with two modes: `strict` (default) and `relaxed`. When set to `relaxed`, the decoder will accept:

				- GELF versions other than 1.1
				- Additional fields without underscore prefixes
				- Additional field names with special characters
				- Additional field values of any type (not just strings/numbers)

				This allows Vector to parse GELF messages from sources that don't strictly follow the GELF specification.
				"""
			contributors: ["ds-hystax"]
		},
		{
			type: "feat"
			description: """
				Add AWS CloudWatch Metrics sink `storage_resolution` config.
				"""
			contributors: ["trxcllnt"]
		},
		{
			type: "fix"
			description: """
				Fixed the `websocket` source entering a "zombie" state when the `connect_timeout_secs` threshold was reached with multiple sources running. The connection timeout is now applied per connect attempt with indefinite retries, rather than as a total timeout limit.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "fix"
			description: """
				Fixed a bug in the `file` source, which could silently corrupt data when using multi-char delimiters.
				"""
			contributors: ["lfrancke"]
		},
		{
			type: "feat"
			description: """
				The `docker_logs` source now includes exponential backoff retry logic for Docker daemon communication failures, with indefinite retry capability. This improves reliability when working with slow or temporarily unresponsive Docker daemons by retrying with increasing delays instead of immediately stopping.
				"""
			contributors: ["titaneric"]
		},
		{
			type: "feat"
			description: """
				A generic [Apache Arrow](https://arrow.apache.org/) codec has been added to
				support [Arrow IPC](https://arrow.apache.org/docs/format/Columnar.html#ipc-streaming-format) serialization across Vector. This enables sinks
				like the `clickhouse` sink to use the ArrowStream format endpoint with significantly better performance and smaller payload sizes compared
				to JSON-based formats.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "enhancement"
			description: """
				The Arrow encoder now supports configurable null handling through the `allow_nullable_fields`
				option. This controls whether nullable fields should be explicitly marked
				as nullable in the Arrow schema, enabling better compatibility with
				downstream systems that have specific requirements for null handling.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue in vector tests where memory enrichment tables would report missing components errors.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "chore"
			description: """
				The `mongodb_metrics` source now requires MongoDB Server 4.2 or later. MongoDB Server 4.0, the previously supported minimum version, reached end-of-life on April 30, 2022.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: """
				Fixed the blackhole sink to properly implement end-to-end acknowledgements. Previously, the sink consumed events without updating finalizer status, causing sources that depend on acknowledgements (like `aws_s3` with SQS) to never delete processed messages from the queue.
				"""
			contributors: ["sanjams2"]
		},
		{
			type: "feat"
			description: """
				Introduced `trace_to_log` transform that allows converting traces to logs.
				"""
			contributors: ["huevosabio"]
		},
		{
			type: "enhancement"
			description: """
				Added a new "Custom Authorization" HTTP auth strategy, allowing users to configure a custom HTTP Authorization Header
				"""
			contributors: ["arunpidugu"]
		},
		{
			type: "feat"
			description: """
				Added `--disable-env-var-interpolation` CLI option to prevent environment variable interpolation. The `VECTOR_DISABLE_ENV_VAR_INTERPOLATION` environment variable can also be used to disable interpolation.
				"""
			contributors: ["graphcareful"]
		},
		{
			type: "feat"
			description: """
				The `aws_s3` source now emits histogram metrics to track S3 object processing times: `s3_object_processing_succeeded_duration_seconds` for successful processing and `s3_object_processing_failed_duration_seconds` for failed processing. These measure the full processing pipeline including download, decompression, and parsing. Both metrics include a `bucket` label to help identify slow buckets.
				"""
			contributors: ["sanjams2"]
		},
		{
			type: "feat"
			description: """
				The `axiom` sink now supports regional edges for data locality. A new optional `region` configuration field allows you to specify the regional edge domain (e.g., `eu-central-1.aws.edge.axiom.co`). When configured, data is sent to `https://{region}/v1/ingest/{dataset}`. The `url` field now intelligently handles paths: URLs with custom paths are used as-is, while URLs without paths maintain backwards compatibility by appending `/v1/datasets/{dataset}/ingest`.
				"""
			contributors: ["toppercodes"]
		},
		{
			type: "enhancement"
			description: """
				Added support for configurable request timeouts to the `datadog_agent` source.

				This change also introduces two new internal metrics:
				- `component_timed_out_events_total` - Counter tracking the number of events that timed out
				- `component_timed_out_requests_total` - Counter tracking the number of requests that timed out
				"""
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: """
				The `http_client` source now fails to start if VRL compilation errors occur in `query` parameters when
				type is set to `vrl`, instead of silently logging a warning and continuing with invalid expressions.
				This prevents unexpected behavior where malformed VRL would be sent as literal strings in HTTP requests.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: """
				Added the following metrics to record the utilization level of the buffer that
				all sources send into:

				- `source_buffer_max_byte_size`
				- `source_buffer_max_event_size`
				- `source_buffer_utilization`
				- `source_buffer_utilization_level`
				"""
			contributors: ["bruceg"]
		},
		{
			type: "enhancement"
			description: """
				Added metrics to record the utilization level of the buffers that each transform receives from:

				- `transform_buffer_max_byte_size`
				- `transform_buffer_max_event_size`
				- `transform_buffer_utilization`
				- `transform_buffer_utilization_level`
				"""
			contributors: ["bruceg"]
		},
	]

	vrl_changelog: """
		### [0.29.0 (2025-12-11)]

		#### Breaking Changes & Upgrade Guide

		- Added required `line` and `file` fields to `vrl::compiler::function::Example`. Also added the
		`example!` macro to automatically populate those fields.

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1557)

		#### Fixes

		- Fixed handling of OR conjunctions in the datadog search query parser (https://github.com/vectordotdev/vrl/pull/1542)
		- Fixed a bug where VRL would crash if `merge` were called without a `to` argument.

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1563)
		- Fixed a bug where a stack overflow would happen in validate_json_schema if the schema had an empty $ref.

		authors: jlambatl (https://github.com/vectordotdev/vrl/pull/1577)


		### [0.28.1 (2025-11-07)]
		"""

	commits: [
		{sha: "6bf28dd5dbcfbd50a7cd5564eff592df860cfc80", date: "2025-11-04 01:33:39 UTC", description: "add serde, tokio, tracing patterns", pr_number: 24132, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 15, deletions_count: 3},
		{sha: "2ed1eb47e3eb40b9fd4a2cd7832c562b09c40bef", date: "2025-11-04 06:39:45 UTC", description: "fix gcp test filter and ignore failing tests", pr_number: 24134, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 9, deletions_count: 1},
		{sha: "9d50f2d4bfd5fdadf72cf5b06af12b96e2958fac", date: "2025-11-04 23:44:44 UTC", description: "rebuild manifests for 0.51.0", pr_number: 24142, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "817be3846e8b932253d3f15ec915e84566675831", date: "2025-11-05 00:00:32 UTC", description: "reorg e2e tests", pr_number: 24136, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 39, insertions_count: 80, deletions_count: 65},
		{sha: "7e2b3223565396db8be2dd130a579e3364cf4a7c", date: "2025-11-05 00:53:53 UTC", description: "typo fix", pr_number: 24146, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "90b395120c694878e2c262cad7ade1c142ef6b7b", date: "2025-11-05 01:09:46 UTC", description: "v0.51.0", pr_number: 24145, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 40, insertions_count: 643, deletions_count: 144},
		{sha: "f40ea0942430d160f6bc9e8bafd7080067ded76e", date: "2025-11-05 19:42:11 UTC", description: "Add an option to prevent interpolation of env vars within config loading process", pr_number: 23910, scopes: ["config"], type: "feat", breaking_change: false, author: "Rob Blafford", files_count: 12, insertions_count: 186, deletions_count: 69},
		{sha: "749fbb078b5fe2fd0083dc731f747ffed9d34c4d", date: "2025-11-05 19:52:22 UTC", description: ".dockerignore should exlcude target dirs", pr_number: 24154, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "6d332b4c48f8fd375cdf417d13a73655f8fa5fee", date: "2025-11-05 23:16:07 UTC", description: "refactor ConfigBuilderLoader (tech debt)", pr_number: 24157, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 191, deletions_count: 169},
		{sha: "2fbe9494c530f1451590ea6932c963957b0c1fb6", date: "2025-11-05 23:50:23 UTC", description: "simplify/improve scripts/ci-free-disk-space.sh", pr_number: 24159, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 30, deletions_count: 49},
		{sha: "c9537a0de423884b0581341a9a09b15a68448094", date: "2025-11-06 06:10:43 UTC", description: "add support for regional edge endpoints in AxiomConfig", pr_number: 24037, scopes: ["axiom"], type: "feat", breaking_change: false, author: "Topper", files_count: 6, insertions_count: 551, deletions_count: 308},
		{sha: "d43ab9ec84836f484a155c8b2d155189dba1789c", date: "2025-11-06 01:43:25 UTC", description: "update toml to 0.9.8", pr_number: 24161, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 56, deletions_count: 55},
		{sha: "99b5835af91aa0423400a18c35e6c2b3619b8ed0", date: "2025-11-06 02:22:35 UTC", description: "remove --reuse-image", pr_number: 24163, scopes: ["vdev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 13, insertions_count: 9, deletions_count: 96},
		{sha: "6913528d50b66cc890b8b34f333c2520e2d24a06", date: "2025-11-06 02:58:47 UTC", description: "refactor SecretBackendLoader (tech debt)", pr_number: 24160, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 98, deletions_count: 118},
		{sha: "e9c81d25045f29c3b6e83030725857f1d25ebdf0", date: "2025-11-06 20:34:35 UTC", description: "fix failing dependabot dockerfile updates", pr_number: 24172, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 20, deletions_count: 1},
		{sha: "325c5c296bad7656e947c853449d5f7bb92a2f2f", date: "2025-11-06 21:01:01 UTC", description: "download toolchain only once", pr_number: 24176, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "81ca9f26c487c3eebdfca6ca8e5f334024bd406c", date: "2025-11-07 02:02:18 UTC", description: "bump the artifact group with 2 updates", pr_number: 24173, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 71, deletions_count: 71},
		{sha: "d2b4f6422a6a1af1fdfc565fad2ff733d8eadf3e", date: "2025-11-06 21:02:37 UTC", description: "bump docker/setup-qemu-action from 3.6.0 to 3.7.0", pr_number: 24174, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "f07e8833e548137ee2c3e8df9585db74b9e8d487", date: "2025-11-07 02:02:55 UTC", description: "bump docker/metadata-action from 5.8.0 to 5.9.0", pr_number: 24175, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "df6d39b7c6a196db300f5e14e34069a34c9d5447", date: "2025-11-07 02:15:12 UTC", description: "delete config subcommand", pr_number: 24181, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 15, insertions_count: 5, deletions_count: 172},
		{sha: "02671f454061bdb41f9600cafcff3b4f26bd3773", date: "2025-11-07 18:20:48 UTC", description: "Allow `datadog_search` to use `&LogEvent` directly", pr_number: 24182, scopes: ["transforms"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 9, deletions_count: 14},
		{sha: "c1e83f9525037e6e8eecced6804f0fac180ebc0f", date: "2025-11-07 18:44:19 UTC", description: "Refactor `source_sender` into modules", pr_number: 24183, scopes: ["sources"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 7, insertions_count: 757, deletions_count: 696},
		{sha: "f453b8b1179c3ce36c211d21cc246945365db36a", date: "2025-11-07 22:03:24 UTC", description: "Move `source_sender` into `vector-core`", pr_number: 24186, scopes: ["sources"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 18, insertions_count: 95, deletions_count: 75},
		{sha: "1e3f38736ee4b3ef592fc0efd4adbb02bcad138b", date: "2025-11-07 23:27:56 UTC", description: "add log verbosity section to the debugging guide", pr_number: 24187, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 16, deletions_count: 0},
		{sha: "61bf5ad14b76ef7f2835eb207e3ebfdc76d538d2", date: "2025-11-11 20:39:47 UTC", description: "remove build-all flag, inspect state instead", pr_number: 24206, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 12, insertions_count: 39, deletions_count: 64},
		{sha: "98b77a1645f8457c01128a0904ce5fb1f5a8e871", date: "2025-11-11 22:33:41 UTC", description: "run fmt before commiting clippy fixes", pr_number: 24210, scopes: ["vdev"], type: "enhancement", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "4eea77c36cf376e7c411df65a30c4fbdc8596b43", date: "2025-11-11 22:41:31 UTC", description: "upgrade Rust to 1.91.1", pr_number: 24209, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 19, insertions_count: 46, deletions_count: 89},
		{sha: "927482bb33e3c5c3210c83ccddff43cbc06a2cb6", date: "2025-11-11 23:56:30 UTC", description: "Add custom authorization header strategy for http client source", pr_number: 24201, scopes: ["http_client"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 13, insertions_count: 100, deletions_count: 0},
		{sha: "6ee7839a2bafced6bed53b4344c05e4f787032e9", date: "2025-11-12 04:45:02 UTC", description: "add missing md file for the incremental_to_absolute transform", pr_number: 24217, scopes: ["website"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 14, deletions_count: 0},
		{sha: "673a19cfcf5ecdea70ba1bd332645bab61b81ea5", date: "2025-11-12 22:17:06 UTC", description: " new blog post - First year of COSE", pr_number: 24179, scopes: ["website"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 172, deletions_count: 0},
		{sha: "068475e18e016c1fe72ea4042d1e58bbd4726c5f", date: "2025-11-12 21:24:30 UTC", description: "introduces transform that converts traces to logs", pr_number: 24168, scopes: ["trace_to_log transform"], type: "feat", breaking_change: false, author: "spencerho777", files_count: 9, insertions_count: 256, deletions_count: 1},
		{sha: "44f34e823699db88dc382f0da5c23e0734181438", date: "2025-11-13 00:29:13 UTC", description: "group imports", pr_number: 24219, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 19, insertions_count: 65, deletions_count: 59},
		{sha: "24099ebe04d83324352612237e3982b1ad4578d1", date: "2025-11-13 02:04:05 UTC", description: "build-test-runner if condition", pr_number: 24224, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 11, deletions_count: 11},
		{sha: "4d22ce1b28f33c69872e57667a5bedd6a50a4b89", date: "2025-11-13 08:58:19 UTC", description: "prevent missing components errors for memory tables in tests", pr_number: 24081, scopes: ["unit tests"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 12, deletions_count: 0},
		{sha: "8d3d623098c4caaed8295a1753235cbde1aa8dc3", date: "2025-11-13 22:21:28 UTC", description: "update manifests 0.51.1", pr_number: 24233, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "889e6a1915ca2277ca44800fe308d9eea5fe961f", date: "2025-11-13 23:52:18 UTC", description: "bump blog post date", pr_number: 24235, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "70b26187826a1ac6f047020740ee8bed65641960", date: "2025-11-14 01:09:29 UTC", description: "v0.51.1", pr_number: 24234, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 8, insertions_count: 110, deletions_count: 28},
		{sha: "b367f7dddd58fea63045ba8bdda02eaa3c9e679a", date: "2025-11-14 01:18:15 UTC", description: "fail on VRL compilation errors in query parameters", pr_number: 24223, scopes: ["http_client"], type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 35, deletions_count: 20},
		{sha: "3ef42ae4c457495a11955fc86d9fdf94cbda1398", date: "2025-11-14 01:20:02 UTC", description: "skip removed files when formatting", pr_number: 24232, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "5553521edc2415325dd28179423cdf02b45f56f6", date: "2025-11-14 02:19:00 UTC", description: "eliminate race condition when aqcuiring socket addresses", pr_number: 24212, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 67, insertions_count: 537, deletions_count: 566},
		{sha: "41e384944f0bce1335b1aadb91f5f091c48d9f9b", date: "2025-11-15 01:27:53 UTC", description: "add arrow IPC stream batch encoder", pr_number: 24124, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Benjamin Dornel", files_count: 16, insertions_count: 2029, deletions_count: 11},
		{sha: "fcd135adadf3c3ff17c6194cc09df0f2597ae99b", date: "2025-11-14 19:08:43 UTC", description: "Refactor handle_request into struct", pr_number: 24238, scopes: ["datadog_agent source"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 94, deletions_count: 121},
		{sha: "62e34462c4219a5a6e2497b08f7d91e2cb0b082b", date: "2025-11-15 02:13:13 UTC", description: "handle custom auth strategy in all sinks", pr_number: 24240, scopes: ["http_client"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 3, insertions_count: 5, deletions_count: 0},
		{sha: "8a8f223012fab640035e65533edf3ad94c3cd3d1", date: "2025-11-15 01:49:57 UTC", description: "Apply review suggestions from PR #24234", pr_number: 24244, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "b9726201642b7e3219f279f9ed7ea3320ed6bdd4", date: "2025-11-15 02:39:56 UTC", description: "flush and sync files in file source tests", pr_number: 24243, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 91, deletions_count: 83},
		{sha: "c8cbfbfe624b5d6df38367727d6e67db181e73c5", date: "2025-11-17 20:41:30 UTC", description: "add default ExponentialBackoff", pr_number: 24246, scopes: ["sources", "sinks"], type: "chore", breaking_change: false, author: "Thomas", files_count: 8, insertions_count: 26, deletions_count: 38},
		{sha: "9c3e7ee88805609492238e4994fb621df90244e1", date: "2025-11-17 20:44:02 UTC", description: "Add request timeout support", pr_number: 24245, scopes: ["datadog_agent source"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 24, insertions_count: 500, deletions_count: 160},
		{sha: "bdb96ce5f6d0ab2558da7e2aba898f51860899db", date: "2025-11-18 08:00:36 UTC", description: "introduce an option to relax GELF validation", pr_number: 24241, scopes: ["codecs"], type: "enhancement", breaking_change: false, author: "Dmitry Sergeenkov", files_count: 24, insertions_count: 666, deletions_count: 183},
		{sha: "d6c21e50eeb0ea390fc9ba64e19e4f53ecadbc0b", date: "2025-11-17 23:08:08 UTC", description: "delete cue.mod", pr_number: 24254, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 0, deletions_count: 10},
		{sha: "ac207396efc9b24b16024d3507cdf6a48c5872a3", date: "2025-11-18 12:17:38 UTC", description: "add exponential retry to docker client", pr_number: 24063, scopes: ["docker_logs source"], type: "feat", breaking_change: false, author: "Eric Huang", files_count: 2, insertions_count: 45, deletions_count: 5},
		{sha: "67509b09756a5f7d112184dd0b3d70457d8ffba7", date: "2025-11-17 23:36:30 UTC", description: "document the global healthcheck option", pr_number: 24253, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 28, deletions_count: 1},
		{sha: "fff7f5a34366cca87a8a71cb18570d8a2f8927c8", date: "2025-11-17 23:50:48 UTC", description: "forbid unwrap and refactor error handling", pr_number: 24247, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 11, insertions_count: 105, deletions_count: 85},
		{sha: "6996ec55d1424be0a68929169c7119dc6baae637", date: "2025-11-18 06:05:41 UTC", description: "journalctl args in case of current_boot_only", pr_number: 23438, scopes: ["journald source"], type: "fix", breaking_change: false, author: "Pascal Bachor", files_count: 3, insertions_count: 102, deletions_count: 6},
		{sha: "61bb16f53d09d009ea4a7a363b83acbb4a753b85", date: "2025-11-18 21:10:37 UTC", description: "add note to 'include_units' option", pr_number: 24260, scopes: ["journald source"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 8, deletions_count: 0},
		{sha: "8a8b981cc15cb4739caa05869721728d46d4fa32", date: "2025-11-18 21:10:49 UTC", description: "improve routes docs", pr_number: 24259, scopes: ["exclusive_route transform"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 11, deletions_count: 3},
		{sha: "f1efa9dc7badd4358c82838e139bef6739b07692", date: "2025-11-18 21:35:43 UTC", description: "fix healthcheck -> healthchecks", pr_number: 24267, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "e38c093e8857ebbdbbab1ff398639b6181a8cea7", date: "2025-11-18 21:42:17 UTC", description: "add aqua deps", pr_number: 24269, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 10, deletions_count: 7},
		{sha: "677f21e4c3d9d9b63a1c73b4bef5272b736b58ec", date: "2025-11-18 22:29:30 UTC", description: "improve build from source guide", pr_number: 24268, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 84, deletions_count: 41},
		{sha: "563251a03e2ef88a0adf871f323669a5982bbd04", date: "2025-11-18 19:32:44 UTC", description: "rework TlsSettings to carry PEM based objects", pr_number: 23146, scopes: ["security"], type: "enhancement", breaking_change: false, author: "rf-ben", files_count: 3, insertions_count: 83, deletions_count: 84},
		{sha: "8c9bc00b712b57519ed09540dc9967ed3a453c4e", date: "2025-11-18 20:06:19 UTC", description: "Support AWS CloudWatch high-resolution metrics", pr_number: 23822, scopes: ["aws_cloudwatch_metrics sink"], type: "feat", breaking_change: false, author: "Paul Taylor", files_count: 4, insertions_count: 58, deletions_count: 2},
		{sha: "5edc39344b6b3f5aad0d12decc9f33c930514b76", date: "2025-11-18 20:17:18 UTC", description: "smp cli: v0.24.1 -> v0.25.1", pr_number: 24262, scopes: ["ci"], type: "chore", breaking_change: false, author: "Geoffrey Oxberry", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "16429fa70de7b240e3c2034fcdc9ec05eba150a7", date: "2025-11-19 02:46:23 UTC", description: "handle out of order reads in test_fair_reads", pr_number: 24270, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 20, deletions_count: 13},
		{sha: "821c1f798b5f1814a7a0b26882dfd391a1f61a91", date: "2025-11-19 20:52:47 UTC", description: "update mongodb crate to 3.3.0", pr_number: 24271, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 252, deletions_count: 253},
		{sha: "12c880f33c5aaa45216d1a97a7977e2a8d1f1855", date: "2025-11-20 03:07:57 UTC", description: "Add CLA signature workflow", pr_number: 24276, scopes: ["ci"], type: "chore", breaking_change: false, author: "@Ara Pulido", files_count: 1, insertions_count: 44, deletions_count: 0},
		{sha: "870b86ffe1c1c8c609a2b7c4532a9836166392cd", date: "2025-11-20 01:47:53 UTC", description: "Allow CLA check to pass on merge queue events", pr_number: 24277, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "b9ad9b3ec1ade765583e4d06fc7142f8e6b745a2", date: "2025-11-22 00:12:37 UTC", description: "remove number-prefix in favor of unit_prefix", pr_number: 24293, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 6, deletions_count: 12},
		{sha: "df4f3621e7941c4eb3ba6ad76c739552b427951f", date: "2025-11-26 21:51:00 UTC", description: "implement end-to-end acknowledgements", pr_number: 24283, scopes: ["blackhole sink"], type: "fix", breaking_change: false, author: "James", files_count: 3, insertions_count: 11, deletions_count: 4},
		{sha: "84c94441223a4e2f83be5e5ae0e56180c2c45931", date: "2025-11-30 10:24:37 UTC", description: "fix return type for `mod` function in VRL function reference", pr_number: 24312, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5c16191caea16363da0baa70b5a7be67a945826a", date: "2025-12-01 23:29:37 UTC", description: "use ci-docs-build flow instead of local docs flow", pr_number: 24319, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "08dceb5e786df3f889e86ef68dccf5ae20d67fff", date: "2025-12-02 00:31:20 UTC", description: "add missing --workspace argument to make docs", pr_number: 24318, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e6277228958fe51d3f37cea69551e57ac51c45f4", date: "2025-12-01 23:57:00 UTC", description: "Add internal metric to record source buffer utilization", pr_number: 24272, scopes: ["sources"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 290, deletions_count: 61},
		{sha: "3eda9d2ec27fe7615f9cf1779d1e9b89ae3ab0a7", date: "2025-12-02 01:28:13 UTC", description: "bump VRL version to include example location", pr_number: 24317, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 9, insertions_count: 18, deletions_count: 16},
		{sha: "3eae9314d1ed3f80d1c1db6ba8ede1e4ccb7183d", date: "2025-12-02 02:17:45 UTC", description: "Fix flaky test_oldest_first by ensuring distinct creation timestamps", pr_number: 24327, scopes: ["file source"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "cea65d0b1688d40be2a819cb935b736c4a75d818", date: "2025-12-03 03:39:49 UTC", description: "bump maxminddb to 0.27 after RUSTSEC-2025-0132", pr_number: 24332, scopes: ["deps"], type: "chore", breaking_change: false, author: "Clément Delafargue", files_count: 4, insertions_count: 64, deletions_count: 60},
		{sha: "80fc73b2ccb2be507b485189de1677900cf24246", date: "2025-12-03 03:51:24 UTC", description: "Bump vrl hash and fix datadog search tests", pr_number: 24334, scopes: ["vrl"], type: "chore", breaking_change: true, author: "Yoenn Burban", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "4902750cf18eae8e201e5849fea1a404bb57afe8", date: "2025-12-02 23:21:18 UTC", description: "emit received events/bytes metrics for UDP mode", pr_number: 24296, scopes: ["syslog source"], type: "fix", breaking_change: false, author: "Steve Hall", files_count: 2, insertions_count: 91, deletions_count: 2},
		{sha: "ea556a288e2e58b1f09bbb1181add57bd1ff5742", date: "2025-12-03 20:43:41 UTC", description: "Introduce `trait NamedInternalEvent` and derive", pr_number: 24313, scopes: ["observability"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 92, insertions_count: 579, deletions_count: 431},
		{sha: "7d1773093745995f8193117855a1436ad71bdbf1", date: "2025-12-04 19:01:59 UTC", description: "Add missing `deny.toml` entry for the new macro crate", pr_number: 24339, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 14},
		{sha: "72e09673fda9d6fbf933adacea1220bdfae162a8", date: "2025-12-05 00:58:22 UTC", description: "Improve deny and make it run on PRs when necessary", pr_number: 24340, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 23, deletions_count: 40},
		{sha: "dbc805a77b51a6b426e067a772ee0eae04f958d1", date: "2025-12-06 00:58:01 UTC", description: "Add internal metric to record buffer utilization", pr_number: 24329, scopes: ["transforms"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 15, insertions_count: 280, deletions_count: 117},
		{sha: "922d970672a79bf3e88ece9bfd020a73bcd7e8e4", date: "2025-12-09 03:13:53 UTC", description: "Ignore RUSTSEC-2025-0134 for rustls-pemfile", pr_number: 24352, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "a7a4106a4c1065fc3e85a933fedda7c2511b7ba1", date: "2025-12-09 22:05:19 UTC", description: "bump hyper, http-body and apply deprecation suggestions", pr_number: 24351, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 26, insertions_count: 118, deletions_count: 75},
		{sha: "b5d718a2da8897e9631f96402889b496620e13c0", date: "2025-12-09 22:17:22 UTC", description: "use compiled vdev with `make` commands", pr_number: 24347, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 101, deletions_count: 40},
		{sha: "250de61049b4862586ddd1885057324c16bccfa4", date: "2025-12-10 06:02:13 UTC", description: "Configure prefetch count", pr_number: 24138, scopes: ["amqp source"], type: "feat", breaking_change: false, author: "elkh510", files_count: 3, insertions_count: 41, deletions_count: 1},
		{sha: "cf6e3293a859c04c50e63d34c857ed183fa5bea5", date: "2025-12-09 22:14:58 UTC", description: "Refactor `EventMetadata` deserialization from protobuf", pr_number: 24336, scopes: ["performance"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 38, deletions_count: 36},
		{sha: "f1eecd0e778784bab08fd72c6e33b0b5631e2d79", date: "2025-12-09 21:42:57 UTC", description: "upgrade rdkafka to 0.38.0 to resolve idempotent-producer fatal \"Inconsistent state\" stalls", pr_number: 24197, scopes: ["kafka sink"], type: "fix", breaking_change: false, author: "skipper", files_count: 3, insertions_count: 68, deletions_count: 17},
		{sha: "a7996cec4d7268dae610e1f9fca8804cd129955e", date: "2025-12-09 23:06:21 UTC", description: "EventMetadata UUID generation optimizations", pr_number: 24358, scopes: ["performance"], type: "chore", breaking_change: false, author: "Jansen", files_count: 6, insertions_count: 78, deletions_count: 14},
		{sha: "538c833d2f5c6529ba1df7b02f4bb73e60b2d778", date: "2025-12-10 21:38:11 UTC", description: "bump actions/checkout from 5.0.0 to 6.0.0", pr_number: 24322, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 26, insertions_count: 83, deletions_count: 83},
		{sha: "0b35fe8791f347a553d4835fcd07cef7a1fb5d61", date: "2025-12-10 19:14:59 UTC", description: "add S3 download processing duration metric", pr_number: 24289, scopes: ["aws_s3 source"], type: "feat", breaking_change: false, author: "James", files_count: 4, insertions_count: 87, deletions_count: 6},
		{sha: "0f998497b88393ba33ee90d6775f0848237e32a3", date: "2025-12-10 23:04:15 UTC", description: "bump aws-actions/configure-aws-credentials from 5.0.0 to 5.1.1", pr_number: 24323, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "a053a2e62dc6c1490af2f9eacb7efafdcad0ab26", date: "2025-12-11 12:31:40 UTC", description: "reconnect indefinitely when connection fails", pr_number: 24069, scopes: ["websocket source"], type: "fix", breaking_change: false, author: "Benjamin Dornel", files_count: 4, insertions_count: 50, deletions_count: 20},
		{sha: "3f48cae746dfaa7d75b110e94cbe3cfedb6ebf82", date: "2025-12-11 12:53:32 UTC", description: "allow configurable null handling in Arrow encoder", pr_number: 24288, scopes: ["codecs"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 2, insertions_count: 217, deletions_count: 3},
		{sha: "d2771c3f5639e9d87ba103a0492d0db05451df86", date: "2025-12-12 14:42:57 UTC", description: "clean up some `allow` statements", pr_number: 24366, scopes: ["dev"], type: "chore", breaking_change: false, author: "WaterWhisperer", files_count: 2, insertions_count: 0, deletions_count: 2},
		{sha: "b9cbce345499d42a691a8d485025068dd1cab3b0", date: "2025-12-12 22:43:29 UTC", description: "README e2e badge", pr_number: 24375, scopes: ["internal docs"], type: "docs", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "feb33ce7f08ec6799963d94ac8f627a2e131cbbe", date: "2025-12-13 03:05:00 UTC", description: "bump VRL to use 0.29.0 sha", pr_number: 24378, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 48, deletions_count: 36},
		{sha: "3921ecb5c14a6b48f89747907af08c7ddb08b207", date: "2025-12-15 19:45:23 UTC", description: "bump github/codeql-action from 3.30.6 to 4.31.6", pr_number: 24324, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "faa2c21fcdbac813e8433afec24fe7849556b197", date: "2025-12-15 19:54:46 UTC", description: "bump docker/metadata-action from 5.9.0 to 5.10.0", pr_number: 24326, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "eae0be26759331ca19fe5d77ecee78cd329e3133", date: "2025-12-15 19:55:09 UTC", description: "bump DataDog/dd-octo-sts-action from 1.0.1 to 1.0.3", pr_number: 24325, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e6397f3fdba0306fb0def6d602c9f3d4053fa109", date: "2025-12-16 12:39:24 UTC", description: "add support for request body", pr_number: 24170, scopes: ["http_client source"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 10, insertions_count: 489, deletions_count: 92},
	]
}
