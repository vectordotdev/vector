package metadata

releases: "0.42.0": {
	date:     "2024-10-21"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.42.0!

		This release contains a number of enhancements and fixes as listed below.

		There are no breaking changes or deprecations with this release and so no upgrade guide.
		"""

	changelog: [
		{
			type: "feat"
			description: """
				VRL was updated to v0.19.0. This includes the following changes:

				### Breaking Changes & Upgrade Guide

				- The multi-line mode of the `parse_groks` VRL function is now enabled by default.
				Use the `(?-m)` modifier to disable this behavior. (https://github.com/vectordotdev/vrl/pull/1022)

				### Enhancements

				- The `keyvalue` grok filter is extended to match the Datadog implementation. (https://github.com/vectordotdev/vrl/pull/1015)

				### Fixes

				- The `parse_xml` function no longer adds an unnecessary `text` key when processing single nodes. (https://github.com/vectordotdev/vrl/pull/849)
				- `parse_grok` and `parse_groks` no longer require field names containing a hyphen (for example, `@a-b`) to be quoted.
				- The function `match_datadog_query` doesn't panic if an invalid path is passed, instead it returns an error. (https://github.com/vectordotdev/vrl/pull/1031)
				- The `parse_ruby_hash` parser is extended to match the Datadog implementation. Previously it would parse the key in `{:key => "value"}` as `:key`, now it parses it as `key`. (https://github.com/vectordotdev/vrl/pull/1050)
				"""
		},
		{
			type: "enhancement"
			description: """
				Adds more aggregations to `aggregate` transform: count, diff, max, min, mean, sum, latest and stdev. The current aggregation is named `auto` and is made the default (acts like a combination of `sum` and `latest`).
				"""
			contributors: ["esensar"]
		},
		{
			type: "enhancement"
			description: """
				Adds support for configuring `gcp_cloud_storage` sink API `endpoint`.
				"""
			contributors: ["movinfinex"]
		},
		{
			type: "feat"
			description: """
				Adds support for `OPTIONS` HTTP-method for `http_server` source
				"""
			contributors: ["sillent"]
		},
		{
			type: "feat"
			description: """
				Adds support for additional `graph` configuration on each component so that users can add arbitrary graphviz node attributes when generating a graph through `vector graph`.
				"""
			contributors: ["esensar"]
		},
		{
			type: "enhancement"
			description: """
				Expose a `retain` boolean config flag in the MQTT sink to tell the server to retain the messages.
				"""
			contributors: ["miquelruiz"]
		},
		{
			type: "fix"
			description: """
				Fixed the `logstash` source decoder to remove a panic that could trigger in the event of certain protocol errors.
				"""
		},
		{
			type: "feat"
			description: """
				The `axiom` sink now uses the native HTTP API rather than the Elasticsearch-compatible API.

				Notable changes:

				- The elasticsearch `@timestamp` semantics that changed in v5.5 of ElasticSearch no
				  longer affect the `axiom` sink as the sink uses the [Axiom native HTTP ingest
				  method](https://axiom.co/docs/send-data/ingest#ingest-api).
				- The `_time` field in data sent to Axiom now supports the same semantics as the
				  Axiom native API and SDK [as
				  documented](https://axiom.co/docs/reference/field-restrictions#requirements-of-the-timestamp-field).
				  In previous versions of Vector, the Axiom sink rejected events with `_time`
				  fields as the sink was following Elasticsearch semantics. This was confusing and
				  suprising for seasoned Axiom users new to Vector and seasoned Vector users new
				  to Axiom alike.
				- If a `@timestamp` field is sent to Axiom it is a normal user defined field.
				- If an `_time` field is sent to Axiom it now follows documented Axiom field semantics.
				"""
			contributors: ["darach"]
		},
		{
			type: "fix"
			description: """
				Fixed a bug in the `new_relic` sink that caused dotted attribute names to be encoded incorrectly with quotes.
				"""
		},
		{
			type: "fix"
			description: """
				Previously, when the `new_relic` sink sent non-standard event fields to the logs
				API, it would include those fields beside the standard event fields (for example,
				`message` and `timestamp`). Now, any such fields are sent in an `attributes`
				object, as specified by the [New Relic Logs
				API](https://docs.newrelic.com/docs/logs/log-api/introduction-log-api/).
				"""
		},
		{
			type: "enhancement"
			description: """
				Timestamps on metrics are now sent to the [New Relic metrics
				API](https://docs.newrelic.com/docs/data-apis/ingest-apis/metric-api/introduction-metric-api/)
				with millisecond resolution.
				"""
		},
		{
			type: "enhancement"
			description: """
				Added wildcard support for `query_parameters` setting in `http_server` and `heroku_logs` sources.
				"""
			contributors: ["uricorin"]
		},
		{
			type: "enhancement"
			description: """
				Add an option `new_naming` the GreptimeDB sink to use `greptime_value` and
				`greptime_timestamp` for value and timestamp fields, in order to keep consistency
				with GreptimeDB's convention.
				"""
			contributors: ["sunng87"]
		},
		{
			type: "enhancement"
			description: """
				Add support for providing Server Name Indication (SNI) in the TLS handshake when
				connecting to a server through `tls.server_name` for components that support TLS.
				"""
			contributors: ["anil-db"]
		},
		{
			type: "fix"
			description: """
				The [SysV init
				script](https://github.com/vectordotdev/vector/blob/v0.42/distribution/init.d/vector)
				now starts Vector in the background.
				"""
			contributors: ["waltzbucks"]
		},
		{
			type: "fix"
			description: """
				Updates the `subscriber_capacity` default for the NATS source to the correct value
				of 65536, which is the same value that the upstream `async_nats` library uses.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "enhancement"
			description: """
				Adds support for multiple URLs in the NATS source by enabling the `url` config
				option to have a comma-delimited list. This allows for greater fault tolerance as
				compared to reading from a single server.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "feat"
			description: """
				Adds `scope` information to logs received through the `opentelemetry` source.
				"""
			contributors: ["srstrickland"]
		},
		{
			type: "fix"
			description: """
				When using the `all_metrics: true` flag in `log_to_metric` transform, the
				`namespace` field is now optional and no longer required. If the `namespace` field
				is not provided, the produced metric does not have a namespace at all.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "fix"
			description: """
				The Loki sink now has support for sending structured metadata when using the JSON
				API (the default).
				"""
			contributors: ["maxboone"]
		},
		{
			type: "fix"
			description: """
				Adds retry support for the 408 HTTP errors (request timed out) to the Google Cloud Storage (GCS) sink.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "fix"
			description: """
				All TCP-based sinks (such as `socket` or `papertrail`) now gracefully handle config
				reloads under load rather than panicking. Previously, when a configuration reload
				occurred and data was flowing through the topology, the Vector process crashed due
				to the TCP-based sink attempting to access the stream when it had been terminated.
				"""
			contributors: ["neuronull"]
		},
		{
			type: "fix"
			description: """
				The adaptive request concurrency mechanism no longer deadlocks if setting
				`adaptive_concurrency.decrease_ratio` in a sink to a value less than 0.5.
				"""
		},
		{
			type: "fix"
			description: """
				Fixed a bug where AWS components would panic when using the [ECS IAM roles for
				authentication](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-iam-roles.html).
				"""
		},
		{
			type: "fix"
			description: """
				Google Cloud Platform components now refresh cached authentication tokens returned
				by v0.4.292 of the GCP metadata server and above to avoid using a stale
				authentication token.
				"""
			contributors: ["garethpelly"]
		},
		{
			type: "fix"
			description: """
				The `new_relic` sink, when sending to the `event` API, would quote field names
				containing periods or other meta-characters. This would produce broken field
				names in the New Relic interface, and so that quoting has been removed.
				"""
		},
	]

	commits: [
		{sha: "0a09104dac4264ac08f94ddd411baf9f37be12e5", date: "2024-09-07 15:16:17 UTC", description: "Bump tokio-stream from 0.1.15 to 0.1.16", pr_number: 21221, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "fbf73c5ebfb7390c3fb4ae045405c549f1aa955e", date: "2024-09-07 15:16:26 UTC", description: "Bump dashmap from 6.0.1 to 6.1.0", pr_number: 21219, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "631e06db9b118ab2f2023d37303837044cbb7bf6", date: "2024-09-07 15:16:38 UTC", description: "Bump the clap group across 1 directory with 2 updates", pr_number: 21218, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "503813a877aea0e9aa32bec85c2fb350fc218858", date: "2024-09-07 15:16:47 UTC", description: "Bump bufbuild/buf-setup-action from 1.39.0 to 1.40.0", pr_number: 21215, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9a78ea30fc10de6b41e24dcddcb2aeb86f54e96e", date: "2024-09-07 15:17:08 UTC", description: "Bump syn from 2.0.75 to 2.0.77", pr_number: 21191, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 50, deletions_count: 50},
		{sha: "f346a318535dbeffb97e5cbebec8c40945e36cdb", date: "2024-09-10 06:35:41 UTC", description: "add more aggregations to aggregate transform", pr_number: 20836, scopes: ["aggregate transform"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 3, insertions_count: 669, deletions_count: 75},
		{sha: "0fc48dd09119916e20cdb5eb393a320eaf44a359", date: "2024-09-09 23:34:59 UTC", description: "Fix example of setting nested field", pr_number: 21241, scopes: ["lua transform"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "da264a8139543309b5329a2c0db8a588b34233d9", date: "2024-09-10 02:25:50 UTC", description: "Bump manifests to v0.36.0 of the chart", pr_number: 21246, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "584c70c6db4354ebac261787cacf6133ce2ca9a2", date: "2024-09-11 03:09:12 UTC", description: "Fix docs for new components in v0.41.0", pr_number: 21260, scopes: ["releasing"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 7, insertions_count: 144, deletions_count: 17},
		{sha: "3f3b07eefd16eda24f53500d921b113ae27b79e0", date: "2024-09-11 08:23:55 UTC", description: "update to latest VRL sha", pr_number: 21259, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 43, deletions_count: 30},
		{sha: "67569635ec1b8b2ac84d59d782d1126f119a9d03", date: "2024-09-12 04:53:26 UTC", description: "Bump express from 4.19.2 to 4.20.0 in /website", pr_number: 21266, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 158, deletions_count: 26},
		{sha: "aba237c0c53186f4624284b55d27148acc8ac597", date: "2024-09-12 09:03:47 UTC", description: "Fix typo in 0.40.0 release changelog", pr_number: 21271, scopes: [], type: "docs", breaking_change: false, author: "nemobis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "712d62eb7a693bb86c9f579ff6c96574595011b9", date: "2024-09-12 00:39:11 UTC", description: "Fix whitespace for `parse_influxdb` notice", pr_number: 21273, scopes: ["vrl stdlib"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "2e702b3b0fe43f7ea747ec5fb75e812a70db4959", date: "2024-09-12 02:35:22 UTC", description: "Update manifests to v0.36.1 of the chart", pr_number: 21275, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "bd65b76b3f87ffae46bedc2a5cf859e763d338dd", date: "2024-09-13 10:18:47 UTC", description: "added support policy", pr_number: 21281, scopes: [], type: "docs", breaking_change: false, author: "shazib", files_count: 1, insertions_count: 14, deletions_count: 9},
		{sha: "9f56d46eba7b4a51a7bc3b4ee98d604c0d11a79c", date: "2024-09-13 05:58:08 UTC", description: "merge 'source_event_id's", pr_number: 21287, scopes: ["metadata"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 29, deletions_count: 0},
		{sha: "e71016cbdbf8aafb18add99e6229cc9e512233c4", date: "2024-09-13 08:00:50 UTC", description: "Fix typo causing a panic on short requests", pr_number: 21286, scopes: ["logstash source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 179, deletions_count: 133},
		{sha: "f5b9265335c9df7bcbd4a57046738fb5e87a67c1", date: "2024-09-17 04:50:11 UTC", description: "add additional config for graph output command", pr_number: 21194, scopes: ["components"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 12, insertions_count: 240, deletions_count: 67},
		{sha: "0291b645a8a59c4598c5410bfd57b4d594d22a8e", date: "2024-09-17 15:12:35 UTC", description: "fix symbol error", pr_number: 21294, scopes: [], type: "chore", breaking_change: false, author: "Hanlu", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "da1d02d2deeed78f958bab3d75e9e206df788d65", date: "2024-09-18 06:54:12 UTC", description: "add two test cases", pr_number: 21306, scopes: ["dedupe"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 100, deletions_count: 0},
		{sha: "b642efd1a7889f2f6543b9f72285a0ee20650bbe", date: "2024-09-18 11:23:42 UTC", description: "Fix handling of dotted attribute names", pr_number: 21305, scopes: ["new_relic sink"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 161, deletions_count: 144},
		{sha: "162d9b5267d8a3f82deb6015fcee2a7c28b5a08b", date: "2024-09-19 05:46:56 UTC", description: "Put log API attributes in separate structure", pr_number: 21313, scopes: ["new_relic sink"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 196, deletions_count: 106},
		{sha: "8238e5ab83605fa88cf657029a896e92d7951f59", date: "2024-09-19 21:10:03 UTC", description: "Use millisecond timestamp with metrics", pr_number: 21317, scopes: ["new_relic sink"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 40, deletions_count: 46},
		{sha: "141ea8c7807fceafaf1f0f2c5a97ba8671dd5772", date: "2024-09-20 01:27:22 UTC", description: "Do not quote paths containing periods for the event API", pr_number: 21323, scopes: ["new_relic sink"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 140, deletions_count: 44},
		{sha: "76ea1c8e7c1493ad9419f9fdf4ad289e8f460e33", date: "2024-09-20 23:43:34 UTC", description: "Fix syntax for function arguments", pr_number: 21327, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 7, deletions_count: 7},
		{sha: "dcdb72d473e9a1374e1188d7f21a31ace48e955c", date: "2024-09-21 04:32:23 UTC", description: "Fix typo in parse_influxdb function description", pr_number: 21328, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f99e052b54fc9c32731694f258b30360e28b68ac", date: "2024-09-21 14:20:23 UTC", description: "expose retain config flag", pr_number: 21291, scopes: ["mqtt sink"], type: "feat", breaking_change: false, author: "Miquel Ruiz", files_count: 5, insertions_count: 23, deletions_count: 1},
		{sha: "e17273230a206d5afd23decfba329beb74bfb1c9", date: "2024-09-24 23:21:58 UTC", description: "Parallelize the adaptive concurrency tests", pr_number: 21343, scopes: ["tests"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 28, deletions_count: 35},
		{sha: "daa02bbe5c3364dae7f9838723bded33f2363adc", date: "2024-09-25 20:46:08 UTC", description: "Fix link to deprecations file in minor release template", pr_number: 21229, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "112f13cfd97118f0843e7946cbfe5db6938ead37", date: "2024-09-25 20:46:22 UTC", description: "Minor tweaks to patch template", pr_number: 21227, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "973f4c1b10ce2f488517e46e322108d5287efdcf", date: "2024-09-26 23:08:30 UTC", description: "Bump the aws group across 1 directory with 3 updates", pr_number: 21268, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 9, deletions_count: 9},
		{sha: "aea5cf887aa1f4a244ce21b7592dac7f32061f19", date: "2024-09-26 20:27:48 UTC", description: "Bump databend-client from 0.20.1 to 0.21.0", pr_number: 21359, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f5c95558e53bd10991b3fa7ad65740535e845396", date: "2024-09-27 00:32:15 UTC", description: "fix vrl code example", pr_number: 21356, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Alexandre de Sá", files_count: 1, insertions_count: 16, deletions_count: 3},
		{sha: "7f376b12c7ab432009839938d1696c84601663a0", date: "2024-09-27 00:43:11 UTC", description: "Enable rt-tokio on all applicable AWS crates", pr_number: 21363, scopes: ["aws provider"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 17, deletions_count: 13},
		{sha: "0cd763ae09026f3df51179e6d1e0f99570775104", date: "2024-09-28 01:04:55 UTC", description: "Bump the clap group across 1 directory with 3 updates", pr_number: 21366, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 13, deletions_count: 13},
		{sha: "219f8b6c1c6853b03bc64fa22d3186f55998d019", date: "2024-09-28 05:07:31 UTC", description: "Bump flate2 from 1.0.33 to 1.0.34", pr_number: 21368, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f8e030c61411e92721bcc04751771d56030ef6df", date: "2024-09-28 17:03:15 UTC", description: "add optional new_naming strategy for greptimedb sink", pr_number: 21331, scopes: ["greptimedb"], type: "enhancement", breaking_change: false, author: "Ning Sun", files_count: 9, insertions_count: 273, deletions_count: 27},
		{sha: "463eb4340a86faab64d11dfc691c18f36efa2a1d", date: "2024-09-28 04:25:05 UTC", description: "Bump async-trait from 0.1.82 to 0.1.83", pr_number: 21350, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "77ce3e59c69663223b0185e922c91af389759889", date: "2024-09-28 04:25:08 UTC", description: "Bump cargo_toml from 0.20.4 to 0.20.5", pr_number: 21349, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "74134299dd78055921d9705035a6c7b582db462d", date: "2024-09-28 11:25:19 UTC", description: "Bump libc from 0.2.158 to 0.2.159", pr_number: 21351, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "438bc8b47c294a954fbb8bbb56f573e74ee749d0", date: "2024-09-28 11:25:29 UTC", description: "Bump ordered-float from 4.2.2 to 4.3.0", pr_number: 21358, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "edb2242f16942f3781423204f1290e72571d4825", date: "2024-10-01 06:13:06 UTC", description: "Bump rstest from 0.22.0 to 0.23.0", pr_number: 21382, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 78, deletions_count: 89},
		{sha: "e318a5c722669c010c854d4a072fed68a2d2bf5f", date: "2024-10-02 00:23:29 UTC", description: "Bump regex from 1.10.6 to 1.11.0", pr_number: 21381, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 17, deletions_count: 17},
		{sha: "f3655c87317a355831223fd17288c7dc32464b11", date: "2024-10-02 00:23:55 UTC", description: "Bump serde from 1.0.209 to 1.0.210", pr_number: 21237, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "145bf0a656d3d54e1d8d8fdc2c3efa82dd66e980", date: "2024-10-02 12:48:46 UTC", description: "use correct default value for subscriber_capacity", pr_number: 21384, scopes: ["nats source"], type: "fix", breaking_change: false, author: "Benjamin Dornel", files_count: 3, insertions_count: 5, deletions_count: 2},
		{sha: "6b775087683d9247985d38055db1bc5d7100db6b", date: "2024-10-02 07:46:36 UTC", description: "Bump similar-asserts from 1.5.0 to 1.6.0", pr_number: 21222, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "7c4867e6b131f21186cae3a437ac352cbbd4bcd0", date: "2024-10-02 07:47:00 UTC", description: "Bump schannel from 0.1.23 to 0.1.24", pr_number: 21234, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "9087dc73fa3b29384af68ce2519a21bd56c65cfb", date: "2024-10-02 07:47:12 UTC", description: "Bump wiremock from 0.6.1 to 0.6.2", pr_number: 21238, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "6b8b661769656b34cbbc50549cf5b6e4cb26bff9", date: "2024-10-02 07:47:21 UTC", description: "Bump owo-colors from 4.0.0 to 4.1.0", pr_number: 21269, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 17, deletions_count: 7},
		{sha: "f484d7893f187d5c94cb1646be8cbb9dd17876f6", date: "2024-10-02 07:47:57 UTC", description: "Bump memmap2 from 0.9.4 to 0.9.5", pr_number: 21297, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "20a49cd65c0996a89930d4348dcb643ee5e97f11", date: "2024-10-02 07:47:59 UTC", description: "Bump tokio-postgres from 0.7.11 to 0.7.12", pr_number: 21299, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "d1d94288878e4c89fcc12ed1b220f05fe0ed83bb", date: "2024-10-02 07:48:05 UTC", description: "Bump anyhow from 1.0.86 to 1.0.89", pr_number: 21300, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "3d2f5c871f3dcd4affc9e13a9ce34a1b3df37bfb", date: "2024-10-02 07:48:11 UTC", description: "Bump tokio-openssl from 0.6.4 to 0.6.5", pr_number: 21301, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 5},
		{sha: "800fd9a97eb12d49161dfdd117521f2bf65ec4f0", date: "2024-10-02 07:48:22 UTC", description: "Bump thiserror from 1.0.63 to 1.0.64", pr_number: 21337, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "59981dbe6b2626dbba1ea99bcf2a87ac8aef8f4b", date: "2024-10-02 07:48:28 UTC", description: "Bump nkeys from 0.4.3 to 0.4.4", pr_number: 21338, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "3fdb77396b8a2e4183af9ed729662ba06184a6fc", date: "2024-10-02 07:48:59 UTC", description: "Bump docker/build-push-action from 6.7.0 to 6.9.0", pr_number: 21388, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5397c09a53281aade7e6e1398342aa0dc179d78b", date: "2024-10-02 08:06:23 UTC", description: "Bump bufbuild/buf-setup-action from 1.40.0 to 1.43.0", pr_number: 21394, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "230f0cae7fa874d85e4bcf9068f90f51972bb856", date: "2024-10-02 09:57:44 UTC", description: "Bump serde_json from 1.0.127 to 1.0.128", pr_number: 21212, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a0f3403e9b8f5910bea0b433581872bc148fa394", date: "2024-10-02 23:12:44 UTC", description: "update VRL to v0.19.0", pr_number: 21392, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 18, deletions_count: 4},
		{sha: "614a0147af88b38c957cf8b274aa75892192dabd", date: "2024-10-02 22:47:23 UTC", description: "Add exception for RUSTSEC-2024-0376", pr_number: 21401, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "308218766b9d4ae4b24f95e9f88d2b69f0cefb98", date: "2024-10-04 03:11:38 UTC", description: "Bump bytes from 1.7.1 to 1.7.2", pr_number: 21318, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 91, deletions_count: 91},
		{sha: "b15431b345a25e34ece051365869b110e48f85a6", date: "2024-10-03 20:12:24 UTC", description: "Bump serde_with from 3.9.0 to 3.10.0", pr_number: 21399, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "6359468ed5e73f37d7cd454b35253e08b8c6c62a", date: "2024-10-03 20:12:34 UTC", description: "Bump the clap group across 1 directory with 2 updates", pr_number: 21405, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 13, deletions_count: 13},
		{sha: "c2e3b7e1094c8ec9bfb828c6bf30b2917734e8ff", date: "2024-10-04 11:44:25 UTC", description: "Bump community-id from 0.2.2 to 0.2.3", pr_number: 21391, scopes: ["deps"], type: "chore", breaking_change: false, author: "Dylan R. Johnston", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "3a1343746b0fbf047bf1f9304707c25febc54ca8", date: "2024-10-04 03:59:57 UTC", description: "Bump async-compression from 0.4.12 to 0.4.13", pr_number: 21406, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "71bcf877ddae163f088d58cd2abd03c973ffcc02", date: "2024-10-04 14:45:00 UTC", description: "add support for multiple URLs", pr_number: 21386, scopes: ["nats source"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 3, insertions_count: 74, deletions_count: 2},
		{sha: "6e48431acdc46d745f100c2f40ed07d911efe2a7", date: "2024-10-04 06:49:18 UTC", description: "Bump ipnet from 2.9.0 to 2.10.0", pr_number: 21236, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "7086dfc147b2ea069cd6952437bb07a65b657730", date: "2024-10-04 16:16:04 UTC", description: "Improvement to sysv script for the vector process start within background ", pr_number: 21370, scopes: ["sysv script"], type: "fix", breaking_change: false, author: "hedy kim", files_count: 3, insertions_count: 7, deletions_count: 3},
		{sha: "15c415a13a2c8908730fe9d696e6eea399951f63", date: "2024-10-04 01:12:56 UTC", description: "Regenerate Cargo.lock", pr_number: 21415, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "04d21fb7e826bd3332cae058b9ca13160f2f46a3", date: "2024-10-04 02:55:40 UTC", description: "Use wasm-pack 0.13.0", pr_number: 21416, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 7, deletions_count: 7},
		{sha: "4588cec35cfae69aa9f606f1a32fdaa2c1283583", date: "2024-10-04 04:49:41 UTC", description: "support of SNI when connecting to remote server", pr_number: 21365, scopes: ["tls settings"], type: "enhancement", breaking_change: false, author: "Anil Gupta", files_count: 74, insertions_count: 682, deletions_count: 17},
		{sha: "88d6fe7faca520bd28eba308bf08846eb1823975", date: "2024-10-05 13:58:43 UTC", description: "Make API endpoint configurable", pr_number: 21158, scopes: ["gcp_cloud_storage"], type: "enhancement", breaking_change: false, author: "movinfinex", files_count: 4, insertions_count: 24, deletions_count: 3},
		{sha: "3b58618b03906e4e9826336ec481a887330e2572", date: "2024-10-05 01:15:53 UTC", description: "Update CODEOWNERS", pr_number: 21424, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 100},
		{sha: "f9b07dba806c0118ef6022dd55847d68cf198bf3", date: "2024-10-05 02:00:45 UTC", description: "Have Vector `master` use `main` of VRL", pr_number: 21417, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 75, deletions_count: 45},
		{sha: "0cda47c60c54a88f28d013160df21ca97c6999f6", date: "2024-10-05 03:08:58 UTC", description: "Update install.sh workflow to not publish to AWS", pr_number: 21412, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 34},
		{sha: "42b0d3e1261699b9b0969aa7d9bd159fe64be9fb", date: "2024-10-05 05:07:20 UTC", description: "Collect Vector telemetry in regression tests", pr_number: 21422, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 27, insertions_count: 110, deletions_count: 0},
		{sha: "3eecbe71635125d1f18af708e31cf23cc66a8536", date: "2024-10-05 05:25:40 UTC", description: "Fix install.sh handling of new directory structure on MacOS", pr_number: 21403, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "51dcf8dfd4b9be926fd45b584908e820df4c2b7a", date: "2024-10-06 17:32:05 UTC", description: "optional metric namespace in `log_to_metric` transform", pr_number: 21429, scopes: ["transforms"], type: "fix", breaking_change: false, author: "Jorge Hermo", files_count: 2, insertions_count: 60, deletions_count: 7},
		{sha: "3a5947e5b8618ef7892f87dd0348ee1655910580", date: "2024-10-08 05:33:19 UTC", description: "redundant clone in `log_to_metric` transform", pr_number: 21431, scopes: ["transforms"], type: "enhancement", breaking_change: false, author: "Jorge Hermo", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "d753ab1c058e02718c9629e49e73661f130c3153", date: "2024-10-07 22:30:34 UTC", description: "Metric `namespace` should be optional", pr_number: 21439, scopes: ["docs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b58e1b22cff3c80abff1ac30a63f7bb638a8848f", date: "2024-10-08 13:15:00 UTC", description: "Added wildcard support for query parameters", pr_number: 21375, scopes: ["http_server and heroku_logs sources"], type: "enhancement", breaking_change: false, author: "Uri Corin", files_count: 7, insertions_count: 239, deletions_count: 29},
		{sha: "49bb1b5cd34eb4721bb08592edc1ece7e147a092", date: "2024-10-09 02:37:57 UTC", description: "make prost* crates workspace dependencies", pr_number: 21426, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 59, deletions_count: 55},
		{sha: "5581b245476834838b37b30f2eb49d783a455fb1", date: "2024-10-09 05:24:41 UTC", description: "Remove \"What is\" from about pages", pr_number: 21452, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 9, deletions_count: 5},
		{sha: "290b9ddd2337e5c9abe26a82436a18ec7d826c94", date: "2024-10-09 21:12:23 UTC", description: "add retry for 408 http error", pr_number: 21449, scopes: ["gcs sink"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 2, insertions_count: 4, deletions_count: 0},
		{sha: "775d9defd3d0c8f4cee5c16473b80b4dced7e288", date: "2024-10-10 02:16:59 UTC", description: "typo", pr_number: 21453, scopes: ["docs"], type: "fix", breaking_change: false, author: "average-gray", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "59b7c9ad55dff61ecc34be71f438e4a7f9826eb8", date: "2024-10-09 23:24:09 UTC", description: "gracefully shutdown on reload when stream is terminated", pr_number: 21455, scopes: ["socket sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 27, deletions_count: 25},
		{sha: "aeaeecdcabd7f162946c5bb4ba7342fb062dc73c", date: "2024-10-10 02:28:43 UTC", description: "Document regex replacement group escaping", pr_number: 21467, scopes: ["vrl stdlib"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "e250dcbff7e5cd89ba5d880065414576c89d4327", date: "2024-10-10 06:54:55 UTC", description: "simplify chanelog validation to always check for authors", pr_number: 21463, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 18, deletions_count: 44},
		{sha: "1fb53f64296853f9dab14b421e81e88c5a9d3428", date: "2024-10-10 23:09:10 UTC", description: "Bump the aws group across 1 directory with 2 updates", pr_number: 21470, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "1aeed4d24f1e9ff3d3f2e38ff4b6d2a89bf89cc9", date: "2024-10-10 23:10:50 UTC", description: "Bump console-subscriber from 0.3.0 to 0.4.0", pr_number: 21460, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 121, deletions_count: 24},
		{sha: "4940ff0b60536caae00bb2eed676f1fb8af830b2", date: "2024-10-11 06:13:47 UTC", description: "Bump schannel from 0.1.24 to 0.1.26", pr_number: 21436, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "91d0fab009356325f05d8e5bff93d926d3047f5e", date: "2024-10-11 06:13:59 UTC", description: "Bump pin-project from 1.1.5 to 1.1.6", pr_number: 21435, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "e7c849ff32689d9272b5172476a59e69659c7a36", date: "2024-10-11 06:14:08 UTC", description: "Bump the futures group with 2 updates", pr_number: 21433, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 52, deletions_count: 52},
		{sha: "f45d9f3489de058d6d9fdc98a93754e9603ba24f", date: "2024-10-11 06:15:30 UTC", description: "Bump docker/setup-buildx-action from 3.6.1 to 3.7.1", pr_number: 21425, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "b3dac6e35782ef9f04f3e2dd3d5e9f135da7f3b7", date: "2024-10-11 06:15:56 UTC", description: "Bump temp-dir from 0.1.13 to 0.1.14", pr_number: 21418, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "71bd2a2c51c14e49c1753cfef9e9910caf92babc", date: "2024-10-11 06:20:42 UTC", description: "Bump tempfile from 3.12.0 to 3.13.0", pr_number: 21380, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 30, deletions_count: 30},
		{sha: "d4015f22989c88e66bbbd872289eb2cec135ba11", date: "2024-10-11 07:00:28 UTC", description: "Bump no-proxy from 0.3.4 to 0.3.5", pr_number: 21434, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 7, deletions_count: 27},
		{sha: "73a03a7fb582d7706e5568d7ec3d50373a46e7b4", date: "2024-10-11 00:02:32 UTC", description: "add instrumentation scope to logs", pr_number: 21407, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Scott Strickland", files_count: 4, insertions_count: 151, deletions_count: 9},
		{sha: "e36654db7854bbf904d537c841d0cd363128a0e7", date: "2024-10-11 11:44:49 UTC", description: "Rebase sink on `http` sink and remove `elasticsearch` compatibility", pr_number: 21362, scopes: ["axiom sink"], type: "fix", breaking_change: false, author: "Darach Ennis", files_count: 6, insertions_count: 155, deletions_count: 48},
		{sha: "a0f2e53a01c27e2e1e788f8b744211e202ec2b3c", date: "2024-10-11 06:29:05 UTC", description: "enable http retries for choco installations", pr_number: 21465, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 43, deletions_count: 3},
		{sha: "bda0ce4939bdde53332bfbdcaff26257e97fdb9a", date: "2024-10-11 14:52:57 UTC", description: "bump no-proxy from 0.3.4 to 0.3.5", pr_number: 21420, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jérémie Drouet", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "4de57932c5a6308161d9a5c30e08110d6b4ad797", date: "2024-10-11 12:53:15 UTC", description: "Bump the clap group with 2 updates", pr_number: 21459, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "a7602ceccbc88d922e9de79dea503d1f5c4ee3ee", date: "2024-10-11 15:50:01 UTC", description: "Bump bufbuild/buf-setup-action from 1.43.0 to 1.45.0", pr_number: 21466, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c03d3f9c5e498ab9e98b9d30d14841284af07ca1", date: "2024-10-11 15:50:49 UTC", description: "Bump ipnet from 2.10.0 to 2.10.1", pr_number: 21419, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "7b96a191ab2c2be07d7e77d7bab3c1da58507a78", date: "2024-10-11 15:50:52 UTC", description: "Bump async-stream from 0.3.5 to 0.3.6", pr_number: 21398, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 8, deletions_count: 8},
		{sha: "45ac1b98afb81bb06f7244b09257c2cfa120891e", date: "2024-10-11 20:03:11 UTC", description: "Clarify that the source only tails logs from the host", pr_number: 21477, scopes: ["kubernetes_logs source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "202da8baa865d7390841a4775d0d37e840f454cf", date: "2024-10-12 03:36:39 UTC", description: "Bump vrl from `3295458` to `dc0311d`", pr_number: 21481, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "0eb9efc28dbd8c00fc4e11851f2d29879bd78755", date: "2024-10-12 06:20:32 UTC", description: "serialize the structured metadata to JSON", pr_number: 21461, scopes: ["loki"], type: "fix", breaking_change: false, author: "Max Boone", files_count: 3, insertions_count: 42, deletions_count: 2},
		{sha: "cb1e4f046d646c194d7f922a76a4147f110fbbf3", date: "2024-10-12 05:33:33 UTC", description: "Bump async-compression from 0.4.13 to 0.4.14", pr_number: 21484, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "763b5a00febccc72e00a02fd75269bf402ae86be", date: "2024-10-12 05:33:52 UTC", description: "Bump crossterm from 0.27.0 to 0.28.1", pr_number: 21483, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 24, deletions_count: 6},
		{sha: "e2afcdf6e5023708dd237967a0a12cadade6a321", date: "2024-10-12 05:34:18 UTC", description: "Bump wasm-bindgen from 0.2.93 to 0.2.95", pr_number: 21482, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 10, deletions_count: 10},
		{sha: "7833a2a8f042baa4a950b25e3dad4a6c69f75740", date: "2024-10-12 00:00:26 UTC", description: "usage of `a deprecated Node.js version`", pr_number: 21479, scopes: ["ci"], type: "fix", breaking_change: false, author: "Hamir Mahal", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "4b3de831cd4f96e1b08e4e784f232dd7e69027d2", date: "2024-10-12 11:15:09 UTC", description: "Bump lru from 0.12.4 to 0.12.5", pr_number: 21445, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 21, deletions_count: 4},
		{sha: "6ff1fd642b88c7719afaf0dcd772c142b84a62d6", date: "2024-10-12 12:18:57 UTC", description: "Adapt to token regen behavior change", pr_number: 21411, scopes: ["gcp service"], type: "fix", breaking_change: false, author: "Gareth Pelly", files_count: 2, insertions_count: 23, deletions_count: 2},
		{sha: "6d71fddae4990e73ce2f770f6ba9e3270987c2ad", date: "2024-10-12 11:21:01 UTC", description: "Bump serde_with from 3.10.0 to 3.11.0", pr_number: 21437, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "22c23c34cb9e38e39a3d92c0b9bfe79f60f313e2", date: "2024-10-12 11:22:33 UTC", description: "Bump proc-macro2 from 1.0.86 to 1.0.87", pr_number: 21446, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 69, deletions_count: 69},
		{sha: "31dc38f2126d94f1ff7800c82fd0b67cf7e5dfa9", date: "2024-10-12 05:57:32 UTC", description: "Bump Rust version to 1.80", pr_number: 20949, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 43, insertions_count: 136, deletions_count: 82},
		{sha: "6075bc2c7272a74de625e4d9ff933c92e00ad038", date: "2024-10-12 23:18:19 UTC", description: "Bump once_cell from 1.19.0 to 1.20.2", pr_number: 21438, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "9bd25931468d0a12911c759a621bde9e86521e76", date: "2024-10-13 05:15:12 UTC", description: "Bump indexmap from 2.5.0 to 2.6.0", pr_number: 21397, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 27, deletions_count: 27},
		{sha: "501e4fd120fbf556895fb68874c9cee35896eec0", date: "2024-10-15 23:14:44 UTC", description: "update windows runner to 2022", pr_number: 21486, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 36, deletions_count: 71},
		{sha: "441bf23e590fcc4f0043657beca25318f8c16cf5", date: "2024-10-16 00:26:01 UTC", description: "tweak nightly scedule", pr_number: 21506, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7feb20b4eb81513be8e51ea301481a98e910df1d", date: "2024-10-16 00:33:41 UTC", description: "Fix event `size_of` test", pr_number: 21508, scopes: ["tests"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "30149204eb8685e6df94198d740868ec2aae088f", date: "2024-10-16 03:34:20 UTC", description: "Update lading to 0.23.3", pr_number: 21510, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 12, insertions_count: 10, deletions_count: 19},
		{sha: "d2f855c3400c946a40a4eb35bf1b960ac4f3b416", date: "2024-10-16 04:52:08 UTC", description: "Drop usage of `once_cell::LazyLock`", pr_number: 21511, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 46, insertions_count: 190, deletions_count: 211},
		{sha: "650e6b215c6dd8cca32513705e81be1b21064d99", date: "2024-10-16 20:22:44 UTC", description: "add 401 and 408 under retry policy", pr_number: 21458, scopes: ["gcs sink"], type: "docs", breaking_change: false, author: "Benjamin Dornel", files_count: 2, insertions_count: 12, deletions_count: 2},
		{sha: "9c67bba358195f5018febca2f228dfcb2be794b5", date: "2024-10-16 08:15:34 UTC", description: "Bump smp to 0.18.0", pr_number: 21513, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
	]
}
