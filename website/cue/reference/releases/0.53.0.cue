package metadata

releases: "0.53.0": {
	date:     "2026-01-27"
	codename: ""

	whats_next: []

	description: """
		The Vector team is excited to announce version `0.53.0`!

		## Release highlights

		- Functions to access internal Vector metrics are now available for VRL: `get_vector_metric`,
		  `find_vector_metrics`, and `aggregate_vector_metrics`. You are now able to fetch snapshots
		  of the metrics that are updated every `metrics_storage_refresh_period`.
		- The `clickhouse` sink now supports the `arrow_stream` format option, enabling high-performance
		  binary data transfer using Apache Arrow IPC. This provides significantly better performance
		  and smaller payload sizes compared to JSON-based formats.
		- Added a new `doris` sink for sending log data to Apache Doris databases using the Stream Load API.
		- Added `syslog` codec for encoding Vector events to Syslog format. RFC5424 and RFC3164 are
		  supported.
		- Added moving-mean gauges for source and transform buffers (`source_buffer_utilization_mean` and `transform_buffer_utilization_mean`), so observers can track an exponentially weighted moving average (EWMA) of buffer utilization in addition to the instant level.

		## Breaking Changes
		- Buffers now emit metric names for sizes that better follow the metric naming standard specification
		  while keeping the old related gauges available for a transition period. Operators should update
		  dashboards and alerts to the new variants as the legacy names are now deprecated.

		  * `buffer_max_size_bytes` deprecates `buffer_max_byte_size`
		  * `buffer_max_size_events` deprecates `buffer_max_event_size`
		  * `buffer_size_bytes` deprecates `buffer_byte_size`
		  * `buffer_size_events` deprecates `buffer_events`


		- Increased the number of buckets in internal histograms to reduce the smallest
		  bucket down to approximately 0.000244 (2.0^-12). If you were manually indexing buckets
		  using VRL, you have to change your indexes since the number of buckets changed
		  from 20 to 26.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				Fixed a `host_metrics` source issue that caused TCP metrics collection to fail with "Could not parse netlink response: invalid netlink buffer" errors on Linux systems.
				"""
			contributors: ["mushrowan"]
		},
		{
			type: "fix"
			description: """
				Fixed recurrent "Failed framing bytes" produced by TCP sources such as Fluent and Logstash by ignoring connection
				resets that occur after complete frames. Connection resets with partial frame data are still reported as errors.
				"""
			contributors: ["gwenaskell"]
		},
		{
			type: "feat"
			description: """
				Functions to access internal Vector metrics are now available for VRL: `get_vector_metric`, `find_vector_metrics` and `aggregate_vector_metrics`. They work with a snapshot of the metrics, and the interval the snapshot is taken in can be controlled with the `metrics_storage_refresh_period` global option. Aggregation supports `max`, `avg`, `min`, and `max` functions.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue in the `postgres` sink which made a TLS connection impossible due to a missing `sqlx` feature flag.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: """
				The `mqtt` source config field `topic` can now be a list of MQTT topic strings instead of just a string. If a list is provided, the `mqtt` source client subscribes to all the topics.
				"""
			contributors: ["december1981"]
		},
		{
			type: "enhancement"
			description: """
				The `clickhouse` sink now supports the `arrow_stream` format option, enabling high-performance binary data transfer using Apache Arrow IPC. This provides significantly better performance and smaller payload sizes compared to JSON-based formats.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "fix"
			description: """
				Fixed the OpenTelemetry source to collect HTTP headers for logs with or without the `use_otlp_decoding` configuration option.
				"""
			contributors: ["ozanichkovsky"]
		},
		{
			type: "fix"
			description: """
				The `opentelemetry` source now correctly emits the `component_received_events_total` metric when `use_otlp_decoding` is enabled for HTTP requests. Previously, this metric would show 0 despite events being received and processed.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: """
				Autocompletion scripts for the Vector CLI can now be generated with `vector completion <SHELL>`.
				"""
			contributors: ["weriomat"]
		},
		{
			type: "fix"
			description: """
				Fixed histogram incremental conversion by ensuring all individual buckets increase or reinitializing the entire metric.
				"""
			contributors: ["dd-sebastien-lb"]
		},
		{
			type: "chore"
			description: """
				Buffers now emit metric names for sizes that better follow the metric naming standard specification
				while keeping the old related gauges available for a transition period. Operators should update
				dashboards and alerts to the new variants as the legacy names are now deprecated.

				* `buffer_max_size_bytes` deprecates `buffer_max_byte_size`
				* `buffer_max_size_events` deprecates `buffer_max_event_size`
				* `buffer_size_bytes` deprecates `buffer_byte_size`
				* `buffer_size_events` deprecates `buffer_events`
				"""
			contributors: ["bruceg"]
		},
		{
			type: "enhancement"
			description: """
				Added moving-mean gauges for source and transform buffers (`source_buffer_utilization_mean` and `transform_buffer_utilization_mean`), so observers can track an EWMA of buffer utilization in addition to the instant level.
				"""
			contributors: ["bruceg"]
		},
		{
			type: "feat"
			description: """
				Add new Apache Doris sink for sending log data to Apache Doris databases using the Stream Load API. The sink supports configurable batching, custom HTTP headers for Doris-specific options, authentication, rate limiting, adaptive concurrency control, and includes comprehensive health checks.
				"""
			contributors: ["bingquanzhao"]
		},
		{
			type: "chore"
			description: """
				Increased the number of buckets in internal histograms to reduce the smallest
				bucket down to approximately 0.000244 (2.0^-12). Since this shifts all the
				bucket values out, it may break VRL scripts that rely on the previous values.
				"""
			contributors: ["bruceg"]
		},
		{
			type: "enhancement"
			description: """
				Add `content_type` option to the `gcp_cloud_storage` sink to override the `Content-Type` of created objects. If unset, defaults to the encoder's content type.
				"""
			contributors: ["AnuragEkkati"]
		},
		{
			type: "feat"
			description: """
				Added `syslog` codec for encoding Vector events to Syslog format.
				It handles RFC5424 and RFC3164 format, including specific field length limitations, character sanitization,
				and security escaping.
				"""
			contributors: ["syedriko", "polarathene", "vparfonov"]
		},
		{
			type: "enhancement"
			description: """
				Added `buffer_utilization_ewma_alpha` configuration option to the global
				options, allowing users to control the alpha value for the exponentially
				weighted moving average (EWMA) used in source and transform buffer utilization
				metrics.
				"""
			contributors: ["bruceg"]
		},
		{
			type: "enhancement"
			description: """
				Vector-specific VRL functions are now available everywhere. Previously some functions were not
				available inside codec VRL transforms and in the VRL CLI (through `vector vrl`).
				"""
			contributors: ["thomasqueirozb"]
		},
	]

	vrl_changelog: """
		### [0.30.0 (2026-01-22)]

		#### Breaking Changes & Upgrade Guide

		- The `usage()` method on the `Function` trait is now required. Custom VRL functions must implement this
		method to return a `&'static str` describing the function's purpose.

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1608)

		#### Fixes

		- Corrected the type definition for the `format_int` function to return bytes instead of integer.

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1586)


		### [0.29.0 (2025-12-11)]
		"""

	commits: [
		{sha: "d5dbab97ba2ba4279b361aa8ec63cd698729ec0b", date: "2025-12-16 23:24:18 UTC", description: "v0.52.0", pr_number: 24388, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 50, insertions_count: 405, deletions_count: 143},
		{sha: "b6b334615b1e13e457f663444ea7f402fdcb5ab3", date: "2025-12-19 23:02:51 UTC", description: "Upgrade to rust 1.92.0", pr_number: 24376, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 5, deletions_count: 11},
		{sha: "01cf516897a00b9bc7f149ab1435415c183dc876", date: "2025-12-20 00:42:27 UTC", description: "Update to React 19", pr_number: 24392, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 1717, deletions_count: 1988},
		{sha: "60fa98017d861bf88141926ad97705f13aa65f1f", date: "2025-12-20 13:49:57 UTC", description: "Add `ArrowStream` format", pr_number: 24373, scopes: ["clickhouse sink"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 18, insertions_count: 1434, deletions_count: 41},
		{sha: "fa499b94c26145e127e7d8e955fe07c4732fa0ac", date: "2025-12-20 01:07:06 UTC", description: "VRL functions return types", pr_number: 24400, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 4, deletions_count: 4},
		{sha: "89bf79564137976a21a7f4ff138b0b1f54c36a02", date: "2025-12-20 02:16:23 UTC", description: "Add VRL crate documentation", pr_number: 24384, scopes: ["external docs"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "fad6e623a97865d0668879e37c03b50df9ec41f2", date: "2025-12-22 23:04:21 UTC", description: "bump cargo deny to 0.18.9", pr_number: 24404, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "2af657dfbb7e476d2966d05d5f2edb5c66112b40", date: "2025-12-23 01:49:46 UTC", description: "consolidate all VRL functions into vector-vrl-functions crate", pr_number: 24402, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Thomas", files_count: 34, insertions_count: 119, deletions_count: 112},
		{sha: "90cf7d044c60e651ff6cec8c3673686cc36f3765", date: "2025-12-23 10:15:28 UTC", description: "support multiple mqtt source topics", pr_number: 23670, scopes: ["mqtt source"], type: "feat", breaking_change: false, author: "Stephen Brown", files_count: 6, insertions_count: 102, deletions_count: 15},
		{sha: "36a935f62bcde10ffb6646f1d99e12e6d9ea7fe1", date: "2025-12-23 23:53:07 UTC", description: "update TypeScript and Node.js dependencies, enable ES modules", pr_number: 24406, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 7, insertions_count: 115, deletions_count: 52},
		{sha: "3749b70194de509fd0e12534f8affa35c490587b", date: "2025-12-24 08:36:03 UTC", description: "collect headers for logs in opentelemetry source with use_otlp_decoding set to true", pr_number: 24307, scopes: ["sources"], type: "fix", breaking_change: false, author: "Oleksandr Zanichkovskyi", files_count: 3, insertions_count: 100, deletions_count: 36},
		{sha: "11aa135eeaeec0b80967725731f483372683ed64", date: "2025-12-31 05:10:17 UTC", description: "Add aggregate transform to semantic PR scope list", pr_number: 24422, scopes: ["ci"], type: "chore", breaking_change: false, author: "Karol Chrapek", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "5f8ab319b847c10912ee01a204b40a1105616d6f", date: "2026-01-06 04:24:58 UTC", description: "Add syslog encoder", pr_number: 23777, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Vitalii Parfonov", files_count: 39, insertions_count: 2403, deletions_count: 3},
		{sha: "c0fda7e06efdfed3b60fcc288a531304dafe13c0", date: "2026-01-05 21:31:59 UTC", description: "bump the clap group with 2 updates", pr_number: 24430, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 10},
		{sha: "3656b659a2e7eced25949c10cd4162514289f1b2", date: "2026-01-05 22:04:25 UTC", description: "bump actions/cache from 4.3.0 to 5.0.1", pr_number: 24439, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 5, deletions_count: 5},
		{sha: "9e12569ba9739f4d9c4e05ffbf306e0d9ed48aa3", date: "2026-01-06 03:05:11 UTC", description: "bump github/codeql-action from 4.31.6 to 4.31.9", pr_number: 24438, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9c1b1f0e5cf3e69da9ffe94d2fbb9415d109e16a", date: "2026-01-05 22:37:08 UTC", description: "bump docker/setup-buildx-action from 3.11.1 to 3.12.0", pr_number: 24437, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "5dcb8262810e8158e8d2b17c9dfc0de9e9c4b846", date: "2026-01-06 02:16:11 UTC", description: "bump the artifact group with 2 updates", pr_number: 24436, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 71, deletions_count: 71},
		{sha: "236928a042a71db77b050d5afbb392ad5d4e6cdc", date: "2026-01-07 02:14:17 UTC", description: "bump rkyv to 0.7.46", pr_number: 24451, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "acd4a737d7f45473c6125791743173027927d4ac", date: "2026-01-07 18:47:54 UTC", description: "add functions for internal vector metrics access in VRL", pr_number: 23430, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 63, insertions_count: 1737, deletions_count: 210},
		{sha: "eabdd5e79cd5124fb24d37c91f174de3cc3d5463", date: "2026-01-08 02:25:34 UTC", description: "add Apache Doris sink support", pr_number: 23117, scopes: ["new sink"], type: "feat", breaking_change: false, author: "bingquanzhao", files_count: 25, insertions_count: 3216, deletions_count: 1},
		{sha: "4daa1f83a22317df88a6ac025ccb19cdcb5214fe", date: "2026-01-08 22:26:39 UTC", description: "Add `_utilization_mean` buffer metrics", pr_number: 24453, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 16, insertions_count: 174, deletions_count: 65},
		{sha: "935f1f742a52f70a83672250e3299bf50082f482", date: "2026-01-08 23:39:31 UTC", description: "Clarify glob syntax in file source documentation", pr_number: 24462, scopes: ["file source"], type: "docs", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 8, deletions_count: 0},
		{sha: "a5a6b5e82f3000c3d1284c8a88a7577806b3ef1d", date: "2026-01-09 01:11:39 UTC", description: "bump lru to 0.16.3", pr_number: 24463, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 143, deletions_count: 89},
		{sha: "94a2f401317257be6c77c0d14b803bad65f0ad4b", date: "2026-01-09 21:20:58 UTC", description: "Add configuration for buffer utilization EWMA alpha", pr_number: 24467, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 16, insertions_count: 111, deletions_count: 22},
		{sha: "a63cde11d6fb45ffee0f478e6464b00f54ca3d72", date: "2026-01-10 06:00:30 UTC", description: "bump alpine from 3.22 to 3.23 in /distribution/docker/alpine in the docker-images group", pr_number: 24426, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1a676fdad8f18f20382d40e175edecf14beff0bb", date: "2026-01-10 03:16:57 UTC", description: "update dependabot.yml to update distroless docker images", pr_number: 24478, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 30, deletions_count: 0},
		{sha: "9d7dd5cff8dd5944bf6768dbe0ea95ad1dcd35c9", date: "2026-01-10 03:17:24 UTC", description: "Add step-security-bot to cla allowlist", pr_number: 24474, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a3a0e3a61305cd79da209cd287667982e4b047c5", date: "2026-01-12 22:42:08 UTC", description: "emit component_received_event* metrics when use_otlp_decoding is enabled", pr_number: 24480, scopes: ["opentelemetry source"], type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 116, deletions_count: 5},
		{sha: "d55bb0b7eb7fb0dec08285e2ea42ea41023df4af", date: "2026-01-13 00:44:02 UTC", description: "enable TLS flag", pr_number: 23536, scopes: ["postgres sink"], type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 6, deletions_count: 1},
		{sha: "0091dadf96d09cd7baefb1e712e1343563645690", date: "2026-01-14 03:11:19 UTC", description: "Tim.sara/transcend removal", pr_number: 24419, scopes: ["website"], type: "chore", breaking_change: false, author: "timsara331", files_count: 2, insertions_count: 0, deletions_count: 42},
		{sha: "6fa2b12a04d18eaa61f3d4d985884f933fb3f1c4", date: "2026-01-15 19:55:07 UTC", description: "bump undici from 7.16.0 to 7.18.2 in /website", pr_number: 24498, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "65afcd214a1cd887ed41af136c6a68ce57d6175d", date: "2026-01-15 21:18:23 UTC", description: "Expand internal histogram precision", pr_number: 24497, scopes: ["observability"], type: "enhancement", breaking_change: true, author: "Bruce Guenter", files_count: 3, insertions_count: 37, deletions_count: 26},
		{sha: "46654ada788d2718473aa587917e4e5500778532", date: "2026-01-16 06:20:18 UTC", description: "absolute to incremental histogram conversion", pr_number: 24472, scopes: ["metrics"], type: "fix", breaking_change: false, author: "dd-sebastien-lb", files_count: 3, insertions_count: 36, deletions_count: 1},
		{sha: "a5d7cc33776f8df7e98ed7f48f38c64e2a974aae", date: "2026-01-16 06:36:12 UTC", description: "Shell autocompletion for vector cli", pr_number: 24414, scopes: ["cli"], type: "enhancement", breaking_change: false, author: "weriomat", files_count: 8, insertions_count: 48, deletions_count: 12},
		{sha: "b8a8d7a4369e72e71771544fba0a3d243e8e4d34", date: "2026-01-15 21:58:00 UTC", description: "add content_type option", pr_number: 24477, scopes: ["gcp_cloud_storage sink"], type: "feat", breaking_change: false, author: "Anurag Ekkati", files_count: 3, insertions_count: 77, deletions_count: 1},
		{sha: "3728a4de07720ad3c9414a7a70bea62dff319c34", date: "2026-01-16 21:01:22 UTC", description: "Standardize buffer size metric names", pr_number: 24493, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 14, insertions_count: 236, deletions_count: 54},
		{sha: "473e31cbbfd7c39fc2d5b0672db6da26e82e28b5", date: "2026-01-17 05:02:32 UTC", description: "fix tcp netlink bug", pr_number: 24441, scopes: ["host_metrics source"], type: "fix", breaking_change: false, author: "rowan", files_count: 3, insertions_count: 8, deletions_count: 2},
		{sha: "18676af53de98c931acdea8c54bbf7a09bec0c01", date: "2026-01-21 00:13:06 UTC", description: "update hugo templates to work with 0.152.2", pr_number: 24140, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 14, deletions_count: 22},
		{sha: "a3bb693380ea7f7217737661b082521c5bc31a8e", date: "2026-01-21 02:18:37 UTC", description: "Add usage method to VRL functions", pr_number: 24504, scopes: ["deps", "internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 18, insertions_count: 161, deletions_count: 27},
		{sha: "249657ba198470dc619f4c0e676ab52392b5469c", date: "2026-01-21 03:00:54 UTC", description: "Merge `src/codecs` into `lib/codecs`", pr_number: 24516, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 52, insertions_count: 260, deletions_count: 237},
		{sha: "f5d0c561da72d2f935bb8696c6b22a2d06576068", date: "2026-01-21 03:12:48 UTC", description: "Add metrics to measure total event processing time", pr_number: 24481, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 42, insertions_count: 673, deletions_count: 67},
		{sha: "399584f10c498e5edfe9e2aebc1f192d33e5e4e6", date: "2026-01-22 04:33:05 UTC", description: "do not log TCP connection resets", pr_number: 24517, scopes: ["sources"], type: "fix", breaking_change: false, author: "Yoenn Burban", files_count: 4, insertions_count: 134, deletions_count: 4},
		{sha: "f6e3282ca0695bcd5fb7ebc022a1a1025d382905", date: "2026-01-21 23:10:33 UTC", description: "move BRANCH into env", pr_number: 24526, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "3c82130af063148556736a36f4e7015487e06b3e", date: "2026-01-22 22:24:36 UTC", description: "use setup action in Master Merge Queue", pr_number: 24473, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 15, insertions_count: 182, deletions_count: 360},
		{sha: "4084133347f049558bdb449b73b73df586642e39", date: "2026-01-23 01:42:30 UTC", description: "Use X logo instead of twitter", pr_number: 24534, scopes: ["website"], type: "feat", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 26, deletions_count: 28},
		{sha: "b90c21aad276360e75088724aa239a83de39e4ea", date: "2026-01-23 00:29:31 UTC", description: "Harden GitHub Actions token permissions", pr_number: 24450, scopes: ["ci"], type: "chore", breaking_change: false, author: "StepSecurity Bot", files_count: 9, insertions_count: 29, deletions_count: 0},
		{sha: "22bd2ae74ded0d991376cbbb2e70ee06a773a26e", date: "2026-01-23 23:24:01 UTC", description: "Pin actions to full commit sha", pr_number: 24538, scopes: ["ci"], type: "chore", breaking_change: false, author: "StepSecurity Bot", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "9cf8e2071318fd599dd87cf6c6fa878970638c69", date: "2026-01-26 19:52:41 UTC", description: "csv enrichment guide incorrect severity", pr_number: 24539, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "15b6bfa5a764cf5c4bdf28454d234d8798e5ec54", date: "2026-01-27 00:17:09 UTC", description: "bump preact from 10.28.0 to 10.28.2 in /website", pr_number: 24457, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "7e0cef1cfe162f870060ae195fa34d6be5a95cc6", date: "2026-01-27 00:22:37 UTC", description: "bump lodash from 4.17.21 to 4.17.23 in /website", pr_number: 24529, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "f5632b6529ebbb236bfe8aef4831b8c13c66eee8", date: "2026-01-27 18:49:58 UTC", description: "respect DISABLE_MOLD in setup action", pr_number: 24548, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "6f6b7ab07af4f4256d272bb8f1910027df98b195", date: "2026-01-27 20:25:56 UTC", description: "Add metrics to measure total event processing time", pr_number: 24546, scopes: ["observability"], type: "revert", breaking_change: false, author: "Thomas", files_count: 42, insertions_count: 67, deletions_count: 673},
		{sha: "049748ed86ba014996da6e469a4deacdaa7121e0", date: "2026-01-27 19:23:43 UTC", description: "count individual items in OTLP batches for component_received_events_total metric", pr_number: 24537, scopes: ["opentelemetry source"], type: "fix", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 153, deletions_count: 2},
	]
}
