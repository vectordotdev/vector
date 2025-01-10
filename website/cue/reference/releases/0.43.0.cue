package metadata

releases: "0.43.0": {
	date:     "2024-11-27"
	codename: ""

	whats_next: []

	known_issues: [
		"""
			The `vector-0.43.0-x86_64-apple-darwin.tar.gz` executable has the wrong architecture, see
			[#22129](https://github.com/vectordotdev/vector/issues/22129). This will be fixed in
			`v0.44`.
			""",
	]

	description: """
		The Vector team is pleased to announce version 0.43.0!

		This release contains a few notable new features along with the numerous enhancements and fixes as listed below:
		- A new `opentelemetry` sink with initial support for emitting logs via OTLP over HTTP. We expect this to expand to support gRPC and emission of other data types.
		- A new `exclusive_route` transform to route events exclusively using an ordered set of conditions
		- A new `cef` encoder to emit events encoded as in [Common Event Format](https://www.microfocus.com/documentation/arcsight/arcsight-smartconnectors/pdfdoc/common-event-format-v25/common-event-format-v25.pdf)
		- A new `chunked_gelf` framing decoder to receive [chunked GELF messages](https://archivedocs.graylog.org/en/latest/pages/gelf.html)
		- Vector's configuration now allows for use of the [YAML merge operator](https://yaml.org/type/merge.html) allowing simplification of configurations with duplication
		- Two new secrets backends for loading secrets into Vector configuration: `file`, which loads secrets from a JSON file, and `directory`, which loads secrets from a tree of files.


		There are no breaking changes or deprecations with this release and so no upgrade guide.
		"""

	changelog: [
		{
			type: "feat"
			description: """
				VRL was updated to v0.20.0. This includes the following changes:

				#### Breaking Changes & Upgrade Guide

				- Fixes the `to_float` function to return an error instead of `f64::INFINITY` when parsing [non-normal](https://doc.rust-lang.org/std/primitive.f64.html#method.is_normal) numbers. (https://github.com/vectordotdev/vrl/pull/1107)

				#### New Features

				- The `decrypt` and `encrypt` VRL functions now support aes-siv (RFC 5297) encryption and decryption. (https://github.com/vectordotdev/vrl/pull/1100)

				#### Enhancements

				- `decode_punycode` and `encode_punycode` with the `validate` flag set to false should be faster now, in cases when input data needs no encoding or decoding. (https://github.com/vectordotdev/vrl/pull/1104)
					Otherwise, it will return `None`. (https://github.com/vectordotdev/vrl/pull/1117)
				- The `encode_proto` function was enhanced to automatically convert valid string fields to numeric proto
					fields. (https://github.com/vectordotdev/vrl/pull/1114)

				#### Fixes

				- The `parse_groks` VRL function and Datadog grok parsing now catch the panic coming from `rust-onig` on too many regex match retries and handle it as a custom error. (https://github.com/vectordotdev/vrl/pull/1079)
				- `encode_punycode` with the `validate` flag set to false should be more consistent with when `validate` is set to true, turning all uppercase character to lowercase as well as doing punycode encoding (https://github.com/vectordotdev/vrl/pull/1115)
				- Removed false warning when using `set_semantic_meaning`. (https://github.com/vectordotdev/vrl/pull/1148)
				"""
		},
		{
			type: "feat"
			description: """
				The Elasticsearch sink can now write to Amazon OpenSearch Serverless via the `opensearch_service_type = "serverless"` option.
				"""
			contributors: ["handlerbot", "AvihaiSam"]
		},
		{
			type: "enhancement"
			description: """
				Add ability to encode messages to [Common Event Format (CEF)](https://www.microfocus.com/documentation/arcsight/arcsight-smartconnectors/pdfdoc/common-event-format-v25/common-event-format-v25.pdf) with the `cef` encoder (widely used in SIEM systems).
				"""
			contributors: ["nabokihms"]
		},
		{
			type: "enhancement"
			description: """
				The Kubernetes Logs source can now enrich logs with pod information on Windows.
				"""
			contributors: ["damoxc"]
		},
		{
			type: "fix"
			description: """
				`exec` and `http_server` sources no longer attach a redundant `timestamp` field
				when log namespacing is enabled.
				"""
			contributors: ["rwakulszowa"]
		},
		{
			type: "feat"
			description: """
				Allows for chunked GELF decoding in message-based sources, such as UDP sockets or unix datagram sockets.
				Implementation is based on [Graylog's documentation](https://go2docs.graylog.org/5-0/getting_in_log_data/gelf.html#GELFviaUDP).
				The implementation also supports payload decompression.

				This framing method can be configured via the `framing.method = "chunked_gelf"` option in the source configuration.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "enhancement"
			description: """
				The `sample` transform can now take in a `group_by` configuration option that allows logs with unique values for the patterns passed in to be sampled independently. This can reduce the complexity of the topology, since users no longer need to create separate samplers with similar configurations to handle different log streams.
				"""
			contributors: ["hillmandj"]
		},
		{
			type: "enhancement"
			description: """
				Expose `connection_retry_options` in the Pulsar sink configuration to allow customizing the connection retry behaviour of the pulsar client. This includes the following options:

				- `min_backoff_ms`: Minimum delay between connection retries.
				- `max_backoff_secs`: Maximum delay between reconnection retries.
				- `max_retries`: Maximum number of connection retries.
				- `connection_timeout_secs`: Time limit to establish a connection.
				- `keep_alive_secs`: Keep-alive interval for each broker connection.
				"""
			contributors: ["FRosner"]
		},
		{
			type: "enhancement"
			description: """
				The `http` sink now retries requests when the response is a request timeout (HTTP 408).
				"""
			contributors: ["noble-varghese", "pront"]
		},
		{
			type: "feat"
			description: """
				Adds support for loading and concatenating multiple VRL files in the `remap` transform via the `files` option.
				This allows users to break down Vector remaps into smaller, more manageable units of configuration, improving organization, reusability, and maintainability of VRL code.
				"""
			contributors: ["brittonhayes"]
		},
		{
			type: "fix"
			description: """
				The `gcp_pubsub` sink now supports emitting metrics and traces.
				"""
			contributors: ["genadipost"]
		},
		{
			type: "feat"
			description: """
				The `opentelemetry` source can now be configured to enrich log events with HTTP headers received in the OTLP/HTTP request.
				"""
			contributors: ["jblazquez"]
		},
		{
			type: "feat"
			description: """
				Introduce a new `exclusive_route` transform, which functions as a switch statement to route events based on user-defined conditions. See the [release highlight](/highlights/2024-11-07-exclusive_route/) for more details on how to use this new transform.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				Vector now supports YAML merges in configuration files, a part of the YAML 1.1
				specification. This functionality is useful for reducing the size of transform
				configurations. See the [YAML documentation](https://yaml.org/type/merge.html).
				"""
			contributors: ["lattwood"]
		},
		{
			type: "enhancement"
			description: """
				Pipeline name is now an optional configuration item for GreptimeDB log sink.
				"""
			contributors: ["sunng87"]
		},
		{
			type: "enhancement"
			description: """
				The `dnstap` source now supports decoding of EDE code 30 (Invalid Query Type) (added in [Compact Denial of Existence in DNSSEC](https://datatracker.ietf.org/doc/draft-ietf-dnsop-compact-denial-of-existence/04/)) and has the correct `purpose` attached to it.
				"""
			contributors: ["esensar"]
		},
		{
			type: "enhancement"
			description: """
				Add `VECTOR_HOSTNAME` env variable to override the hostname used in the Vector events and internal metrics.
				This is useful when Vector is running on a system where the hostname is not meaningful, such as in a container (Kubernetes).
				"""
			contributors: ["nabokihms"]
		},
		{
			type: "feat"
			description: """
				The `elasticsearch` sink now supports publishing events as bulk "update"s by configuring `bulk.action` to `update`.
				While using this mode has a couple of constraints:

				1. The message must be added in `.doc` and have `.doc_as_upsert` to true.
				2. `id_key` must be set, and the `encoding` field should specify `doc` and `doc_as_upsert` as values
				"""
			contributors: ["blackrez"]
		},
		{
			type: "feat"
			description: """
				Introducing the first version of the [OpenTelemetry](https://opentelemetry.io/docs/what-is-opentelemetry/) sink. This initial implementation supports emitting logs as OTLP over HTTP. Support is expected to expand in the future.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				The request body of the Honeycomb sink should be encoded as an array according to [the API docs](https://docs.honeycomb.io/api/tag/Events#operation/createEvents).
				"""
			contributors: ["hgiasac"]
		},
		{
			type: "enhancement"
			description: """
				Support compression for the Honeycomb sink. The `zstd` format is enabled by default.
				"""
			contributors: ["hgiasac"]
		},
		{
			type: "enhancement"
			description: """
				Vector now supports two additional back-ends for loading secrets: `file`, for reading a set of
				secrets from a JSON file, and `directory`, for loading secrets from a list of files.
				"""
			contributors: ["tie"]
		},
		{
			type: "enhancement"
			description: """
				Add Gzip compression support to the `gcp_chronicle_unstructured` sink. See the [documentation](https://cloud.google.com/chronicle/docs/reference/ingestion-api#frequently_asked_questions).
				"""
			contributors: ["chocpanda"]
		},
		{
			type: "enhancement"
			description: """
				The `sample` transform now has a `sample_rate_key` configuration option, which default to `sample_rate`. It allows configuring which key is used to attach the sample rate to sampled events. If set to an empty string, the sample rate will not be attached to sampled events.
				"""
			contributors: ["dekelpilli"]
		},
		{
			type: "chore"
			description: """
				The global `expire_metrics_secs` configuration option now defaults to `300s` rather than being
				disabled. To preserve the old behavior, set to a negative value to disable expiration.
				"""
			contributors: ["jszwedko"]
		},
		{
			type: "fix"
			description: """
				Fix bug in implementation of Datadog search syntax which causes queries based on attributes with boolean values to be ignored.
				"""
			contributors: ["ArunPiduguDD"]
		},
		{
			type: "fix"
			description: """
				The `gelf` codec now correctly deserializes the subsecond portion of timestamps rather than dropping
				them.
				"""
			contributors: ["jszwedko"]
		},
		{
			type: "enhancement"
			description: """
				Support for watching config file changes by polling at certain interval rather than relying on notifications. This can be enabled setting `--watch-config-method` to `poll` where the interval can be configured via `--watch-config-poll-interval-seconds`.
				"""
			contributors: ["amribm"]
		},
		{
			type: "feat"
			description: """
				The `host_metrics` now supports process metrics collection, configurable via the `process` option.
				"""
			contributors: ["leeteng2001"]
		},
	]

	commits: [
		{sha: "c0375c0a37d3de9c8f71d512ee8e96a677fa9717", date: "2024-10-16 18:54:42 UTC", description: "Downgrade smp to 0.16.1", pr_number: 21522, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "334c1c04b4b039352082491e990a5cc0c47c3efb", date: "2024-10-16 19:02:53 UTC", description: "Fix link in DEPRECATIONS.md", pr_number: 21524, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "cd8039d392b23049f577a427112ff7d97e56ea4b", date: "2024-10-16 20:00:09 UTC", description: "Update minor-release template with a few tweaks", pr_number: 21526, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 7, deletions_count: 2},
		{sha: "7ebe30d2a395b067cd855e947e904c8c4d3259fc", date: "2024-10-17 03:49:33 UTC", description: "Bump openssl from 0.10.66 to 0.10.67", pr_number: 21517, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "ac0e4cacfb64fd45c72c2bdfd918cfc3da2a8b12", date: "2024-10-17 00:00:16 UTC", description: "delete unused output in the regression workflow", pr_number: 21527, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "dfaabf707b4861764ba9329dd50895d8c1f2b926", date: "2024-10-16 21:41:33 UTC", description: "Add SMP as CODEOWNERS for .github/workflows/regression.yml", pr_number: 21529, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "1309eb925dd4735bc61a8cbe20e85f69a8dbe5b7", date: "2024-10-16 23:42:56 UTC", description: "Run regression tests if workflow changed", pr_number: 21521, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c22f6bddefeef4933dd1cec0d8ca46900e02ba86", date: "2024-10-17 06:44:05 UTC", description: "Bump async-compression from 0.4.14 to 0.4.15", pr_number: 21502, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "55fd45540ebafd869853a7a29d984efd40fc4e88", date: "2024-10-17 02:47:11 UTC", description: "skip regression checks for PRs with the `ci-condition: skip-regression` label", pr_number: 21507, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 34, deletions_count: 8},
		{sha: "1e11b92a5b00d0a4169e9e396ee3991235815bf4", date: "2024-10-17 00:29:43 UTC", description: "add support for loading and joining an array of vrl files in remap", pr_number: 21497, scopes: ["transform"], type: "feat", breaking_change: false, author: "Britton Hayes", files_count: 3, insertions_count: 52, deletions_count: 16},
		{sha: "6f352ff8a350d7d8258fbb52b9ff83c2adaa3465", date: "2024-10-17 16:16:48 UTC", description: "Expose `vrl` through `vector-lib`", pr_number: 21491, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 8, deletions_count: 3},
		{sha: "ca429665187786d47df46fe1cbc584a9bd0ca59a", date: "2024-10-17 16:16:54 UTC", description: "Bump Rust version to 1.81.0", pr_number: 21509, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 50, insertions_count: 81, deletions_count: 107},
		{sha: "ad3de7eebb17533a41e96b893e13a6c838b9dd67", date: "2024-10-17 18:45:31 UTC", description: "add source changed gate in test.yml", pr_number: 21539, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "5c11d099ef3b46a32174e05cc76459292e27d158", date: "2024-10-17 22:53:14 UTC", description: "Bump libc from 0.2.159 to 0.2.160", pr_number: 21537, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "21ba31ab5b3bc400003d0cd1ee80c04ccb52887f", date: "2024-10-17 20:46:05 UTC", description: "Bump uuid from 1.10.0 to 1.11.0", pr_number: 21535, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "93f71f1b52e7d54cee0aa1a586571f601119695d", date: "2024-10-18 00:57:45 UTC", description: "revert recent changes to the regression workflow", pr_number: 21541, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 62, deletions_count: 87},
		{sha: "849aa2d0efad38558e2d56fbacd83ef2d2e0ba82", date: "2024-10-17 22:59:41 UTC", description: "Bump ordered-float from 4.3.0 to 4.4.0", pr_number: 21536, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "400a17c8aa3245512ae79c1c594034f46b4eb976", date: "2024-10-18 06:00:20 UTC", description: "Bump async-compression from 0.4.15 to 0.4.16", pr_number: 21533, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2883ca7e10a0455e93ad0b92e5a94192be3e4989", date: "2024-10-18 06:00:30 UTC", description: "Bump proc-macro2 from 1.0.87 to 1.0.88", pr_number: 21532, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 69, deletions_count: 69},
		{sha: "f2d5ea44f1be4e85ed11e2f9dd1882687d0cdc81", date: "2024-10-18 06:00:37 UTC", description: "Bump cargo-lock from 9.0.0 to 10.0.0", pr_number: 21518, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 24},
		{sha: "624fa1a0a7b21161461669156b01feb3082f8fe4", date: "2024-10-18 15:39:55 UTC", description: "Handle recent GitHub Actions deprecations", pr_number: 21528, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 2},
		{sha: "448ce648bec90e882d20eebda910347c261b5735", date: "2024-10-18 19:03:43 UTC", description: "add stratified sampling capability", pr_number: 21274, scopes: ["sample transform"], type: "enhancement", breaking_change: false, author: "Daniel Hillman", files_count: 4, insertions_count: 106, deletions_count: 6},
		{sha: "90b32f864f50c6440873613275d593423bf109ce", date: "2024-10-18 18:40:33 UTC", description: "Bump vrl from `dc0311d` to `3e1c7b0`", pr_number: 21542, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 9, deletions_count: 9},
		{sha: "dc441a28bb8dda527e79bbc3e8145bcb5ee0cadf", date: "2024-10-18 20:28:19 UTC", description: "Bump `metrics` to 0.24.0", pr_number: 21550, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 12, deletions_count: 14},
		{sha: "baf6a555051ead0890801920fe88b6f86cd972ae", date: "2024-10-22 01:29:57 UTC", description: "Add customizable connection retry options for Pulsar client in Pulsar sink", pr_number: 21245, scopes: ["components"], type: "enhancement", breaking_change: false, author: "Frank Rosner", files_count: 3, insertions_count: 118, deletions_count: 2},
		{sha: "efc9db951f144d56f2f6b0182f5659661d79a082", date: "2024-10-21 21:03:02 UTC", description: "print line numbers with trailing spaces", pr_number: 21568, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "22a51cddde5f2de950a67bc89956070b3b6cd89d", date: "2024-10-21 18:51:30 UTC", description: "Bump k8s manifests to v0.37.0 of the chart", pr_number: 21569, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "a2b152df8a27b74521df2bc437e64fcbf7978307", date: "2024-10-22 03:55:48 UTC", description: "fix unexpected timestamp field", pr_number: 21558, scopes: ["sources"], type: "fix", breaking_change: false, author: "rwa", files_count: 3, insertions_count: 9, deletions_count: 3},
		{sha: "75c5a4d5f0b7004091ee4dd1b1595d9daf64b572", date: "2024-10-21 22:36:33 UTC", description: "update actions/checkout to v4", pr_number: 21571, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 21, insertions_count: 64, deletions_count: 64},
		{sha: "a8c0b6ee596999b28fb0bb136fdc55dce7eb361d", date: "2024-10-22 05:40:18 UTC", description: "support loading secrets from files and directories", pr_number: 21282, scopes: ["config"], type: "feat", breaking_change: false, author: "Ivan Trubach", files_count: 9, insertions_count: 191, deletions_count: 0},
		{sha: "dd39ab35290316e34cbeebba8e1b26d455bc9d0a", date: "2024-10-21 23:32:43 UTC", description: "upgrade `typesense-sync` package", pr_number: 21572, scopes: [], type: "chore", breaking_change: false, author: "Brian Deutsch", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "eb699ef64dd40c3c2ce4d7366bae44e44b68b1f8", date: "2024-10-22 04:46:32 UTC", description: "Bump serde_json from 1.0.128 to 1.0.132", pr_number: 21565, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "75d59e957a716622fec2cb5590b5d71cbb659a52", date: "2024-10-22 01:11:51 UTC", description: "Properly hand off the `proptest` feature to `vrl`", pr_number: 21574, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "5d8736f1a3ec40165f027138bd8913eb94df59cf", date: "2024-10-22 16:20:47 UTC", description: "rework Semantic check workflow", pr_number: 14723, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 238, deletions_count: 253},
		{sha: "4a44e41ac1929408a4142c63b6f10e17812be50e", date: "2024-10-22 18:54:56 UTC", description: "Prevent workflow from triggering unless previous workflow succeeds", pr_number: 21579, scopes: ["gh_action"], type: "fix", breaking_change: false, author: "Devin Ford", files_count: 2, insertions_count: 4, deletions_count: 2},
		{sha: "2c24f7e41a39363b23ac33a9a7108e3d0034e31e", date: "2024-10-22 19:02:27 UTC", description: "Bump bytes from 1.7.2 to 1.8.0", pr_number: 21576, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 96, deletions_count: 94},
		{sha: "25fe9b897b4076cd1df3adc630518cd8903328f6", date: "2024-10-22 22:35:18 UTC", description: "fix ignore filter", pr_number: 21580, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5d517b9f75c38a33670f5ecb1c6374cd9d0ac9e3", date: "2024-10-23 01:23:41 UTC", description: "Bump syn from 2.0.79 to 2.0.82", pr_number: 21560, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 50, deletions_count: 50},
		{sha: "86c313f7fecff3c7300296f70f7667398e145eed", date: "2024-10-22 20:38:56 UTC", description: "Wrap event metadata in `Arc` for performance", pr_number: 21188, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 71, deletions_count: 54},
		{sha: "21ae5d007d693e74e539cc584fdc41cd72c83437", date: "2024-10-22 23:50:57 UTC", description: "set regression workflow timeouts to 70 mins", pr_number: 21582, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "8c4e27651f554db8c543bc29d184d03a7e5b69e2", date: "2024-10-23 02:19:17 UTC", description: "regression detection overhaul", pr_number: 21567, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 703, deletions_count: 0},
		{sha: "461714564471189788e77eb9ebb63acd3a9fdb0a", date: "2024-10-23 21:57:02 UTC", description: "regression detection v2 followups", pr_number: 21594, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 12, deletions_count: 10},
		{sha: "776f4c62907c5cbe49244b250592d2c732604e09", date: "2024-10-23 19:18:19 UTC", description: "Group together patch updates with dependabot", pr_number: 21595, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 7, deletions_count: 0},
		{sha: "7b74ea857df75a462484b83120306208d43cbc9c", date: "2024-10-23 23:39:53 UTC", description: "Bump tokio from 1.40.0 to 1.41.0", pr_number: 21589, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 15, deletions_count: 15},
		{sha: "0078939e0ba08707d85511714c62cc9b643538d3", date: "2024-10-23 23:58:22 UTC", description: "Bump vrl from `3e1c7b0` to `d030427`", pr_number: 21601, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "f890a5028be00593d5ba59aebd06f0aa4d16617a", date: "2024-10-24 10:57:54 UTC", description: "Bump the patches group across 1 directory with 10 updates", pr_number: 21603, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 155, deletions_count: 154},
		{sha: "31732b0ff1331fa112cb3530547b76a5645dfd15", date: "2024-10-24 18:01:06 UTC", description: "replace regression workflow", pr_number: 21600, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 152, deletions_count: 949},
		{sha: "e2166a1cb27fd14f056ea162f942cb9b4d119ab1", date: "2024-10-24 19:48:38 UTC", description: "Bump vrl from `d030427` to `a07648f`", pr_number: 21608, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "f8897c8f6c6d433a93bbd2123316ea3832aef0c3", date: "2024-10-24 20:32:08 UTC", description: "add SMP as code owners for the \"regression/\" dir tree", pr_number: 21597, scopes: ["administration"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "35fdfaff5c04896edc2df492deef1b1978d68a26", date: "2024-10-25 00:40:35 UTC", description: "Bump clap_complete from 4.5.33 to 4.5.35 in the patches group", pr_number: 21606, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "099ca7f02654bd0b5ab95bef35e97f059961076b", date: "2024-10-24 21:02:24 UTC", description: "update support and community docs", pr_number: 21612, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 30, deletions_count: 24},
		{sha: "4d1f3fde4a2ec42165e3ffd062fd28e3ead83b16", date: "2024-10-24 18:56:06 UTC", description: "fix MetricsKind typo", pr_number: 21573, scopes: ["core"], type: "docs", breaking_change: false, author: "Nicholas Ionata", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "57837b96cc3f063a482111373e6921462828f75c", date: "2024-10-28 18:32:45 UTC", description: "improve vrl.dev tutorial", pr_number: 21619, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 51, deletions_count: 28},
		{sha: "b05331e06eab11538fec8eb1c283a05e1b50a67b", date: "2024-10-28 15:01:37 UTC", description: "Fix gelf deserialization of subsecond timestamps", pr_number: 21613, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 13, deletions_count: 10},
		{sha: "9b540e8adbbf6f492a58a2fd37449ce684f9a966", date: "2024-10-28 22:05:29 UTC", description: "Bump the patches group across 1 directory with 8 updates", pr_number: 21629, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 35, deletions_count: 35},
		{sha: "53e7df2d9c2058e5fd655e2375a671d77e0957f5", date: "2024-10-28 18:29:28 UTC", description: "format markdown tables", pr_number: 21498, scopes: [], type: "chore", breaking_change: false, author: "DemoYeti", files_count: 1, insertions_count: 30, deletions_count: 30},
		{sha: "01a562a71f7a7412e5812da3d65862f3b788fd99", date: "2024-10-28 18:51:17 UTC", description: "bump ts package", pr_number: 21620, scopes: ["javascript website"], type: "chore", breaking_change: false, author: "Brian Deutsch", files_count: 3, insertions_count: 14, deletions_count: 9},
		{sha: "4bb1f99cd72a7fae84a14735eaa5fb97ba38dae5", date: "2024-10-28 18:51:53 UTC", description: "additional scopes", pr_number: 21621, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "241b2f1b6dd5a5a0c16a714696ae3bc4722815d7", date: "2024-10-28 23:07:12 UTC", description: "support scraping metadata from Kubernetes log files on Windows", pr_number: 21505, scopes: ["kubernetes_logs source"], type: "enhancement", breaking_change: false, author: "Damien Churchill", files_count: 3, insertions_count: 88, deletions_count: 28},
		{sha: "b242e4aacc4bde40c41f3564e12781646f67a3a5", date: "2024-10-28 19:07:22 UTC", description: "Clarify SAS connection_string support ", pr_number: 21611, scopes: ["azure_blob sink"], type: "docs", breaking_change: false, author: "Marc Sensenich", files_count: 2, insertions_count: 28, deletions_count: 3},
		{sha: "42e2bb6e446fead5ffdb9a4c12c7f027528536b6", date: "2024-10-28 19:46:33 UTC", description: "add CI flag and use cue.sh", pr_number: 21634, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 2},
		{sha: "29d58c919acfefc53f19a08ae4eab9c0e4da46d9", date: "2024-10-28 19:09:10 UTC", description: "Drop use of `infer` crate", pr_number: 21623, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 42, deletions_count: 23},
		{sha: "d90a95cb1919fdc870e45c17811c12877478e314", date: "2024-10-28 19:49:16 UTC", description: "Fix example configuration", pr_number: 21636, scopes: ["static_metrics source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 1},
		{sha: "4c3aa3537724f746d1d712c6bff75efd0d8fe0a9", date: "2024-10-28 23:24:51 UTC", description: "Fix matching boolean values for Datadog search s…", pr_number: 21624, scopes: ["transforms"], type: "fix", breaking_change: false, author: "ArunPiduguDD", files_count: 2, insertions_count: 23, deletions_count: 5},
		{sha: "cb177a2da05bc1940d7b8ebea81566d2893616b0", date: "2024-10-29 17:19:59 UTC", description: "Bump databend-client from 0.21.1 to 0.22.2", pr_number: 21645, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1fe415263b5fc779a331730c7ca3684f47bb129a", date: "2024-10-29 17:24:21 UTC", description: "remove ubuntu 16 from publishing targets", pr_number: 21639, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 8, deletions_count: 9},
		{sha: "98e72c82383a736aa0a538b5b0e7af4fd32d8953", date: "2024-10-29 23:30:44 UTC", description: "Bump notify from 6.1.1 to 7.0.0", pr_number: 21646, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 18, deletions_count: 7},
		{sha: "a91956819d78d166efb43ae4abad597b741d051d", date: "2024-10-29 16:42:54 UTC", description: "remove unnecessary string hashes", pr_number: 21559, scopes: [], type: "chore", breaking_change: false, author: "Hamir Mahal", files_count: 25, insertions_count: 123, deletions_count: 127},
		{sha: "83a71cdaebebe397ec3bf942f921b6747eebc12f", date: "2024-10-30 02:35:46 UTC", description: "fixed numerous API tests issues ", pr_number: 21650, scopes: ["tests"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 269, deletions_count: 666},
		{sha: "a59f59cdda2852d38758460b52af88e3c8cf7368", date: "2024-10-30 04:47:59 UTC", description: "improve GitHub PR template", pr_number: 21651, scopes: ["administration"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 37, deletions_count: 3},
		{sha: "456e98e34393bceb12f986ee3c0fc9affac65020", date: "2024-10-30 21:24:41 UTC", description: "improve `route` transform docs", pr_number: 21635, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 126, deletions_count: 16},
		{sha: "f72213580b48358add79f37a9b5f2b3ef5b511c9", date: "2024-10-31 01:32:32 UTC", description: "Bump the artifact group with 2 updates", pr_number: 20055, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 65, deletions_count: 65},
		{sha: "3b35d1ac9f269f4f0ce96a8d227d7214d140c1b6", date: "2024-10-30 23:31:43 UTC", description: "tweak PR template", pr_number: 21660, scopes: ["administration"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "21e1b8247ae7c8a17589f61d5859e76ae26ead10", date: "2024-10-30 20:50:28 UTC", description: "Update docs for new AES-SIV encryption/decryption types available in latest VRL.", pr_number: 21659, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Justin Hawkins", files_count: 3, insertions_count: 5, deletions_count: 0},
		{sha: "c20d358805ef86fbaee4c92bfcdf0f4060b08e9f", date: "2024-10-31 00:36:44 UTC", description: "tiny improvement to changelog README", pr_number: 21663, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "f2f38bc09554946a895134feb85fea5636c8a87b", date: "2024-10-31 18:45:28 UTC", description: "deb-verify cannot use actions/checkout@v4", pr_number: 21658, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 11, deletions_count: 10},
		{sha: "08bf15a51a6f54741140de164b06ae0219b9a4b7", date: "2024-11-01 10:39:58 UTC", description: "add sample_rate_key config option", pr_number: 21283, scopes: ["sample transform"], type: "feat", breaking_change: false, author: "Dekel Pilli", files_count: 4, insertions_count: 85, deletions_count: 20},
		{sha: "c55741b8e64431ced32277f4196f158e643012fb", date: "2024-10-31 18:17:21 UTC", description: "Bump bufbuild/buf-setup-action from 1.45.0 to 1.46.0", pr_number: 21661, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "93e4b7828041bcd698088710c9278cd5e692b7cf", date: "2024-10-31 18:17:31 UTC", description: "Bump the patches group across 1 directory with 2 updates", pr_number: 21656, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "d0252e2163a26420d248ac8ec27f377c80136463", date: "2024-11-01 02:04:52 UTC", description: "Bump vrl from `a07648f` to `c8c382b`", pr_number: 21637, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 44, deletions_count: 16},
		{sha: "9a0fe478f92537626a1e5d5f588bccd3d15b79d7", date: "2024-11-01 00:16:53 UTC", description: "Revert to actions/[upload,download]-artifact@v3", pr_number: 21668, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 51, deletions_count: 51},
		{sha: "b22b732b584fb6636b1d891b8db4af621ce4a94e", date: "2024-10-31 22:25:30 UTC", description: "Update VRL to c69a52e67 and mlua to 0.10.0", pr_number: 21670, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 12, insertions_count: 132, deletions_count: 90},
		{sha: "45b5224f3fe18683d109061edcf3760e879b80e2", date: "2024-11-01 06:08:00 UTC", description: "Bump ratatui from 0.27.0 to 0.28.1", pr_number: 21501, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 24, deletions_count: 40},
		{sha: "4c03f6c254a92a19f4d998cfac4e2affa6e9ba38", date: "2024-11-01 15:47:14 UTC", description: "Bump governor from 0.6.3 to 0.7.0", pr_number: 21577, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 7},
		{sha: "3c26a506d70d4ed576f9acf520efc37c54bb6634", date: "2024-11-01 15:59:50 UTC", description: "Add configuration for aqua to manage tooling dependencies", pr_number: 18020, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 54, deletions_count: 0},
		{sha: "7985d4589ad3e9fdd5a290e30f4524ed1e1f9281", date: "2024-11-01 23:19:12 UTC", description: "Bump twox-hash from 1.6.3 to 2.0.0", pr_number: 21566, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 8},
		{sha: "710f2245f9098d09afbdf7d2f8f2102938f66821", date: "2024-11-01 21:02:01 UTC", description: "component feature check fix", pr_number: 21677, scopes: ["sample transform"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 325, deletions_count: 340},
		{sha: "e044856e3073980262904ccc0a80d0d17ab0a3cc", date: "2024-11-01 18:38:00 UTC", description: "Bump the patches group with 2 updates", pr_number: 21675, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 55, deletions_count: 55},
		{sha: "e0b804996a7d899962ce6cc8df4d9e4861f1f77f", date: "2024-11-02 02:16:56 UTC", description: "Added proxy option to Vector sink", pr_number: 21609, scopes: ["vector sink"], type: "docs", breaking_change: false, author: "Jonathan Davies", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "ab679fbfe137da7544873662637ec0404b60d0dd", date: "2024-11-01 19:19:44 UTC", description: "Default `expire_internal_metrics` to 300s", pr_number: 20710, scopes: ["config"], type: "chore", breaking_change: true, author: "Jesse Szwedko", files_count: 4, insertions_count: 46, deletions_count: 24},
		{sha: "b1e93ee6dd103ed423de098e53a30ab7bbee997d", date: "2024-11-01 22:41:27 UTC", description: "update Cargo.lock", pr_number: 21679, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "21d9e50f82bac249247743906ca9bb7fbfead90d", date: "2024-11-01 19:55:32 UTC", description: "Fix scraping of target metrics by lading", pr_number: 21530, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 27, insertions_count: 27, deletions_count: 27},
		{sha: "431baac2ec3ccf472e86e79be2a3ed82f3d6ce4a", date: "2024-11-01 21:17:45 UTC", description: "Limit spellchecker to user-facing content", pr_number: 21680, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 29, deletions_count: 807},
		{sha: "b5de16c0c12d1c2270f54dd2d6aeef1ef3bdd25b", date: "2024-11-01 23:32:45 UTC", description: "Remove environment stage name", pr_number: 21681, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c7441ec082948e633e1a898e319d26ef19d74424", date: "2024-11-02 02:26:01 UTC", description: "Use free OSS 4 vCPU runners", pr_number: 19683, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 7, insertions_count: 135, deletions_count: 8},
		{sha: "0660ad5d44af116724a8e56f4b13bc54a7737cc1", date: "2024-11-04 19:29:44 UTC", description: "Bump proptest-derive from 0.4.0 to 0.5.0", pr_number: 21690, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 15},
		{sha: "9d74d12274abd16a90d04cf3a7308675c1c3d3ac", date: "2024-11-04 22:58:32 UTC", description: "update community page", pr_number: 21693, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 20, deletions_count: 5},
		{sha: "a8e82dd71a38065d32db872e489e5dc4edb6dbc3", date: "2024-11-05 04:38:31 UTC", description: "Bump the patches group across 1 directory with 6 updates", pr_number: 21692, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 373, deletions_count: 73},
		{sha: "da0680c01597c03eb9c2306a731138843ddefa2a", date: "2024-11-05 21:57:01 UTC", description: "Bump ratatui from 0.28.1 to 0.29.0", pr_number: 21701, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 22, deletions_count: 16},
		{sha: "872d0b5d4ac1cb591341da94a4acf19145ecbeae", date: "2024-11-05 19:05:12 UTC", description: "add support for enriching logs with HTTP headers", pr_number: 21674, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Javier Blazquez", files_count: 13, insertions_count: 401, deletions_count: 69},
		{sha: "68c2a19b99fd4b1f297a022c781ecc5a8db3ff01", date: "2024-11-06 02:08:04 UTC", description: "Bump the patches group across 1 directory with 3 updates", pr_number: 21706, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 10},
		{sha: "c6357a1cf4bf88aaf361ee9c3df4f98cb2507594", date: "2024-11-07 00:43:57 UTC", description: "document how to update licenses", pr_number: 21719, scopes: ["dev"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 12, deletions_count: 0},
		{sha: "ac19c7cc05d85aeb615bd10f2a58b080b013b250", date: "2024-11-06 22:49:59 UTC", description: "Clear space at the start of the k8s e2e workflow", pr_number: 21723, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "ec54dccdcc147c5e9e06e692b144e5c7b3a4cd70", date: "2024-11-07 07:01:50 UTC", description: "Bump ordered-float from 4.4.0 to 4.5.0", pr_number: 21700, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "02a756efc012b065b316fbd6ab30e1a6b5a71b91", date: "2024-11-07 07:06:51 UTC", description: "Bump check-spelling/check-spelling from 0.0.21 to 0.0.24", pr_number: 21678, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e3cd14335fc80c87a25f525a130c697469d63822", date: "2024-11-07 14:29:53 UTC", description: "Retrying the HTTP sink in case of 404s and request timeouts", pr_number: 21457, scopes: ["http sink"], type: "enhancement", breaking_change: false, author: "Noble Varghese", files_count: 3, insertions_count: 8, deletions_count: 2},
		{sha: "a17312e0c4acd3cea09847a4d5e2a1b75cde3a02", date: "2024-11-07 04:00:10 UTC", description: "update all remaining GitHub actions v3 packages to v4", pr_number: 21722, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 55, deletions_count: 64},
		{sha: "fb53a2d2d1b5d8771011fe36f1027a99800e8882", date: "2024-11-07 04:00:17 UTC", description: "make the `TELEMETRY` singleton thread local for tests", pr_number: 21697, scopes: ["tests"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 73, deletions_count: 25},
		{sha: "e2b83f21949d8744a67d3e4ec39c0e1389169672", date: "2024-11-07 04:00:28 UTC", description: "log component graph when there's an error ", pr_number: 21669, scopes: ["config"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 70, deletions_count: 11},
		{sha: "07e2eae75ad6c6f9c75e5be99a23c1c52ac1e3fc", date: "2024-11-08 04:31:03 UTC", description: "Implement chunked GELF decoding", pr_number: 20859, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Jorge Hermo", files_count: 31, insertions_count: 1992, deletions_count: 95},
		{sha: "c394bfc4fc614e4bcf8ff991de526dbcd13bb157", date: "2024-11-08 04:32:49 UTC", description: "update datadog-signing-keys recommended version", pr_number: 21712, scopes: ["dev"], type: "chore", breaking_change: false, author: "Hugo Beauzée-Luyssen", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0527544711c68f673b46cad3168332b8bec2684d", date: "2024-11-07 22:04:14 UTC", description: "Update check-spelling to 0.0.22", pr_number: 21733, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 131, deletions_count: 58},
		{sha: "ffa06ad376cd6fe2657d083df01ff4ccc840e8f6", date: "2024-11-07 23:00:22 UTC", description: "Update CONTRIBUTING.md", pr_number: 21730, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "bdfd14661e82cffc3ed6efafe0aa379f14e0d891", date: "2024-11-07 22:22:01 UTC", description: "Upgrade check-spelling to 0.0.23", pr_number: 21736, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "a89b6b2cf763fa7520887b00b299a020b3fd75ef", date: "2024-11-08 00:27:41 UTC", description: "Upgrade check-spelling to 0.0.24", pr_number: 21737, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "ac1e975d109e4ab575e2acb2f3766aa8084c8ea3", date: "2024-11-09 01:22:06 UTC", description: "Add CEF encoder", pr_number: 17389, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Maksim Nabokikh", files_count: 34, insertions_count: 2824, deletions_count: 10},
		{sha: "fb9c5c1bb21507b9e79f465463d6a9a12c8be195", date: "2024-11-09 03:23:20 UTC", description: "Add automatic bearer token acquisition in http-client [version 2]", pr_number: 21583, scopes: ["http provider"], type: "enhancement", breaking_change: false, author: "Bartek Kowalczyk", files_count: 9, insertions_count: 1120, deletions_count: 22},
		{sha: "246576bf58f4b3a592a9aa8944b1a3e9baf36a62", date: "2024-11-09 04:50:15 UTC", description: "add EDE code 30 to known EDE codes", pr_number: 21743, scopes: ["dnstap source"], type: "enhancement", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 4, deletions_count: 0},
		{sha: "85e2f547580b06bbb306da84de8bc8d07f651b44", date: "2024-11-08 19:55:14 UTC", description: "Update wrapped_json.yaml example", pr_number: 21746, scopes: ["config"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "530d02df6813c0353ef74dc32d49967e5655a1c3", date: "2024-11-08 23:21:17 UTC", description: "update cargo lock", pr_number: 21744, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0827ed9f3344da51ee322ec833f68bf187549f7d", date: "2024-11-09 00:43:01 UTC", description: "fix route config test", pr_number: 21745, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 6},
		{sha: "6d1d52116088dd41313e113aabb60ad3347ca96a", date: "2024-11-09 01:27:12 UTC", description: "Bump the patches group across 1 directory with 3 updates", pr_number: 21738, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 13, insertions_count: 24, deletions_count: 24},
		{sha: "f4eddbcdca671f6843ecafb4ec05e48bd899c51d", date: "2024-11-11 20:48:58 UTC", description: "Ignore RUSTSEC-2024-0336", pr_number: 21758, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "7fb5dad7752893929f55144d796f468ad2f1a12a", date: "2024-11-11 18:57:21 UTC", description: "Support Amazon OpenSearch Serverless", pr_number: 21676, scopes: ["elasticsearch sink"], type: "feat", breaking_change: false, author: "Michael Handler", files_count: 9, insertions_count: 158, deletions_count: 28},
		{sha: "b3c8dc88fcc43dcf14cab53167f6e005d07172f8", date: "2024-11-12 08:57:33 UTC", description: "add support for poll watcher", pr_number: 21290, scopes: ["cli"], type: "feat", breaking_change: false, author: "Ameer Ibrahim", files_count: 6, insertions_count: 177, deletions_count: 31},
		{sha: "f799d49aff1b8ebabd4e58beb0d760eef9405aff", date: "2024-11-11 23:03:37 UTC", description: "include kafka source in `librdkafka` section", pr_number: 21760, scopes: ["kafka source", "kafka sink"], type: "docs", breaking_change: false, author: "Tess Neau", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c2abea9b7875257017ede863fb26ffb672116514", date: "2024-11-12 01:25:33 UTC", description: "Upgrade cargo-deny to latest", pr_number: 21761, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 11, deletions_count: 6},
		{sha: "a7acdfe19c7b913accb03a75260c616b841fe72a", date: "2024-11-12 19:50:44 UTC", description: "Gate additional parameters in elasticsearch sink", pr_number: 21768, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 16, deletions_count: 5},
		{sha: "aa35e35ad9a7d578a130ac231a6f6b178d85fe3b", date: "2024-11-13 02:49:16 UTC", description: "document punycode as an IDN function", pr_number: 21734, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "43d47bf0fa19c4ba8c7219bad5a21d7f1929c76a", date: "2024-11-13 02:05:01 UTC", description: "Bump the patches group across 1 directory with 3 updates", pr_number: 21764, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 13, deletions_count: 13},
		{sha: "b1e8f46ace516829ff6d95457b42449f5ecdbbd4", date: "2024-11-12 21:56:29 UTC", description: "timestamp clarification", pr_number: 21769, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "473ef43a881cd65add0850993129f1bce3b592a7", date: "2024-11-13 04:03:25 UTC", description: "bump vrl", pr_number: 21774, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 16},
		{sha: "954798dfd1841411d5055b1aec9a2207e9847305", date: "2024-11-13 20:18:20 UTC", description: "Bump tempfile from 3.13.0 to 3.14.0", pr_number: 21781, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 19, deletions_count: 19},
		{sha: "6ac72ba6a6de9ef4dab33dc2b00301cf4bf89986", date: "2024-11-13 20:24:27 UTC", description: "Add fallback key_prefix for object storage destinations", pr_number: 21770, scopes: ["internal"], type: "feat", breaking_change: false, author: "ArunPiduguDD", files_count: 6, insertions_count: 61, deletions_count: 23},
		{sha: "cac47ff4b15b16b7ed5baa92fe3f70861478ae0c", date: "2024-11-13 20:43:10 UTC", description: "Remove vestigial commit status steps from regression workflow", pr_number: 21784, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 149},
		{sha: "ca05055a6a55abf1a201b51d9418e7262e6005ec", date: "2024-11-13 21:17:36 UTC", description: "fix publish workflow permissions", pr_number: 21773, scopes: ["ci"], type: "fix", breaking_change: false, author: "Bryce Thuilot", files_count: 4, insertions_count: 16, deletions_count: 0},
		{sha: "9fa3516eb21b5404d447bd434e2bfed6fa1b555f", date: "2024-11-14 02:14:19 UTC", description: "Add statuses write permission to workflows", pr_number: 21788, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 13, insertions_count: 39, deletions_count: 0},
		{sha: "b32ea2f820833874623f4163d13aced7195da97c", date: "2024-11-14 04:50:08 UTC", description: "remove 'unset HOMEBREW_NO_INSTALL_FROM_API' line", pr_number: 21787, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "3463c9fad44b0ff35d2336cbbf16a0022f7883b8", date: "2024-11-14 18:00:41 UTC", description: "Bump the patches group across 1 directory with 3 updates", pr_number: 21790, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 23, deletions_count: 13},
		{sha: "9ae24976d0c010396c0c243accd8a2c346dfac42", date: "2024-11-14 23:15:33 UTC", description: "delete transforms-pipelines feature", pr_number: 21796, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 0, deletions_count: 4},
		{sha: "dadf5e4d0b4a60c43333386233a761966bcaa9be", date: "2024-11-14 23:54:52 UTC", description: "Bump bufbuild/buf-setup-action from 1.46.0 to 1.47.2", pr_number: 21797, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "98b63cf75e52b02e5d6fb3bc8b7c65831c5cda8f", date: "2024-11-15 00:03:27 UTC", description: "implements new transform", pr_number: 21707, scopes: ["exclusive_route transform"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 16, insertions_count: 574, deletions_count: 11},
		{sha: "9f243aade7f631b564164d9ec7e68d6f79ed23b1", date: "2024-11-15 00:19:37 UTC", description: "make snafu a workspace dependency", pr_number: 21798, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 12, insertions_count: 19, deletions_count: 22},
		{sha: "b36080fb384bf7aa2dfc3ef12e4e0238f98371a8", date: "2024-11-15 18:57:15 UTC", description: "Bump vrl from `d5fb838` to `ac57e23`", pr_number: 21809, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "7355e9ad086577913c3c92f792f5f15586fa6b1e", date: "2024-11-15 20:15:19 UTC", description: "Bump bstr from 1.10.0 to 1.11.0", pr_number: 21807, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "911156972f8d01ed030e5e4acc3eeebbfa396b80", date: "2024-11-15 21:43:05 UTC", description: "Bump flate2 from 1.0.34 to 1.0.35 in the patches group", pr_number: 21800, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9e973857738b5e3bce1fc04d0b5dd9090c4afe63", date: "2024-11-16 00:55:41 UTC", description: "Document limitation of log namespacing with disk buffers", pr_number: 21813, scopes: ["config"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "983a9937ac5fc24db0203eec0390550351ec2cbe", date: "2024-11-16 02:49:44 UTC", description: "Note transparent decompression", pr_number: 21814, scopes: ["azure_blob sink", "gcp_cloud_storage sink"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 18, deletions_count: 0},
		{sha: "9613dfd6828561bfb747c292e42fb1adfc7d8560", date: "2024-11-18 17:52:26 UTC", description: "Bump openssl-src from 300.3.2+3.3.2 to 300.4.1+3.4.0", pr_number: 21819, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "e90df6b75ef9d27a0c7341da18fadba1787db1c7", date: "2024-11-18 21:02:59 UTC", description: "publish to docker retry", pr_number: 21825, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 2},
		{sha: "a192aa7dae1fdfc51c9787cb0aa39fcda87ef945", date: "2024-11-19 10:33:30 UTC", description: "provide a default value for greptimedb logs pipeline_name", pr_number: 21739, scopes: ["greptimedb_logs sink"], type: "enhancement", breaking_change: false, author: "Ning Sun", files_count: 6, insertions_count: 59, deletions_count: 4},
		{sha: "d187ba859e23cd32cc5f8dd81abbbea40e2bfd86", date: "2024-11-19 03:16:24 UTC", description: "Bump the patches group with 3 updates", pr_number: 21817, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 9, deletions_count: 9},
		{sha: "3a45992c23a51ffd7329df87d3ab418062e803ba", date: "2024-11-19 02:42:11 UTC", description: "delete 'configurable_package_name_hack' and refactor", pr_number: 21826, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 16, insertions_count: 105, deletions_count: 152},
		{sha: "cb386ca369873dca803d1b6327669643bed79050", date: "2024-11-19 08:44:18 UTC", description: "Enable support for YAML merge in configuration", pr_number: 21731, scopes: ["config"], type: "fix", breaking_change: false, author: "Logan Attwood", files_count: 2, insertions_count: 26, deletions_count: 4},
		{sha: "572e8f491056ab35691f204de22040c5fb0e51ff", date: "2024-11-19 20:41:56 UTC", description: "Bump vrl from `ac57e23` to `03cac27`", pr_number: 21828, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "2864d5446a0b339f2e523424ea28d77a562785e1", date: "2024-11-20 20:33:37 UTC", description: "Bump the patches group with 2 updates", pr_number: 21838, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 11, deletions_count: 11},
		{sha: "6cf6df5af282514cd48b534e027faf2c5c69b0fe", date: "2024-11-20 22:32:12 UTC", description: "delete unusued deps from ubuntu bootstrap", pr_number: 21841, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 3, deletions_count: 26},
		{sha: "fb9b42d34af91d7d56a292c135993556e2a5b088", date: "2024-11-20 22:38:33 UTC", description: "add issues write perms to build_preview_sites.yml", pr_number: 21845, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "798f30005b436abdf0d366b8856771f055b50dea", date: "2024-11-21 10:41:07 UTC", description: "Implement process collection for host metrics", pr_number: 21791, scopes: ["host_metrics source"], type: "feat", breaking_change: false, author: "LeeTeng2001", files_count: 8, insertions_count: 251, deletions_count: 6},
		{sha: "8ef8925770c61474d4bc9a88a9c27868d95a7bbc", date: "2024-11-20 22:58:17 UTC", description: "Bump vrl from `03cac27` to `a958c5d`", pr_number: 21839, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "6253e0638f4083b12adb8214ccb559c2f7e15e37", date: "2024-11-21 00:49:20 UTC", description: "add more perms for website preview workflows", pr_number: 21849, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 0},
		{sha: "0a65b569b6f1fb9676305833f2835085a3c0b07d", date: "2024-11-21 00:49:28 UTC", description: "remove invalid link", pr_number: 21848, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "fdecc6f969069b052004435e8b073223d9ef81a6", date: "2024-11-21 06:22:34 UTC", description: "Bump docker/metadata-action from 5.5.1 to 5.6.1", pr_number: 21842, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f5bbdb9e1e1a9072e930426729464cd6d51e84ff", date: "2024-11-21 03:50:35 UTC", description: "typo in permissions", pr_number: 21856, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "3d7f06dca693517900610a9533b61f848179c101", date: "2024-11-21 22:19:17 UTC", description: "Bump the patches group with 2 updates", pr_number: 21858, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 139, deletions_count: 139},
		{sha: "1f02dfb081327382da6f4ad9211605fbea504dfe", date: "2024-11-22 08:44:35 UTC", description: "Add VECTOR_HOSTNAME env variable", pr_number: 21789, scopes: ["internal"], type: "feat", breaking_change: false, author: "Maksim Nabokikh", files_count: 3, insertions_count: 26, deletions_count: 1},
		{sha: "6184dad1c9857b5fd959ae22ce9e8d106775ac8d", date: "2024-11-22 20:21:52 UTC", description: "Bump prettydiff from 0.7.0 to 0.8.0", pr_number: 21870, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 3},
		{sha: "0aad6230309ddde765164061ccba51b879cdb7f9", date: "2024-11-22 20:22:18 UTC", description: "Bump the patches group with 2 updates", pr_number: 21869, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 137, deletions_count: 137},
		{sha: "74f605b46b98c594453fbca6fbaa11092d3e3a5b", date: "2024-11-23 01:31:39 UTC", description: "Bump clap-verbosity-flag from 2.2.3 to 3.0.0 in the clap group", pr_number: 21859, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fd9166b8db11a85fec2d5000eb9f7b537f91d18d", date: "2024-11-23 07:56:06 UTC", description: "Add update support to bulk action", pr_number: 21860, scopes: ["elasticsearch sink"], type: "enhancement", breaking_change: false, author: "Nabil Servais", files_count: 6, insertions_count: 17, deletions_count: 2},
		{sha: "5c7db961cf6aecbf70a188810d3d26d19c2fb805", date: "2024-11-26 03:10:14 UTC", description: "chunked gelf decoder decompression support", pr_number: 21816, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Jorge Hermo", files_count: 29, insertions_count: 760, deletions_count: 50},
		{sha: "a7204fa549bf2501cface048f0abbbc8e5b2cb1c", date: "2024-11-26 12:09:21 UTC", description: "The batch body should be encoded as array", pr_number: 21878, scopes: ["honeycomb sink"], type: "fix", breaking_change: false, author: "Toan Nguyen", files_count: 2, insertions_count: 9, deletions_count: 6},
		{sha: "e0d3fb30ec5dc2a4e10db20df5d2110a725712a5", date: "2024-11-26 03:44:32 UTC", description: "revert 21583", pr_number: 21885, scopes: ["http sink"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 22, deletions_count: 1120},
		{sha: "58a4055f8b7a4a90b239eead3df40dc896a1bb2a", date: "2024-11-27 09:55:03 UTC", description: "Support compression", pr_number: 21889, scopes: ["honeycomb sink"], type: "feat", breaking_change: false, author: "Toan Nguyen", files_count: 6, insertions_count: 56, deletions_count: 3},
		{sha: "3e995042dcaeec60dbddfe16acbc63062162e5d9", date: "2024-11-27 03:40:06 UTC", description: "Bump docker/build-push-action from 6.9.0 to 6.10.0", pr_number: 21894, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "69736cdc054e6ac09e0227107669e5c659e93d0f", date: "2024-11-27 01:51:07 UTC", description: "drop ubuntu 23.04, use 24.04(LTS)", pr_number: 21897, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2e606e6512fad8b595f93419c9ad9d345eea72c5", date: "2024-11-27 03:08:59 UTC", description: "new sink", pr_number: 21866, scopes: ["opentelemetry sink"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 11, insertions_count: 1187, deletions_count: 6},
		{sha: "daf08db13ffc91e831baa316f94aecc3c1226467", date: "2024-11-27 19:27:31 UTC", description: "Bump the patches group across 1 directory with 6 updates", pr_number: 21898, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 20, deletions_count: 20},
		{sha: "4251fe0fb842f72c5460910ec605b8aac54a0231", date: "2024-11-27 21:57:16 UTC", description: "add missing dep", pr_number: 21900, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "064406dd0cac0f9efa866e1c3536da3e93d7b692", date: "2024-11-27 22:49:18 UTC", description: "mark set_semantic_meaning impure", pr_number: 21896, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Steven Fackler", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c3d91af5288cc4d10806f7724bf7ba0401ba2b7e", date: "2024-11-28 01:59:40 UTC", description: "remove incorrect failure reason for parse_regex_all fallibility", pr_number: 21884, scopes: ["external"], type: "docs", breaking_change: false, author: "Mike Del Tito", files_count: 1, insertions_count: 1, deletions_count: 3},
	]
}
