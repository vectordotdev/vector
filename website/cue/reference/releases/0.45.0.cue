package metadata

releases: "0.45.0": {
	date:     "2025-02-20"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.45.0!

		Be sure to check out the [upgrade guide](/highlights/2025-02-24-0-45-0-upgrade-guide) for
		breaking changes in this release.

		This release contains a few notable new features, along with numerous enhancements and fixes as listed below:

		- A new type of `enrichment_table`, called `memory`, was introduced! This table can also act
		  can also act as a sink, which enables several new use cases. For instance, this table
		  can be used as a cache or as an interface with an external key value store.
		- A new `websocket_server` sink that acts as a websocket server and broadcasts events to all connected clients, rather than just the first connected client.
		- The `tag_cardinality_limit` transform now supports customizing limits for metrics that are based on the metric name and namespace.
		"""

	known_issues: [
		"To prevent a crash at startup, avoid name clashes between enrichment table names and other components like sources, transforms, and sinks in the configuration. To resolve this, you can rename the enrichment table to a unique name that isn't already used by any other component.",
		"There are connectivity issues that might prevent `vector top` command connect to a running Vector instance.",
	]

	changelog: [
		{
			type: "feat"
			description: """
				VRL was updated to v0.22.0. This includes the following changes:

				#### Breaking Changes & Upgrade Guide

				- Removed deprecated `ellipsis` argument from the `truncate` function. Use `suffix` instead. (https://github.com/vectordotdev/vrl/pull/1188)
				- Fixed `slice` type definition. This is a breaking change because it might change the fallibility of the `slice` function. VRL scripts will
				  need to be updated accordingly. (https://github.com/vectordotdev/vrl/pull/1246)

				#### New Features

				- Added new `to_syslog_facility_code` function to convert syslog facility keyword to syslog facility code. (https://github.com/vectordotdev/vrl/pull/1221)
				- Downgraded the "can't abort infallible function" error to a warning. (https://github.com/vectordotdev/vrl/pull/1247)
				- `ip_cidr_contains` method now also accepts an array of CIDRs. (https://github.com/vectordotdev/vrl/pull/1248)
				- Faster bytes to Unicode string conversions by using SIMD instructions provided by simdutf8 crate. (https://github.com/vectordotdev/vrl/pull/1249)
				- Added `shannon_entropy` function to generate [entropy](https://en.wikipedia.org/wiki/Entropy_(information_theory)) from a string. (https://github.com/vectordotdev/vrl/pull/1267)

				#### Fixes

				- Fix decimals parsing in parse_duration function (https://github.com/vectordotdev/vrl/pull/1223)
				- Fix `parse_nginx_log` function when a format is set to error and an error message contains a comma. (https://github.com/vectordotdev/vrl/pull/1280)
				"""
		},
		{
			type: "feat"
			description: """
				Allows users to specify a KMS key and tags for newly created AWS CloudWatch log groups.
				"""
			contributors: ["johannesfloriangeiger"]
		},
		{
			type: "feat"
			description: """
				Query parameters can now contain either `single value` or an array of `multiple values`.
				For example:

				```yaml
				singe_key: single_value
				"match[]":
					- '{job="somejob"}'
					- '{__name__=~"job:.*"}'
				```
				"""
			contributors: ["sainad2222"]
		},
		{
			type: "feat"
			description: """
				Add a new type of `enrichment_table` - `memory`, which can also act as a sink, ingesting all the
				data and storing it per key, enabling it to be read from all other enrichment tables.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type:        "feat"
			description: """
				A new sink for [Keep](\(urls.keep)) was added.
				"""
			contributors: ["sainad2222"]
		},
		{
			type: "feat"
			description: """
				The `host_metrics` source has a new collector, `tcp`. The `tcp`
				collector exposes three metrics related to the TCP stack of the
				system:

				* `tcp_connections_total`: The total number of TCP connections. It
				  includes the `state` of the connection as a tag.
				* `tcp_tx_queued_bytes_total`: The sum of the number of bytes in the
				   send queue across all connections.
				* `tcp_rx_queued_bytes_total`: The sum of the number of bytes in the
				  receive queue across all connections.

				This collector is enabled only on Linux systems.
				"""
			contributors: ["aryan9600"]
		},
		{
			type: "fix"
			description: """
				The `chronicle_unstructured` sink now sets the `content-encoding` header when compression is enabled.
				"""
			contributors: ["chocpanda"]
		},
		{
			type: "feat"
			description: """
				The `tag_cardinality_limit` transform now supports customizing limits for specific metrics, matched by metric name and optionally, its namespace.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Add `websocket_server` sink that acts as a websocket server and broadcasts events to all clients.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Sources running HTTP servers (`http_server` source, `prometheus` source, `datadog_agent`, and so on) now support a new `custom` authorization strategy.
				If a strategy is not explicitly defined, it defaults to `basic`, which is the current behavior.

				You can read more in this [how it works](/docs/reference/configuration/sources/http_server/#authorization-configuration) section.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				The systemd service now validates the config with parameter `--no-environment` on service reload.
				"""
			contributors: ["rsrdesarrollo"]
		},
		{
			type: "feat"
			description: """
				The `dnstap` source now uses [v20250201](https://github.com/dnstap/dnstap.pb/releases/tag/v20250201) dnstap protobuf schema.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Allow users to specify `session_name` when using AWS authentication in a component.
				"""
			contributors: ["akutta"]
		},
		{
			type: "enhancement"
			description: """
				Add support for more Google Chronicle regional endpoints:
				- SãoPaulo
				- Canada
				- Dammam
				- Doha
				- Frankfurt
				- London
				- Mumbai
				- Paris
				- Singapore
				- Sydney
				- TelAviv
				- Tokyo
				- Turin
				- Zurich
				"""
			contributors: ["chocpanda"]
		},
		{
			type: "enhancement"
			description: """
				Add an option to Google Chronicle sink to set a fallback index if the provided template in the `log_type` field cannot be resolved
				"""
			contributors: ["ArunPiduguDD"]
		},
		{
			type: "feat"
			description: """
				Add a new virtual memory metric `process_memory_virtual_usage` to the process host metrics collector.
				This method returns the size of virtual memory (the amount of memory that the
				process can access), whether it is currently mapped in a physical RAM or not.
				"""
			contributors: ["nionata"]
		},
		{
			type: "enhancement"
			description: """
				When using the Datadog Search syntax as a condition on components that support it, the following now support matching on multiple fields (using OR):

				- `tags` will look up the fields `tags` and `ddtags`
				- `source` will look up the fields `source` and `ddsource`
				"""
			contributors: ["20agbekodo"]
		},
		{
			type: "fix"
			description: """
				`enrichment_table`s loaded from a CSV file with `include_headers: false` no longer drop the first row of data
				"""
			contributors: ["B-Schmidt"]
		},
		{
			type: "enhancement"
			description: """
				The `generate-schema` subcommand accepts an optional `output_path` option.
				"""
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: """
				Allow additional types to be used in `tests.inputs.log_fields` values, including
				nested objects and arrays.
				"""
			contributors: ["tmccombs"]
		},
		{
			type: "feat"
			description: """
				The `pulsar` source and sink now support configuration of TLS options via the `tls` configuration field.
				"""
			contributors: ["pomacanthidae"]
		},
		{
			type: "fix"
			description: """
				Downgraded some noisy `info!` statements in the `aws_s3` source to `debug!`.
				"""
			contributors: ["pront"]
		},
	]

	commits: [
		{sha: "c68bd7af375797c0e4e9f5d94dd2e0c082babe5c", date: "2025-01-13 18:38:01 UTC", description: "Add process virtual memory metric", pr_number: 22183, scopes: ["host_metrics source"], type: "feat", breaking_change: false, author: "Nicholas Ionata", files_count: 3, insertions_count: 10, deletions_count: 0},
		{sha: "7f10bf91c2c1a4cfebbccfe6e5fd10605cecac4c", date: "2025-01-14 04:38:37 UTC", description: "Bump the patches group across 1 directory with 31 updates", pr_number: 22190, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 348, deletions_count: 352},
		{sha: "26bd3d1b9fa6f3fe23b785ca2b6b205de2839e44", date: "2025-01-14 04:46:55 UTC", description: "add content encoding header when compression is enabled", pr_number: 22009, scopes: ["gcp_chronicle sink"], type: "fix", breaking_change: false, author: "Matt Searle", files_count: 2, insertions_count: 37, deletions_count: 11},
		{sha: "acec9259026d4ec35eec66cb621bb686b2c9cf5d", date: "2025-01-14 06:38:02 UTC", description: "Bump ordered-float from 4.5.0 to 4.6.0", pr_number: 22174, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 12, deletions_count: 12},
		{sha: "64735f31bfd9367af3db4cd761fb6c40e7f3b9eb", date: "2025-01-14 21:00:08 UTC", description: "fix 'parse-groks' VRL doc", pr_number: 22194, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 25, deletions_count: 0},
		{sha: "aaa90cbdf133cf9a21374ab7008036c6c664a22f", date: "2025-01-15 00:16:45 UTC", description: "cargo vdev build manifests", pr_number: 22201, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 18, insertions_count: 36, deletions_count: 22},
		{sha: "025ac6cf85e82ddfd315a6122e06bb672ce778d5", date: "2025-01-15 10:18:18 UTC", description: "documentation fixes for style consistency", pr_number: 22200, scopes: ["external"], type: "docs", breaking_change: false, author: "Sainath Singineedi", files_count: 62, insertions_count: 329, deletions_count: 329},
		{sha: "b77e6c17cdc4416fc12a2a9c8aa3ff0cfeaf97a3", date: "2025-01-15 00:27:15 UTC", description: "refactor homebrew.rs to support ARM builds", pr_number: 22156, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 79, deletions_count: 56},
		{sha: "a0b4e0ab7048cad723aeca94d7c747724adf4fe3", date: "2025-01-15 00:27:24 UTC", description: "tweak release templates", pr_number: 22202, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 0},
		{sha: "e28d6e8d6514c990009c254f3e16b358478651bb", date: "2025-01-15 03:25:55 UTC", description: "cherry picking 0.44.0 commits", pr_number: 22207, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 33, insertions_count: 405, deletions_count: 111},
		{sha: "bbaa34c6cbd06848e24399b442b451f5e95796bc", date: "2025-01-15 21:20:13 UTC", description: "update Cargo.lock", pr_number: 22214, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 3, deletions_count: 2},
		{sha: "b92e2856422347a65c5744578f8573d2b011ab80", date: "2025-01-15 22:36:42 UTC", description: "add link to macOS ARM builds", pr_number: 22144, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 12, deletions_count: 1},
		{sha: "2d7cbd88e5a6b408c540df2516d414215a354796", date: "2025-01-16 10:32:54 UTC", description: "add a new collector for tcp stats", pr_number: 22057, scopes: ["host_metrics source"], type: "feat", breaking_change: false, author: "Sanskar Jaiswal", files_count: 10, insertions_count: 506, deletions_count: 3},
		{sha: "d599ec002692e6a206750d187bfa32ea66dd29d3", date: "2025-01-16 03:10:34 UTC", description: "run integration tests in parallel", pr_number: 22205, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 98, deletions_count: 409},
		{sha: "14b630fca53ef5f7f2bab49e5a081b7d30dd61b9", date: "2025-01-16 05:19:11 UTC", description: "fix markdown lint failures", pr_number: 22219, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "e27ea3a702a5ed31119d718ca3b099186de79303", date: "2025-01-16 05:25:36 UTC", description: "replace info! with debug! to avoid spam", pr_number: 22215, scopes: ["aws_s3 source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 6, deletions_count: 3},
		{sha: "8e15aebcb1cea013a29f6e747026a58a416a69bb", date: "2025-01-16 20:07:21 UTC", description: "replace plork images", pr_number: 22217, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 51, deletions_count: 4},
		{sha: "110c883cf60d4f1ac87a5a952697fd7d966bd9d6", date: "2025-01-16 21:39:13 UTC", description: "fix OpenTelemetry Sink Quickstart to match expected log data model", pr_number: 22222, scopes: ["external"], type: "docs", breaking_change: false, author: "mdesson", files_count: 1, insertions_count: 44, deletions_count: 28},
		{sha: "ca73a067a3a16bf6aeac0e06f51322efdd126476", date: "2025-01-17 09:27:09 UTC", description: "fix protoc installation script for macOS arm64", pr_number: 22221, scopes: ["dev"], type: "chore", breaking_change: false, author: "Sanskar Jaiswal", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "720f367fbc444c86a1bfceaaa26c1ef0dda9af26", date: "2025-01-17 00:01:08 UTC", description: "allow users to specify session_name for aws auth", pr_number: 22206, scopes: ["auth"], type: "fix", breaking_change: false, author: "Andrew Kutta", files_count: 13, insertions_count: 188, deletions_count: 0},
		{sha: "aaa54ec9bb3052bd676f8baa2fb8d97324533c30", date: "2025-01-17 01:25:28 UTC", description: "add known issue for 0.44.0", pr_number: 22226, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "543e609036078a6882a1a4fe4bb11484805bf10d", date: "2025-01-17 19:41:16 UTC", description: "Adds example log payload Vector ships OTEL Collector", pr_number: 22225, scopes: ["external"], type: "docs", breaking_change: false, author: "mdesson", files_count: 1, insertions_count: 72, deletions_count: 0},
		{sha: "394c4e37f69bf5920af8f2dd5ebf9a7014eb6772", date: "2025-01-18 00:55:03 UTC", description: "integration test suite enhancements", pr_number: 22237, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 30, deletions_count: 2},
		{sha: "78cb547daf50b638dd2cae65399c25e52931aa5c", date: "2025-01-18 01:20:32 UTC", description: "bump async-nats to v0.38", pr_number: 22238, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 94, deletions_count: 37},
		{sha: "56d6a7d89a7306fe682daf90f6c35fb2b3f4d072", date: "2025-01-18 06:30:27 UTC", description: "add support for all Google SecOps regional endpoints", pr_number: 22033, scopes: ["gcp_chronicle sink"], type: "enhancement", breaking_change: false, author: "Matt Searle", files_count: 4, insertions_count: 87, deletions_count: 6},
		{sha: "681f08dc027e8b88e032cb86109bf52eff6b7ecf", date: "2025-01-18 02:20:09 UTC", description: "bandaid for flaky macOS tests", pr_number: 22239, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 9, deletions_count: 1},
		{sha: "36f88481d02333d7cdffce418f39b51d31de48eb", date: "2025-01-21 20:05:33 UTC", description: "Bump bufbuild/buf-setup-action from 1.49.0 to 1.50.0", pr_number: 22258, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7ee7bf7362d8abcc375079821a498f6075d85bef", date: "2025-01-21 20:07:59 UTC", description: "Bump docker/build-push-action from 6.11.0 to 6.12.0", pr_number: 22259, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a60b8bb9b84e7f70a2fe58c3b01d79f8542ef91b", date: "2025-01-22 01:58:01 UTC", description: "Bump data-encoding from 2.6.0 to 2.7.0", pr_number: 22250, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "504a596649b2ff3d0f92bb1c8e2fa0ac06b0af8d", date: "2025-01-22 01:58:29 UTC", description: "Bump directories from 5.0.1 to 6.0.0", pr_number: 22251, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 48, deletions_count: 6},
		{sha: "02d40b9f32f7694a6b60073f605adc81d9b9e192", date: "2025-01-22 01:59:06 UTC", description: "Bump cargo-lock from 10.0.1 to 10.1.0", pr_number: 22252, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c0830fd72f7118540d807a1bd62802e9b8c7c6b5", date: "2025-01-22 02:01:34 UTC", description: "Bump uuid from 1.11.1 to 1.12.0", pr_number: 22253, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "91beca71452c3636c7969f4281cc51cb89338b35", date: "2025-01-22 02:01:59 UTC", description: "Bump ipnet from 2.10.1 to 2.11.0", pr_number: 22254, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "2378a21892adfd259ea8746c3c3ed4da717dce79", date: "2025-01-22 02:35:25 UTC", description: "Bump the aws group with 2 updates", pr_number: 22245, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "dd63177195eb1a9bc1ff2a434f9bcb7e908d1503", date: "2025-01-22 02:38:35 UTC", description: "Bump convert_case from 0.6.0 to 0.7.1", pr_number: 22249, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 13, deletions_count: 3},
		{sha: "26421c7f328c121eed4c9ea0a809b20660d36fcb", date: "2025-01-22 02:39:51 UTC", description: "Bump notify from 7.0.0 to 8.0.0", pr_number: 22173, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 23, deletions_count: 26},
		{sha: "7eebd9471d3cc11b54717253f44ee8487a36bd1e", date: "2025-01-22 03:20:32 UTC", description: "Bump the patches group with 15 updates", pr_number: 22244, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 80, deletions_count: 92},
		{sha: "b03b3b34ce55924a3fbeb88e943da581123a072c", date: "2025-01-21 20:49:31 UTC", description: "smp version: 0.19.3 -> 0.20.1", pr_number: 22266, scopes: ["ci"], type: "chore", breaking_change: false, author: "Geoffrey Oxberry", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "604a51bf7b46e3aa27ebdd278dcbeb5022629e40", date: "2025-01-22 02:17:17 UTC", description: "Update lading to 0.25.4", pr_number: 22271, scopes: ["ci"], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b4aaaa88fe96ef23d3e8c4e40a8c2045aa9f56c5", date: "2025-01-22 19:17:50 UTC", description: "support tls options", pr_number: 22148, scopes: ["pulsar sink"], type: "feat", breaking_change: false, author: "pomacanthidae", files_count: 19, insertions_count: 469, deletions_count: 32},
		{sha: "0df7c486e7c7251da200167d55f6dbbc1910e6fb", date: "2025-01-23 06:32:19 UTC", description: "add notice on timestamp field renaming in the Elasticsearch sink, when use data_stream mode", pr_number: 22196, scopes: ["website"], type: "chore", breaking_change: false, author: "up2neck", files_count: 2, insertions_count: 6, deletions_count: 0},
		{sha: "48fd7ea164009de1f6b302c74574779b0ef85a93", date: "2025-01-23 02:35:05 UTC", description: "make doc generation platform agnostic", pr_number: 22223, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 57, deletions_count: 28},
		{sha: "21776bbd15113e34841dc6269358fc69b9166546", date: "2025-01-23 02:43:26 UTC", description: "bump bitflags", pr_number: 22283, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "9161ca75417db37fad78848d31ca2a586321c2b2", date: "2025-01-23 02:44:50 UTC", description: "fix `vdev check scripts` warnings", pr_number: 22277, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 13, deletions_count: 16},
		{sha: "082a154e69c65166ed52f1a1b44deb9b067069e3", date: "2025-01-23 03:12:03 UTC", description: "display regression report in summary", pr_number: 22284, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 16, deletions_count: 0},
		{sha: "17d14d3b150569a87f7fec9288c16601ce0064de", date: "2025-01-23 05:25:19 UTC", description: "improve proto codecs docs", pr_number: 22280, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 48, insertions_count: 338, deletions_count: 106},
		{sha: "b890bf6b7c855cd1970a22f8a28ba9f2872eaaea", date: "2025-01-24 01:40:47 UTC", description: "#22264 add `--no-environment` to systemd service file at `ExecReload`", pr_number: 22279, scopes: ["deployment"], type: "fix", breaking_change: false, author: "Raúl Sampedro", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "318930b819872ce12c23bbee8475aafda1deeedb", date: "2025-01-24 02:23:16 UTC", description: "add `memory` enrichment table", pr_number: 21348, scopes: ["enriching"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 24, insertions_count: 1429, deletions_count: 37},
		{sha: "5bf6071554ed155817b986e75b8b7c1a2040ef98", date: "2025-01-24 01:27:09 UTC", description: "Bump proptest from 1.5.0 to 1.6.0", pr_number: 22172, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 9, deletions_count: 10},
		{sha: "fd515ea86e3a3378c69953a4a59556045eb93fe4", date: "2025-01-24 12:44:51 UTC", description: "Keep sink", pr_number: 22072, scopes: ["new sink"], type: "feat", breaking_change: false, author: "Sainath Singineedi", files_count: 16, insertions_count: 814, deletions_count: 0},
		{sha: "e382afe60cc42bbb6855c28cfab59015832e230f", date: "2025-01-27 21:09:18 UTC", description: "fix summary paths", pr_number: 22305, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "cbcbeb3f36fb22efc7ff856bc967b82ebda641ef", date: "2025-01-28 03:09:34 UTC", description: "support filtering on ddtags and ddsource in datadog search syntax", pr_number: 22281, scopes: ["datadog service"], type: "feat", breaking_change: false, author: "Josué AGBEKODO", files_count: 2, insertions_count: 432, deletions_count: 44},
		{sha: "ffd359786b8b1cd6d78d59e29dce3c5c1b276aeb", date: "2025-01-28 03:52:47 UTC", description: "fix flush metrics for `memory` enrichment table", pr_number: 22296, scopes: ["enriching"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 234, deletions_count: 22},
		{sha: "6fa2099880417573436e72f8626855266d79d9e3", date: "2025-01-28 04:16:51 UTC", description: "Bump docker/build-push-action from 6.12.0 to 6.13.0", pr_number: 22306, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "559cb4667f43cca891189b7179e0f37c778d5c49", date: "2025-01-28 00:10:57 UTC", description: "Bump vrl from `c0245e1` to `2ccb98e`", pr_number: 22303, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "47e348cc8cab3bcbd8d6cf6937f9d0e2e329e080", date: "2025-01-29 03:03:20 UTC", description: "enable per metric limits for `tag_cardinality_limit`", pr_number: 22077, scopes: ["tag_cardinality_limit transform"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 5, insertions_count: 410, deletions_count: 34},
		{sha: "1af287f1f05050e0382d3dac6efb888508dce390", date: "2025-01-29 01:46:45 UTC", description: "populated the new github 'type' field for features", pr_number: 22315, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "9f9a573fca334c42f92bca7752ca00c458c3e106", date: "2025-01-29 19:47:10 UTC", description: "update smp to 0.20.2", pr_number: 22318, scopes: ["ci"], type: "chore", breaking_change: false, author: "Geoffrey Oxberry", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "d49cf33748ad82292198888687b6b50a01d8500f", date: "2025-01-30 00:45:44 UTC", description: "Swap traditional file generator for logrotate_fs for file_to_blackhole test", pr_number: 22285, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 10, deletions_count: 7},
		{sha: "79852bbd794338c1ceda3f2c22fcb9eb175f930e", date: "2025-01-30 19:57:25 UTC", description: "make some final regression steps optional", pr_number: 22324, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "0431a079273432e69cb5a1fe9add9c85ca4e1edc", date: "2025-01-31 13:23:43 UTC", description: "fix `enrichment_table` and `secret` docs", pr_number: 22319, scopes: ["external"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 101, deletions_count: 128},
		{sha: "acd7dd25d1192b344285c73a1541b42909d1af7d", date: "2025-01-31 01:42:49 UTC", description: "fix getting starting guide rendering", pr_number: 22332, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 7, deletions_count: 0},
		{sha: "0d57e2aed0b300a8fbe9626283582578484c1620", date: "2025-01-31 19:51:51 UTC", description: "Add default fallback logic if log_type template cannot be resolved fo…", pr_number: 22323, scopes: ["gcp_chronicle sink"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 4, insertions_count: 47, deletions_count: 11},
		{sha: "f09af60af4d76516a065ef9483c8cc525450d028", date: "2025-02-01 00:25:02 UTC", description: "add config IDE autocompletion guide ", pr_number: 22329, scopes: ["dev"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 110, deletions_count: 12},
		{sha: "d1e3dda4c4d14f8a4f2eca7f965182d9fcf6e5ff", date: "2025-02-01 07:06:50 UTC", description: "fix wording in common sinks components docs", pr_number: 22317, scopes: ["external"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 91, insertions_count: 918, deletions_count: 918},
		{sha: "59ff175a514be619437113fc4ef3e80304415d33", date: "2025-02-04 02:10:28 UTC", description: "update dnstap protobuf schema", pr_number: 22348, scopes: ["dnstap source"], type: "chore", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 258, deletions_count: 242},
		{sha: "3f3428e6dac1830364812c64aa9834de1cc30d1e", date: "2025-02-03 20:59:25 UTC", description: "fix report path", pr_number: 22351, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 3},
		{sha: "3dbab771c26aa2459577a0f70a5a7e67fe936963", date: "2025-02-04 00:26:54 UTC", description: "Bump aws-actions/configure-aws-credentials from 4.0.2 to 4.0.3", pr_number: 22354, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "f66fe74055eace62cba3ce3618dce804d7552861", date: "2025-02-04 00:47:10 UTC", description: "make Integration Test Suite runnable on Actions", pr_number: 22356, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "ff77761e2c305f0b0904295654d30f73a039a75c", date: "2025-02-04 02:10:29 UTC", description: "update openssl related crates", pr_number: 22352, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 13, deletions_count: 12},
		{sha: "1346acd3b704c9c84f1445c7396bce023c22fd9c", date: "2025-02-05 20:50:25 UTC", description: "revert to a previous async-nats version ", pr_number: 22359, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 29, deletions_count: 86},
		{sha: "1c1ded8bf3c564331fd4067878c95386a189bf4c", date: "2025-02-05 22:12:33 UTC", description: "always run IT suite when in a merge group", pr_number: 22368, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "4858750a9282b0089a068aea0ac2c6d4c03aeeca", date: "2025-02-06 08:14:43 UTC", description: "Bump the patches group across 1 directory with 19 updates", pr_number: 22370, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 237, deletions_count: 152},
		{sha: "84bc0e4523137692234abe1cf6812ff46382058c", date: "2025-02-07 01:09:14 UTC", description: "check for merge_group in check-all job", pr_number: 22372, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "cf40d77f79d1ba771b2812822ab009cb7f0516e2", date: "2025-02-07 20:16:32 UTC", description: "temporarily ignore failing tests", pr_number: 22378, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 12, deletions_count: 10},
		{sha: "22ee402ad82118634cb46e84aca8cef3755fb87d", date: "2025-02-07 23:44:29 UTC", description: "refactor and parallelize integration-tests (CI review)", pr_number: 22380, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 14, deletions_count: 360},
		{sha: "9bb3712cdf88c07cfeffed07d7640297464c8ed3", date: "2025-02-08 11:44:29 UTC", description: "unifying http query parameters", pr_number: 22242, scopes: ["http source"], type: "enhancement", breaking_change: false, author: "Sainath Singineedi", files_count: 14, insertions_count: 171, deletions_count: 27},
		{sha: "e84e0bf2abc2cc55479ae39dc176f46dc68b929d", date: "2025-02-08 07:57:20 UTC", description: "Add http sink tls cert/key to `config::watcher`", pr_number: 22386, scopes: ["cli"], type: "feat", breaking_change: false, author: "Guillaume Le Blanc", files_count: 4, insertions_count: 47, deletions_count: 14},
		{sha: "8ee3b2bfe1d8790cc0c7ba4d1d2db1bd9851fea0", date: "2025-02-08 03:07:18 UTC", description: "use safe semver version", pr_number: 22381, scopes: ["website"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 978, deletions_count: 811},
		{sha: "f71ba42bf384371ea97ea74bf71f139da12e0335", date: "2025-02-08 22:57:51 UTC", description: "fix CVE-2024-47614 by bumping async-graphql", pr_number: 22371, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 31, deletions_count: 32},
		{sha: "9593581f775d7132bcf697605e738a90c9d046aa", date: "2025-02-09 12:32:41 UTC", description: "generate global option configuration automatically from Rust code", pr_number: 22345, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Huang Chen-Yi", files_count: 13, insertions_count: 703, deletions_count: 451},
		{sha: "b7ae78856ed25bc46cd36e7fa5991363d4528c3b", date: "2025-02-10 19:50:16 UTC", description: "Bump heim from `4925b53` to `f3537d9`", pr_number: 22399, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "3a82b88280eddde6f7193f38bc65dbd0fc298629", date: "2025-02-10 20:00:58 UTC", description: "Bump vrl from `2ccb98e` to `7612a8b`", pr_number: 22401, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 36, deletions_count: 72},
		{sha: "950f15ea54a5f65f8446143871f206237f175bf7", date: "2025-02-11 01:01:23 UTC", description: "Bump lru from 0.12.5 to 0.13.0", pr_number: 22397, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 9, deletions_count: 3},
		{sha: "e36616f14d7951c908d6e6a3741324de188d9509", date: "2025-02-11 01:01:55 UTC", description: "Bump bytes from 1.9.0 to 1.10.0", pr_number: 22404, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 96, deletions_count: 96},
		{sha: "d2f216efc3ec9def0b3ad3b43ff1527ee0dd7f80", date: "2025-02-11 01:02:21 UTC", description: "Bump tempfile from 3.15.0 to 3.16.0", pr_number: 22400, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "da6886b5f3578648c2621a1660e0b3b9c51437ca", date: "2025-02-11 02:33:11 UTC", description: "csv file enrichment tables no longer drop the first row", pr_number: 22257, scopes: ["enriching"], type: "fix", breaking_change: false, author: "B-Schmidt", files_count: 2, insertions_count: 67, deletions_count: 4},
		{sha: "9d67c79724b65c2d31e136c3100d5b950eb3db69", date: "2025-02-11 01:44:59 UTC", description: "Bump the patches group with 3 updates", pr_number: 22393, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "f7842e6faef7112dc6c7c7d1e6776d728da70241", date: "2025-02-10 22:58:13 UTC", description: "Bump docker/setup-qemu-action from 3.3.0 to 3.4.0", pr_number: 22412, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "c23095b17855ab36a3602062a97193d5d38be7c7", date: "2025-02-11 03:58:29 UTC", description: "Bump docker/setup-buildx-action from 3.8.0 to 3.9.0", pr_number: 22411, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "b0506a336c345638f1ee307505bebac1cdf7a6d9", date: "2025-02-11 03:58:38 UTC", description: "Bump data-encoding from 2.7.0 to 2.8.0", pr_number: 22402, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "896be62f6d912f993ec011dc9b392e7cc1faf0ec", date: "2025-02-11 04:47:13 UTC", description: "Bump hickory-proto from 0.24.2 to 0.24.3 in the cargo group", pr_number: 22415, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "1d65736e4e39467ec5f320f43b0afc95cd6ad70c", date: "2025-02-11 00:45:22 UTC", description: "only run integration tests when secrets are available", pr_number: 22414, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 2},
		{sha: "530c4d183658d64deb1ad91f51b9fb96c41aae27", date: "2025-02-12 19:18:38 UTC", description: "Make fields in HumioLogsConfig public", pr_number: 22421, scopes: ["internal"], type: "feat", breaking_change: false, author: "ArunPiduguDD", files_count: 1, insertions_count: 14, deletions_count: 14},
		{sha: "64c56ed302502bae1e0f5fa29608a3616d7e90e5", date: "2025-02-13 02:25:19 UTC", description: "add custom auth strategy for components with HTTP server", pr_number: 22236, scopes: ["sources"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 28, insertions_count: 911, deletions_count: 182},
		{sha: "c23119676042a7964bf19a0b8ac2e4ade5624a36", date: "2025-02-13 02:27:52 UTC", description: "use `as_str()` as implemented by `Value` ", pr_number: 22416, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jakub Onderka", files_count: 6, insertions_count: 14, deletions_count: 26},
		{sha: "c9076aa240e46b7299f4caa9c0ca89de7704ae08", date: "2025-02-13 10:04:23 UTC", description: "generate global option and common field from Rust macro", pr_number: 22408, scopes: ["external"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 10, insertions_count: 276, deletions_count: 336},
		{sha: "39973ab03211c234e56983914a816787ae1c520c", date: "2025-02-14 01:12:20 UTC", description: "Revert add http sink tls cert/key to `config::watcher`", pr_number: 22434, scopes: ["cli"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 14, deletions_count: 47},
		{sha: "e878c892c14607a651732a93d428ef029e072713", date: "2025-02-14 03:10:59 UTC", description: "add note for input/output typs in remap docs", pr_number: 22435, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 9, deletions_count: 2},
		{sha: "1200b739833d058cfc95d63a335069bc076472e9", date: "2025-02-14 23:45:16 UTC", description: "add zstd decompression to DD metrics E2E tests", pr_number: 22441, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 59, deletions_count: 20},
		{sha: "490526d2a69f63f91a4d0abdb8d7ef02c4899e66", date: "2025-02-15 01:11:50 UTC", description: "E2E comment triggers", pr_number: 22443, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "2b63353b68403b188c7126657336675fb24eb39e", date: "2025-02-15 14:15:08 UTC", description: "Generate API docs from Rust code", pr_number: 22437, scopes: ["external"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 11, insertions_count: 111, deletions_count: 50},
		{sha: "4c3f3ca68f586dc8c62a3603ea77a0583ba6e9e0", date: "2025-02-15 00:57:30 UTC", description: "Allow objects and arrays in log_fields test input", pr_number: 22406, scopes: ["unit tests"], type: "enhancement", breaking_change: false, author: "Thayne McCombs", files_count: 5, insertions_count: 53, deletions_count: 33},
		{sha: "db44877a6a48561700c6e44256dfac1e66452c7b", date: "2025-02-18 20:29:57 UTC", description: "Bump tempfile from 3.16.0 to 3.17.0", pr_number: 22450, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "b46dbece64d53f3e8b6f838e13a75d512be99505", date: "2025-02-18 20:30:13 UTC", description: "Bump smallvec from 1.13.2 to 1.14.0", pr_number: 22452, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "a96a97009f36387d11031e0553e1a41e5d184689", date: "2025-02-19 01:30:54 UTC", description: "Bump vrl from `7612a8b` to `acfd9f9`", pr_number: 22453, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 9, deletions_count: 7},
		{sha: "0e214505f9f532078ea9fc6d45af78a94c24a3e7", date: "2025-02-19 05:02:29 UTC", description: "Enable simdutf8 feature for maxminddb", pr_number: 22456, scopes: ["performance"], type: "chore", breaking_change: false, author: "Jakub Onderka", files_count: 2, insertions_count: 6, deletions_count: 8},
		{sha: "450de36904f3d1524057e8cdb736941194da8d22", date: "2025-02-19 02:47:16 UTC", description: "Bump aws-actions/configure-aws-credentials from 4.0.3 to 4.1.0", pr_number: 22462, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "bd06afcc6d2df680b3ef42a6873dfba5ab0b27be", date: "2025-02-20 01:46:00 UTC", description: "add docs for `shannon_entropy` function", pr_number: 22428, scopes: ["external"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 3, insertions_count: 65, deletions_count: 0},
		{sha: "78adec73b3ddc6fe403b59ea1a81780649cd059b", date: "2025-02-20 02:55:11 UTC", description: "initial `websocket_server` sink", pr_number: 22213, scopes: ["new sink"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 13, insertions_count: 1227, deletions_count: 2},
		{sha: "970606eb6f14c393b7d7e9cb9dd271a8a202d401", date: "2025-02-20 08:19:24 UTC", description: "Add parse_cbor vrl function documentation", pr_number: 22082, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Semyon Uchvatov", files_count: 3, insertions_count: 38, deletions_count: 1},
		{sha: "63cadc68a606d0f0033a17b9965d133cd6c293a6", date: "2025-02-20 05:53:27 UTC", description: "Update documentation for ip_cidr_contains", pr_number: 22463, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jakub Onderka", files_count: 2, insertions_count: 10, deletions_count: 4},
		{sha: "30f7a12c736f93a74990355de0708d56cb81e895", date: "2025-02-21 02:37:31 UTC", description: "add `how_it_works` section for `websocket_server`", pr_number: 22482, scopes: ["websocket_server sink"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 35, deletions_count: 0},
		{sha: "aa43a1674cfbeb2e94ac31196a1dade34dc24b75", date: "2025-02-21 02:41:32 UTC", description: "add `how_it_works` section for auth config", pr_number: 22483, scopes: ["external"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 6, insertions_count: 48, deletions_count: 0},
		{sha: "ae84bcd9de4b9a47f4b760c6451d1aea11baa438", date: "2025-02-21 04:06:14 UTC", description: "fix missing configuration section for `websocket_server` docs", pr_number: 22480, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 2, deletions_count: 0},
	]
}
