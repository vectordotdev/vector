package metadata

releases: "0.27.0": {
	date:     "2023-01-12"
	codename: ""

	whats_next: []

	known_issues: [
		"""
			Vector sources do not correctly tag the `component_events_in_total` and
			`component_events_out_total` internal metrics with their component tags (`component_id`,
			`component_kind`, and `component_type`). This affects reporting in `vector top`.
			Fixed in v0.27.1.
			""",
		"""
			The `log_schema.timestamp_key` cannot be set to `""` to suppress adding a timestamp.
			Fixed in 0.28.2.
			""",
		"""
			TCP-based sources like `socket`, `logstash`, and `fluent` would sometimes panic when
			back-pressure logic calculated a lower limit on the number of incoming
			connections than 2, which is intended to be the minimum limit. Fixed in 0.28.2.
			""",
	]

	description: """
		The Vector team is pleased to announce version 0.27.0!

		This release includes the usual litany of smaller enhancements and bug fixes as well as:

		- Support for a new metric tag data model that supports a wider range of tags than the
		  simple key/value pairs previously allowed including tags that are not key/value as
		  well as tag keys that appear multiple times, with different values. See [the release
		  highlight](/highlights/2022-12-22-enhanced-metric-tags) for more details about this
		  feature and how to enable it.
		- Support for tracing memory allocations within Vector to aid in troubleshooting
		  Vector's memory use. See the [announcement blog post](/blog/tracking-allocations/) for
		  more details about this feature and how to enable it.

		Be sure to check out the [upgrade guide](/highlights/2023-01-17-0-27-0-upgrade-guide) for
		breaking changes in this release.
		"""

	changelog: [
		{
			type: "feat"
			scopes: ["metrics"]
			description: """
				Support for a new metric tag data model that supports a wider range of tags than the
				simple key/value pairs previously allowed including tags that are not key/value as
				well as tag keys that appear multiple times, with different values. See [the release
				highlight](/highlights/2022-12-22-enhanced-metric-tags) for more details about
				this feature and how to enable it.
				"""
			pr_numbers: [15286, 15344, 14309, 15345, 15272, 15402, 15505, 15626, 15617, 15717,
				15716, 15639]
		},
		{
			type: "enhancement"
			scopes: ["sink: loki"]
			description: """
				A new `path` option was added to `loki` sink to override the default URL path of
				`/loki/api/v1/push`.
				"""
			pr_numbers: [15333]
			contributors: ["Sh4d1"]
		},
		{
			type: "enhancement"
			scopes: ["vrl: compiler"]
			description: """
				VRL no longer rejects querying an object returned by the `merge` function. This fixes
				programs that look like:

				```
				.object.key = "some value"
				. |= parse_key_value!(del(.message))
				.object.key = "some other value" # previously would error
				```
				"""
			pr_numbers: [15369]
		},
		{
			type: "feat"
			scopes: ["observability"]
			description: """
				Vector has added support for tracking component memory allocations to help users
				understand which components may be allocating more memory than expected. This is an
				opt-in feature, due to ~20% performance overhead, available by passing
				`--allocation-tracing` when running Vector. When enabled, the `internal_metrics`
				source will publish new `component_allocated_bytes`,
				`component_allocated_bytes_total`, and `component_deallocated_bytes_total` metrics.
				These are will then also be viewable in `vector top`.

				There is a caveat to the reported metrics in that memory passed between components
				is not tracked as allocations. For example, if a source allocates memory to store
				incoming events, those events are passed along to a downstream component, the memory
				will still appear as though owned by the source component that allocated it. We hope
				to improve this in the future to track memory passed between component boundaries.

				See the [announcement blog post](/blog/tracking-allocations/) for more details!
				"""
			pr_numbers: [15401, 15393, 15366, 15441]
		},
		{
			type: "enhancement"
			scopes: ["cli"]
			description: """
				Supported enrichment table types are now output by `vector list`.
				"""
			pr_numbers: [15446]
			contributors: ["w4"]
		},
		{
			type: "fix"
			scopes: ["source: kafka", "sink: kafka"]
			description: """
				The `kafka` source and sink now accepts inline PEM-encoded certificates for
				`tls.crt_file` to match other sources and sinks (and the documented behavior).
				"""
			pr_numbers: [15448]
			contributors: ["nabokihms"]
		},
		{
			type: "fix"
			scopes: ["reload"]
			description: """
				Vector avoids panicking when reloading under load.
				"""
			pr_numbers: [14875]
			contributors: ["Zettroke"]
		},
		{
			type: "fix"
			scopes: ["source: kubernetes_logs"]
			description: """
				The `kubernetes_logs` source again disabling annotation of namespace labels by
				setting `namespace_labels` to `""` again. This was regression in v0.26.0.
				"""
			pr_numbers: [15493]
		},
		{
			type: "fix"
			scopes: ["api", "shutdown"]
			description: """
				Vector now gracefully shutsdown if the [Vector API](/docs/reference/api/) cannot
				bind to the configured port.
				"""
			pr_numbers: [15087]
			contributors: ["zamazan4ik"]
		},
		{
			type: "fix"
			scopes: ["sink: prometheus_exporter", "shutdown"]
			description: """
				Vector now gracefully shutsdown if the a configured `prometheus_exporter` cannot
				bind to the configured port.
				"""
			pr_numbers: [15529]
			contributors: ["zamazan4ik"]
		},
		{
			type: "fix"
			scopes: ["sink: http", "shutdown"]
			description: """
				Vector now gracefully shutsdown if the a configured `http` cannot
				bind to the configured port.
				"""
			pr_numbers: [15528]
			contributors: ["zamazan4ik"]
		},
		{
			type: "fix"
			scopes: ["sink: elasticsearch"]
			description: """
				The `elasticsearch` sink now accepts reading compressed responses. It uses the
				`compression` option to set an `Accept-Encoding` option in requests to Elasticsearch.
				"""
			pr_numbers: [15478]
		},
		{
			type: "enhancement"
			scopes: ["source: pulsar"]
			description: """
				The `pulsar` source now supports configuring a producer name to use via the
				new `producer_name` option.
				"""
			pr_numbers: [15151]
			contributors: ["zamazan4ik"]
		},
		{
			type: "fix"
			scopes: ["transform: aws_ec2_metadata"]
			description: """
				The `aws_ec2_metadata` transform is now capable of fetching instance tags by
				specifying the tags to fetch in the new `tags` option.
				"""
			pr_numbers: [15314]
			contributors: ["blefevre"]
		},
		{
			type: "enhancement"
			scopes: ["vrl: stdlib"]
			description: """
				A new `abs` function was added to VRL to calculate the absolute value of a numeric
				value.
				"""
			pr_numbers: [15332]
			contributors: ["zamazan4ik"]
		},
		{
			type: "fix"
			scopes: ["sink: aws_cloudwatch_metrics"]
			description: """
				The `aws_cloudwatch_metrics` now supports sending 30 dimensions, rather than 10, as
				the AWS API now accepts this number of dimensions.
				"""
			pr_numbers: [15559]
		},
		{
			type: "fix"
			scopes: ["vrl: repl"]
			description: """
				Use of the `filter` function in the VRL REPL no longer panics.
				"""
			pr_numbers: [15508]
		},
		{
			type: "enhancement"
			scopes: ["vrl: stdlib"]
			description: """
				The `parse_cef` now has a `transform_custom_fields` parameter that can be set to
				extract custom key/value fields from the CEF message.
				"""
			pr_numbers: [15208]
			contributors: ["ktff"]
		},
		{
			type: "enhancement"
			scopes: ["provider: aws"]
			description: """
				AWS components now support configuring custom IMDS request parameters that are used
				during authentication: `auth.max_attempts`, `auth.connect_timeout`, and
				`auth.read_timeout`.
				"""
			pr_numbers: [15518]
			contributors: ["kevinpark1217"]
		},
		{
			type: "fix"
			scopes: ["sink: http"]
			description: """
				The `http` sink now allows overriding the `Content-Type` header via
				`request.headers`.
				"""
			pr_numbers: [15625]
			contributors: ["EdMcBane"]
		},
		{
			type: "enhancement"
			scopes: ["source: pulsar"]
			description: """
				The `pulsar` sink now supports compression via the standard `compression` option.
				"""
			pr_numbers: [15519]
			contributors: ["zamazan4ik"]
		},
		{
			type: "enhancement"
			scopes: ["source: kubernetes_logs"]
			description: """
				The `kubernetes_logs` source now more efficiently parses CRI logs.
				"""
			pr_numbers: [15591]
			contributors: ["Ilmarii"]
		},
		{
			type: "enhancement"
			scopes: ["source: nats"]
			description: """
				The `nats` source now allows adding the subject key to the event as metadata by
				setting `subject_key_field` to the name of the field you'd like to store the subject
				key in.
				"""
			pr_numbers: [15447]
			contributors: ["makarchuk"]
		},
		{
			type: "fix"
			scopes: ["platform"]
			description: """
				Vector now supports being run on operating systems with an older version of GNU libc
				(>=2.17) for the x86_64 architecture. This includes OSes such as CentOS 7, Amazon
				Linux 1, and Ubuntu 14.04.
				"""
			pr_numbers: [15695]
			contributors: ["apollo13"]
		},
		{
			type: "enhancement"
			scopes: ["sink: datadog_logs"]
			description: """
				The `datadog_logs` sink now allows configuring custom HTTP request headers. This is
				not typically needed when sending logs directly to Datadog.
				"""
			pr_numbers: [15745]
		},
		{
			type: "fix"
			scopes: ["transform: remap"]
			description: """
				The `remap` transform now falls back to using the globally configured `timezone` if
				one is not set at the `remap` transform level.
				"""
			pr_numbers: [15740]
		},
		{
			type: "enhancement"
			scopes: ["sink: loki"]
			description: """
				The `loki` sink is now more efficient by sending events with multiple label sets in
				the same request rather than separating them.
				"""
			pr_numbers: [15637]
			contributors: ["atodekangae"]
		},
		{
			type: "feat"
			scopes: ["vrl: stdlib"]
			description: """
				New `decode_base16` and `encode_base16` functions were added to VRL for interacting
				with base16 encoding.
				"""
			pr_numbers: [15851]
			contributors: ["WilliamApted-Org"]
		},
		{
			type: "enhancement"
			scopes: ["source: kubernetes_logs"]
			description: """
				The `kubernetes_logs` source now supports the `read_from` and `ignore_older_secs`
				options that exist on the `file` sink which offer further control how Vector reads
				from the pod log files.
				"""
			pr_numbers: [15746]
			contributors: ["zamazan4ik"]
		},
		{
			type: "enhancement"
			scopes: ["source: aws_kinesis_firehose"]
			description: """
				The `aws_kinesis_firehose` source now supports configuring multiple valid access
				keys. This is useful during key rotation.
				"""
			pr_numbers: [15828]
			contributors: ["dizlv"]
		},
	]

	commits: [
		{sha: "33bf10c4e98a12794eb44081e5c99098e77b63a6", date: "2022-11-28 21:08:53 UTC", description: "Add support for enhanced tags to native protobuf codec", pr_number: 15286, scopes: ["codecs"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 1204, insertions_count: 199, deletions_count: 95},
		{sha: "2f68aad68c7b3fad118c426d560854d1ae06cc77", date: "2022-11-29 04:21:00 UTC", description: "Add `path` option to configure the URL path", pr_number: 15333, scopes: ["loki sink"], type: "feat", breaking_change: false, author: "Patrik", files_count: 6, insertions_count: 47, deletions_count: 7},
		{sha: "2f8d89b94993465df5c83f74e2aed80148d48d2f", date: "2022-11-29 00:37:27 UTC", description: "allow protobuf building on >=3.12", pr_number: 15367, scopes: ["core"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 2, insertions_count: 1, deletions_count: 0},
		{sha: "7d779c58f4debcd9b03f57d4cd48315542a0d50b", date: "2022-11-29 01:11:32 UTC", description: "allow protobuf building on >=3.12 for vector-core", pr_number: 15370, scopes: ["core"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "7da6688b1f907329ceb10e8ea73a23602b1466ff", date: "2022-11-29 01:47:02 UTC", description: "Fix Netlify builds ", pr_number: 15374, scopes: ["vrl playground"], type: "fix", breaking_change: false, author: "Arshia Soleimani", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4db9d83e1908b25d454a1b397dcc38c18684f76b", date: "2022-11-29 20:47:08 UTC", description: "fix internal merge function when using overwrite", pr_number: 15369, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 15, deletions_count: 2},
		{sha: "cf7947d2bfb8075328850491cca9b2adb2a2d2ff", date: "2022-11-30 02:30:28 UTC", description: "change InsertIfEmpty to Overwrite", pr_number: 15383, scopes: ["gcp_pubsub source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "5a455ea780fc81a1c689b69c2c0c22d325b8dbb7", date: "2022-11-29 20:20:17 UTC", description: "Add log namespace and schema def support", pr_number: 15312, scopes: ["vector source"], type: "feat", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 133, deletions_count: 66},
		{sha: "d8815df7fac3647293c635f0989b489a23cdcaa3", date: "2022-11-30 03:34:22 UTC", description: "Add log namespace and schema def support", pr_number: 15363, scopes: ["nats source"], type: "feat", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 100, deletions_count: 14},
		{sha: "eaf8859c9eca09c9ef1334e61d3024438daf5615", date: "2022-11-29 22:50:22 UTC", description: "Rework tag value insert operation", pr_number: 15344, scopes: ["metrics"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 24, insertions_count: 86, deletions_count: 72},
		{sha: "ea17c1f2c95c3b6b950f20079f1ec11b6efe03b7", date: "2022-11-30 01:24:14 UTC", description: "add log namespace support to `metric_to_log` transform", pr_number: 15144, scopes: ["metric_to_log transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 29, insertions_count: 299, deletions_count: 63},
		{sha: "c1fc393e419b404d0f88a5dec0f02f3ee51d9836", date: "2022-11-30 02:42:00 UTC", description: "Add support for enhanced metric tags to `native_json`", pr_number: 15309, scopes: ["codecs"], type: "enhancement", breaking_change: true, author: "Bruce Guenter", files_count: 3, insertions_count: 56, deletions_count: 10},
		{sha: "ac28e1a14c71c603b998bd970e34b33ccb80f7a5", date: "2022-11-30 02:49:03 UTC", description: "Add multi-valued tag support", pr_number: 15345, scopes: ["log_to_metric transform"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 109, deletions_count: 16},
		{sha: "5db6e675bed9f253d56fa3b201d7b2892ac5c245", date: "2022-11-30 21:41:12 UTC", description: "Fix example log in docs", pr_number: 15396, scopes: ["docker_logs source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "c301e55dba1e9e138f31d67a5bea4b24fa7e99f6", date: "2022-11-30 19:40:55 UTC", description: "bump sha-1 from 0.10.0 to 0.10.1", pr_number: 15381, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "2adb745b0735c21d1f7af69e847cf3d5d2674360", date: "2022-11-30 19:41:50 UTC", description: "bump pest from 2.4.1 to 2.5.0", pr_number: 15348, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7fe13704c2bc3899fbfce5aaec1377acced2f867", date: "2022-11-30 19:42:22 UTC", description: "bump pulsar from 4.1.3 to 5.0.0", pr_number: 15360, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7e6727983e83c533956e63a06f4f1a4a792d3bea", date: "2022-12-01 04:10:42 UTC", description: "bump tokio-tungstenite from 0.17.2 to 0.18.0", pr_number: 15380, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 37, deletions_count: 6},
		{sha: "552d4b68acb5b0c56aeeae83405388f1d9461398", date: "2022-12-01 10:12:44 UTC", description: "fix typo in the documentation", pr_number: 15398, scopes: [], type: "docs", breaking_change: false, author: "Alexander Zaitsev", files_count: 63, insertions_count: 65, deletions_count: 65},
		{sha: "966fd1c25a21f2064f0e3d3db411b172f9516b52", date: "2022-12-01 02:44:09 UTC", description: "Regenerate chart manifests", pr_number: 15403, scopes: ["kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 65, deletions_count: 21},
		{sha: "3fdb996393bbac808a2e8940746737b4074dca1d", date: "2022-12-01 02:44:22 UTC", description: "Update prost to 0.11.3", pr_number: 15397, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 8, insertions_count: 44, deletions_count: 99},
		{sha: "03f762d96dfa6fd5813730a40a792fe607d9a72e", date: "2022-12-01 18:43:43 UTC", description: "bump governor from 0.5.0 to 0.5.1", pr_number: 15407, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0e431b039805df39b0659752da2218772a67205b", date: "2022-12-01 18:44:03 UTC", description: "bump tonic-build from 0.8.3 to 0.8.4", pr_number: 15408, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "fef6838a0bd066ff7f82f891fe983589bd721b00", date: "2022-12-01 18:44:30 UTC", description: "bump nix from 0.25.0 to 0.26.1", pr_number: 15410, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "17fe236ba409a2f717075a0738583bac80055820", date: "2022-12-01 18:45:04 UTC", description: "bump clap from 4.0.27 to 4.0.29", pr_number: 15411, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 12, deletions_count: 12},
		{sha: "ab57041be4a03d95b918b9f375e3ed5bb2775c06", date: "2022-12-01 19:52:14 UTC", description: "Regenerate manifests for 0.17.1", pr_number: 15406, scopes: ["kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "0007da49fc4a835c955b635358a36397425c692d", date: "2022-12-01 20:27:14 UTC", description: "Configurable reporting rates for tracking allocations. ", pr_number: 15401, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 3, insertions_count: 24, deletions_count: 4},
		{sha: "3cce4de71dd694d3203a84ec1bbcf387024a8d57", date: "2022-12-01 22:52:44 UTC", description: "Add log namespace and schema support", pr_number: 15399, scopes: ["kafka source"], type: "feat", breaking_change: false, author: "David Huie", files_count: 1, insertions_count: 317, deletions_count: 56},
		{sha: "b6f0e7bd44c3e3f7bd5c5df4958b398cce0f80f0", date: "2022-12-02 02:03:22 UTC", description: "Add log namespace support", pr_number: 15139, scopes: ["kubernetes_logs source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 9, insertions_count: 1509, deletions_count: 441},
		{sha: "a99bd4005491aa9d7f11e37dbe79b5339b3aebe0", date: "2022-12-01 23:26:30 UTC", description: "Track total allocations/deallocations", pr_number: 15393, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 1, insertions_count: 57, deletions_count: 21},
		{sha: "a833af8d553c6fcb1d1f8812734ea6a2f851f366", date: "2022-12-01 23:27:24 UTC", description: "Add allocation tracing visualization support to ```vector top```", pr_number: 15366, scopes: ["observability"], type: "feat", breaking_change: false, author: "Arshia Soleimani", files_count: 10, insertions_count: 359, deletions_count: 14},
		{sha: "2800613d0d9fc81e3b8dc5e19d9af705e9995398", date: "2022-12-02 02:36:00 UTC", description: "Update smp binary version, introduce regression detection ding", pr_number: 15376, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 88, deletions_count: 3},
		{sha: "2a3d6df0e89e18a1f674c4e8e8d90329103c2275", date: "2022-12-01 23:38:57 UTC", description: "Avoid indexing branch deploys", pr_number: 15421, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 0},
		{sha: "f19f3f11af73c80748ee7df494e595813388e992", date: "2022-12-02 03:51:35 UTC", description: "Correct secret name in Regression Detector trusted flow", pr_number: 15424, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "95fabca330cbb3bbb06245e7ad0996de7fb7a086", date: "2022-12-02 04:51:07 UTC", description: "Disable but do not remove soak workflows, related infra", pr_number: 15422, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 4, insertions_count: 0, deletions_count: 0},
		{sha: "0c6cb2f82b30b2ebd64645d35d4138c3c2eba610", date: "2022-12-02 22:24:32 UTC", description: "Add log namespace and schema definition support", pr_number: 15416, scopes: ["docker_logs source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 1219, deletions_count: 780},
		{sha: "8ceb3c7fb1d16b21ad6da3323194aa1101f6e365", date: "2022-12-02 22:07:02 UTC", description: "Mark soak directory as disabled", pr_number: 15436, scopes: ["performance"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 105, insertions_count: 2, deletions_count: 0},
		{sha: "3d5b53d49b947bf2c9f3153fbdea6a2f8e62378d", date: "2022-12-03 00:12:36 UTC", description: "Remove unused tests", pr_number: 15429, scopes: ["tests"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 0, deletions_count: 1264},
		{sha: "9fe487493c1b4bd08423e2416a106ee6aae0f7b9", date: "2022-12-02 22:22:23 UTC", description: "bump assert_cmd from 2.0.6 to 2.0.7", pr_number: 15433, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0e581300a301be047461e823eebbbc57a5de4dde", date: "2022-12-02 22:22:44 UTC", description: "bump gloo-utils from 0.1.5 to 0.1.6", pr_number: 15434, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "a828fc5dae16cb5e3108eced82e1bbbbb98dd704", date: "2022-12-02 22:31:02 UTC", description: "fix http pipelines regression tests", pr_number: 15443, scopes: ["ci"], type: "fix", breaking_change: false, author: "Arshia Soleimani", files_count: 2, insertions_count: 0, deletions_count: 2},
		{sha: "29e18e0cccee1d5c220aa9b78658c8792192622b", date: "2022-12-03 07:35:52 UTC", description: "bump quoted_printable from 0.4.5 to 0.4.6", pr_number: 15431, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "bb082950e1c3e00bb8a9458a66f8ca43ba209641", date: "2022-12-05 19:46:42 UTC", description: "Add enrichment tables to list subcommand", pr_number: 15446, scopes: ["cli"], type: "feat", breaking_change: false, author: "jordan", files_count: 1, insertions_count: 12, deletions_count: 1},
		{sha: "4179b95209fdbb785c3b5b9dc1abd76f3f95348f", date: "2022-12-06 03:47:33 UTC", description: "fix source TLS docs", pr_number: 15187, scopes: ["nats"], type: "docs", breaking_change: false, author: "Alexander Zaitsev", files_count: 2, insertions_count: 102, deletions_count: 0},
		{sha: "ba43f5af731a35eeed178f2f2dd10a9f30cb5c04", date: "2022-12-05 18:43:57 UTC", description: "Add log namespace and schema def support", pr_number: 15326, scopes: ["journald source"], type: "feat", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 269, deletions_count: 32},
		{sha: "a05dbaf685fc741f407eef540262350176aadbdd", date: "2022-12-05 21:37:24 UTC", description: "bump pest from 2.5.0 to 2.5.1", pr_number: 15451, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "69b5654e1fc2aadfbd3d5e02b989ea53bb556503", date: "2022-12-05 21:38:13 UTC", description: "bump syn from 1.0.104 to 1.0.105", pr_number: 15449, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "121f2be5b97fdf1eb10ad07560a77c91f5bfe31e", date: "2022-12-05 21:38:36 UTC", description: "bump num-format from 0.4.3 to 0.4.4", pr_number: 15450, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "bea39007a12bf2a98de4a184773cf0dc4cd58cdc", date: "2022-12-05 21:46:39 UTC", description: "bump chrono-tz from 0.8.0 to 0.8.1", pr_number: 15409, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "4c88fae4cbbfa573ca42d30de45c90c149102950", date: "2022-12-06 00:31:45 UTC", description: "RFC for a tooling revamp", pr_number: 15056, scopes: ["dev"], type: "chore", breaking_change: false, author: "Ofek Lev", files_count: 1, insertions_count: 200, deletions_count: 0},
		{sha: "3114cac7f1e168174fa3d642763cbafeea708b19", date: "2022-12-06 01:05:24 UTC", description: "add unix path to err msg when listening fails", pr_number: 15391, scopes: ["unix source"], type: "enhancement", breaking_change: false, author: "Scott Opell", files_count: 1, insertions_count: 7, deletions_count: 1},
		{sha: "ab75d8926db6eed2ea6170bd2adfbf0c2b313dc2", date: "2022-12-06 01:13:59 UTC", description: "bump axum from 0.5.17 to 0.6.0", pr_number: 15412, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 57},
		{sha: "cb724c1f431c969ecfeb061dafd538eac7e49f3a", date: "2022-12-05 22:29:41 UTC", description: "Fix markdown", pr_number: 15457, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3af832a0910f0379e22b05dfa3b659c747358756", date: "2022-12-06 06:48:52 UTC", description: "Document the use of hyphens in key names in input data", pr_number: 15454, scopes: ["unit tests"], type: "docs", breaking_change: false, author: "Danny Staple", files_count: 1, insertions_count: 11, deletions_count: 0},
		{sha: "debf9cc04c0e78f67a9e208f880d225a38caf510", date: "2022-12-06 03:53:55 UTC", description: "Refactor javascript component of web playground", pr_number: 15322, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jonathan Padilla", files_count: 2, insertions_count: 224, deletions_count: 177},
		{sha: "9b70b5a48cde5a998009cdab84cd3d22b611a002", date: "2022-12-06 01:14:43 UTC", description: "Bump version to 0.27.0", pr_number: 15458, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "6b61a78eb3cb1d3c5c33c094339db645b1d9c767", date: "2022-12-06 01:24:48 UTC", description: "Upgrade manifests to 0.18.0 of Helm chart", pr_number: 15459, scopes: ["kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "272a238b74d90f623f635fa52cdeb490b362749c", date: "2022-12-06 18:40:07 UTC", description: "Convert `EventsSent` to a registered event", pr_number: 14724, scopes: ["observability"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 19, insertions_count: 385, deletions_count: 358},
		{sha: "c8b3c1c2cb39d7538b8c46bec3a3fbbbefa7c96c", date: "2022-12-06 20:03:35 UTC", description: "bump axum from 0.6.0 to 0.6.1", pr_number: 15466, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "c3a9be3b418943c17404d1d75361979af1f09c90", date: "2022-12-06 20:04:44 UTC", description: "bump async-trait from 0.1.58 to 0.1.59", pr_number: 15464, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c1086f38441336ca3f0e555ccd6e3709e7d9625f", date: "2022-12-06 20:06:56 UTC", description: "bump libc from 0.2.137 to 0.2.138", pr_number: 15463, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8730389d9e1ed70cff577aadbc0ddfa71f7f8e1c", date: "2022-12-06 20:07:12 UTC", description: "bump pest_derive from 2.5.0 to 2.5.1", pr_number: 15462, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "bde93dc122d9f2eee657bfc6951b4c32e961b7b8", date: "2022-12-06 21:28:14 UTC", description: "bump serde from 1.0.148 to 1.0.149", pr_number: 15465, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "999ff60ee625ce44dbbcfc1a11b89da4c92da490", date: "2022-12-06 21:29:29 UTC", description: "bump express from 4.17.2 to 4.18.2 in /website", pr_number: 15471, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 134, deletions_count: 110},
		{sha: "69d8ca6c37009f78e70265deca5dddf9fd2d7e88", date: "2022-12-06 23:06:07 UTC", description: "fix up some of the generated descriptions of various shared config types", pr_number: 15470, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 61, insertions_count: 2032, deletions_count: 708},
		{sha: "e224705d69ebbfbfe3063beea8cc899c0350744a", date: "2022-12-06 21:13:22 UTC", description: "Add migration example for geoip transform", pr_number: 15475, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 70, deletions_count: 0},
		{sha: "b10e017a6a5dadb79e27c5f736caf6f0274f57fe", date: "2022-12-07 01:33:12 UTC", description: "Add log namespace and schema support", pr_number: 15437, scopes: ["aws_sqs source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 103, deletions_count: 15},
		{sha: "898b4fec6b168e6cf2c1877e58ab85d0605aaf6e", date: "2022-12-07 01:19:32 UTC", description: "Add macro to help implementing registered internal events", pr_number: 14753, scopes: ["observability"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 196, deletions_count: 162},
		{sha: "d78c87bc2598e5815eccaa7f836b636b5fbc2309", date: "2022-12-07 19:47:01 UTC", description: "Update config example due to breaking changes", pr_number: 15486, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 20, deletions_count: 17},
		{sha: "86daf18232f8d66c8d6a3a2a4593c9ea0348e370", date: "2022-12-07 20:03:53 UTC", description: "fix examples of automatic namespacing", pr_number: 15487, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 0, deletions_count: 3},
		{sha: "522b6dd510d2314dd42310f03d7afdaaf6825d3c", date: "2022-12-07 20:55:08 UTC", description: "bump data-encoding from 2.3.2 to 2.3.3", pr_number: 15481, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "7551b1e7dfc95a9659e03ab1d5cdda5507e8a6c5", date: "2022-12-07 18:04:19 UTC", description: "Add missing paren to 0.26.0 upgrade guide", pr_number: 15480, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "8558ec3defd6769a8dc0961d2d394e7119969e21", date: "2022-12-08 06:07:11 UTC", description: "support PEM encoded certificates", pr_number: 15448, scopes: ["kafka"], type: "fix", breaking_change: false, author: "Maksim Nabokikh", files_count: 3, insertions_count: 28, deletions_count: 7},
		{sha: "ad47f9539a87d66d8bdec584b6e08a9d373c6fd1", date: "2022-12-07 21:07:42 UTC", description: "Update smp, lading, and otlp config to try and fix experiment", pr_number: 15479, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 5, deletions_count: 4},
		{sha: "383b4131124ed8acd530dde0d9062fea4ea5052b", date: "2022-12-07 22:00:17 UTC", description: "Add multi-value metric tag support to the `remap` transform.", pr_number: 15272, scopes: ["metrics"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 9, insertions_count: 187, deletions_count: 25},
		{sha: "9c04cfd890f63e63c2de3b8857c9e83a261e8ddf", date: "2022-12-07 22:05:26 UTC", description: "generate better schema for `Compression` that leads to better docs output", pr_number: 15476, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 42, insertions_count: 625, deletions_count: 617},
		{sha: "65f352d919f4703049a54230ec74a09147192a39", date: "2022-12-07 23:26:54 UTC", description: "bump tokio from 1.22.0 to 1.23.0", pr_number: 15482, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "39d2cf7e36e8b9c299e79975a1bde0267d23a96e", date: "2022-12-07 23:27:34 UTC", description: "bump openssl from 0.10.43 to 0.10.44", pr_number: 15483, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "550aa8b1f50c7674fe367d02d8d9777ed35d89d7", date: "2022-12-08 13:39:27 UTC", description: "Fix panic when reloading config under load", pr_number: 14875, scopes: ["core"], type: "fix", breaking_change: false, author: "Zettroke", files_count: 2, insertions_count: 73, deletions_count: 41},
		{sha: "21a093e669f9e963bb90f0b7d660ce83642e6526", date: "2022-12-08 20:05:49 UTC", description: "Allow `namespace_labels` to be disabled", pr_number: 15493, scopes: ["kubernetes_logs source"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 43, deletions_count: 38},
		{sha: "5775c5539166d82e96cfefe0ff1e10d296d7c7d8", date: "2022-12-09 01:46:23 UTC", description: "Add log namespace and schema def support", pr_number: 15342, scopes: ["splunk_hec source"], type: "feat", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 318, deletions_count: 70},
		{sha: "c922ca29e1a34d93e6f3fe13bd60e5a35f4284b3", date: "2022-12-09 00:26:53 UTC", description: "rearrange integration test files to speed up tests", pr_number: 15491, scopes: ["core"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 9, insertions_count: 30, deletions_count: 15},
		{sha: "22f0df4feff7dd11c0c3b30e532b312b23ba8165", date: "2022-12-08 23:36:29 UTC", description: "Test the rest of the global option merging", pr_number: 15461, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 103, deletions_count: 9},
		{sha: "ff9d66dbc0d43a4198bd5f1cdb14f00e366970a1", date: "2022-12-09 08:48:37 UTC", description: "gracefully shutdown Vector if API cannot bind", pr_number: 15087, scopes: ["api"], type: "fix", breaking_change: false, author: "Alexander Zaitsev", files_count: 16, insertions_count: 112, deletions_count: 70},
		{sha: "4e55b2827169be8b4e66d1c2ee4b7f758fadb285", date: "2022-12-08 21:59:09 UTC", description: "Pin cloudsmith github action version", pr_number: 15490, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "2e897ab4a1fb4d09e6971010e64109cf2afc3edc", date: "2022-12-09 01:30:36 UTC", description: "Gunzip Elasticsearch responses ", pr_number: 15478, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "David Huie", files_count: 6, insertions_count: 63, deletions_count: 6},
		{sha: "36fb42b331b0a8ba0f230a272b9ffdb481f997dc", date: "2022-12-09 05:10:20 UTC", description: "set log namespace in decoder config", pr_number: 15510, scopes: ["socket source"], type: "fix", breaking_change: false, author: "David Huie", files_count: 2, insertions_count: 18, deletions_count: 16},
		{sha: "6af497183e95ecd2ffae7619ba21cb4df6320de1", date: "2022-12-09 15:47:52 UTC", description: "Do not publish nightlies to CloudSmith", pr_number: 15506, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "e00c372ce7f0f2f2046f506febb655b8c05cffca", date: "2022-12-10 01:09:49 UTC", description: "Add log namespace and schema def support", pr_number: 15386, scopes: ["syslog source"], type: "feat", breaking_change: false, author: "Stephen Wakely", files_count: 4, insertions_count: 464, deletions_count: 65},
		{sha: "c8d2bc4d4499b9e63a290e5ff5d578f05259998e", date: "2022-12-10 04:17:09 UTC", description: "add custom producer name support", pr_number: 15151, scopes: ["pulsar sink"], type: "enhancement", breaking_change: false, author: "Alexander Zaitsev", files_count: 3, insertions_count: 41, deletions_count: 23},
		{sha: "f2aa9b8471d6fb4b68882086a980c588ac2a7b5e", date: "2022-12-09 20:33:40 UTC", description: "Add meta tag for domain verification", pr_number: 15509, scopes: ["template website"], type: "enhancement", breaking_change: false, author: "David Weid II", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "a95b755ce775dd0d1c292edef06d150e16fa2869", date: "2022-12-09 22:52:40 UTC", description: "bump cloudsmith-io/action from 0.5.2 to 0.5.3", pr_number: 15515, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "adf9f50f01f699c89890b6b1b1b661fceebfe871", date: "2022-12-12 18:36:45 UTC", description: "Add multi-value metric tag support to the `lua` transform", pr_number: 15402, scopes: ["lua transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 4, insertions_count: 327, deletions_count: 94},
		{sha: "add1141740b44af0d4014611d1c1c17cf8581b16", date: "2022-12-12 18:37:21 UTC", description: "Add multi-value metric tag support to the `tag_cardinality_limit` transform", pr_number: 15505, scopes: ["tag_cardinality_limit transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 6, insertions_count: 622, deletions_count: 547},
		{sha: "0bd85bb33d14f59ec166c82831de1b821adaf148", date: "2022-12-13 10:55:10 UTC", description: "add support for fetching ec2 instance tags", pr_number: 15314, scopes: ["aws_ec2_metadata transform"], type: "feat", breaking_change: false, author: "Ben LeFevre", files_count: 4, insertions_count: 147, deletions_count: 33},
		{sha: "ab173ff72cbe1978f25c3b10f867e24e6f5e60fa", date: "2022-12-12 20:18:16 UTC", description: "Convert `EventsReceived` to a registered event", pr_number: 15477, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 34, insertions_count: 463, deletions_count: 471},
		{sha: "cd85d5655a4204c24bc9de11f4519bef545ecaa9", date: "2022-12-12 20:44:02 UTC", description: "Convert stream sink driver to registered `BytesSent`", pr_number: 15503, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 34, insertions_count: 234, deletions_count: 201},
		{sha: "34a2d4e266e5259ecae5d35e942b6160ca85cbe9", date: "2022-12-12 20:24:04 UTC", description: "insert TLS metadata correctly in Vector namespaces", pr_number: 15511, scopes: ["socket source"], type: "fix", breaking_change: false, author: "David Huie", files_count: 7, insertions_count: 181, deletions_count: 12},
		{sha: "908cc67936116b9b557b08fbd4e10726973cad79", date: "2022-12-13 00:01:36 UTC", description: "automatic component validation and verification", pr_number: 15140, scopes: ["rfc", "observability"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 31, insertions_count: 2861, deletions_count: 66},
		{sha: "eb565fdf4c2bde59c2eb9740d0ca56565716ac2c", date: "2022-12-13 08:24:02 UTC", description: "add abs function", pr_number: 15532, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 4, insertions_count: 154, deletions_count: 0},
		{sha: "e419eb8358cf0d91443bf7d0bc317e83c31fe8fb", date: "2022-12-13 00:46:54 UTC", description: "bump prost-build from 0.11.3 to 0.11.4", pr_number: 15537, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "56d3058d5e9b34c0e9d242842cea295409e8ca8e", date: "2022-12-12 22:08:39 UTC", description: "Enable runtime allocation tracking by default. ", pr_number: 15441, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 5, insertions_count: 105, deletions_count: 106},
		{sha: "ca83f7f7d4efc84f2727bda384a68b1a19fc891a", date: "2022-12-13 01:29:49 UTC", description: "Update Regression Detector lading, smp versions", pr_number: 15558, scopes: ["ci"], type: "feat", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "0ec5232551fe5cb1d86d0024a85e0f112c80243b", date: "2022-12-13 03:32:07 UTC", description: "fix bug in component validation runner", pr_number: 15560, scopes: ["core"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 4, deletions_count: 3},
		{sha: "1fc16c461293f60fe99c98cec5554faf27104bb9", date: "2022-12-13 05:54:17 UTC", description: "taps lost during config reload", pr_number: 15400, scopes: ["tap"], type: "fix", breaking_change: false, author: "Michael Penick", files_count: 2, insertions_count: 44, deletions_count: 19},
		{sha: "16b05d3afc012e3b60423affbf664b452e1b74c4", date: "2022-12-13 19:20:10 UTC", description: "remove panic from filter fn", pr_number: 15508, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 6, deletions_count: 1},
		{sha: "94c3c0880f8da1979a0bdad761a91df350c69968", date: "2022-12-13 18:32:40 UTC", description: "bump cargo-deb and cargo-nextest to latest versions", pr_number: 15563, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "e6f5f8c94199c13bdd244ef45ef857eae7d0a89f", date: "2022-12-13 18:33:05 UTC", description: "Convert ARC metrics to registered events", pr_number: 15545, scopes: ["sinks"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 55, deletions_count: 57},
		{sha: "2486d549d0bc1d244fc098b6e5794d96a3f192e0", date: "2022-12-13 16:50:07 UTC", description: "Brush off trace support docs", pr_number: 15561, scopes: ["datadog_agent source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 14, deletions_count: 4},
		{sha: "8eac72562c0522ea2607edb7652720fa5a0716e9", date: "2022-12-13 20:46:27 UTC", description: "Allow up to 30 metric dimensions", pr_number: 15559, scopes: ["aws_cloudwatch_metrics sink"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 44, deletions_count: 2},
		{sha: "2179049c052f4f7b07797d6da05da2f33f213805", date: "2022-12-13 19:01:16 UTC", description: "set log namespace in decoding config", pr_number: 15556, scopes: ["kafka source"], type: "fix", breaking_change: false, author: "David Huie", files_count: 1, insertions_count: 11, deletions_count: 8},
		{sha: "947f2ad2cc226863c4b34d2f395524d54eb2712b", date: "2022-12-14 04:47:53 UTC", description: "Add option to translate custom fields in `parse_cef`", pr_number: 15208, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 2, insertions_count: 181, deletions_count: 10},
		{sha: "14f1ad59c5ba5d9bdf3a7280234a99033874eab5", date: "2022-12-14 01:43:55 UTC", description: "Better handling of host_key input with OptionalValuePath", pr_number: 15574, scopes: ["syslog source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 28, deletions_count: 22},
		{sha: "cca596c5a3acaee1253e64a39946dff7d2a7def0", date: "2022-12-14 02:24:20 UTC", description: "Better handling of key configuration with OptionalValuePath", pr_number: 15575, scopes: ["amqp source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 37, deletions_count: 39},
		{sha: "815f0b9b99b6dffcbb61b09d0a44f1c124cbd809", date: "2022-12-14 03:10:30 UTC", description: "Better handling of key configuration with OptionalValuePath", pr_number: 15576, scopes: ["http_server source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 9, deletions_count: 10},
		{sha: "070278678556fe5b6fedebe2a202adaa7699bb32", date: "2022-12-15 05:28:43 UTC", description: "Bump `anymap` version", pr_number: 15580, scopes: ["deps"], type: "chore", breaking_change: false, author: "boraarslan", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1ffc41ef508fe92f0479417c8f55372b15ff9c12", date: "2022-12-14 22:22:16 UTC", description: "add syntax highlighting to playground", pr_number: 15557, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Jonathan Padilla", files_count: 2, insertions_count: 649, deletions_count: 5},
		{sha: "bfd9d6046134be96ef29b8a940362328609a7932", date: "2022-12-14 21:51:22 UTC", description: "Add a blog related to the allocation tracking work ", pr_number: 15544, scopes: ["blog website"], type: "feat", breaking_change: false, author: "Arshia Soleimani", files_count: 3, insertions_count: 47, deletions_count: 0},
		{sha: "d955aa39e228a0b2aabbfcb24a7ded5cf4b436e6", date: "2022-12-15 00:09:52 UTC", description: "Remove pipelines transform and macro expansion", pr_number: 15577, scopes: ["transforms"], type: "chore", breaking_change: true, author: "Luke Steensen", files_count: 40, insertions_count: 25, deletions_count: 19205},
		{sha: "e0f34d46ed033772e86117d13d72585a0d6514c5", date: "2022-12-14 23:35:47 UTC", description: "bump serde from 1.0.149 to 1.0.150", pr_number: 15536, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "78dd939d1c1dd18d6cbb873667b3629172648df1", date: "2022-12-15 04:13:52 UTC", description: "Make IMDS client configurable for AWS authentication", pr_number: 15518, scopes: ["aws config"], type: "enhancement", breaking_change: false, author: "Kevin Park", files_count: 16, insertions_count: 506, deletions_count: 12},
		{sha: "0ad0e83b3f15135e6fe6c46aecbd300aa0fe71c0", date: "2022-12-15 19:08:04 UTC", description: "use configured `LogNamespace` in `DecoderConfig` construction", pr_number: 15588, scopes: ["http_server source", "heroku_logs source"], type: "fix", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "bfddcb68b04ebcbbf40eb3cbb614fd15fa03b9c5", date: "2022-12-15 19:54:15 UTC", description: "Improve allocation tracing docs", pr_number: 15608, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "6662c54f33898195b9f6192f3d86e5f7f9a61b93", date: "2022-12-16 04:43:03 UTC", description: "add tests for duplicate labels", pr_number: 15605, scopes: ["prometheus_scrape source", "prometheus_remote_write source", "prometheus_exporter sink", "prometheus_remote_write sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 4, insertions_count: 200, deletions_count: 3},
		{sha: "389b6bebf6fc37f01c5bb6e26d016e0c936b9549", date: "2022-12-16 01:02:21 UTC", description: "Improve host_key handling with OptionalValuePath", pr_number: 15610, scopes: ["docker_logs source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 18, deletions_count: 16},
		{sha: "8204e0b2380fdfa394b3a094fca92980ef7e9553", date: "2022-12-16 01:27:25 UTC", description: "Use OptionalValuePath for host_key", pr_number: 15613, scopes: ["file_descriptor source", "stdin source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 32, deletions_count: 26},
		{sha: "67f8d3511debf3e82af1c240a264147510cf9d4a", date: "2022-12-16 10:41:53 UTC", description: "implement graceful shutdown", pr_number: 15529, scopes: ["prometheus"], type: "fix", breaking_change: false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 9, deletions_count: 10},
		{sha: "a39ba040b60b62ecf64afbd663b1da7cee582cdf", date: "2022-12-16 08:02:35 UTC", description: "Use OptionalValuePath for redis_key", pr_number: 15615, scopes: ["redis source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 19, deletions_count: 18},
		{sha: "d7738a39486da5f552a5f7b39f8a598be79ad3ea", date: "2022-12-17 02:23:27 UTC", description: "document handling of duplicate tags", pr_number: 15606, scopes: ["prometheus_scrape source", "prometheus_remote_write source", "prometheus_exporter sink", "prometheus_remote_write sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 4, insertions_count: 36, deletions_count: 0},
		{sha: "680ba604ed71d34d804ca515f5e6ef4e7d6ca890", date: "2022-12-17 02:52:42 UTC", description: "update docs re duplicate tags", pr_number: 15618, scopes: ["gcp_stackdriver_metrics sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 7, deletions_count: 0},
		{sha: "98d687dc3828da5e15a79f2a097fd8cd80b7bf63", date: "2022-12-17 02:44:49 UTC", description: "bootstrap docs conversion to machine-generated Cue for sources/sinks", pr_number: 15502, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 137, insertions_count: 4265, deletions_count: 4383},
		{sha: "dfb3a11dedaa521cda87ca3fda889a4805d000a0", date: "2022-12-20 04:37:26 UTC", description: "implement graceful shutdown", pr_number: 15528, scopes: ["http_server"], type: "fix", breaking_change: false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 14, deletions_count: 7},
		{sha: "1bfd98a844f27bb231b86ac477a8f7f1935d743a", date: "2022-12-20 04:22:18 UTC", description: "allow overriding content-type header", pr_number: 15625, scopes: ["http sink"], type: "enhancement", breaking_change: false, author: "Francesco Degrassi", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "60a9dc44177f3276657e8ee939f112d0ff4c95dd", date: "2022-12-20 07:11:16 UTC", description: "enable Pulsar sink compression", pr_number: 15519, scopes: ["pulsar"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 7, insertions_count: 102, deletions_count: 10},
		{sha: "b0c758601723a84a7b5af06ed109b0e22ac9c8b9", date: "2022-12-20 05:08:38 UTC", description: "Replace regex with a simple parser for CRI logs", pr_number: 15591, scopes: ["kubernetes_logs source"], type: "enhancement", breaking_change: false, author: "Alex- Savitskii", files_count: 1, insertions_count: 111, deletions_count: 100},
		{sha: "3e4886441e15dee5f1c6ffd956ffcb4d5784e9ff", date: "2022-12-20 00:43:34 UTC", description: "remove mentions of Vector from source descriptions", pr_number: 15624, scopes: ["docs"], type: "chore", breaking_change: false, author: "May Lee", files_count: 25, insertions_count: 94, deletions_count: 107},
		{sha: "63e72ea2b3075820bf250d0a640656c60473502b", date: "2022-12-20 00:54:17 UTC", description: "Add log namespace and schema support", pr_number: 15469, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 8, insertions_count: 449, deletions_count: 112},
		{sha: "c5088fbdd41ce742d685b8d5bf3907f2335716ae", date: "2022-12-20 12:21:04 UTC", description: "added subject key in event metadata", pr_number: 15447, scopes: ["nats source"], type: "enhancement", breaking_change: false, author: "Timur Makarchuk", files_count: 3, insertions_count: 72, deletions_count: 6},
		{sha: "1c0716f75f1e01870ee9075ef966e839fa7b3d8a", date: "2022-12-20 03:58:19 UTC", description: "generate component docs for nats source", pr_number: 15643, scopes: ["external docs"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 4},
		{sha: "4728300eaf795599084e0b60218ed4e620acf650", date: "2022-12-20 20:48:41 UTC", description: "remove mentions of Vector from sinks and transforms descriptions", pr_number: 15634, scopes: ["docs"], type: "chore", breaking_change: false, author: "May Lee", files_count: 43, insertions_count: 59, deletions_count: 61},
		{sha: "e7710fcf9882edaf5d984188e2617048490a1c48", date: "2022-12-20 20:04:34 UTC", description: "Add compatibility support for enhanced tags", pr_number: 15626, scopes: ["metric_to_log transform"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 145, deletions_count: 16},
		{sha: "07738d78790b03a13c838c7a576425aa0f3cccc8", date: "2022-12-20 21:24:11 UTC", description: "Fix schema def for the host key path", pr_number: 15645, scopes: ["heroku_logs source"], type: "fix", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "0ea9236f185ace2739292b66de88a3aa387c85f9", date: "2022-12-20 21:24:24 UTC", description: "Use the global log schema for the legacy host key", pr_number: 15647, scopes: ["exec source"], type: "fix", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 5, deletions_count: 3},
		{sha: "d55f3cea2371e5083cfa44c99dbf1d508910fa4c", date: "2022-12-20 22:45:49 UTC", description: "Source doesn't support tls client metadata enrichment", pr_number: 15655, scopes: ["vector source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "c9859e1c484d8b006e383ba14561f8d54971cb16", date: "2022-12-20 22:51:42 UTC", description: "Use OptionalValuePath instead of String for key config", pr_number: 15654, scopes: ["kafka source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 51, deletions_count: 50},
		{sha: "a44af6cc585b0fec6cea247b94a99c4c601d5684", date: "2022-12-20 22:55:24 UTC", description: "Replace String with OptionalValuePath in config file", pr_number: 15646, scopes: ["nats source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 34, deletions_count: 26},
		{sha: "8fa0fa358a54b7d1e555e8b10b262f38b77e8dc2", date: "2022-12-20 23:08:19 UTC", description: "Use OptionalValuePath instead of String for host_key", pr_number: 15652, scopes: ["dnstap source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 33, deletions_count: 20},
		{sha: "4f57c793d3c9d2fafb223c5d0e0cceb2a910cac8", date: "2022-12-22 06:50:05 UTC", description: "Add licenses to unlicensed libs", pr_number: 15678, scopes: ["deps"], type: "chore", breaking_change: false, author: "boraarslan", files_count: 16, insertions_count: 2912, deletions_count: 0},
		{sha: "72ce41d5a62350d2be9967f87b44e9b17bbfd6de", date: "2022-12-22 00:53:24 UTC", description: "Use OptionalValuePath instead of String for client_metadata_key", pr_number: 15657, scopes: ["fluent source", "logstash source", "socket source", "statsd source", "syslog source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 34, deletions_count: 30},
		{sha: "a7001a7a37e5c385d6dfbb8a9c73b6289647299d", date: "2022-12-21 22:44:35 UTC", description: "Switch to apache_avro", pr_number: 15603, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 56, deletions_count: 69},
		{sha: "b1f8d75b6be81ad6f9164bbeabf5dc4de575e4b8", date: "2022-12-22 02:02:27 UTC", description: "Use OptionalValuePath in the tcp socket config", pr_number: 15665, scopes: ["socket"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 30, deletions_count: 29},
		{sha: "1f82c3ba94b9c6f57604810e4ef291d98b2e20d5", date: "2022-12-22 02:07:56 UTC", description: "Use OptionalValuePath in the udp socket config", pr_number: 15666, scopes: ["socket"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 28, deletions_count: 28},
		{sha: "57f0b411667d1e8246b4a9dabfafc88082fbacb1", date: "2022-12-22 02:19:36 UTC", description: "Remove http_datadog_filter_blackhole entirely", pr_number: 12743, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 5, insertions_count: 0, deletions_count: 418},
		{sha: "765f908f0f2fb1823759f64758390c6a6d775e89", date: "2022-12-22 08:48:39 UTC", description: "bump zstd from 0.11.2+zstd.1.5.2 to 0.12.1+zstd.1.5.2", pr_number: 15472, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 27, deletions_count: 6},
		{sha: "cd7a456a44df4ff8132ef0f8fd0c4737c4031935", date: "2022-12-22 09:05:22 UTC", description: "bump hashbrown from 0.12.3 to 0.13.1", pr_number: 15230, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 2},
		{sha: "9da919996af7bbacd3055b227f1ad3068e0e3f86", date: "2022-12-22 04:59:52 UTC", description: "Correct check names in trusted Regression Detector workflow", pr_number: 15684, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "124a46f83e5d8285e315a7899a14a176ded8e614", date: "2022-12-22 02:53:20 UTC", description: "Switch back to upstream openssl-src", pr_number: 15375, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 7},
		{sha: "90670d3565c7f09cf81af9abd87fe17358e4ecdf", date: "2022-12-22 16:25:03 UTC", description: "bump async-graphql-warp from 4.0.16 to 5.0.4", pr_number: 15632, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 96, deletions_count: 26},
		{sha: "075f854a93884c93bdd2e2f3242bc4c7eb00049d", date: "2022-12-22 16:33:43 UTC", description: "bump base64 from 0.13.1 to 0.20.0", pr_number: 15538, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 80, deletions_count: 57},
		{sha: "258848ac37ff367efca60d18b711a21081614c0d", date: "2022-12-22 20:36:15 UTC", description: "Use generated configuration docs", pr_number: 15682, scopes: ["aws_ecs_metrics source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 44, deletions_count: 79},
		{sha: "abce406e1a3124797e9f863904cc00c9be77a84e", date: "2022-12-22 20:49:33 UTC", description: "Use OptionalValuePath in the unix socket config", pr_number: 15667, scopes: ["socket"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 23, deletions_count: 20},
		{sha: "6500fc7465426b7e91503563571cdde2124fe524", date: "2022-12-22 19:24:57 UTC", description: "update `TlsConfig` with configurable examples", pr_number: 15658, scopes: ["docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 62, insertions_count: 445, deletions_count: 355},
		{sha: "95d697c3b622a8fd83d56f014920f37e9e9314d9", date: "2022-12-22 19:03:07 UTC", description: "bump snafu from 0.7.3 to 0.7.4", pr_number: 15693, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 11, deletions_count: 11},
		{sha: "392a319c9c38a262cc5bfd7b93f8522354567ab4", date: "2022-12-22 19:03:42 UTC", description: "bump clap from 4.0.29 to 4.0.30", pr_number: 15689, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 12, deletions_count: 12},
		{sha: "9510a9ac9251c07288ef76d19772478659577ee7", date: "2022-12-22 19:06:37 UTC", description: "bump thiserror from 1.0.37 to 1.0.38", pr_number: 15690, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "45403e64018c02b353b49b86cf2be4863b2a64dc", date: "2022-12-23 03:47:47 UTC", description: "bump prost from 0.11.3 to 0.11.5", pr_number: 15694, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 10, deletions_count: 10},
		{sha: "e87f38001a3d6b8f5b06221126ef7e797877eb39", date: "2022-12-22 21:25:43 UTC", description: "bump bstr from 1.0.1 to 1.1.0", pr_number: 15688, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "b9626ddc936bac522ac461d799dbe5880b941141", date: "2022-12-22 22:50:17 UTC", description: "support enhanced metric tags", pr_number: 15617, scopes: ["statsd source", "statsd sink"], type: "enhancement", breaking_change: true, author: "neuronull", files_count: 6, insertions_count: 122, deletions_count: 34},
		{sha: "162c7fe8ddb28d152af179110054074475b1c8a7", date: "2022-12-23 07:14:17 UTC", description: "Build packages with an older glibc to reenable centos7 and others.", pr_number: 15695, scopes: ["x86_64 platform"], type: "chore", breaking_change: false, author: "Florian Apolloner", files_count: 3, insertions_count: 14, deletions_count: 3},
		{sha: "c1223946fc36e856e413aa7f1512bacf2da653fe", date: "2022-12-23 02:03:17 UTC", description: "Use generated docs for configuration", pr_number: 15699, scopes: ["aws_kinesis_firehose source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 58, deletions_count: 88},
		{sha: "1c315daf6dfebe4c9bf1e66f1dde516a00f64b29", date: "2022-12-23 16:51:51 UTC", description: "bump typetag from 0.2.3 to 0.2.4", pr_number: 15713, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "f45c259931314bb669170d8319217fe6c83d3fd9", date: "2022-12-23 16:52:06 UTC", description: "bump indoc from 1.0.7 to 1.0.8", pr_number: 15711, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "e2d08efb90b087fa4aea9ef345b3d152d8519523", date: "2022-12-23 16:52:20 UTC", description: "bump serde_bytes from 0.11.7 to 0.11.8", pr_number: 15712, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "499688e417a4dfaac9dab16c09e86ebf7d2b1b58", date: "2022-12-23 19:57:19 UTC", description: "Use generated docs for configuration", pr_number: 15680, scopes: ["heroku_logs source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 21, deletions_count: 18},
		{sha: "e3e231f2cb48a04c17ae29e786ad283468b5eda2", date: "2022-12-23 17:20:54 UTC", description: "Remove misleading documentation about supported databases", pr_number: 15714, scopes: ["enriching"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 4},
		{sha: "a3c54891d63c37da227bb1d7ba83f30e84c8fc5a", date: "2022-12-24 01:35:09 UTC", description: "bump libc from 0.2.138 to 0.2.139", pr_number: 15706, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3c9e41f5041ab209e8f041027b28802bf6fc4726", date: "2022-12-24 03:10:42 UTC", description: "bump serde from 1.0.150 to 1.0.151", pr_number: 15710, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "a29c5fbad418b07504e81bea53dea467b55fa32c", date: "2022-12-23 23:41:06 UTC", description: "Update Kafka support", pr_number: 15722, scopes: ["kafka"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "859fe61141808d245d3cc8077027ad79d842416c", date: "2022-12-24 00:03:04 UTC", description: "Remove partitioning docs", pr_number: 15723, scopes: ["sinks"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 12, deletions_count: 22},
		{sha: "a50976abfe043ac1f06c0a0521970cba311f82cc", date: "2022-12-27 17:04:19 UTC", description: "bump openssl from 0.10.44 to 0.10.45", pr_number: 15729, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "08f5ce17b63c91fb99624adccd81eb0253a3ab8f", date: "2022-12-27 17:04:28 UTC", description: "bump anyhow from 1.0.66 to 1.0.68", pr_number: 15732, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "da9be5d6df22634fcf660ec40ab60bc41051af22", date: "2022-12-27 17:04:37 UTC", description: "bump async-trait from 0.1.59 to 0.1.60", pr_number: 15733, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7693c4a23dc4fb4b13847513252b3dac95850fa0", date: "2022-12-27 17:04:46 UTC", description: "bump semver from 1.0.14 to 1.0.16", pr_number: 15735, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a99c6ed7ae97c61acc2a28de22fdf271fc23d266", date: "2022-12-27 17:04:57 UTC", description: "bump toml from 0.5.9 to 0.5.10", pr_number: 15736, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 7, deletions_count: 7},
		{sha: "f8fac114e5c0a3e1760066acd8994d1bc558fab3", date: "2022-12-27 20:16:05 UTC", description: "Lock nextest install on Windows", pr_number: 15744, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a7590f98833c2cfc322a6f122155dd8b337f94fe", date: "2022-12-28 06:59:10 UTC", description: "bump serde_json from 1.0.89 to 1.0.91", pr_number: 15737, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "defc77147b5e3af0bc33340579006b8c68911527", date: "2022-12-28 17:26:44 UTC", description: "bump prettytable-rs from 0.9.0 to 0.10.0", pr_number: 15748, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "d1b86cd140eb6370a44d775c45bf1811ccc94983", date: "2022-12-28 17:26:57 UTC", description: "bump paste from 1.0.9 to 1.0.11", pr_number: 15749, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "3a995304d09327c4e72d354f04a44cc2e53a2219", date: "2022-12-28 17:27:11 UTC", description: "bump prost-types from 0.11.2 to 0.11.5", pr_number: 15750, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "0736816c153a4200279350065db6213270d82692", date: "2022-12-28 17:27:24 UTC", description: "bump serde_yaml from 0.9.14 to 0.9.16", pr_number: 15751, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "4753d85a87724bc1ee723d9301f61587547aa20b", date: "2022-12-28 17:27:40 UTC", description: "bump ryu from 1.0.11 to 1.0.12", pr_number: 15752, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "29ae7fb7ba1c4c33460a0555537c6e9476b1bfca", date: "2022-12-28 17:27:52 UTC", description: "bump syn from 1.0.105 to 1.0.107", pr_number: 15753, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "429b5dc2b848f7b04e4cbda0579e63dabfff1a67", date: "2022-12-28 17:28:04 UTC", description: "bump proc-macro2 from 1.0.47 to 1.0.49", pr_number: 15756, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "e62edc2d79227126c9d9b082e6f33f59387a061c", date: "2022-12-28 17:28:16 UTC", description: "bump dyn-clone from 1.0.9 to 1.0.10", pr_number: 15757, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "4560728e5ee393324bf7d20bdb63766a2cb50fe9", date: "2022-12-29 04:36:51 UTC", description: "typo fix", pr_number: 15747, scopes: ["docs"], type: "chore", breaking_change: false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "228addd2c6d9b3feb081275056effd6026c2be7a", date: "2022-12-28 22:31:20 UTC", description: "Add custom header configuration", pr_number: 15745, scopes: ["datadog_logs sink"], type: "enhancement", breaking_change: false, author: "Will", files_count: 8, insertions_count: 118, deletions_count: 79},
		{sha: "a605391e1f4a651d9427e250036c407af4db938d", date: "2022-12-29 00:11:37 UTC", description: "Remove warning lacking evidence", pr_number: 15766, scopes: ["docker_logs source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 8},
		{sha: "3f0404b3e3bc008037c294f5b80c3dd7f8dbc51c", date: "2022-12-29 17:27:09 UTC", description: "bump inventory from 0.3.2 to 0.3.3", pr_number: 15768, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e57a654ed089c3ba69c51066329d0f4e16853c22", date: "2022-12-29 17:27:20 UTC", description: "bump quote from 1.0.21 to 1.0.23", pr_number: 15769, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "0c153842f021c61cd7b81222254db635a2080fdd", date: "2022-12-29 17:27:30 UTC", description: "bump pest from 2.5.1 to 2.5.2", pr_number: 15770, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e8f6ec3260a14be781d1d34baf53912f686a020f", date: "2022-12-29 17:27:42 UTC", description: "bump serde from 1.0.151 to 1.0.152", pr_number: 15771, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "3cb55af87be81f23c3b143343ab1737538cb513b", date: "2022-12-29 17:27:53 UTC", description: "bump prettydiff from 0.6.1 to 0.6.2", pr_number: 15772, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 100},
		{sha: "982a6c7dc9eb1593bd627df3654d79ddb39d1af7", date: "2022-12-29 17:28:07 UTC", description: "bump prost-build from 0.11.4 to 0.11.5", pr_number: 15773, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "06e1f29c02d918a01072610fba437d51d0dd2d3a", date: "2022-12-29 17:28:20 UTC", description: "bump clap from 4.0.30 to 4.0.32", pr_number: 15774, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 12, deletions_count: 12},
		{sha: "0b65417cfffbb0b23d5681be5d81adea71cd9cd3", date: "2022-12-29 17:28:30 UTC", description: "bump inherent from 1.0.2 to 1.0.3", pr_number: 15775, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "3823b4e60e0504d06c492a28e7254bd196374a6d", date: "2022-12-29 17:55:27 UTC", description: "bump json5 from 2.2.0 to 2.2.2 in /website", pr_number: 15777, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 6},
		{sha: "266823b58545e44289185b60a51f0676c1d6ab0b", date: "2022-12-29 22:52:21 UTC", description: "remove vector from component descriptions that were missed", pr_number: 15779, scopes: ["docs"], type: "chore", breaking_change: false, author: "May Lee", files_count: 60, insertions_count: 108, deletions_count: 108},
		{sha: "21834e7f43110cd8f35c3c4b605f1e9005ff3c2a", date: "2022-12-30 17:30:02 UTC", description: "Remove unneeded deny for RUSTSEC-2020-0159", pr_number: 15781, scopes: ["security"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 8},
		{sha: "21d39317fb3268e5e26c81fdac41d9664e729251", date: "2022-12-31 22:19:12 UTC", description: "bump pest_derive from 2.5.1 to 2.5.2", pr_number: 15783, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 9},
		{sha: "0feff4bb7b62d1246def7d1e969af164bc5febca", date: "2023-01-04 01:07:36 UTC", description: "Rename option to `metric_tag_values`", pr_number: 15717, scopes: ["metric_to_log transform"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 105, deletions_count: 74},
		{sha: "c491c2dbd0e1d3bf165c33e61a13c3e9b9e85294", date: "2023-01-04 04:05:54 UTC", description: "Fix year-dependent tests for 2023", pr_number: 15797, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 4, deletions_count: 4},
		{sha: "a319267f99085abb793ed5eb4796c959e9fd606a", date: "2023-01-04 09:18:13 UTC", description: "Add some metric tag model documentation", pr_number: 15721, scopes: ["data model"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 89, deletions_count: 11},
		{sha: "66107d8befcc671c40d1e922c96d7c4aaa6e2e8e", date: "2023-01-04 15:09:54 UTC", description: "bump once_cell from 1.16.0 to 1.17.0", pr_number: 15784, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 11, deletions_count: 11},
		{sha: "0fa073528dd43ac52319214269a6cf65aeea98d2", date: "2023-01-04 15:33:50 UTC", description: "bump webbrowser from 0.8.2 to 0.8.4", pr_number: 15794, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 41, deletions_count: 4},
		{sha: "531092d86b0f38bba3526070316a3eee10950125", date: "2023-01-04 15:34:05 UTC", description: "bump async-graphql from 5.0.4 to 5.0.5", pr_number: 15799, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 36, deletions_count: 92},
		{sha: "7d0968b521d85b0441f402c0b4a3f9502985fd2b", date: "2023-01-04 19:48:44 UTC", description: "Document Proptest as preferred property testing framework", pr_number: 15803, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "523f10099df755452373c62838d5b985c3d5c1f2", date: "2023-01-05 01:30:10 UTC", description: "bump nom from 7.1.1 to 7.1.2", pr_number: 15791, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "db5d9ad20be94afbfb6be0e07c73310bc35a8f0e", date: "2023-01-05 01:36:23 UTC", description: "bump lru from 0.8.1 to 0.9.0", pr_number: 15788, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f3530495769f35830886660f3acb262aedb4beaa", date: "2023-01-05 01:37:28 UTC", description: "bump cidr-utils from 0.5.9 to 0.5.10", pr_number: 15792, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "bfad26604df8982ec60a438d284f9862e31edc3e", date: "2023-01-05 01:45:53 UTC", description: "bump enum_dispatch from 0.3.8 to 0.3.9", pr_number: 15785, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e00f4e77533a978fb2bf25098f35001ed3136e93", date: "2023-01-05 02:08:25 UTC", description: "fix documentation errors", pr_number: 15800, scopes: ["nats source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 5, insertions_count: 38, deletions_count: 24},
		{sha: "e1ebe4d0278c3374107a91b2cb241de12a2305da", date: "2023-01-05 02:11:13 UTC", description: "bump infer from 0.11.0 to 0.12.0", pr_number: 15793, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "94caa59cee55a518e9b861d77a99796260be07ab", date: "2023-01-05 02:13:03 UTC", description: "bump wiremock from 0.5.15 to 0.5.16", pr_number: 15789, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "323e48372c515fbcf43bd073da526b9489d82c57", date: "2023-01-04 21:56:49 UTC", description: "avoid duplicate span enter in message stream map", pr_number: 15653, scopes: ["file source"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "7e040435c8b61bf1518708bb85b9337160358ed5", date: "2023-01-04 20:44:27 UTC", description: "Use global timezone if configured", pr_number: 15740, scopes: ["remap transform"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 76, deletions_count: 16},
		{sha: "506c1cd368c9278acd7422eb0e432cf123c966d5", date: "2023-01-04 23:07:20 UTC", description: "Add RFC for Reloading API", pr_number: 15562, scopes: [], type: "chore", breaking_change: false, author: "Luke Steensen", files_count: 2, insertions_count: 400, deletions_count: 97},
		{sha: "4c898a4f5a5928ce1aa5c784603a2cb442184904", date: "2023-01-04 22:20:31 UTC", description: "Rmeove old soak tests", pr_number: 15814, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 88, insertions_count: 0, deletions_count: 2453},
		{sha: "0a86d9a920c6b0ef6839a6967b3dcf6c3f6544f6", date: "2023-01-05 07:43:43 UTC", description: "bump arc-swap from 1.5.1 to 1.6.0", pr_number: 15790, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "dc3faa1d9f1809b1d0e695139ec8ec95bc7a931e", date: "2023-01-05 01:55:24 UTC", description: "Add enhanced metric tag option to json and text encoders", pr_number: 15716, scopes: ["codecs"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 82, insertions_count: 1014, deletions_count: 314},
		{sha: "8117034dc45043fa61990bfba3c6095f2fe353b8", date: "2023-01-05 05:36:17 UTC", description: "support enhanced metric tags", pr_number: 15639, scopes: ["datadog_agent source", "datadog_metrics sink"], type: "enhancement", breaking_change: true, author: "neuronull", files_count: 6, insertions_count: 139, deletions_count: 66},
		{sha: "4f83ed287bd09fd881e67dac757655a7de1f6f5e", date: "2023-01-05 22:40:10 UTC", description: "update docs re duplicate tags", pr_number: 15807, scopes: ["splunk_hec sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "0cac1414d3d926bc1b6e537f00662b8f024dd34d", date: "2023-01-05 15:07:07 UTC", description: "bump async-graphql-warp from 5.0.4 to 5.0.5", pr_number: 15819, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8349ae23ecbb0a874c0ce2c3523d56fa79a312d0", date: "2023-01-05 15:07:27 UTC", description: "bump arbitrary from 1.2.0 to 1.2.2", pr_number: 15820, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "f6ae0ce23b41d068b82965a2cf98084f1384ed7d", date: "2023-01-05 15:07:42 UTC", description: "bump mlua from 0.8.6 to 0.8.7", pr_number: 15821, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "094e5692a84472325ea51cb767d88b83ccd1491f", date: "2023-01-05 15:07:56 UTC", description: "bump tokio from 1.23.0 to 1.23.1", pr_number: 15822, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "88a30db4d8aea6ac3576563ebff5694d75564e5d", date: "2023-01-05 19:46:07 UTC", description: "add support for any `serde_json`-capable value for `configurable` metadata KV pairs", pr_number: 15818, scopes: ["config"], type: "enhancement", breaking_change: false, author: "Toby Lawrence", files_count: 13, insertions_count: 143, deletions_count: 80},
		{sha: "95f2f3aa01bb0fbc1ab6df70ceb60ab7709c227a", date: "2023-01-05 16:54:33 UTC", description: "Remove unused soak workflows", pr_number: 15816, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 0, deletions_count: 1093},
		{sha: "b56193e6769ea01cf375f90c7da57f540f609d6c", date: "2023-01-05 20:22:12 UTC", description: "upgrade to rust 1.66.0", pr_number: 15093, scopes: [], type: "chore", breaking_change: false, author: "Luke Steensen", files_count: 181, insertions_count: 503, deletions_count: 549},
		{sha: "bca52c3d3d26c3993656eee844295af8a27eb221", date: "2023-01-06 03:29:58 UTC", description: "add test for duplicate metrics", pr_number: 15620, scopes: ["humio_metrics sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 44, deletions_count: 0},
		{sha: "253f4416ea8622dea68bdf20c7d23eb0f7f45ce9", date: "2023-01-06 17:20:05 UTC", description: "bump arr_macro from 0.1.3 to 0.2.1", pr_number: 15848, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 5},
		{sha: "65cb0c5e031127ee8fff1eb309ab0044843043a4", date: "2023-01-06 17:20:24 UTC", description: "bump tokio from 1.23.1 to 1.24.0", pr_number: 15849, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "65591020ffe2ce2999bef1a3aa1c15c43b868022", date: "2023-01-07 06:11:17 UTC", description: "Add Telegraf features to README", pr_number: 15585, scopes: [], type: "docs", breaking_change: false, author: "Thomas Casteleyn", files_count: 1, insertions_count: 16, deletions_count: 16},
		{sha: "f55e0cca916bf52b4d8a2eed5e502a776fe5d7c5", date: "2023-01-06 22:26:28 UTC", description: "Use generated docs", pr_number: 15847, scopes: ["apache_metrics"], type: "docs", breaking_change: false, author: "David Huie", files_count: 4, insertions_count: 21, deletions_count: 41},
		{sha: "eda64e7cd15054ddc98474783249e88b2a0a9fb9", date: "2023-01-07 02:40:01 UTC", description: "properly enforce specifying `docs::enum_tag_description` for internally tagged enums", pr_number: 15853, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 34, insertions_count: 146, deletions_count: 105},
		{sha: "aa086e3e98daa521e70fce0adcf9f83dcd6b019b", date: "2023-01-09 21:01:03 UTC", description: "bump glob from 0.3.0 to 0.3.1", pr_number: 15862, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "8cc6e8879594fac953356879fcf462a5592006b8", date: "2023-01-09 21:03:00 UTC", description: "bump async-trait from 0.1.60 to 0.1.61", pr_number: 15859, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0d96f881c6ae4ea0ea66561c9edd82f1b1571c18", date: "2023-01-09 21:33:14 UTC", description: "Use generated docs for config", pr_number: 15703, scopes: ["opentelemetry source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 141, deletions_count: 191},
		{sha: "f1daacb332973dc9433e757bfa8a3f253c72d0b0", date: "2023-01-09 21:47:39 UTC", description: "bump redis from 0.22.1 to 0.22.2", pr_number: 15858, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "124a4b85972bb9a7d386337e4e4ecd32a2ffd71c", date: "2023-01-10 12:47:51 UTC", description: "Allow records with different labels in a batch", pr_number: 15637, scopes: ["loki"], type: "enhancement", breaking_change: false, author: "atodekangae", files_count: 3, insertions_count: 78, deletions_count: 45},
		{sha: "d04826e4c2c7ce7f18d293d4d35c85f161b007d4", date: "2023-01-10 03:49:07 UTC", description: "add `decode_base16`, `encode_base16` functions", pr_number: 15851, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Will Apted", files_count: 8, insertions_count: 256, deletions_count: 0},
		{sha: "b56ea01c02d9c3ed06677619858b246b0306a37c", date: "2023-01-10 07:33:51 UTC", description: "add 'read_from' and 'ignore_older_secs' options", pr_number: 15746, scopes: ["kubernetes_logs"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 8, insertions_count: 101, deletions_count: 29},
		{sha: "24e9cb4305a124ce472ec00ee640e60d0f5dd92a", date: "2023-01-13 10:33:48 UTC", description: "Configure multiple access keys", pr_number: 15828, scopes: ["aws_kinesis_firehose_source"], type: "feat", breaking_change: false, author: "Dmitrijs Zubriks", files_count: 5, insertions_count: 168, deletions_count: 23},
		{sha: "0508745f9ecbee1dac0926ee06b6fda267957573", date: "2023-01-10 17:56:08 UTC", description: "bump axum from 0.6.1 to 0.6.2", pr_number: 15878, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "e52b7f3dd37eaf83400dc0cbe453d7886b206e48", date: "2023-01-12 20:49:46 UTC", description: "Add experimental reload API", pr_number: 15856, scopes: ["administration"], type: "feat", breaking_change: false, author: "Luke Steensen", files_count: 10, insertions_count: 359, deletions_count: 98},
	]
}
