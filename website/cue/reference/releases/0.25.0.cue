package metadata

releases: "0.25.0": {
	date:     "2022-10-31"
	codename: ""

	whats_next: []

	known_issues: [
		"""
			Vector fails to load configurations using environment variables outside of string values
			for configuration options. Fixed in 0.25.1.
			""",
		"""
			Vector fails to load multi-file configurations using the global `timezone` configuration
			option. Fixed in 0.25.1.
			""",
		"""
			The `prometheus_remote_write` sink doesn't support specifying the configuration
			`auth.bearer` as it should. Fixed in 0.25.1.
			""",
		"""
			The `abort` VRL function emits ERROR, rather than DEBUG, logs when discarding an event. Fixed in 0.25.2.
			""",
		"""
			The `azure_blob` sink incorrectly passes a redacted value for the `connection_string`, rather than the actual
			contents. Fixed in 0.25.2.
			""",
	]

	description: """
		The Vector team is pleased to announce version 0.25.0!

		Be sure to check out the [upgrade guide](/highlights/2022-10-04-0-25-0-upgrade-guide) for
		breaking changes in this release.

		In addition to the new features, enhancements, and fixes listed below, this release adds:

		- A new `http_client` source
		- A new `amqp` source and sink that supports AMQP 0.9.1 (used by RabbitMQ)
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["sink: datadog_metrics"]
			description: """
				The `datadog_metrics` sink now correctly aggregates counters emitted for the same
				timeseries within a single second (the timestamp granularity). Previously, it sent
				them through as-is to Datadog which processed them as last-write-wins.
				"""
			pr_numbers: [13960]
		},
		{
			type: "enhancement"
			scopes: ["sources", "transforms", "sinks", "observability"]
			description: """
				All components were audited to ensure compliance with the [component
				specification](https://github.com/vectordotdev/vector/blob/0c4d878a4574d4571a0ec7fb3990940034a42fc1/docs/specs/component.md)
				with respect to `component_discarded_events_total` and `component_errors_total`
				internal metrics. Generally this meant adding missing metrics and logs, missing metric
				and logs labels, and applying internal log rate limits consistently.
				"""
			pr_numbers: [
				14006, 14026, 14017, 14013, 14126, 14147, 14159, 14155, 14052, 14151, 14176, 14223,
				14217, 14302, 14300, 14245, 14327, 14323, 14329, 14229, 14225, 14356, 14118, 14357,
				14376, 14361, 14364, 14388, 13121, 14355, 14344, 14119, 14120, 14407, 14405, 14410,
				14122, 14406, 14422, 14431, 14435, 14218, 14439, 14123, 14448, 14434, 14465, 14466,
				14467, 14478, 14480, 14482, 14484, 14476, 14449, 14485, 14519, 14575, 14956, 14615,
				14634, 14638, 14658, 14667, 14670, 14425, 14770, 14768, 14767, 14844, 14843, 14842,
				14769, 14516, 14511, 14765, 14487, 14530, 14540, 14764, 14513, 14483, 14748, 14736,
				14715, 14731,
			]
		},
		{
			type: "feat"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				A [`chunks`](https://vrl.dev/functions#chunks) function was added to VRL to enable
				breaking up a text field into multiple chunks of equal or lesser length.
				"""
			pr_numbers: [13794]
			contributors: ["briankung"]
		},
		{
			type: "chore"
			scopes: ["vrl", "vrl: parser"]
			breaking: true
			description: """
				The deprecated `%` operator was removed from VRL in-lieu of the new
				[`mod`](https://vrl.dev/functions#mod) function that was added in v0.24.0.

				 Please see [the upgrade
				 guide](/highlights/2022-10-04-0-25-0-upgrade-guide#module-removal) for more
				 details.
				"""
			pr_numbers: [14111, 14011]
		},
		{
			type: "feat"
			scopes: ["source: http_client"]
			description: """
				A new [`http_client` source](/docs/reference/configuration/sources/http_client) has
				been added. This source makes HTTP requests to the configured endpoint, on an
				interval, and turns the response into events based on the configured framing and
				decoding options.
				"""
			pr_numbers: [13793]
		},
		{
			type: "enhancement"
			scopes: ["sink: file"]
			description: """
				The [`file` sink](/docs/reference/configuration/sinks/file) has added support for
				zstandard compressed output.
				"""
			contributors: ["hdhoang"]
			pr_numbers: [14037]
		},
		{
			type: "fix"
			scopes: ["source: internal_metrics"]
			breaking: true
			description: """
				The `internal_metrics` source now defaults to setting the `host` tag as this
				behavior seems to be less surprising to users. It can be suppressed by setting the
				`host_key` option to `""`.

				 Please see [the upgrade
				 guide](/highlights/2022-10-04-0-25-0-upgrade-guide#internal-metrics-host-tag)
				 for more details.
				"""
			pr_numbers: [14111, 14011]
		},
		{
			type: "feat"
			scopes: ["vrl", "vrl: stdlib", "config"]
			description: """
				Event metadata fields can now be referred to in VRL and configuration options that
				take event field paths by using the `%<field name>` syntax. For example, to refer to
				a metadata field on the event called `foo` you would use `%foo`. For now, most
				metadata fields are user-defined (e.g. `%foo ="bar"` in VRL), but in the future
				Vector will add more metadata, like event ingest timestamp.

				As part of this, the metadata functions in VRL (`set_metadata_field`,
				`get_metadata_field', and `remove_metadata_field`) have been deprecated.
				Instead, the new `%<metadata field>` syntax should be used to access, modify,
				and remove metadata fields using normal VRL path operations. For example,
				setting a metadata field of `foo` would look like `%foo = "bar"`. Please see
				[the upgrade
				guide](/highlights/2022-10-04-0-25-0-upgrade-guide#metadata-function-deprecation)
				for more details on the deprecation.
				"""
			pr_numbers: [14128, 14097, 14264]
		},
		{
			type: "chore"
			scopes: ["source: vector", "sink: vector"]
			breaking: true
			description: """
				The long deprecated `v1` protocol of the `vector` source and sink (indicated by
				specifying `version = "1"`) has finally been removed.

				 Please see [the upgrade
				 guide](/highlights/2022-10-04-0-25-0-upgrade-guide#vector-v1-removal)
				 for more details.
				"""
			pr_numbers: [14296, 14315]
		},
		{
			type: "fix"
			scopes: ["transform: lua"]
			description: """
				The [`lua` transform](/docs/reference/configuration/transforms/lua) can now load
				dynamically linked libraries. Previously the needed symbols were being stripped from
				Vector.
				"""
			pr_numbers: [14326]
		},
		{
			type: "enhancement"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				The [`parse_key_value`](https://vrl.dev/functions#parse_key_value) function now
				handles duplicate keys by grouping the values into an array.
				"""
			contributors: ["ktff"]
			pr_numbers: [14248]
		},
		{
			type: "feat"
			scopes: ["sink: prometheus_remote_write"]
			description: """
				The [`prometheus_remote_write`
				sink](/docs/reference/configuration/sinks/prometheus_remote_write) can now be
				used with [Amazon Managed Service for
				Prometheus](https://docs.aws.amazon.com/prometheus/latest/userguide/what-is-Amazon-Managed-Service-Prometheus.html)
				by using AWS request signing.
				"""
			pr_numbers: [14150]
			contributors: ["notchairmk"]
		},
		{
			type: "fix"
			scopes: ["sink: honeycomb"]
			description: """
				The [`honeycomb` sink](/docs/reference/configuration/sinks/honeycomb) now uses the
				correct timestamp field name.
				"""
			pr_numbers: [14417]
			contributors: ["McSick"]
		},
		{
			type: "feat"
			scopes: ["observability"]
			description: """
				The internal log rate limiting that Vector does to avoid flooding its output during
				catastrophic events is now configurable via `--internal-log-rate-limit` on the CLI
				or the `VECTOR_INTERNAL_LOG_RATE_LIMIT` environment variable. The default is 10
				seconds.
				"""
			pr_numbers: [14381, 14458]
		},
		{
			type: "enhancement"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				The [`del`](https://vrl.dev/functions#del) function now has an optional `compact`
				parameter that can be used to delete the parent of the path being deleted if there
				are no other fields (for objects) or elements (for arrays) in it.
				"""
			pr_numbers: [14314]
		},
		{
			type: "feat"
			scopes: ["source: amqp", "sink: amqp"]
			description: """
				A new `amqp` [source](/docs/reference/configuration/sources/amqp) and
				[sink](/docs/reference/configuration/sinks/amqp) have been added to receive or send
				data via the AMQP
				0.9.1 protocol, including RabbitMQ.
				"""
			contributors: ["dbcfd"]
			pr_numbers: [7120]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				A new [`build_info` internal
				metric](/docs/reference/configuration/sources/internal_metrics/#build_info) was
				added to report Vector version and build information. This can be useful for
				monitoring a fleet of Vector instances.
				"""
			pr_numbers: [14497]
		},
		{
			type: "feat"
			scopes: ["sink: pulsar"]
			description: """
				The [`pulsar` sink](/docs/reference/configuration/sinks/pulsar) now supports
				configuring which field name to use as the partition key in Pulsar via
				`partition_key_field`.
				"""
			pr_numbers: [14491]
			contributors: ["miton18"]
		},
		{
			type: "enhancement"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				A [`keys`](https://vrl.dev/functions#keys) and
				[`values`](https://vrl.dev/functions#values) function were added to VRL to fetch the
				keys and values, respectively, of an object, into an array.
				"""
			pr_numbers: [14441]
		},
		{
			type: "feat"
			scopes: ["sink: prometheus_exporter"]
			description: """
				The [`prometheus_exporter` sink](/docs/reference/configuration/sinks/prometheus_exporter) now supports
				configuring HTTP basic auth credentials to restrict access.
				"""
			pr_numbers: [14461]
			contributors: ["zamazan4ik"]
		},
		{
			type: "feat"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				The `parse_xml` VRL function avoids a panic due to certain invalid XML.
				"""
			pr_numbers: [14479]
			contributors: ["Zettroke"]
		},
		{
			type:     "chore"
			breaking: true
			scopes: ["source: vector"]
			description: """
				The unused `shutdown_timeout_secs` option on the `vector` source was removed. Please
				see the [upgrade
				guide](/highlights/2022-10-04-0-25-0-upgrade-guide#shutdown-timeout-secs) for
				more details.
				"""
			pr_numbers: [14479]
			contributors: ["Zettroke"]
		},
		{
			type: "fix"
			scopes: ["sink: clickhouse"]
			description: """
				The `clickhouse` sink now fills in unprovided URL parts with defaults, for example
				by prefixing `http://` if only an address and port are provided.
				"""
			pr_numbers: [14557]
			contributors: ["zamazan4ik"]
		},
		{
			type: "feat"
			scopes: ["sink: elasticsearch"]
			description: """
				The `elasticsearch` sink can now be configured to send data to multiple
				Elasticsearch instances via the new `endpoints` parameter.
				"""
			contributors: ["ktff"]
			pr_numbers: [14088]
		},
		{
			type: "feat"
			scopes: ["source: aws_s3"]
			description: """
				The `aws_s3` source now ignores the `s3:TestEvent` SQS messages that AWS sends when
				wiring up S3 bucket notifications. Previously, Vector would error when consuming
				these events.
				"""
			pr_numbers: [14572]
			contributors: ["bencord0"]
		},
		{
			type: "fix"
			scopes: ["source: syslog"]
			description: """
				The `syslog` source no longer panics when parsing invalid dates, instead it logs an
				error.
				"""
			pr_numbers: [14666]
		},
		{
			type: "feat"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				A [`parse_cef`](https://vrl.dev/functions#parse_cef) VRL function was added for
				parsing [ArcSight Common Event Format
				(CEF)](https://www.protect724.hpe.com/docs/DOC-1072).
				"""
			contributors: ["ktff"]
			pr_numbers: [14382]
		},
		{
			type: "fix"
			scopes: ["source: stdin"]
			description: """
				Using  `vector validate` with a configuration that includes the `stdin` source no
				longer blocks.
				"""
			pr_numbers: [14665]
		},
		{
			type: "feat"
			scopes: ["sink: loki"]
			breaking: true
			description: """
				The `loki` sink now supports sending data to Loki via its native snappy-compressed
				protobuf protocol by setting `compression` to `snappy`. This is the new default, but
				can be reverted to the previous behavior by setting `compression` to `none`.

				Please see the [upgrade
				guide](/highlights/2022-10-04-0-25-0-upgrade-guide#loki-request-encoding) for
				more details.
				"""
			contributors: ["xdatcloud"]
			pr_numbers: [12927]
		},
		{
			type: "fix"
			scopes: ["source: mongodb_metrics"]
			description: """
				The `mongodb_metrics` source no longer requires that the fetched statistics have
				a `record` field (used to emit `mongod_metrics_record_moves_total`). This field is
				not returned by MongoDB 6.
				"""
			contributors: ["KernelErr"]
			pr_numbers: [14612]
		},
		{
			type: "fix"
			scopes: ["source: file_descriptor", "source: stdin"]
			description: """
				The `file_descriptor` and `stdin` sources no longer assume inputs are always logs so
				that the source can be used to ingest metrics and traces via the `native` and
				`native_json` codecs.
				"""
			pr_numbers: [14778]
			contributors: ["vimalk78"]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				VRL now correctly handles assignment to `.tags` for metrics. Previously it would
				overwrite instead of assign.
				"""
			pr_numbers: [14756]
		},
		{
			type: "chore"
			scopes: ["transform: lua"]
			description: """
				Version 1 of the `lua` transform API has been officially deprecated and will be
				removed in a newer version of Vector. Please see [the upgrade
				guide](/highlights/2022-10-04-0-25-0-upgrade-guide#lua-v1-api-deprecation) for
				more details.
				"""
			pr_numbers: [14735]
		},
		{
			type: "fix"
			scopes: ["log_to_metric transform"]
			description: """
				Pre-compile the configuration field templates for the `log_to_metric` transform.
				This improves performance at runtime.
				"""
			pr_numbers: [14836]
		},
	]

	commits: [
		{sha: "e9dcf29686e9e94164f4f1c7896f6de7b4799ae5", date: "2022-08-16 09:30:07 UTC", description: "bump pin-project from 1.0.11 to 1.0.12", pr_number: 13977, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 8, deletions_count: 8},
		{sha: "18740adb7df1b99cca4d81e29baf1fb9327eb749", date: "2022-08-16 09:30:54 UTC", description: "bump prettytable-rs from 0.8.0 to 0.9.0", pr_number: 13978, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 23, deletions_count: 4},
		{sha: "ba1e1c404f1278316cc55931cf4d3d620f5a536c", date: "2022-08-16 14:59:36 UTC", description: "bump memmap2 from 0.5.6 to 0.5.7", pr_number: 13981, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fee0950f2c0b8fd53c816954d3ca7bb97ddda357", date: "2022-08-16 23:23:02 UTC", description: "Dockerfile updates and improvements", pr_number: 13483, scopes: ["docker platform"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 204, deletions_count: 60},
		{sha: "98f8ccc524b21b5baecc5e54c27a49f78deb1ce6", date: "2022-08-17 00:53:00 UTC", description: "Improve packaging and service best practices", pr_number: 13456, scopes: ["apt platform", "debian platform", "dpkg platform", "centos platform", "rhel platform", "rpm platform", "yum platform"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 16, insertions_count: 226, deletions_count: 274},
		{sha: "366b169900afed82d3c7dbb3228cba8daf2e225c", date: "2022-08-17 05:00:24 UTC", description: "bump roxmltree from 0.14.1 to 0.15.0", pr_number: 13979, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d4f0afa8aad2b32026939df97cddce7272811d9c", date: "2022-08-17 04:40:14 UTC", description: "Update all internal crates to be hyphenated", pr_number: 13994, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 18, insertions_count: 107, deletions_count: 107},
		{sha: "d604aa5706331993ced4d1efa013058a0e5778f3", date: "2022-08-17 06:01:27 UTC", description: "bump futures from 0.3.21 to 0.3.23", pr_number: 13971, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 57, deletions_count: 57},
		{sha: "6ea0bd1f649153bc5821c85955f55a15d878e905", date: "2022-08-17 06:15:36 UTC", description: "bump pest from 2.1.3 to 2.2.1", pr_number: 13767, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "5cbda857b78bcf7950ae2c510b758379224b3683", date: "2022-08-17 06:42:47 UTC", description: "Move opentelemetry proto code into libs/", pr_number: 13980, scopes: ["opentelemetry source"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 17, insertions_count: 437, deletions_count: 48},
		{sha: "9e5040e838c3a78d65a0df37924e2187d4f173e2", date: "2022-08-17 11:32:22 UTC", description: "bump hdrhistogram from 7.5.0 to 7.5.1", pr_number: 13973, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "48077f87848104b05c8b1a06186fc79bd8c7abaf", date: "2022-08-17 07:52:50 UTC", description: "Revert recent packaging and service changes", pr_number: 13997, scopes: ["packaging"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 21, insertions_count: 334, deletions_count: 430},
		{sha: "01c7a36a9d8fe796c4da5e0b92d7660cfe6d4433", date: "2022-08-17 13:08:52 UTC", description: "bump once_cell from 1.13.0 to 1.13.1", pr_number: 13990, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "358b87fa797c6a34efbf47262735641d6a5760f9", date: "2022-08-17 23:24:43 UTC", description: "try and tighten up component feature check scripts", pr_number: 13996, scopes: ["ci"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 2, insertions_count: 25, deletions_count: 9},
		{sha: "1c68635b16328384babc41beb19276d8c233482b", date: "2022-08-18 02:44:58 UTC", description: "bump anyhow from 1.0.61 to 1.0.62", pr_number: 13999, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "350d3d22bfb6e6ce120c06c47739b12ffc098852", date: "2022-08-18 07:00:40 UTC", description: "bump libc from 0.2.131 to 0.2.132", pr_number: 13992, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fda860ad25f1572b2d85c839074f72d8e35e811c", date: "2022-08-18 03:28:54 UTC", description: "Add intentional and reason for StreamClosedError", pr_number: 14006, scopes: ["observability"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 4, deletions_count: 1},
		{sha: "90c99a6f66a6ed06348378918faa3ec5bf5acee4", date: "2022-08-18 08:15:14 UTC", description: "bump pest_derive from 2.1.0 to 2.2.1", pr_number: 13998, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 18, deletions_count: 93},
		{sha: "eea8ed05d933b0b4d9e2d8ba9db7799819efdeb2", date: "2022-08-19 00:12:28 UTC", description: "Simplify lookup path coalesce segments", pr_number: 14004, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 4, insertions_count: 115, deletions_count: 172},
		{sha: "2967ebd49851a14d8fe62323a907138899ac31ba", date: "2022-08-19 00:09:32 UTC", description: "Adds `chunks` function", pr_number: 13794, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Brian Kung", files_count: 8, insertions_count: 251, deletions_count: 0},
		{sha: "81059e708bd6fb30794be36eca0424a9dc7075d3", date: "2022-08-19 04:49:31 UTC", description: "bump security-framework from 2.6.1 to 2.7.0", pr_number: 14020, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "60ec1a4afcdc5a46321f2158ce9d6dae05708a11", date: "2022-08-19 05:52:17 UTC", description: "fix error msg spelling", pr_number: 14022, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6d8e925bd1445a9660a761681ef6b8f47bc62b88", date: "2022-08-19 06:21:56 UTC", description: "Lower `cargo deny` log level", pr_number: 14024, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3b3629ba53d609d211cb8463ceaf61a178107b13", date: "2022-08-19 10:41:49 UTC", description: "bump nix from 0.24.2 to 0.25.0", pr_number: 13969, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 14, deletions_count: 2},
		{sha: "8e2201bcfd7b7ec460d783ed8b8489cafef55902", date: "2022-08-19 07:13:37 UTC", description: "Add 10s rate limit to all errors", pr_number: 14026, scopes: ["file source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 8, deletions_count: 2},
		{sha: "fc44685db64905043de0a73e4fd4397539cd821d", date: "2022-08-24 07:21:39 UTC", description: "Add distributed service helper", pr_number: 13918, scopes: ["sinks"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 8, insertions_count: 450, deletions_count: 10},
		{sha: "59c6262e5ba63424e688f3a750f2884ba6b98403", date: "2022-08-23 23:45:48 UTC", description: "bump serde_json from 1.0.83 to 1.0.85", pr_number: 14042, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "3a9a99e7178f135a5130883c5104d31f8af665da", date: "2022-08-23 23:49:55 UTC", description: "bump pest from 2.2.1 to 2.3.0", pr_number: 14043, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "20969149742ea6e2395401718ad04a8badbf3620", date: "2022-08-23 23:54:53 UTC", description: "bump tokio-postgres from 0.7.6 to 0.7.7", pr_number: 14045, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 18},
		{sha: "4ec2060e5ba541ab3f6b26e134320c38f26e1936", date: "2022-08-24 02:55:06 UTC", description: "Adhere TemplateRenderingError to EventsDropped spec", pr_number: 14017, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 18, deletions_count: 9},
		{sha: "00a9875b6e1b63ce76a777516cb099da18889c7d", date: "2022-08-24 03:43:35 UTC", description: "update component_discarded_events_total for `apache_metrics` source", pr_number: 14054, scopes: ["observability"], type: "feat", breaking_change: false, author: "Kyle Criddle", files_count: 2, insertions_count: 4, deletions_count: 2},
		{sha: "e04bba42e6fa7f7ccc0f2a58c8b1bb6e8f1bafa8", date: "2022-08-24 07:12:30 UTC", description: "prepare component types for high-level usage in configuration", pr_number: 14036, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 114, insertions_count: 2209, deletions_count: 2325},
		{sha: "92dfb84a47eff94a85e49f8eea8278d35b550c04", date: "2022-08-24 12:42:37 UTC", description: "bump serde from 1.0.143 to 1.0.144", pr_number: 14041, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "275ccd544b02824e2cb1dd536f9324e861190061", date: "2022-08-25 00:07:45 UTC", description: "Adhere encoding to EventsDropped and improve EventStatus", pr_number: 14021, scopes: ["nats sink", "codecs"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 99, deletions_count: 57},
		{sha: "eb1aae249213da780619a4d9ed3c34e7582731ea", date: "2022-08-25 06:26:43 UTC", description: "use common test helpers in tests", pr_number: 14080, scopes: ["prometheus_exporter sink"], type: "enhancement", breaking_change: false, author: "prognant", files_count: 1, insertions_count: 37, deletions_count: 28},
		{sha: "8426cb2ade475df17e42a1d87e563c335ef3c7fb", date: "2022-08-25 02:03:05 UTC", description: "Clarify dropped events for sources", pr_number: 14085, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "64703a9608469a0637c1cd7cf4d6bb2aed77896e", date: "2022-08-25 05:01:51 UTC", description: "Remove `grok` crate", pr_number: 14098, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 2, deletions_count: 16},
		{sha: "cb0a19b20994b4703d2a7ab39469340dd4dd1a12", date: "2022-08-25 13:43:42 UTC", description: "bump redis from 0.21.5 to 0.21.6", pr_number: 14099, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 12},
		{sha: "f4e60185de872563a90c5a311b33dfa7fa3f165a", date: "2022-08-25 07:54:17 UTC", description: "bump async-graphql from 4.0.6 to 4.0.11", pr_number: 14090, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 12},
		{sha: "2448fa81bdc86e9f217d7cc8b62d1e90487a9eaf", date: "2022-08-25 07:54:27 UTC", description: "bump ndarray-stats from 0.5.0 to 0.5.1", pr_number: 14058, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "19b3d8946b76f1968c1a24d69dd4a5d1237146ab", date: "2022-08-25 07:54:37 UTC", description: "bump pest_derive from 2.2.1 to 2.3.0", pr_number: 14044, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "553763f6ed37f1bfda7655f9aa6679e2423d27f9", date: "2022-08-25 07:56:48 UTC", description: "bump serde_yaml from 0.8.26 to 0.9.4", pr_number: 13812, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 29, deletions_count: 10},
		{sha: "48bbd146e1765b857632cecb2fe7167a4cdfe772", date: "2022-08-25 23:07:35 UTC", description: "bump async-graphql-warp from 4.0.6 to 4.0.11", pr_number: 14104, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "99886e20ddec010d78f85b62a545c9aec24521df", date: "2022-08-26 12:34:10 UTC", description: "settle a minor ambiguity", pr_number: 14091, scopes: ["internal docs"], type: "docs", breaking_change: false, author: "prognant", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6d3207cc41969011e55acb3fce2d5a8e48799bfe", date: "2022-08-26 18:34:50 UTC", description: "fix markdown in internal docs", pr_number: 14094, scopes: [], type: "docs", breaking_change: false, author: "Kian-Meng Ang", files_count: 11, insertions_count: 80, deletions_count: 84},
		{sha: "9cf51039488ab477ae906c8f315ff8be64ae06a5", date: "2022-08-26 03:35:11 UTC", description: "Remove AUTOINSTALL from CI", pr_number: 14023, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 0, deletions_count: 4},
		{sha: "d8fd8b891abd29682c54314f329a57638095e8ff", date: "2022-08-26 11:38:21 UTC", description: "bump serde_yaml from 0.8.26 to 0.9.10", pr_number: 14105, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "deea758b8ea2fc8b7808dd81ebc3d95ea56ac79b", date: "2022-08-26 07:39:19 UTC", description: "Move TLS library to `vector-core`", pr_number: 13913, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 19, insertions_count: 151, deletions_count: 194},
		{sha: "e2ed87c3b78608a664ae3f9bd7cd84eaf335e5ba", date: "2022-08-26 23:20:57 UTC", description: "cleanup unused code (saved for LLVM runtime)", pr_number: 14110, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 25, insertions_count: 14, deletions_count: 837},
		{sha: "a00a3f914a7fc75bb57373514d4b3c7f7f7325af", date: "2022-08-26 22:12:57 UTC", description: "Add unit test with assert_source_compliance test helper", pr_number: 14103, scopes: ["host_metrics source"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 15, deletions_count: 1},
		{sha: "b68a54dd57f85979f80dfe4d177187ff29e1b913", date: "2022-08-27 00:28:37 UTC", description: "Create std locks in const context", pr_number: 14084, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 8, deletions_count: 18},
		{sha: "0e0d7465a58c046553bd88e98efa939ac2766b00", date: "2022-08-27 00:33:23 UTC", description: "A generic `http_scrape` source", pr_number: 13793, scopes: ["new source"], type: "feat", breaking_change: false, author: "Kyle Criddle", files_count: 41, insertions_count: 1951, deletions_count: 329},
		{sha: "573b3097633c784399f59c26e6c24a6009cc1fdd", date: "2022-08-27 01:24:59 UTC", description: "Convert `BytesReceived` to a registered event", pr_number: 13934, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 31, insertions_count: 262, deletions_count: 170},
		{sha: "f88819bc39c3adbe711335e829303c5c57ff46c5", date: "2022-08-27 01:14:28 UTC", description: "bump socket2 from 0.4.4 to 0.4.6", pr_number: 14125, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "36ea6cbd56b39e26e841bb892d3d9859604ebc75", date: "2022-08-29 23:11:22 UTC", description: "remove deprecated modulo expr", pr_number: 14111, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 8, insertions_count: 54, deletions_count: 82},
		{sha: "1325b905cb58cccb6e67082adef4ef7d7cf25eb2", date: "2022-08-30 06:35:26 UTC", description: "Use booleans in `intentional` tag", pr_number: 14116, scopes: [], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "0e93d1276971fa54d217a668cfb7e47186a0a01c", date: "2022-08-29 21:35:39 UTC", description: "Clarify events dropped internal event for retries", pr_number: 14127, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 7, deletions_count: 3},
		{sha: "7e76d3cb613736c5627b736349e1e53625a3ab0b", date: "2022-08-29 21:35:48 UTC", description: "Update instrumentation spec to pull up namespaces", pr_number: 14124, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 13, deletions_count: 2},
		{sha: "e81d020e88a06f1f3b19b89e2cfd93e3a107bf26", date: "2022-08-29 22:36:40 UTC", description: "correctly emit error events per the component instrumentation spec", pr_number: 14126, scopes: ["host_metrics source"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 8, insertions_count: 147, deletions_count: 28},
		{sha: "e7437df97711b6a660a3532fe5026244472a900f", date: "2022-08-30 12:58:30 UTC", description: "minor fix debug key", pr_number: 14030, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Eric Wang", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "4b650395d56a2af278d324a89924c89f372ce402", date: "2022-08-29 23:33:37 UTC", description: "Add test helpers to assert source compliance", pr_number: 14133, scopes: ["internal_logs", "internal_metrics source"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 4, insertions_count: 57, deletions_count: 24},
		{sha: "534b5128254096cf1891c071e8c6274d54ed8eb1", date: "2022-08-30 04:26:29 UTC", description: "Re-add dropped `stage` field to component error spec", pr_number: 14153, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "b937cee24b3faf07081f1d3c2b3970c40083df08", date: "2022-08-30 21:10:23 UTC", description: "bump clap from 3.2.17 to 3.2.18", pr_number: 14161, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 12, deletions_count: 12},
		{sha: "bb4f0a6e2487c5a399374615f611fed1bc85d198", date: "2022-08-30 21:10:35 UTC", description: "bump arbitrary from 1.1.3 to 1.1.4", pr_number: 14162, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "48a04fabdceacb0a97607ecc115338921344cd94", date: "2022-08-30 21:10:45 UTC", description: "bump futures from 0.3.23 to 0.3.24", pr_number: 14164, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 51, deletions_count: 51},
		{sha: "e3b88a886a45e636164583261c246408c1e7d0c5", date: "2022-08-30 21:11:50 UTC", description: "bump async-graphql from 4.0.11 to 4.0.12", pr_number: 14163, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 13, deletions_count: 12},
		{sha: "d8d30d848ba3392e7348b59913f16824daf6623c", date: "2022-08-30 21:12:09 UTC", description: "Clarify events dropped log should be rate limited", pr_number: 14167, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "273107152d4f914cb7d824a5c09bde0a341db6e8", date: "2022-08-31 01:00:37 UTC", description: "prepare component types for high-level usage in configuration (transforms)", pr_number: 14146, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 48, insertions_count: 1080, deletions_count: 908},
		{sha: "5cf6316268798e8280d7615dcd353ff6b697d78d", date: "2022-08-31 06:10:04 UTC", description: "bump pretty_assertions from 1.2.1 to 1.3.0", pr_number: 14168, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 6},
		{sha: "3a336a55c97a08f317750377fe728c08e65e521e", date: "2022-08-31 06:47:29 UTC", description: "bump async-graphql-warp from 4.0.11 to 4.0.12", pr_number: 14170, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "875425437f93618adb75420181d2ad3b89752d80", date: "2022-08-31 02:59:13 UTC", description: "update wording around deprecation warnings in CLI to emphasize optionality", pr_number: 14171, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "4cb953a66fbfcea709f0d08f7dbdbaa2fe281260", date: "2022-08-31 01:30:29 UTC", description: "emit events to comply with Error instrumentation spec", pr_number: 14147, scopes: ["journald source"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 2, insertions_count: 116, deletions_count: 18},
		{sha: "f6399837a19d570023d49db1eec9d5a847302199", date: "2022-08-31 07:42:20 UTC", description: "bump sha2 from 0.10.2 to 0.10.3", pr_number: 14175, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 9, deletions_count: 9},
		{sha: "0804026b3644022a63d2f2a51f5dfbd58746996c", date: "2022-08-31 01:59:14 UTC", description: "emit Error and ErrorDropped events per instrumentation spec", pr_number: 14159, scopes: ["http source"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 5, insertions_count: 63, deletions_count: 14},
		{sha: "ca99bbf70b44bf5142bc4087c976379f463c0a0d", date: "2022-08-31 02:43:16 UTC", description: "Emit `StreamClosedError` to adhere to DroppedEvents spec", pr_number: 14155, scopes: ["file source"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 5, insertions_count: 16, deletions_count: 5},
		{sha: "8018392dae98e373be131b4ab57f7599a6110155", date: "2022-08-31 09:13:36 UTC", description: "bump md-5 from 0.10.1 to 0.10.2", pr_number: 14174, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "e3deb120760f8f3ec81426bcaa74c8940cd750c6", date: "2022-08-31 04:55:56 UTC", description: "Re-add bytes events to the spec", pr_number: 14181, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 59, deletions_count: 0},
		{sha: "b412ea0ee6945f5dccd22b91359516dd4a0f03bf", date: "2022-08-31 05:25:18 UTC", description: "Bump version to 0.25.0", pr_number: 14180, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "51418dd3fd6fa7d15260c10b6f8ae8a60ca8b7f9", date: "2022-08-31 06:16:51 UTC", description: "Bump k8s manifests to v0.16.0 of the Helm chart", pr_number: 14183, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 17, insertions_count: 21, deletions_count: 21},
		{sha: "c7d2d3d8fadd8f9d5e3943cb884545603336fcba", date: "2022-08-31 14:08:38 UTC", description: "bump clap from 3.2.18 to 3.2.19", pr_number: 14185, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "8c669b37f4ee9e158a3a84e17ba8509b979bd810", date: "2022-08-31 08:51:28 UTC", description: "update component_discarded_events_total for `datadog_agent` source", pr_number: 14052, scopes: ["observability"], type: "feat", breaking_change: false, author: "Kyle Criddle", files_count: 14, insertions_count: 201, deletions_count: 146},
		{sha: "c64f58e4d710c72376ee32429b4fff921b8a4902", date: "2022-09-01 06:23:59 UTC", description: "fix missing drop count", pr_number: 14151, scopes: ["pulsar sink"], type: "enhancement", breaking_change: false, author: "prognant", files_count: 4, insertions_count: 48, deletions_count: 3},
		{sha: "2e0d452ec3a83e182069223c6ac80c499585fa56", date: "2022-09-01 00:54:48 UTC", description: "use enum_dispatch to forward config traits to underlying components", pr_number: 14176, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 21, insertions_count: 159, deletions_count: 837},
		{sha: "e655a59f233f647f1a956f48ba6dfb516cb2fdf8", date: "2022-09-01 00:55:43 UTC", description: "adhere to new DroppedEvents checks / instrumentation spec", pr_number: 14222, scopes: ["pulsar sink"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 2, insertions_count: 10, deletions_count: 12},
		{sha: "93efac49a94035cf09a7276ad2f7acfb72a53f7e", date: "2022-09-01 04:49:34 UTC", description: "Add log rate limits", pr_number: 14223, scopes: ["aws_ecs_metrics source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 4, deletions_count: 1},
		{sha: "a4009896a7dceede49bb18c9ba6916c71263ffe3", date: "2022-09-01 06:15:55 UTC", description: "Confrom error events to spec, add dropped events", pr_number: 14217, scopes: ["nats sink"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 10, deletions_count: 1},
		{sha: "c417f62cbfecc4f5451c66e44e8b5717e96da0be", date: "2022-09-02 02:05:30 UTC", description: "Account for normalization error", pr_number: 14227, scopes: ["prometheus_exporter sink"], type: "fix", breaking_change: false, author: "prognant", files_count: 2, insertions_count: 30, deletions_count: 3},
		{sha: "0a870c789a9b8db607500dbad1a5348126f770c7", date: "2022-09-01 23:07:52 UTC", description: "Dedicated query syntax that points to metadata", pr_number: 14011, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 48, insertions_count: 1173, deletions_count: 985},
		{sha: "350217ec4f3e18bcec2fb00caf708c11b3709ebd", date: "2022-09-01 22:40:50 UTC", description: "Add support for `api` and `enterprise` features", pr_number: 14233, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 10, deletions_count: 3},
		{sha: "05fc485f52eba72eb4bb68ab6d392753138f0f2d", date: "2022-09-02 03:02:05 UTC", description: "switch \"&mut self\" to \"&self\" in `ArgumentList`", pr_number: 14243, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 141, insertions_count: 161, deletions_count: 168},
		{sha: "1cd785ac29c37982e10ace53f588c74e0bdc6d4e", date: "2022-09-02 06:34:14 UTC", description: "bump thiserror from 1.0.32 to 1.0.33", pr_number: 14189, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "9da998db3d1ecd9a40bc48f846e1a4e5da5bbdac", date: "2022-09-02 06:35:38 UTC", description: "bump anyhow from 1.0.62 to 1.0.63", pr_number: 14190, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f1cbc74f9aee126b7869a80f74bd240549cb4638", date: "2022-09-02 14:01:56 UTC", description: "bump socket2 from 0.4.6 to 0.4.7", pr_number: 14256, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "f3eb444a5c3d867508777a5a231d017917524aba", date: "2022-09-02 14:09:09 UTC", description: "bump paste from 1.0.8 to 1.0.9", pr_number: 14219, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "57bc77c5c2978003995a296e58a0b8bef91607e5", date: "2022-09-03 16:11:11 UTC", description: "support Zstandard-compressed output", pr_number: 14037, scopes: ["file sink"], type: "feat", breaking_change: false, author: "hdhoang", files_count: 6, insertions_count: 67, deletions_count: 6},
		{sha: "8190f963d3a00df428bc98bafa4ae45b700c9a4f", date: "2022-09-03 02:12:28 UTC", description: "Default to setting the host tag", pr_number: 14249, scopes: ["internal_metrics"], type: "enhancement", breaking_change: true, author: "Jesse Szwedko", files_count: 3, insertions_count: 29, deletions_count: 12},
		{sha: "7b877d68934b514722152a03bb3930211b1014e0", date: "2022-09-03 03:29:15 UTC", description: "Add registered `BytesSent` event", pr_number: 13961, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 22, insertions_count: 227, deletions_count: 169},
		{sha: "bf681649ba3956cd19498f4a8ed9b884ce51e291", date: "2022-09-03 08:00:46 UTC", description: "Deprecate metadata functions", pr_number: 14128, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 12, insertions_count: 239, deletions_count: 28},
		{sha: "9cb833cdb5c9022512fd73b7e4de491c51af4102", date: "2022-09-03 06:24:40 UTC", description: "Update AWS crates to versions 0.18.0/0.48.0", pr_number: 14270, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 66, deletions_count: 65},
		{sha: "2fd8a51cbe1813cc2afd3e27d05268e5173f367c", date: "2022-09-03 13:54:51 UTC", description: "prepare component types for high-level usage in configuration (sinks)", pr_number: 14229, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 120, insertions_count: 1259, deletions_count: 1931},
		{sha: "4f1f1004b69e4c66d37b351f4a151fa9ac3dedd3", date: "2022-09-06 23:09:18 UTC", description: "bump md-5 from 0.10.2 to 0.10.4", pr_number: 14279, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "cd5b2adae1cd29af5f3a27ec8b75c9b586b67d73", date: "2022-09-07 01:48:32 UTC", description: "Allow non-vrl paths to point to metadata", pr_number: 14097, scopes: ["core"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 66, insertions_count: 750, deletions_count: 589},
		{sha: "129ad5becac9d1bd603ffafd01a52539b907eaa9", date: "2022-09-06 23:56:57 UTC", description: "bump clap from 3.2.19 to 3.2.20", pr_number: 14276, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "af84d771c59ca120d3abf9f5360a11068d808837", date: "2022-09-07 06:54:15 UTC", description: "bump serde_yaml from 0.9.10 to 0.9.11", pr_number: 14277, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "c0f7176b9a9220ab984a524602c901963b7db580", date: "2022-09-07 07:34:39 UTC", description: "bump once_cell from 1.13.1 to 1.14.0", pr_number: 14278, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 11, deletions_count: 11},
		{sha: "8e862df36eb6a845c4e9c8d5e7c9ccd362b594d8", date: "2022-09-07 02:49:41 UTC", description: "bump sha2 from 0.10.3 to 0.10.5", pr_number: 14275, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 9, deletions_count: 9},
		{sha: "7bff208471c57a259d58d3a0203754dfbf1608a5", date: "2022-09-07 02:50:18 UTC", description: "bump headers from 0.3.7 to 0.3.8", pr_number: 14273, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "db6413286a9b8f6f9da1c5f926da561f3d737bd9", date: "2022-09-07 02:50:47 UTC", description: "bump zstd from 0.10.0+zstd.1.5.2 to 0.10.2+zstd.1.5.2", pr_number: 14272, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "49bf27a5425eb708e236534cef12007b1e2d51eb", date: "2022-09-07 04:35:25 UTC", description: "bump anyhow from 1.0.63 to 1.0.64", pr_number: 14297, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ebff78be5500ded2b7414f67f6a319d6f2357b24", date: "2022-09-07 11:09:09 UTC", description: "bump thiserror from 1.0.33 to 1.0.34", pr_number: 14295, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "60d439d32403a6c8c99109ef5c7cead7e854c4af", date: "2022-09-07 11:13:32 UTC", description: "bump roaring from 0.9.0 to 0.10.0", pr_number: 14292, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "05f4e4e756ead4002cdd1b91a1b1489759c794f4", date: "2022-09-07 22:51:20 UTC", description: "bump console-subscriber from 0.1.7 to 0.1.8", pr_number: 14293, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a298d2650cf1b018d8c9c991ac8ab9175e7c8957", date: "2022-09-07 22:52:56 UTC", description: "Remove v1 protocol support", pr_number: 14296, scopes: ["vector source", "vector sink"], type: "chore", breaking_change: true, author: "Bruce Guenter", files_count: 15, insertions_count: 448, deletions_count: 1186},
		{sha: "2586b5277795962a75330bb2828947706e09e2ad", date: "2022-09-08 01:40:10 UTC", description: "Add de-dot example for map_keys VRL func", pr_number: 14307, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 16, deletions_count: 0},
		{sha: "b9e1fc40b7ab5541c4c2e7d2b5fb3654ed8eec1b", date: "2022-09-08 02:43:45 UTC", description: "Add metadata to disk buffers", pr_number: 14264, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 2036, insertions_count: 1047, deletions_count: 1015},
		{sha: "0ae5e204ae3541afc2ecd4c89f58e225c18f62c4", date: "2022-09-08 00:51:11 UTC", description: "Adhere to instrumentation spec", pr_number: 14302, scopes: ["file sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 5, insertions_count: 40, deletions_count: 19},
		{sha: "f2fdfdd572c3309486a870d9313c7caad7bd15a7", date: "2022-09-08 00:52:26 UTC", description: "Add compliance test helpers.", pr_number: 14300, scopes: ["datadog_traces sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 2, insertions_count: 41, deletions_count: 29},
		{sha: "8ec9db3cdbd60d5afdd7bc2561b6ec8a60e2ac80", date: "2022-09-08 06:59:44 UTC", description: "bump url from 2.2.2 to 2.3.0", pr_number: 14308, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 6},
		{sha: "e1d8736610e92c1582c91d225231fdcc994479ef", date: "2022-09-08 02:37:24 UTC", description: "Remove vector v1 tests", pr_number: 14315, scopes: ["tests"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 0, deletions_count: 63},
		{sha: "caefdadcf53541419235498701e16195ddd20b03", date: "2022-09-08 12:04:19 UTC", description: "In `parse_key_value` group duplicate keys into an array", pr_number: 14248, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 3, insertions_count: 97, deletions_count: 17},
		{sha: "9f06a55edd0bb7733ce986b41d61bc724f42b805", date: "2022-09-08 06:40:11 UTC", description: "Add 10s rate limit to all errors, use existing internal events", pr_number: 14245, scopes: ["aws_sqs source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 38, deletions_count: 30},
		{sha: "69bc02b39401e553423660f0be71e73286bf82ca", date: "2022-09-08 06:56:19 UTC", description: "Restore the `-rdynamic` linker flag", pr_number: 14326, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "1db0048e41787632083c588f74dc036d074621e4", date: "2022-09-08 09:09:15 UTC", description: "Rate limit internal errors", pr_number: 14327, scopes: ["mongodb_metrics source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "9a622bdb69a1cb1ddcb1b381e9757c5556b2c634", date: "2022-09-08 10:03:56 UTC", description: "Update to newer EventsReceived metric", pr_number: 14323, scopes: ["nats source"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 10, deletions_count: 8},
		{sha: "a0a2aba4d3890f71446ed9eecf4c03008f2d5fb2", date: "2022-09-08 10:05:13 UTC", description: "Rate limit internal errors", pr_number: 14329, scopes: ["nginx_metrics source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "e9c79f915f6108d1bb9c6d28a297504af70aa980", date: "2022-09-08 10:25:42 UTC", description: "annotate top-level config type for schema generation + add new subcommand", pr_number: 14316, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 55, insertions_count: 1296, deletions_count: 487},
		{sha: "012e023e0da9c5af25d5708062e9fb602e066b3f", date: "2022-09-09 01:23:08 UTC", description: "Remove unneeded OldEventsReceived", pr_number: 14330, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 12, deletions_count: 37},
		{sha: "4f5224b241684cf2b9a926b7841e6ba54c9579df", date: "2022-09-09 05:51:22 UTC", description: "bump arbitrary from 1.1.4 to 1.1.5", pr_number: 14331, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "c745d2501e679fbc5d505a2f54c42a4e72f09a23", date: "2022-09-09 08:06:12 UTC", description: "Fix formatting for requirement blurb", pr_number: 14341, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3c8f8de28240c050da5357a008126e84c0027283", date: "2022-09-09 20:36:01 UTC", description: "update environment with rustup cache, podman compat", pr_number: 14237, scopes: [], type: "chore", breaking_change: false, author: "hdhoang", files_count: 5, insertions_count: 16, deletions_count: 2},
		{sha: "1c952012cb82d92b04e4236eb47e282acbd45fd6", date: "2022-09-10 00:50:18 UTC", description: "Adhere to instrumentation spec", pr_number: 14299, scopes: ["datadog_metrics sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 66, insertions_count: 212, deletions_count: 137},
		{sha: "142beaf239c5d10292ce4755428b982c5b865e53", date: "2022-09-10 01:04:11 UTC", description: "bump percent-encoding from 2.1.0 to 2.2.0", pr_number: 14343, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "045ae39cb2fd66fd1336f52461b06241103a43ea", date: "2022-09-10 01:05:11 UTC", description: "bump arbitrary from 1.1.5 to 1.1.6", pr_number: 14346, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "e2db5fabcfe62488e5420c5ec38d831bbea3e16d", date: "2022-09-10 01:06:52 UTC", description: "bump roaring from 0.10.0 to 0.10.1", pr_number: 14342, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1790289ab09b6d2bd05b9cfbbd6bd95db927aada", date: "2022-09-10 01:09:19 UTC", description: "bump bstr from 0.2.17 to 1.0.0", pr_number: 14332, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 14, deletions_count: 5},
		{sha: "675cbf44f8a1404894f0e407dbbc20e62a1a716e", date: "2022-09-10 01:12:31 UTC", description: "Adhere to instrumentation spec", pr_number: 14225, scopes: ["datadog_logs sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 18, insertions_count: 369, deletions_count: 191},
		{sha: "74f20ded49ec7d21b3f7e2ba6d683d474327d481", date: "2022-09-10 09:01:27 UTC", description: "bump async-graphql from 4.0.12 to 4.0.13", pr_number: 14353, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 14},
		{sha: "f705842ef7ab6bc72b028935a6eb9cd74c3610fa", date: "2022-09-10 03:44:11 UTC", description: "Double emission of `Error` event in sink framework code.", pr_number: 14356, scopes: ["internal events"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 2, insertions_count: 2, deletions_count: 28},
		{sha: "0083d15aa7ba31da598c63a05ba8d034edb976fa", date: "2022-09-10 14:49:16 UTC", description: "bump url from 2.3.0 to 2.3.1", pr_number: 14352, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 19, deletions_count: 10},
		{sha: "565165d6503094acb7711c52110f71a8cc6ddda6", date: "2022-09-13 04:34:37 UTC", description: "add note about errors that prevent Vector starting to Component Spec", pr_number: 14350, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "f3c090506a756704b36b29fc3f659d7beb1049be", date: "2022-09-13 07:29:16 UTC", description: "Comply to `*EventsDropped` instrumentation spec in `dedupe` transform", pr_number: 14118, scopes: ["dedupe transform"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count: 4, insertions_count: 17, deletions_count: 9},
		{sha: "4b21d55a632d6e8214388228759e13acb0c79add", date: "2022-09-12 23:29:39 UTC", description: "Adhere to instrumentation spec", pr_number: 14357, scopes: ["datadog_events sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 3, insertions_count: 23, deletions_count: 9},
		{sha: "f8722435b208ea74053748cd72446751fb35392c", date: "2022-09-13 06:58:39 UTC", description: "add assert_source_compliance to unit tests", pr_number: 14376, scopes: ["demo_logs source"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 22, deletions_count: 12},
		{sha: "0056ffef8d82cf0e5692d614d5fbb8aefb29ff08", date: "2022-09-13 00:15:13 UTC", description: "Adhere to instrumentation spec", pr_number: 14361, scopes: ["http sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 2, insertions_count: 117, deletions_count: 100},
		{sha: "61b03794f0df4e8c0bde0ca5140ae0120df527c9", date: "2022-09-13 01:08:11 UTC", description: "Adhere to instrumentation spec", pr_number: 14364, scopes: ["kafka sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 3, insertions_count: 19, deletions_count: 4},
		{sha: "2eeda83e45c2c7e22b9d5122b70648e93eed0b48", date: "2022-09-13 06:52:20 UTC", description: "bump webbrowser from 0.7.1 to 0.8.0", pr_number: 14358, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 39, deletions_count: 4},
		{sha: "eea2951be721a6d756396cb8e200650bbb47a75c", date: "2022-09-13 06:52:39 UTC", description: "bump async-graphql-warp from 4.0.12 to 4.0.13", pr_number: 14359, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5230fcdbb348376a35729a1a2ad7920b7243aa1c", date: "2022-09-13 06:55:15 UTC", description: "bump axum from 0.5.15 to 0.5.16", pr_number: 14373, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 5},
		{sha: "2189555dacebea410f7f52e98d53f4e28bd22be1", date: "2022-09-14 03:38:36 UTC", description: "Test compliance to instrumentation spec in `dedupe` transform", pr_number: 14388, scopes: ["dedupe transform"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 6, insertions_count: 434, deletions_count: 187},
		{sha: "cecaba7748b177969a355b1b789cb8a5258ab627", date: "2022-09-14 06:32:52 UTC", description: "Comply to `*EventsDropped` instrumentation spec in `filter` transform", pr_number: 14121, scopes: ["filter transform"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count: 2, insertions_count: 13, deletions_count: 5},
		{sha: "a70f868291719e3f5e40a190ff591850057001e2", date: "2022-09-13 21:36:05 UTC", description: "bump pest from 2.3.0 to 2.3.1", pr_number: 14384, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4e66baf8ae9c8131abccdbd2ee7496129bdee4cb", date: "2022-09-13 21:36:23 UTC", description: "bump clap from 3.2.20 to 3.2.21", pr_number: 14386, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "057d07e72970e16f0d178665d240ef367a3ec2f5", date: "2022-09-13 21:36:37 UTC", description: "bump bstr from 1.0.0 to 1.0.1", pr_number: 14385, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "ad2b3adf0d59bd546234839f99f1a80a4323ad02", date: "2022-09-13 21:37:29 UTC", description: "Fix calendar link in RELEASES.md", pr_number: 14383, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "fd9e7332055eb44d1347dc5160aa1b3ee9a35f07", date: "2022-09-13 23:28:02 UTC", description: "Adhere to instrumentation spec", pr_number: 14355, scopes: ["influxdb_logs", "influxdb_metrics sinks"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 5, insertions_count: 69, deletions_count: 10},
		{sha: "75ec9d0b7ef6d82380fae8d32af525f811a3b07f", date: "2022-09-14 04:12:09 UTC", description: "bump cfb-mode from 0.8.1 to 0.8.2", pr_number: 14396, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2c585892f243ff7eb74456e3ffbb96d292f16cb4", date: "2022-09-14 07:47:27 UTC", description: "Adhere to errors spec", pr_number: 14344, scopes: ["aws_s3 source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 18, deletions_count: 20},
		{sha: "290ea9712a8f0961166078914f4d83a743fefb5e", date: "2022-09-14 06:41:50 UTC", description: "bump anyhow from 1.0.64 to 1.0.65", pr_number: 14402, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5f6ac909d2fa84e1d20644fc2220fdc8c31bc994", date: "2022-09-14 14:44:11 UTC", description: "bump lru from 0.7.8 to 0.8.0", pr_number: 14371, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 16, deletions_count: 10},
		{sha: "96003531105213c8e2a245f68605e6764c261850", date: "2022-09-14 20:16:16 UTC", description: "allow aws auth request signing", pr_number: 14150, scopes: ["prometheus_remote_write sink"], type: "enhancement", breaking_change: false, author: "Taylor Chaparro", files_count: 7, insertions_count: 247, deletions_count: 54},
		{sha: "0c5cd09478bdd10345a0aa603380f68cc2e4a73e", date: "2022-09-15 04:45:14 UTC", description: "remove duplicate integration tests", pr_number: 14412, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "d81348f0b0d71f95802fadfc6f6f5d1754fc9ab8", date: "2022-09-14 20:53:01 UTC", description: "bump thiserror from 1.0.34 to 1.0.35", pr_number: 14408, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "8c1c0c91ba724f108e81eff64a99c1cfdf3caf0b", date: "2022-09-15 06:34:14 UTC", description: "Test compliance to instrumentation spec in `aggregate` transform", pr_number: 14119, scopes: ["aggregate transform"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 50, deletions_count: 50},
		{sha: "cc9155fd7caef1087d59364024c7cbd10e3292c9", date: "2022-09-15 06:34:30 UTC", description: "Test compliance to instrumentation spec in `aws_ec2_metadata` transform", pr_number: 14120, scopes: ["aws_ec2_metadata transform"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 214, deletions_count: 172},
		{sha: "5d1550a2056f32a885f4d13df3a75b45f777440a", date: "2022-09-14 22:06:00 UTC", description: "bump serde_yaml from 0.9.11 to 0.9.12", pr_number: 14415, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "de83fb909dc0e87609dc144fb7e4396a8e305067", date: "2022-09-14 23:32:33 UTC", description: "Adhere to instrumentation spec", pr_number: 14407, scopes: ["sematext_metrics sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 2, insertions_count: 19, deletions_count: 5},
		{sha: "833e0075bbee16fc7112db720281947926eff3c1", date: "2022-09-15 01:55:53 UTC", description: "Fixes timestamp field to match Honeycomb spec", pr_number: 14417, scopes: ["honeycomb sink"], type: "fix", breaking_change: false, author: "McSick", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "de8e4109728cdd6d3593b397b862e406dcba115f", date: "2022-09-15 06:23:51 UTC", description: "bump pest_derive from 2.3.0 to 2.3.1", pr_number: 14397, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "10b6d9257b7e3b74a8548349d2f3e16d5911440c", date: "2022-09-15 03:57:07 UTC", description: "allow empty log schema keys", pr_number: 14421, scopes: ["config"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 35, deletions_count: 20},
		{sha: "4186459d72a1f52a7c6476b90733f80f9e553feb", date: "2022-09-15 08:08:25 UTC", description: "bump itertools from 0.10.3 to 0.10.4", pr_number: 14398, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 14, deletions_count: 14},
		{sha: "52f20157e2271e04916c18dd6c6b0f66ab169d70", date: "2022-09-15 02:15:32 UTC", description: "Adhere to instrumentation spec", pr_number: 14405, scopes: ["prometheus_remote_write sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 93, deletions_count: 74},
		{sha: "66441f3eeb0f56fa8f1aefe9883c9e563b01598b", date: "2022-09-15 02:39:00 UTC", description: "Require TEST_DATADOG_API_KEY for Datadog integration tests", pr_number: 14420, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 8, insertions_count: 11, deletions_count: 5},
		{sha: "b4b7dae1829620b96c9125d87a89af54856ca935", date: "2022-09-16 05:32:39 UTC", description: "recoverable parse errors should be a warning", pr_number: 14410, scopes: ["dnstap source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 27, deletions_count: 11},
		{sha: "b1a80afef452138f5be62124a7299e2560f04370", date: "2022-09-16 06:49:06 UTC", description: "Test compliance to instrumentation spec in `filter` transform", pr_number: 14122, scopes: ["filter transform"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 36, deletions_count: 9},
		{sha: "2f4d8a558e0a18ba91770bf887da4bec9126817b", date: "2022-09-15 23:19:48 UTC", description: "Adhere to instrumentation spec", pr_number: 14406, scopes: ["redis sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 3, insertions_count: 31, deletions_count: 59},
		{sha: "2db1aabf1444e593c77904277a07b990f2f898de", date: "2022-09-16 01:41:42 UTC", description: "Adhere to events spec", pr_number: 14422, scopes: ["logstash source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 50, deletions_count: 40},
		{sha: "fb4ae492040dbfa716fbcaa33b7e0a08e6149490", date: "2022-09-16 08:58:04 UTC", description: "add assert_source_compliance calls to integration tests", pr_number: 14431, scopes: ["docker_logs source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 303, deletions_count: 260},
		{sha: "afc4140a22dd1116e7ce0a40f8f4e85f189f37c2", date: "2022-09-16 06:29:33 UTC", description: "Use HTTP_SINK_TAGS for compliance", pr_number: 14435, scopes: ["honeycomb sink"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 5, deletions_count: 6},
		{sha: "a6a1f95c93ff60cdb4672725ab385fe3e14f2498", date: "2022-09-16 04:31:49 UTC", description: "Adhere to instrumentation spec", pr_number: 14218, scopes: ["kubernetes_logs source"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 4, insertions_count: 229, deletions_count: 11},
		{sha: "47dbcb3e98f0915c7ed5f0646134ea69bec3dfa3", date: "2022-09-16 07:26:55 UTC", description: "Adhere to InternalEvents spec", pr_number: 14439, scopes: ["new_relic sink"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 9, deletions_count: 10},
		{sha: "a22d8590b9a3b45c85f3d6051f70eed2b3d10c5e", date: "2022-09-16 07:30:51 UTC", description: "Remove old inventory::submit for SinkDescription", pr_number: 14440, scopes: ["papertrail sink"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 5},
		{sha: "0d55dc8c95425ce325888130b545ac827cc460ff", date: "2022-09-17 04:01:15 UTC", description: "Comply to `*EventsDropped` instrumentation spec in `geoip` transform", pr_number: 14123, scopes: ["geoip transform"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count: 7, insertions_count: 32, deletions_count: 22},
		{sha: "d94bc95d92ecc4e6cd72207a866b979383b6fb90", date: "2022-09-17 04:35:04 UTC", description: "Test compliance to instrumentation spec in `geoip` transform", pr_number: 14448, scopes: ["geoip transform"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 276, deletions_count: 173},
		{sha: "e5516e9a555d5e2986e7732c3ea43df37d00d5c9", date: "2022-09-17 07:49:56 UTC", description: "hide secrets when enterprise mode is enabled", pr_number: 14305, scopes: ["enterprise"], type: "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count: 51, insertions_count: 308, deletions_count: 157},
		{sha: "d498040a770ae2bb5c9d25efce62acadcb17ee57", date: "2022-09-17 05:37:51 UTC", description: "make internal log rate limits configurable", pr_number: 14381, scopes: ["vector"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 62, insertions_count: 225, deletions_count: 143},
		{sha: "9db87b27af5b1f057ce7759042d26046cc3cebc1", date: "2022-09-20 05:58:25 UTC", description: "Fix misspelling", pr_number: 14459, scopes: ["alpn"], type: "docs", breaking_change: false, author: "Alexander Zaitsev", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "8f10836c8e49044ed648cfdeb4c2adf3f5563fe0", date: "2022-09-20 00:22:30 UTC", description: "Adhere to instrumentation spec", pr_number: 14434, scopes: ["statsd sink"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 8, insertions_count: 10, deletions_count: 13},
		{sha: "9622db48373f3f7dcc06891990497164c1dfa1c8", date: "2022-09-20 03:08:06 UTC", description: "internal log rates init usage fix", pr_number: 14458, scopes: ["vector"], type: "fix", breaking_change: false, author: "Arshia Soleimani", files_count: 10, insertions_count: 17, deletions_count: 11},
		{sha: "cd101d6dd8ab7472e2e8f8c09475cd6af11ec26a", date: "2022-09-20 05:22:23 UTC", description: "Adhere to InternalEvents spec", pr_number: 14465, scopes: ["loki sink"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 8, insertions_count: 54, deletions_count: 51},
		{sha: "34a977a68ab28a5912f90eced4777eb67ca881e0", date: "2022-09-20 05:27:37 UTC", description: "Adhere to InternalEvents spec", pr_number: 14466, scopes: ["aws_cloudwatch_logs sink"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 19, deletions_count: 57},
		{sha: "b1c6a50d328634205d558eade0f77a262e896050", date: "2022-09-20 05:34:51 UTC", description: "Fix merge arg types", pr_number: 14469, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a3de63325ca611bb383bb197ba5a31bc81b9ea76", date: "2022-09-20 02:35:41 UTC", description: "bump semver from 1.0.13 to 1.0.14", pr_number: 14447, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "dec33dc4956fb375fe889d5e41c1db911ec13835", date: "2022-09-20 20:30:29 UTC", description: "New amqp (Rabbitmq) Source/Sink", pr_number: 7120, scopes: ["sources"], type: "feat", breaking_change: false, author: "Danny Browning", files_count: 25, insertions_count: 2077, deletions_count: 2},
		{sha: "14fefee7ebef098fdb2b5d02ea37d5f759ade48d", date: "2022-09-20 23:33:17 UTC", description: "Audit `reduce` transform - assert compliance", pr_number: 14467, scopes: ["reduce transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 211, deletions_count: 191},
		{sha: "512da4076a67a229996f96e51e711dc2af37dcf2", date: "2022-09-21 01:13:40 UTC", description: "File cleanup and use SinkRequestBuildError", pr_number: 14478, scopes: ["aws_kinesis_firehose sink"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 32, deletions_count: 36},
		{sha: "4efd57cd7177ff5741e479ecbe9877ff3fcf958e", date: "2022-09-21 02:45:11 UTC", description: "Reorganize and cleanup file", pr_number: 14477, scopes: ["aws_cloudwatch_metrics sink"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 340, deletions_count: 335},
		{sha: "a658e5147c8ebaf85062d2c0f305dc460c30bf99", date: "2022-09-21 08:04:44 UTC", description: "emit discarded error when channel is closed", pr_number: 14480, scopes: ["exec source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 66, deletions_count: 3},
		{sha: "4f3048ee9eec85837c3751d26da92cd1063daf12", date: "2022-09-21 03:23:53 UTC", description: "File cleanup and use SinkRequestBuildError", pr_number: 14482, scopes: ["aws_kinesis_streams sink"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 29, deletions_count: 26},
		{sha: "9cf1ea9b08ed745e3872c1cc81757f6078c82419", date: "2022-09-21 02:02:07 UTC", description: "Move global config merging into core", pr_number: 14442, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 67, deletions_count: 36},
		{sha: "31a247e8eff3f236902daf8cf345160f6723ccfa", date: "2022-09-22 05:44:51 UTC", description: "Explicitly require errors to emit an `EventsDropped` event / explicitly require `intentional` property", pr_number: 14481, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "d7501245824704f795838c322e1131864f2a6479", date: "2022-09-21 23:54:53 UTC", description: "Cleaning up markdown files and typos", pr_number: 14488, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 13, insertions_count: 80, deletions_count: 76},
		{sha: "3e9eba5e6e5e2d2233426bbff6c64967966dbcdd", date: "2022-09-22 05:21:24 UTC", description: "add documentation to review doc.", pr_number: 14391, scopes: ["internal docs"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 15, deletions_count: 0},
		{sha: "197ed5b27452aee5b51ba4db2443ca3ac1814634", date: "2022-09-22 01:58:45 UTC", description: "assert that host has enough disk capacity for all configured disk buffers at startup", pr_number: 14455, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 5, insertions_count: 223, deletions_count: 6},
		{sha: "50f37f5b1a5910d51d698773d23026270b07b7e8", date: "2022-09-22 02:28:22 UTC", description: "File cleanup and use SinkRequestBuildError", pr_number: 14484, scopes: ["aws_sqs sink"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 49, deletions_count: 40},
		{sha: "952b608e963d209b174df69605b42272d6ea3c20", date: "2022-09-22 08:17:19 UTC", description: "Add `assert_source_compliance` to tests", pr_number: 14476, scopes: ["exec source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 5, deletions_count: 2},
		{sha: "a0dcedee6a67f1fb359ac9ead1063fa5812e3daf", date: "2022-09-22 01:37:48 UTC", description: "Allow loading YAML configs with `run-vector` script", pr_number: 14443, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 12, deletions_count: 3},
		{sha: "34c005e7281dcfb73217fa9f6957a0a8640e2c25", date: "2022-09-22 01:56:29 UTC", description: "Skip expiring registered metrics", pr_number: 14489, scopes: ["observability"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 427, deletions_count: 7},
		{sha: "6982fc837fe65598057383c2cfc71767236091a8", date: "2022-09-22 04:42:51 UTC", description: "Emit address when building server.", pr_number: 14499, scopes: ["prometheus_exporter sink"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 5, deletions_count: 6},
		{sha: "7b26f65c6bde9fe824ea15948fa0147cf026db5e", date: "2022-09-22 04:52:25 UTC", description: "Improve `batch.max_bytes` description.", pr_number: 14470, scopes: [], type: "docs", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 5, deletions_count: 2},
		{sha: "ab3ca33efecdc91eff89fb9c0bb3a1ab81cd2762", date: "2022-09-22 06:45:11 UTC", description: "Added ```vector_info```  internal metric, capturing build information", pr_number: 14497, scopes: ["internal_metrics"], type: "feat", breaking_change: false, author: "Arshia Soleimani", files_count: 3, insertions_count: 46, deletions_count: 1},
		{sha: "5149fef19f249b9990803c68c3b3a615abc063ac", date: "2022-09-22 23:23:08 UTC", description: "report filename when check-events errors", pr_number: 14490, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jérémie Drouet", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "248583fe27dfb3c1393c80580af5c3340a89cd3f", date: "2022-09-23 00:11:44 UTC", description: "Fix title and alias", pr_number: 14517, scopes: ["http_scrape source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 1, deletions_count: 2},
		{sha: "966cd18412bcb0d86a186734690a255758421f0a", date: "2022-09-23 00:57:58 UTC", description: "Don't include name field for SinkRequestBuildError", pr_number: 14510, scopes: ["observability"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 12, insertions_count: 42, deletions_count: 62},
		{sha: "2917eb5e8881213847841436deaecf98c7e0545d", date: "2022-09-23 03:20:39 UTC", description: "File cleanup and use SinkRequestBuildError", pr_number: 14485, scopes: ["aws_s3 sink"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 474, deletions_count: 465},
		{sha: "e80c7afaf7601cf936c7c3468bd7b4b230ef6149", date: "2022-09-23 05:41:21 UTC", description: "Upgrade Rust to 1.64", pr_number: 14520, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 29, insertions_count: 64, deletions_count: 73},
		{sha: "0d94ae2b80c80e57deea9af62388766577f4b7b1", date: "2022-09-23 12:27:21 UTC", description: "check that the sensitive strings in the configuration contain variables", pr_number: 14454, scopes: ["enterprise"], type: "feat", breaking_change: false, author: "Jérémie Drouet", files_count: 14, insertions_count: 387, deletions_count: 105},
		{sha: "a53cfefa5c5fd0c5c4603c0725adbdce4d4aeedd", date: "2022-09-24 00:40:47 UTC", description: "update emitted errors", pr_number: 14519, scopes: ["azure blob sink"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 7, insertions_count: 25, deletions_count: 91},
		{sha: "f19053c3fb46b4c79c1ca2f43af11027c25229c7", date: "2022-09-23 23:31:47 UTC", description: "bump sha2 from 0.10.5 to 0.10.6", pr_number: 14445, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 18, deletions_count: 18},
		{sha: "dd5bdd2582876f5913ac9bd6f858b505f498cc6c", date: "2022-09-24 08:13:44 UTC", description: "message keys as config field", pr_number: 14491, scopes: ["pulsar"], type: "feat", breaking_change: false, author: "Collignon-Ducret Rémi", files_count: 2, insertions_count: 55, deletions_count: 17},
		{sha: "a149beffee12bc035c94ff32465755a4c58f14d0", date: "2022-09-24 03:32:08 UTC", description: "Adds `keys()` and `values()` function", pr_number: 14441, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Jonathan Padilla", files_count: 7, insertions_count: 262, deletions_count: 0},
		{sha: "94bd9f97a2ac83c73802e53150a760b1e9223e52", date: "2022-09-24 10:34:28 UTC", description: "add auth implementation to Prometheus Exporter", pr_number: 14461, scopes: ["prometheus"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 4, insertions_count: 276, deletions_count: 2},
		{sha: "70c5272eb48ba023b97a5aa7c22d25aedf7d7c5b", date: "2022-09-24 02:53:42 UTC", description: "Fix integration test for partition key", pr_number: 14541, scopes: ["pulsar sink"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "52175fdf0f05b736c6e194df127b6334bdd7c5b9", date: "2022-09-24 04:05:39 UTC", description: "bump notify from 4.0.17 to 5.0.0", pr_number: 14177, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 139, deletions_count: 245},
		{sha: "4cd19514e46703635f3d9c72e9bd08932eda3527", date: "2022-09-24 05:55:39 UTC", description: "bump prost from 0.10.4 to 0.11.0", pr_number: 13766, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 42, insertions_count: 1455, deletions_count: 143},
		{sha: "11905070fbe0795bdc975995269f17208e8041b2", date: "2022-09-24 13:32:01 UTC", description: "bump serde_yaml from 0.9.12 to 0.9.13", pr_number: 14427, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "e892bbdcc40e6595896c1e2e30307a3469b25be5", date: "2022-09-24 13:49:36 UTC", description: "bump criterion from 0.3.6 to 0.4.0", pr_number: 14372, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 12, insertions_count: 53, deletions_count: 39},
		{sha: "6da68884de18b1593dbc477296421a74fa88e8d2", date: "2022-09-24 07:43:41 UTC", description: "Improve error messages for out of range values", pr_number: 14546, scopes: ["prometheus_scrape source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 66, deletions_count: 13},
		{sha: "285baaeb721218c6fc8dc89344b8d22806259b77", date: "2022-09-24 15:27:23 UTC", description: "bump libc from 0.2.132 to 0.2.133", pr_number: 14471, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 3},
		{sha: "9307c70d79bab6dd73c6834c4cd3be7b2b2a25a2", date: "2022-09-26 01:39:45 UTC", description: "Upgrade all AWS crates", pr_number: 14548, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 65, deletions_count: 65},
		{sha: "cbe3c309de49603185b953dda5208f5812700524", date: "2022-09-26 01:59:48 UTC", description: "bump trust-dns-proto from 0.21.2 to 0.22.0", pr_number: 14294, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 54, deletions_count: 14},
		{sha: "65d7bbb5bdf9039795a9983d1b9c9dad164e2af4", date: "2022-09-26 02:00:12 UTC", description: "bump k8s-openapi from 0.15.0 to 0.16.0", pr_number: 14446, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 31, deletions_count: 19},
		{sha: "487d2caf1a5d93efaca3a4e6fa600150b38fe313", date: "2022-09-26 02:00:38 UTC", description: "bump clap from 3.2.21 to 3.2.22", pr_number: 14544, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 14, deletions_count: 14},
		{sha: "ae16d014a0b9a4fe9e417e951dfc22ac5cf70504", date: "2022-09-26 02:00:57 UTC", description: "bump tonic from 0.8.0 to 0.8.1", pr_number: 14549, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "9b9e303f9ffdb6b70afcc5c3f39b58d1450d3ca2", date: "2022-09-26 02:01:14 UTC", description: "bump tokio-stream from 0.1.9 to 0.1.10", pr_number: 14551, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "cf7843b1284afc6542287af5c379cca52db07761", date: "2022-09-26 07:24:42 UTC", description: "Update chrono to v0.4.22", pr_number: 14568, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 31, deletions_count: 7},
		{sha: "4048a86a73eaed3d9e8ba2f02c9b1a4d3a050d80", date: "2022-09-26 19:55:24 UTC", description: "Install protoc during OSX release", pr_number: 14561, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7ab9c86c471386210074026d022336de344a364e", date: "2022-09-26 23:03:42 UTC", description: "bump serde from 1.0.144 to 1.0.145", pr_number: 14567, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "ce6c2df883bafcdbcd84b14a74e19a2b7c4e0f83", date: "2022-09-26 23:05:04 UTC", description: "bump once_cell from 1.14.0 to 1.15.0", pr_number: 14560, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 11, deletions_count: 11},
		{sha: "d9bf36d070a78dd1aac70190577a80a3158dc62b", date: "2022-09-26 23:06:17 UTC", description: "bump reqwest from 0.11.11 to 0.11.12", pr_number: 14550, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "8b8c645a02dc717ddc8b57d15984cbd89cee3441", date: "2022-09-27 06:56:25 UTC", description: "Fix panic in parse_xml vrl function", pr_number: 14479, scopes: ["vrl parse_xml"], type: "fix", breaking_change: false, author: "Zettroke", files_count: 1, insertions_count: 17, deletions_count: 1},
		{sha: "2a01e25303637aab1cd3f0256a23d9ebf792b3a0", date: "2022-09-27 00:02:38 UTC", description: "Remove unused `shutdown_timeout_secs` option", pr_number: 14542, scopes: ["vector source"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 9, deletions_count: 21},
		{sha: "677ef7f88e7c2eb81d6c40c0ae821597fe9bb7f4", date: "2022-09-27 04:48:54 UTC", description: "bump ordered-float from 3.0.0 to 3.1.0", pr_number: 14563, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 19, deletions_count: 19},
		{sha: "78a16d7c205c9665bf9a3bd5e7d0955357b63c4d", date: "2022-09-27 05:04:42 UTC", description: "bump serde_with from 2.0.0 to 2.0.1", pr_number: 14367, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "1b24559f0abaf9879a2c03f98276a7ad75e1aff3", date: "2022-09-27 05:04:48 UTC", description: "bump itertools from 0.10.4 to 0.10.5", pr_number: 14566, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 15, deletions_count: 15},
		{sha: "24c5739887c2cc8718f62fce21297ee8c19c9918", date: "2022-09-27 05:12:46 UTC", description: "bump hdrhistogram from 7.5.1 to 7.5.2", pr_number: 14571, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "641ad2b11a1fc4094f85f9bad42be28cf3da3f71", date: "2022-09-27 06:19:15 UTC", description: "bump env_logger from 0.9.0 to 0.9.1", pr_number: 14574, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "6b3fe88450919775b4b631d629841e6fbde09794", date: "2022-09-27 06:22:42 UTC", description: "bump thiserror from 1.0.35 to 1.0.36", pr_number: 14564, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "f5b91036c835ae78e43f53af08ff737004d89fb6", date: "2022-09-27 09:33:44 UTC", description: "fix URI configuration", pr_number: 14557, scopes: ["clickhouse"], type: "fix", breaking_change: false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "a4ed79736d12f89b36f039741a2cafa25e8b0a38", date: "2022-09-27 08:59:04 UTC", description: "use vector version when computing configuration hash", pr_number: 14518, scopes: ["enterprise"], type: "feat", breaking_change: false, author: "Jérémie Drouet", files_count: 4, insertions_count: 17, deletions_count: 13},
		{sha: "3bfb49a5dab3b1bf1096d8e2653a979abc778c13", date: "2022-09-27 07:14:00 UTC", description: "bump async-graphql from 4.0.13 to 4.0.14", pr_number: 14569, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 13, deletions_count: 12},
		{sha: "109af3bc2cb4a35c369a4ac47ef979fc146f6df2", date: "2022-09-27 07:23:44 UTC", description: "bump md-5 from 0.10.4 to 0.10.5", pr_number: 14578, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "fa80ef764d7dbf400ba001dab1ea225f5fd8838b", date: "2022-09-27 08:12:41 UTC", description: "bump proc-macro2 from 1.0.43 to 1.0.44", pr_number: 14576, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "7760e6abb35b25c27bb241160aed35b0807d3b23", date: "2022-09-27 08:54:11 UTC", description: "bump bitmask-enum from 2.0.0 to 2.0.1", pr_number: 14565, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b6ac5e789bd3bed9ac88700487f89441cb14d31f", date: "2022-09-27 05:16:18 UTC", description: "bump async-graphql-warp from 4.0.13 to 4.0.14", pr_number: 14579, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2a0e620a0759d63469a72df71c6f02740b15bce2", date: "2022-09-27 12:03:15 UTC", description: "Limit search in `check-events` script to `src`/`lib`", pr_number: 14529, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "bf1d834a8a04f8256b0c2ff488cca102d87afc6d", date: "2022-09-27 06:06:16 UTC", description: "establish a basic style guide", pr_number: 12482, scopes: [], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 6, insertions_count: 397, deletions_count: 21},
		{sha: "9215ec5fdfe76e82e9df028abe70f2330455b9b9", date: "2022-09-27 06:07:55 UTC", description: "Handle installing protoc on arm and x86", pr_number: 14581, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 52, deletions_count: 10},
		{sha: "6b8c9da0ed73fa6c05b8a9ea74ce68c25a05251c", date: "2022-09-27 10:09:58 UTC", description: "bump tokio from 1.20.1 to 1.21.0", pr_number: 14274, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 18, deletions_count: 29},
		{sha: "63c2ba8248d1ea906eafeca25eb13d4591e6365a", date: "2022-09-27 10:53:44 UTC", description: "bump pulsar from 4.1.2 to 4.1.3", pr_number: 14577, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 3},
		{sha: "ff503c200408807f434c0e5d5da2c597db9ab01a", date: "2022-09-27 12:31:02 UTC", description: "bump syn from 1.0.99 to 1.0.101", pr_number: 14584, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "e463399a532413bc240d3db4eac17da35ae12713", date: "2022-09-27 13:22:24 UTC", description: "bump governor from 0.4.2 to 0.5.0", pr_number: 14562, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "88506e819b596a3f69a275fafa2c5b31e3a336d6", date: "2022-09-27 11:15:01 UTC", description: "bump openssl from 0.10.41 to 0.10.42", pr_number: 14588, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "f38e3c39e6984e61ec000278d37e74dce7960c5b", date: "2022-09-28 01:05:48 UTC", description: "emit `RequestBuildError` when `HttpSink::request_builder` errors", pr_number: 14575, scopes: ["observability"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 7, deletions_count: 2},
		{sha: "fd5ba448c73261601b722705e0a788a20492d3d8", date: "2022-09-28 04:53:02 UTC", description: "add run_and_assert_sink_error to unhappy path test", pr_number: 14596, scopes: ["gcp_chronicle sink"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 35, deletions_count: 13},
		{sha: "155648d988845ad9b918dd2f3085cfb04aaebdd5", date: "2022-09-27 23:54:53 UTC", description: "Use `std::task::ready!` instead of `futures::ready!`", pr_number: 14599, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 26, insertions_count: 46, deletions_count: 60},
		{sha: "6ad67f6d8b751a0b9529f838fcc15ab161fd3ed8", date: "2022-09-28 01:23:39 UTC", description: "Use `std::future::poll_fn` instead of `futures::future::poll_fn`", pr_number: 14600, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 8, deletions_count: 14},
		{sha: "b1d9f09c687b0c1870907972a31bb3e3c212f8c9", date: "2022-09-28 03:07:02 UTC", description: "Emit `EventsSent` in driver in new-style sinks", pr_number: 14589, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 20, insertions_count: 77, deletions_count: 136},
		{sha: "b1798450ebadd049cc258d308a83120ba0d830fa", date: "2022-09-28 23:45:03 UTC", description: "Refactor modules", pr_number: 14556, scopes: ["clickhouse sink"], type: "chore", breaking_change: false, author: "Deen", files_count: 5, insertions_count: 756, deletions_count: 729},
		{sha: "c3f310f140249eec1f70711de48744e272079a1f", date: "2022-09-28 22:49:42 UTC", description: "fix syslog docs", pr_number: 14605, scopes: ["docs"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "c3988f59c6977153315d95f901d8927492c51b55", date: "2022-09-28 22:54:10 UTC", description: "bump tokio from 1.21.1 to 1.21.2", pr_number: 14606, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 13},
		{sha: "f56f3c8e9773b5b9b4d557a38161e89d72826467", date: "2022-09-29 02:02:18 UTC", description: "fix invalid enum schemas + provide more enum metadata", pr_number: 14586, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 11, insertions_count: 297, deletions_count: 93},
		{sha: "b1f44307d266484722418d0d3621cedcc9b16434", date: "2022-09-29 03:02:11 UTC", description: "improve unrecoverable error messages for disk v2", pr_number: 14524, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 34, insertions_count: 393, deletions_count: 176},
		{sha: "8bb3d58a39b60b1b7ae676a5aeda5739b0082bac", date: "2022-09-29 03:03:52 UTC", description: "bump thiserror from 1.0.36 to 1.0.37", pr_number: 14607, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "ab07ed583a1f7ddb4615dde0a9ca1474648c8b95", date: "2022-09-29 03:22:26 UTC", description: "bump warp from 0.3.2 to 0.3.3", pr_number: 14608, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 29, deletions_count: 87},
		{sha: "42a4359dbe5756d6e0c09fba9e9ce043d97fd292", date: "2022-09-29 08:53:59 UTC", description: "bump rmp-serde from 1.1.0 to 1.1.1", pr_number: 14609, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "dfbb7202c369a72923cc5f8b8b8197e5a1bb47ae", date: "2022-09-29 08:59:52 UTC", description: "bump tonic from 0.8.1 to 0.8.2", pr_number: 14613, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "3f463dc8f99bea02a386dbed7e0b5fd5cbfa41d6", date: "2022-09-29 05:17:31 UTC", description: "Remove v1 related options", pr_number: 14619, scopes: ["vector sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 4, deletions_count: 19},
		{sha: "1ef045608a75637f524902f18b70a0c178e4c693", date: "2022-09-29 09:50:53 UTC", description: "bump tonic-build from 0.8.0 to 0.8.2", pr_number: 14614, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f44a7d52746b981d8084e6fbfb9e2b97a9f53c9c", date: "2022-09-29 16:22:19 UTC", description: "Multiple endpoints", pr_number: 14088, scopes: ["elasticsearch sink"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 13, insertions_count: 384, deletions_count: 78},
		{sha: "26a268d983ef3412d2e2182b20ea8ff7f2ee53e0", date: "2022-09-30 02:08:19 UTC", description: "update ElasticSearch emitted errors to comply with spec", pr_number: 14615, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 6, insertions_count: 19, deletions_count: 65},
		{sha: "ff934215af47ccbdb2d555419c1ccc36a54f2e59", date: "2022-09-30 03:37:27 UTC", description: "Handle s3:TestEvent and drop it", pr_number: 14572, scopes: ["aws_s3 source"], type: "fix", breaking_change: false, author: "Ben Cordero", files_count: 1, insertions_count: 27, deletions_count: 3},
		{sha: "875fb76ec1ab5d69a230f1f3a302e18fe3a2a914", date: "2022-09-29 22:51:31 UTC", description: "bump proc-macro2 from 1.0.44 to 1.0.46", pr_number: 14627, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "cf9a2b3609b47dbd2289d0871d60cf631eaba820", date: "2022-09-30 03:59:35 UTC", description: "bump crossbeam-utils from 0.8.11 to 0.8.12", pr_number: 14625, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 6},
		{sha: "be79d1e420f55a8c5b80df96d766471b9206cd6a", date: "2022-09-30 05:38:09 UTC", description: "update gcs common emitted errors.", pr_number: 14634, scopes: ["observability"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 14, deletions_count: 5},
		{sha: "317da46c0597dc508993cbe37ca682a39917c6cf", date: "2022-09-30 01:52:31 UTC", description: "bump libc from 0.2.133 to 0.2.134", pr_number: 14636, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "54787d07a676530ee8ec6ee1dd766502c4da3297", date: "2022-09-30 07:25:46 UTC", description: "Add `run_and_assert_sink_error` to integration test", pr_number: 14638, scopes: ["gcp_pubsub sink"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "05125d4d0b17b536acfcc14a871d05b6f66e97be", date: "2022-09-30 03:37:08 UTC", description: "add initial version of buffering docs", pr_number: 14617, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 4, insertions_count: 291, deletions_count: 11},
		{sha: "bf6efe98887ce65f58ef06e2b26159d452fb01a4", date: "2022-09-30 08:49:08 UTC", description: "bump actions/github-script from 3.1.0 to 6.3.0", pr_number: 14643, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 9, deletions_count: 9},
		{sha: "ef7f76bada0ea89a1ce04bdb8ab59844a4a8711b", date: "2022-10-01 03:37:04 UTC", description: "don't assert a single endpoint for tests with multiple endpoints", pr_number: 14649, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "6e8064653c04b19e6568e7f3467d8dbde17a3bce", date: "2022-10-01 04:00:24 UTC", description: "bump ctr from 0.9.1 to 0.9.2", pr_number: 14648, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "30742a68e36d877de814a3a51ec74a76c225a444", date: "2022-10-01 00:33:42 UTC", description: "Don't render required tag on telemetry tags", pr_number: 14646, scopes: ["template website"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 3},
		{sha: "01e2c46230fdf09a8de4ff5fa93a64a26ff00526", date: "2022-10-01 05:34:12 UTC", description: "add `component_spec_compliance` test", pr_number: 14658, scopes: ["gcp_stackdriver_metrics sink"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 54, deletions_count: 2},
		{sha: "b50f685d7e55ed8cef820973a63e516cb8299a39", date: "2022-10-01 00:47:58 UTC", description: "fix a broken link in the sink buffer docs + spelling error in upgrade notes", pr_number: 14664, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "0e94a5b01f36d616ecb0bb47a652faf86097e963", date: "2022-10-01 13:41:06 UTC", description: "don't panic on invalid dates", pr_number: 14666, scopes: ["syslog source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "86f6c38a2bffbf5d918ec6a391eb1ad712010b8f", date: "2022-10-04 04:43:57 UTC", description: "Add `parse_cef` function", pr_number: 14382, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 6, insertions_count: 739, deletions_count: 4},
		{sha: "d4a3e4eeaa55604c1b9ff2c1b71f84afb7f48088", date: "2022-10-04 00:17:46 UTC", description: "Add missing container_id and pod_owner information", pr_number: 14684, scopes: ["docs"], type: "fix", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 32, deletions_count: 6},
		{sha: "05f57e49e95bf1be6e5c2ade21633bdbe5464898", date: "2022-10-04 06:43:43 UTC", description: "update to latest azure SDK", pr_number: 13453, scopes: ["azure_blob sink"], type: "enhancement", breaking_change: false, author: "Yves Peter", files_count: 6, insertions_count: 117, deletions_count: 103},
		{sha: "0c3c38d2a3e09b71609ee16b18a9ec973b8a2f57", date: "2022-10-04 01:13:26 UTC", description: "Correct serde tagging on event enum", pr_number: 14686, scopes: ["aws_s3 source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "69a4923e220f0b008f6e2ae4f2d45c01ba5468a8", date: "2022-10-04 01:18:56 UTC", description: "Correct `datadog_traces` sink warning about APM stats.", pr_number: 14647, scopes: ["docs"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a316a57f4ff9b24ec96e78e39622e913ec33be63", date: "2022-10-04 07:41:42 UTC", description: "bump actions/github-script from 6.3.0 to 6.3.1", pr_number: 14691, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 9, deletions_count: 9},
		{sha: "abfc608176ef394a63adfe05420743a0aa5ed258", date: "2022-10-04 08:30:29 UTC", description: "bump styfle/cancel-workflow-action from 0.10.0 to 0.10.1", pr_number: 14690, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "d100bf90217e29250fc5c5cedf048fdc6f142a2a", date: "2022-10-04 04:35:30 UTC", description: "Increment dropped events metric on each drop", pr_number: 14667, scopes: ["filter transform"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 86, deletions_count: 41},
		{sha: "84bf26d769088f1cc30673e701c30b94156e7cee", date: "2022-10-04 07:29:36 UTC", description: "bump clap from 3.2.22 to 4.0.5", pr_number: 14663, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 18, insertions_count: 208, deletions_count: 185},
		{sha: "346b8137a32b385407ce215b7ae9503f7e21494a", date: "2022-10-04 13:22:41 UTC", description: "bump bitmask-enum from 2.0.1 to 2.1.0", pr_number: 14704, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c0fd777b9ea31b39db91ed3dd5fb70cfc8cb851c", date: "2022-10-04 13:34:21 UTC", description: "bump inventory from 0.3.1 to 0.3.2", pr_number: 14705, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1248a0bd978edf2ed70d76eeb3deb85fa03a60f3", date: "2022-10-04 15:24:59 UTC", description: "bump clap from 4.0.5 to 4.0.9", pr_number: 14703, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 12, deletions_count: 12},
		{sha: "9a07fdd90e731b3a4fb09f36c8b027d9ddf79dff", date: "2022-10-04 22:04:13 UTC", description: "Refactor inner select loop", pr_number: 14692, scopes: ["throttle transform"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 56, deletions_count: 62},
		{sha: "655f8a35d5364c057c55177a9bd20b5ce55d08f2", date: "2022-10-05 00:15:03 UTC", description: "Avoid blocking `vector validate` on input", pr_number: 14665, scopes: ["stdin source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "2b7d170d8845a0062b7c90c6b7151709e9fab2d3", date: "2022-10-05 13:04:25 UTC", description: "Support snappy compressed proto", pr_number: 12927, scopes: ["loki sink"], type: "feat", breaking_change: false, author: "xd@cloud", files_count: 17, insertions_count: 1083, deletions_count: 18},
		{sha: "f4c74a619c95eda2559ddca83266ea8d82331720", date: "2022-10-05 05:24:15 UTC", description: "bump mongodb from 2.3.0 to 2.3.1", pr_number: 14706, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fd046559c11a1d8ef6a72be0e89d4666a2051576", date: "2022-10-05 14:16:46 UTC", description: "Change metrics record to optional", pr_number: 14612, scopes: ["mongodb_metrics source"], type: "fix", breaking_change: false, author: "Rui Li", files_count: 2, insertions_count: 8, deletions_count: 6},
		{sha: "5ea50ecefe31f43c819171713e7ea66d44012731", date: "2022-10-05 06:55:30 UTC", description: "bump arbitrary from 1.1.4 to 1.1.6", pr_number: 14709, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "7b11473b0c8e42c5ebd8bf493b772b9127975a20", date: "2022-10-05 07:20:16 UTC", description: "bump lru from 0.8.0 to 0.8.1", pr_number: 14682, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ca8faf5a4e0c034b146c470cff0fccd11883bdb9", date: "2022-10-05 01:25:55 UTC", description: "Make docs/contributing reference an actual link", pr_number: 14719, scopes: [], type: "docs", breaking_change: false, author: "Mike Perrone", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9182031cb173f467d1d293c1e7853eb0e2681f5c", date: "2022-10-05 04:42:25 UTC", description: "use `undefined` when merging instead of `null`", pr_number: 14670, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Nathan Fox", files_count: 4, insertions_count: 15, deletions_count: 15},
		{sha: "795e35ac966d2fad6e33d1978c1c263d70e23f16", date: "2022-10-05 04:43:06 UTC", description: "Audit metric_to_log transform", pr_number: 14486, scopes: ["metric_to_log transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 51, deletions_count: 29},
		{sha: "052022e5f04007d53ae60edd2ffe384c780bf150", date: "2022-10-05 05:32:18 UTC", description: "link docs/CONTRIBUTING.md to CONTRIBUTING.md", pr_number: 14720, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "015cc8a9550d054a661889e7cfdacc20dcc2ef64", date: "2022-10-05 07:48:17 UTC", description: "Add run_vrl() with wasm-compatible vrl stdlib", pr_number: 14668, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Jonathan Padilla", files_count: 6, insertions_count: 376, deletions_count: 11},
		{sha: "b3a02000f3f05084c5ba8cbb9e1b984cacbd3f65", date: "2022-10-05 06:12:32 UTC", description: "Check before installing cargo-deny", pr_number: 14721, scopes: ["tests"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 3, deletions_count: 1},
		{sha: "9cbae1ea9cce6ca4b7c74800e16ac67906e1301c", date: "2022-10-06 04:42:17 UTC", description: "emit `StreamClosedError` on event_stream error", pr_number: 14731, scopes: ["syslog source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "68516dd8e4e1fde78ad1cf03c215174ab841a914", date: "2022-10-06 04:43:03 UTC", description: "emit errors on grpc errors", pr_number: 14715, scopes: ["vector source", "opentelemetry source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 76, deletions_count: 9},
		{sha: "bb3ab0b2d266f64e2ff63d06cb72f410b8533742", date: "2022-10-06 03:20:32 UTC", description: "fix another couple panics caused by the clap v4 upgrade", pr_number: 14734, scopes: ["cli"], type: "fix", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 6, deletions_count: 20},
		{sha: "a873281f0f571a1c2b1e035e74b96328f4342d86", date: "2022-10-06 10:55:08 UTC", description: "emit event on error", pr_number: 14736, scopes: ["stdin source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 5, insertions_count: 75, deletions_count: 29},
		{sha: "5702598600506d9d9fdf7a96d2ebb2305f7f0643", date: "2022-10-06 05:39:51 UTC", description: "Add deprecation notice for version 1 API", pr_number: 14735, scopes: ["lua transform"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 41, deletions_count: 2},
		{sha: "8fcea649adecc7f83b3292345a39c037e4f40781", date: "2022-10-07 03:37:48 UTC", description: "remove duplicate emitted error", pr_number: 14748, scopes: ["statsd source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 72, deletions_count: 42},
		{sha: "38ef9e1e5b19730ced4a84113221eb4cd013c545", date: "2022-10-07 12:48:10 UTC", description: "add custom method support to http_scrape source", pr_number: 14737, scopes: ["http"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 13, insertions_count: 106, deletions_count: 43},
		{sha: "915804226fd52ecb43a264fd36bc07210bccf973", date: "2022-10-07 08:08:19 UTC", description: "Fix `target_insert` for metric tags", pr_number: 14756, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 49, deletions_count: 0},
		{sha: "97f5cb595724be22397998ba0157380ab9bce7db", date: "2022-10-08 00:49:18 UTC", description: "Audit pipelines transform", pr_number: 14483, scopes: ["pipelines transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 45, deletions_count: 0},
		{sha: "f4ce9f535e6f1e699a17bbf93857e85df37981cf", date: "2022-10-08 06:56:55 UTC", description: "Comply to `*EventsDropped` instrumentation spec in `splunk_hec_logs` sink", pr_number: 14513, scopes: ["splunk_hec_logs sink"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "9f56c48910d3436b26abfecd6c7297cb0cbce114", date: "2022-10-08 06:27:20 UTC", description: "add assert_source_error to error path test", pr_number: 14764, scopes: ["splunk_hec source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 16, deletions_count: 10},
		{sha: "5b0cd345e69dc128a0c7bc4393560d7e67c7da58", date: "2022-10-08 02:07:01 UTC", description: "Audit lua v2 transform", pr_number: 14540, scopes: ["lua transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 247, deletions_count: 194},
		{sha: "35a091deda5f5524c1a2f76fa8acaacd35a10385", date: "2022-10-08 10:57:46 UTC", description: "Share common `SocketConnectionError` between TCP, UDP and Unix sinks", pr_number: 14530, scopes: ["observability"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 8, insertions_count: 87, deletions_count: 83},
		{sha: "a88303b4e450a44325c796ad789327751fb6e075", date: "2022-10-08 05:49:10 UTC", description: "Audit log_to_metric transform", pr_number: 14487, scopes: ["log_to_metric transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 148, deletions_count: 99},
		{sha: "8e0b693a33dc78df952b0774ea59a11f386c63ce", date: "2022-10-08 03:02:13 UTC", description: "Clarify passing in example data via `vector vrl`", pr_number: 14722, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 14, deletions_count: 2},
		{sha: "e62725d69c66759ce901ca888832b14154c9c8dc", date: "2022-10-11 03:00:38 UTC", description: "add internal_log_rate_limit to `error!` calls", pr_number: 14765, scopes: ["observability"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 22, insertions_count: 53, deletions_count: 11},
		{sha: "87511d5df76f14a6ef72837de99234e8b40ea2f4", date: "2022-10-11 13:13:20 UTC", description: "Add `internal_log_rate_limit = true` to internal `*Error` events", pr_number: 14511, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count: 13, insertions_count: 19, deletions_count: 15},
		{sha: "31fa69c6a75ae1adf61ad27a0744fd87b443182b", date: "2022-10-12 05:33:57 UTC", description: "Comply to `*EventsDropped` instrumentation spec in `vector` sink", pr_number: 14516, scopes: ["vector sink"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count: 12, insertions_count: 33, deletions_count: 34},
		{sha: "61583365670f8613f4340ecf0ac3c51377d99fbd", date: "2022-10-13 22:33:30 UTC", description: "validate internal events", pr_number: 14769, scopes: ["remap transform"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 1, insertions_count: 28, deletions_count: 1},
		{sha: "202087b591c747b2e373892bf29d70d63f03216c", date: "2022-10-14 06:17:23 UTC", description: "process event as per type", pr_number: 14778, scopes: ["file_descriptor"], type: "fix", breaking_change: false, author: "Vimal Kumar", files_count: 1, insertions_count: 17, deletions_count: 9},
		{sha: "418e435e4135b10f6404f33f7cba864bd0f64836", date: "2022-10-14 04:51:28 UTC", description: "document enrichment table `type` field", pr_number: 14830, scopes: ["enriching"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 12, deletions_count: 0},
		{sha: "31d97e9e8eb50e3ba832de2f7942427f2e1bd2cb", date: "2022-10-15 06:39:54 UTC", description: "Fix `byte_size` in `EventsReceived` internal event", pr_number: 14842, scopes: ["amqp source"], type: "fix", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "b7ebd47cd6060b57c8a63b3b89f8ec4827fdbf56", date: "2022-10-15 08:34:06 UTC", description: "Test compliance to instrumentation spec in `socket` source", pr_number: 14843, scopes: ["socket source"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 32, deletions_count: 26},
		{sha: "35fd86dc2e2ba65da1c45598321ae37c3c6717da", date: "2022-10-15 08:35:05 UTC", description: "Fix duplicate `internal_log_rate_limit` attribute", pr_number: 14844, scopes: ["socket source"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "13353b19ad15982bdfc9d4b05ed6081c0722a03f", date: "2022-10-15 02:58:45 UTC", description: "Remove extra deprecation warning", pr_number: 14847, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "caba2d2fafaf811b8821bd3a23e93fab4957a4ab", date: "2022-10-15 06:08:50 UTC", description: "Update template and field path syntax", pr_number: 14849, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 34, deletions_count: 112},
		{sha: "2a058082d021d5372ceea133f67c5404cc2658ab", date: "2022-10-19 03:12:56 UTC", description: "emit `ComponentEventsDropped` internal event", pr_number: 14767, scopes: ["sample transform"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 2, insertions_count: 34, deletions_count: 3},
		{sha: "72ce6f765636944719908fa3a0e18fbc5f40fa71", date: "2022-10-19 03:13:26 UTC", description: "emit `ComponentEventsDropped` internal event", pr_number: 14768, scopes: ["tag_cardinality_limit transform"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 2, insertions_count: 171, deletions_count: 115},
		{sha: "157a83c2267b1a7460a4ec380edd68b3048ae2df", date: "2022-10-19 03:13:38 UTC", description: "emit `ComponentEventsDropped` internal event", pr_number: 14770, scopes: ["throttle transform"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 2, insertions_count: 39, deletions_count: 3},
		{sha: "afd273aa658c4ca19b69d41df9027fd9ffd3c10d", date: "2022-10-13 06:36:42 UTC", description: "unify address type for socket-based component configs", pr_number: 14799, scopes: ["core"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 25, insertions_count: 360, deletions_count: 394},
		{sha: "2a121fe2c81304caeaf8fae5a594c4c34f7104ec", date: "2022-10-18 22:10:39 UTC", description: "Adhere to instrumentation spec", pr_number: 14425, scopes: ["socket sink"], type: "enhancement", breaking_change: false, author: "neuronull", files_count: 13, insertions_count: 89, deletions_count: 17},
		{sha: "b5050d5abd904d2d5f4b33062dcd6e9aca31112e", date: "2022-10-27 09:18:52 UTC", description: "added docs for using kafka components with Azure Event Hubs", pr_number: 14927, scopes: ["kafka source", "kafka sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 24, deletions_count: 0},
		{sha: "4722011c6eb35d9143143d114d64652be5df23fd", date: "2022-10-28 01:24:21 UTC", description: "update file source docs with details about include/exclude semantics", pr_number: 14776, scopes: [], type: "docs", breaking_change: false, author: "Andrew Roberts", files_count: 2, insertions_count: 11, deletions_count: 4},
		{sha: "ec40d45e8e0a525b59ce6ab62293c3c0e436b479", date: "2022-10-22 04:37:21 UTC", description: "Add templated tags to log2metric soaks", pr_number: 14912, scopes: ["soak tests"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 15, deletions_count: 6},
		{sha: "e8c3e38b2699ecbd880effb5b39b98343f96717d", date: "2022-10-22 06:17:41 UTC", description: "Pre-parse templates for performance", pr_number: 14908, scopes: ["templating"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 191, deletions_count: 93},
		{sha: "84afb8e9a057ce1be1ffe91e1660c618f769105e", date: "2022-11-01 04:15:03 UTC", description: "(new style sinks) Emit `EventsDropped` and `Error` internal events in the service driver", pr_number: 14836, scopes: ["observability"], type: "chore", breaking_change: false, author: "neuronull", files_count: 103, insertions_count: 1453, deletions_count: 754},
		{sha: "c10616b5e752106a2ccddc354f8f10c850271037", date: "2022-11-01 06:24:31 UTC", description: "Fix a few more usages of internal_log_rate_secs", pr_number: 15041, scopes: ["observability"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
	]
}
