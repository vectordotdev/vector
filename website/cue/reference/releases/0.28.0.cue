package metadata

releases: "0.28.0": {
	date: "2023-02-23"

	known_issues: [
		"""
			AWS components, except the `aws_s3` sink, are not functional due to issues with request
			signing. This is fixed in v0.28.1.
			""",
		"""
			The `framing.*.max_length` configuration options cannot be used on the `socket` source
			as Vector returns an error about them conflicting with the deprecated top-level
			`max_length` configuration option. This is fixed in v0.28.1.
			""",
		"""
			The `http_server` source incorrectly defaults to `GET` rather than `POST` for `method`.
			Fixed in 0.28.2.
			""",
		"""
			The `elasticsearch` sink panics when starting if `bulk.index` is unspecified and the
			default `mode` of `bulk` is used. Fixed in 0.28.2.
			""",
		"""
			The `syslog` source incorrectly inserted the source IP of incoming messages as
			`source_id` rather than `source_ip`. Fixed in 0.28.2.
			""",
	]

	description: """
		The Vector team is pleased to announce version 0.28.0!

		This is a smaller maintenance release primarily including bug fixes and small enhancements,
		while we do some background work to enable upcoming new features.

		With this release we also completed an initiative to generate Vector's [reference
		documentation](/docs/reference/) from the configuration structures in the code which
		will result in less inaccuracies in published documentation.

		Be sure to check out the [upgrade guide](/highlights/2023-02-28-0-28-0-upgrade-guide) for
		breaking changes in this release.

		"""

	changelog: [
		{
			type: "fix"
			scopes: ["reload", "config"]
			description: """
				Vector reload better detects which components have changed, no longer reloading
				components that have configuration options that are maps where none of the
				keys or values changed.
				"""
			contributors: ["aholmberg"]
			pr_numbers: [15868]
		},
		{
			type: "enhancement"
			scopes: ["azure_blob sink"]
			description: """
				The `azure_blob` sink now allows setting a custom `endpoint` for use with
				alternative Azure clouds like USGov and China.
				"""
			contributors: ["archoversight"]
			pr_numbers: [15336]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Loading of secrets in configuration now allows the same secret to be used multiple
				times. Previously this would result in Vector raising an error that the secret was
				undefined on the second or later uses.
				"""
			pr_numbers: [15815]
		},
		{
			type: "enhancement"
			scopes: ["clickhouse sink"]
			description: """
				The `clickhouse` sink now supports a `date_time_best_effort` config option to
				have ClickHouse parse a greater variety of timestamps (like RFC3339).
				"""
			contributors: ["DarkWanderer"]
			pr_numbers: [15787]
		},
		{
			type: "enhancement"
			scopes: ["http sink"]
			description: """
				The `http` sink now supports `payload_prefix` and `payload_suffix` options to
				prepend and append text in the HTTP bodies it is sending. This happens after batches
				are encoded and so can be used, for example, to wrap the batches in a JSON
				envelope.
				"""
			contributors: ["jdiebold"]
			pr_numbers: [15696]
		},
		{
			type: "chore"
			scopes: ["buffers"]
			description: """
				The deprecated `disk_v1` buffer type was removed. See [the upgrade
				guide](#disk_v1-removal) for details and how to migrate if you are using this buffer
				type.
				"""
			breaking: true
			pr_numbers: [15928]
		},
		{
			type: "enhancement"
			scopes: ["aws_kinesis_firehose source"]
			description: """
				The `aws_kinesis_firehose` now has a `store_access_key`, similar to the `splunk_hec`
				and `datadog_agent` sources, to store the token that the incoming request was sent
				with in the event secrets. This can be read later in VRL to drive behavior.
				"""
			contributors: ["dizlv"]
			pr_numbers: [15904]
		},
		{
			type: "enhancement"
			scopes: ["reduce transform"]
			description: """
				The `reduce` transform now has a `max_events` option that can be used to limit the
				total number of events in a reduced batch.
				"""
			contributors: ["jches"]
			pr_numbers: [14817]
		},
		{
			type: "fix"
			scopes: ["file source"]
			description: """
				Changing `max_line_bytes` on the `file` source no longer typically invalidates all
				previous checksums. It still will invalidate the checksum if the value is set to
				lower than the length of the line used that checksum but this should be much less
				common.
				"""
			contributors: ["Ilmarii"]
			pr_numbers: [15899]
		},
		{
			type: "feat"
			scopes: ["vrl: stdlib"]
			description: """
				`encode_gzip` and `decode_gzip` functions were added to VRL to interact with gzip'd
				data.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16039]
		},
		{
			type: "fix"
			scopes: ["gcp provider"]
			description: """
				Vector GCP sinks now correctly refresh authentication tokens when healthchecks
				are disabled.
				"""
			contributors: ["punkerpunker"]
			pr_numbers: [15827]
		},
		{
			type: "enhancement"
			scopes: ["kafka source"]
			description: """
				The `kafka` source now tries to commit offsets during shutdown to avoid duplicate
				processing on start-up.
				"""
			contributors: ["aholmberg"]
			pr_numbers: [15870]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				Vector no longer panics when attempting to create more than 254 allocation groups
				for memory allocation tracking. This would happen when there were more than 254
				components in a config or during unit tests where a lot of components are spun up
				independently.
				"""
			pr_numbers: [16201]
		},
		{
			type: "feat"
			scopes: ["vrl: stdlib"]
			description: """
				`encode_zlib` and `decode_zlib` functions were added to VRL to interact with zlib
				compressed data.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16059]
		},
		{
			type: "chore"
			scopes: ["journald source"]
			description: """
				The deprecated `units` of the `journald` source was removed. `include_units` should
				be used instead. See [the upgrade
				guide](/highlights/2023-02-28-0-28-0-upgrade-guide#journald-units) for
				more details.
				"""
			breaking: true
			pr_numbers: [16194]
		},
		{
			type: "enhancement"
			scopes: ["reduce transform"]
			description: """
				The `reduce` transform performance improved by only flushing when events were ready
				to be flushed and avoiding repeated checks for stale events.
				"""
			contributors: ["dbcfd"]
			pr_numbers: [9502]
		},
		{
			type: "enhancement"
			scopes: ["vrl: stdlib"]
			description: """
				A `seahash` function was added to VRL for a non-cryptographic fast hash.
				"""
			contributors: ["psemeniuk"]
			pr_numbers: [16073]
		},
		{
			type: "enhancement"
			scopes: ["pulsar sink"]
			description: """
				The `pulsar` sink now supports batching via the added `batch.max_events`
				configuration option.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16063]
		},
		{
			type: "chore"
			scopes: ["aws provider"]
			description: """
				Vector's AWS components now use OpenSSL for the TLS implementation rather than
				`rustls`. This shouldn't have any observable effects to end-users but allows for the
				use of these components on FIPS-compliant systems. See [the upgrade
				guide](/highlights/2023-02-28-0-28-0-upgrade-guide#aws-openssl) for more
				details. [Let us know](https://vector.dev/community/) if you see any issue
				related to this change.
				"""
			breaking: true
			pr_numbers: [16335]
		},
		{
			type: "chore"
			scopes: ["vrl: stdlib"]
			description: """
				The deprecated VRL functions for accessing metadata were removed:

				- `get_metadata_field`
				- `set_metadata_field`
				- `delete_metadata_field`

				In favor of the new metadata syntax of `%<field name>` to interact with these
				fields. See [the upgrade
				guide](/highlights/2023-02-28-0-28-0-upgrade-guide#metadata-functions-removal)
				for more details.
				"""
			breaking: true
			pr_numbers: [14821]
		},
		{
			type: "docs"
			scopes: ["aws_s3 source"]
			description: """
				The `strategy` field on the `aws_s3` source has been hidden from the docs given it
				only has one value, that is also the default: `sqs`. See [the upgrade
				guide](/highlights/2023-02-28-0-28-0-upgrade-guide#aws_s3-strategy).
				"""
			pr_numbers: [16417]
		},
		{
			type: "fix"
			scopes: ["vrl: compiler"]
			description: """
				VRL now correctly calculates the type definitions of boolean expressions that
				short-circuit. This should avoid panics and the inability to write what should be
				valid VRL programs in rare situations.
				"""
			pr_numbers: [16391]
		},
		{
			type: "fix"
			scopes: ["gcp_stackdriver_metrics sink"]
			description: """
				The `gcp_stackdriver_metrics` now correctly encodes metrics to send to GCP.
				"""
			contributors: ["jasonahills"]
			pr_numbers: [16394]
		},
		{
			type: "feat"
			scopes: ["vrl: stdlib"]
			description: """
				`encode_zstd` and `decode_zstd` functions were added to VRL to interact with zstd
				compressed data.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16060]
		},
		{
			type: "chore"
			scopes: ["apex sink"]
			description: """
				The `apex` sink was dropped given that the service no longer exists. See [the upgrade
				guide](/highlights/2023-02-28-0-28-0-upgrade-guide#apex-removal) for details.
				"""
			breaking: true
			pr_numbers: [16533]
		},
		{
			type: "fix"
			scopes: ["axiom sink"]
			description: """
				The `axiom` sink now always sets the `timestamp-field` header, which tells Axiom
				where to find the timestamp, to `@timestamp` rather than the configured
				`log_schema.timestamp_key` since Vector was always sending it as `@timestamp`.[the
				upgrade guide](/highlights/2023-02-28-0-28-0-upgrade-guide#axiom-header) for
				details.
				"""
			pr_numbers: [16536]
		},
		{
			type: "enhancement"
			scopes: ["kafka source"]
			description: """
				The `kafka` source is now capable of making consumer lag metrics available via the
				`internal_metrics` source. This can be enabled by setting `metrics.topic_lag_metric`
				to `true`. Note that this can result in high cardinality metrics given they are
				tagged with the topic and partition id.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [15106]
		},
		{
			type: "enhancement"
			scopes: ["redis source"]
			description: """
				The `redis` source now retries failed requests with an exponential back-off rather
				than immediately (which can cause high resource usage).
				"""
			contributors: ["hargut"]
			pr_numbers: [16518]
		},
	]

	commits: [
		{sha: "36940105f525ceade9276ecd538ec3b6fe044496", date: "2023-01-09 21:02:11 UTC", description: "Add dedicated helper tool", pr_number: 15833, scopes: ["dev"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 33, insertions_count: 2578, deletions_count: 0},
		{sha: "6bbce0c6a6ca3d29eb9777ebb3bc814721adc801", date: "2023-01-09 22:07:26 UTC", description: "compare serialized JSON Values instead of string", pr_number: 15868, scopes: ["config"], type: "fix", breaking_change: false, author: "Adam Holmberg", files_count: 1, insertions_count: 7, deletions_count: 4},
		{sha: "cee6300adcac09a0b58f243e851b03d55a004b21", date: "2023-01-09 22:39:33 UTC", description: "Set up vdev to allow `cargo vdev`", pr_number: 15873, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 8, insertions_count: 177, deletions_count: 905},
		{sha: "9e797a7768307e2a70f6cee574d2bbfe48fec3cb", date: "2023-01-10 05:48:35 UTC", description: "bump git from 1.11.0 to 1.13.0 in /scripts", pr_number: 15874, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 3},
		{sha: "f2874d076a7cdddf9a7dda38c46d9f3943b1433e", date: "2023-01-10 13:55:47 UTC", description: "bump assert_cmd from 2.0.7 to 2.0.8", pr_number: 15881, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "23c5e075eaccbb76a1a93af7e10bd0ea614b9e0f", date: "2023-01-10 19:11:24 UTC", description: "bump regex from 1.7.0 to 1.7.1", pr_number: 15883, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "c41baa0a06489535cadf6894a45782d04b89dd48", date: "2023-01-10 19:16:19 UTC", description: "bump schannel from 0.1.20 to 0.1.21", pr_number: 15879, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 5},
		{sha: "6d5b47c8d0d9cafc2420878b1bf117d50860ac2e", date: "2023-01-10 19:23:50 UTC", description: "bump enum_dispatch from 0.3.9 to 0.3.10", pr_number: 15882, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fac37dd8006725aefaec4635054d0a1d2dba952f", date: "2023-01-10 19:34:13 UTC", description: "bump serde_with from 2.1.0 to 2.2.0", pr_number: 15877, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 12},
		{sha: "e3d8c0181c247a7524e350dac5ec8f69e679a956", date: "2023-01-10 19:45:18 UTC", description: "bump cached from 0.40.0 to 0.42.0", pr_number: 15880, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 7},
		{sha: "cbe52f0c5bd7ed4161f7b8fa36ac88ec27b2bffa", date: "2023-01-10 17:08:21 UTC", description: "bump tokio from 1.24.0 to 1.24.1", pr_number: 15860, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "d1f9d1779f2e0acd125bf9cba165b4721461b7b6", date: "2023-01-10 14:54:22 UTC", description: "Auto-generate docs", pr_number: 15875, scopes: ["postgresql_metrics source"], type: "docs", breaking_change: false, author: "David Huie", files_count: 4, insertions_count: 49, deletions_count: 102},
		{sha: "c2dad337aa98372fe7042168f118b8db6dafac1f", date: "2023-01-10 16:11:47 UTC", description: "Allow secrets to be reused multiple times", pr_number: 15815, scopes: ["config"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 44, deletions_count: 22},
		{sha: "a708ba48765b31d939e77b329750b1574a246a84", date: "2023-01-10 20:50:07 UTC", description: "bump base64 from 0.20.0 to 0.21.0", pr_number: 15863, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 19, insertions_count: 70, deletions_count: 52},
		{sha: "1727e729487c4075c29d1ca30cda5053def52085", date: "2023-01-10 22:32:36 UTC", description: "Convert the `run-vector` script to a vdev subcommand", pr_number: 15876, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 235, deletions_count: 101},
		{sha: "c434b8c8d4799334b87beb26bf93fd4781c99da3", date: "2023-01-11 16:25:57 UTC", description: "bumps rum and logs version, adds IA config", pr_number: 15903, scopes: [], type: "chore", breaking_change: false, author: "Brian Deutsch", files_count: 3, insertions_count: 27, deletions_count: 30},
		{sha: "750d3fb3c0e15bb8cf5fb106fad6d94d4290ff83", date: "2023-01-11 16:31:48 UTC", description: "bump prost from 0.11.5 to 0.11.6", pr_number: 15896, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 9, deletions_count: 9},
		{sha: "71c1a455cc83740b0b61b6382dc70227d46ae956", date: "2023-01-11 14:34:43 UTC", description: "use auto-generated config docs", pr_number: 15887, scopes: ["vector source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 4, deletions_count: 26},
		{sha: "33e566a9b1d6a619684e37822e375410354f74d0", date: "2023-01-11 17:05:11 UTC", description: "bump roxmltree from 0.15.1 to 0.17.0", pr_number: 15861, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 4},
		{sha: "135fe237b8564570d9198694c0a2bfb307081a6d", date: "2023-01-11 22:10:49 UTC", description: "use generated docs for config ", pr_number: 15829, scopes: ["demo_logs source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 7, insertions_count: 78, deletions_count: 95},
		{sha: "9db6e4025f5cb6e419c9c2e298901f074694e0bb", date: "2023-01-11 18:33:05 UTC", description: "properly combine field metadata with type metadata for optional fields", pr_number: 15906, scopes: ["config"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 6, insertions_count: 6, deletions_count: 80},
		{sha: "d04c4f5bdef2e53e4b1f6e179efa460ac02962e0", date: "2023-01-11 19:08:48 UTC", description: "bump docker/metadata-action from 4.1.1 to 4.2.0", pr_number: 15907, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4b2b7130e71d82fbbebce91cfab4f0c5afbfa2fe", date: "2023-01-11 17:50:59 UTC", description: "add note about cue versioning", pr_number: 15910, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 8, deletions_count: 0},
		{sha: "19e910215dbaebf002c7291c4622753e0c25781e", date: "2023-01-11 17:55:25 UTC", description: "Update Rust to 1.66.1", pr_number: 15893, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a0951dfab78c14296eafce0ae0355ad49c4e9f21", date: "2023-01-11 18:49:50 UTC", description: "Auto-gen nginx_metrics docs", pr_number: 15889, scopes: ["nginx_metrics source"], type: "docs", breaking_change: false, author: "David Huie", files_count: 4, insertions_count: 22, deletions_count: 49},
		{sha: "1da9769148fbe1ce08ce00bfe84c1af014b95851", date: "2023-01-12 04:38:58 UTC", description: "Add configuration options `payload_prefix` and `payload_suffix`.", pr_number: 15696, scopes: ["http sink"], type: "feat", breaking_change: false, author: "Jakob Diebold", files_count: 3, insertions_count: 202, deletions_count: 2},
		{sha: "603ff90e6ce3a3ec490e3da11f2b701e6bd942ea", date: "2023-01-11 21:19:23 UTC", description: "use auto-generated config docs", pr_number: 15895, scopes: ["internal_logs source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 42, deletions_count: 62},
		{sha: "411969c4dc1edce3d3a0841167f4bec9e724c9c6", date: "2023-01-11 21:07:51 UTC", description: "Revert update Rust to 1.66.1", pr_number: 15913, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4af6d78b88661d874547f336a740b845aacd9334", date: "2023-01-11 23:47:05 UTC", description: "Auto-gen docs", pr_number: 15890, scopes: ["eventstoredb_metrics source"], type: "docs", breaking_change: false, author: "David Huie", files_count: 4, insertions_count: 24, deletions_count: 42},
		{sha: "fb755521dc2da3ee577034e2f6f0049bec615600", date: "2023-01-12 13:46:27 UTC", description: "Update Rust to 1.66.1", pr_number: 15926, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6131a6f0930bc62e02d56be6ffa899990b235ece", date: "2023-01-12 16:50:30 UTC", description: "bump pest from 2.5.2 to 2.5.3", pr_number: 15921, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "410314d714fd779bb0d53d9a15631ba10c742a3b", date: "2023-01-12 16:51:32 UTC", description: "bump hashbrown from 0.13.1 to 0.13.2", pr_number: 15916, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "fd2955c3a1b7fbff8db2492ba0bd5db7b2b90f3d", date: "2023-01-12 16:53:11 UTC", description: "bump zstd from 0.12.1+zstd.1.5.2 to 0.12.2+zstd.1.5.2", pr_number: 15917, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "69ede64bf2654256d708f3e6edf23e2b39e134a2", date: "2023-01-12 16:53:39 UTC", description: "bump prost-types from 0.11.5 to 0.11.6", pr_number: 15915, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "3a4e71b2a05a49014d2bd2197f9aed653b9508b0", date: "2023-01-12 18:48:23 UTC", description: "Fix scrape duration for nginx_metrics integration test", pr_number: 15929, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "26e7fe1ddb3358d9ebeb1bcc4c18e79a060cd8ab", date: "2023-01-12 19:40:31 UTC", description: "remove LevelDB-based disk_v1 buffer impl", pr_number: 15928, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 33, insertions_count: 41, deletions_count: 2523},
		{sha: "f90ca9e3e82b7577608b0c82a5feffb3f96d5f4c", date: "2023-01-12 18:52:48 UTC", description: "make consumer group instance id configurable", pr_number: 15869, scopes: ["kafka"], type: "feat", breaking_change: false, author: "Adam Holmberg", files_count: 2, insertions_count: 12, deletions_count: 0},
		{sha: "a1b87b51c68cbff5e5c12672753e1780a5e50f6a", date: "2023-01-12 22:28:07 UTC", description: "apply object examples to the correct part of the docs output", pr_number: 15932, scopes: ["docs"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 3, insertions_count: 37, deletions_count: 40},
		{sha: "10547f97c05164670f6ae588436d0031cd06f20f", date: "2023-01-12 20:23:40 UTC", description: "autogen cue docs", pr_number: 15914, scopes: ["mongodb_metrics source"], type: "docs", breaking_change: false, author: "David Huie", files_count: 4, insertions_count: 17, deletions_count: 37},
		{sha: "ce003848558b7ee602336c7f2f0feda9e675e0ab", date: "2023-01-12 20:52:43 UTC", description: "Update manifests from Helm charts", pr_number: 15937, scopes: ["kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 5, deletions_count: 5},
		{sha: "1e3ba42ff6b58e540786f608af1424553e8d723e", date: "2023-01-13 14:41:50 UTC", description: "bump prost-build from 0.11.5 to 0.11.6", pr_number: 15943, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "d3614176b7b388ba6aec0e9facfb2e9d8ac34bc7", date: "2023-01-13 14:42:44 UTC", description: "bump enum_dispatch from 0.3.10 to 0.3.11", pr_number: 15941, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "593edba1a25ebd8e9d6189848896bc1d6868f519", date: "2023-01-13 14:43:19 UTC", description: "bump graphql_client from 0.11.0 to 0.12.0", pr_number: 15946, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "99395355b0b1ce07e9704ba77d3b00703bc174db", date: "2023-01-13 14:46:17 UTC", description: "bump pest_derive from 2.5.2 to 2.5.3", pr_number: 15944, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "0aee8e5dafe36e4f5723b20d5f012b614ab7511c", date: "2023-01-13 15:41:21 UTC", description: "throw an error when a wildcard map field is missing its description", pr_number: 15934, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 34, insertions_count: 79, deletions_count: 43},
		{sha: "0eba53dfe7553e85e5a19fe6a20629ae167136ca", date: "2023-01-13 23:31:48 UTC", description: "use generated docs for config ", pr_number: 15924, scopes: ["gcp_pubsub source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 5, insertions_count: 91, deletions_count: 174},
		{sha: "81072b74bfe8e12ed8585d3770c2d440f6d1d68e", date: "2023-01-13 23:49:31 UTC", description: "bump docker/metadata-action from 4.2.0 to 4.3.0", pr_number: 15953, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6d05e13337bf8daacf47f0e289a521408fe079cc", date: "2023-01-13 19:38:31 UTC", description: "fully implement HTTP external resource for component validation", pr_number: 15804, scopes: ["observability"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 16, insertions_count: 608, deletions_count: 71},
		{sha: "39c7f015d7715d246e42b15eb66e6041999e944f", date: "2023-01-13 19:41:21 UTC", description: "autogen docs", pr_number: 15931, scopes: ["kafka source"], type: "docs", breaking_change: false, author: "David Huie", files_count: 6, insertions_count: 164, deletions_count: 204},
		{sha: "e65a65d0e3fc1187c5d784790c4aab373978782e", date: "2023-01-13 22:48:02 UTC", description: "Add support for alternate container tools in `vdev`", pr_number: 15936, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 58, deletions_count: 47},
		{sha: "94fcd43bbe562e290ba277b1e78d324cd6000f31", date: "2023-01-13 22:49:46 UTC", description: "Add wrappers for check/generate scripts to `vdev`", pr_number: 15891, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 43, insertions_count: 550, deletions_count: 337},
		{sha: "3f550b4ae8b0c4e667097f0e96d0d1c305a914be", date: "2023-01-16 15:57:35 UTC", description: "bump aws-sigv4 from 0.51.0 to 0.53.0", pr_number: 15967, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 119, deletions_count: 43},
		{sha: "c66dd2a5f6700de1ba4e9ca84fed2f330f0f018a", date: "2023-01-16 15:57:59 UTC", description: "bump typetag from 0.2.4 to 0.2.5", pr_number: 15964, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "889c167b78c93c2de8b02576f65608e128071110", date: "2023-01-16 18:55:39 UTC", description: "bump docker/build-push-action from 3.2.0 to 3.3.0", pr_number: 15968, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "34af7e601cc043635efd1081b5a2823c8f225092", date: "2023-01-17 05:09:09 UTC", description: "bump wiremock from 0.5.16 to 0.5.17", pr_number: 15948, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a255ef47174db7911ba4ae16175ea3c47f407168", date: "2023-01-17 16:31:37 UTC", description: "Add permissions to labeler", pr_number: 15973, scopes: ["ci"], type: "fix", breaking_change: false, author: "Josh Soref", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "0fbbe1479d09b164c6c2d15401ac7656e0cd8a38", date: "2023-01-17 21:33:19 UTC", description: "bump rustyline from 10.0.0 to 10.1.0", pr_number: 15975, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 14},
		{sha: "e12a08ca7bd2d6f1e0cadecad7616cd4adf587ee", date: "2023-01-17 14:00:40 UTC", description: "Update OS test targets for packages", pr_number: 15981, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 9, deletions_count: 1},
		{sha: "460c39a8ecfb3f2ee5e4fb3cb43e41ca01bb7a2f", date: "2023-01-17 16:53:41 UTC", description: "Replace `app::exec*` with a more generic framework in `vdev`", pr_number: 15958, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 15, insertions_count: 96, deletions_count: 99},
		{sha: "2b446f7a434a74bd7d65b930797fc2f518540320", date: "2023-01-18 00:55:58 UTC", description: "Pass `access_key` to Firehose source metadata", pr_number: 15904, scopes: ["aws_kinesis_firehose"], type: "feat", breaking_change: false, author: "Dmitrijs Zubriks", files_count: 5, insertions_count: 103, deletions_count: 4},
		{sha: "2e1fb378173447f3d0feb4cc94d78c5de5b1b29d", date: "2023-01-17 21:05:12 UTC", description: "Fix vdev check fmt command", pr_number: 15986, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b1fc33c8bf11a9c3cd8a89db65c59e346b578cb0", date: "2023-01-17 18:24:43 UTC", description: "Remove unused script", pr_number: 15930, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 20},
		{sha: "00b0d9c68b6991e6e953ad80dad243dd7a10ac2f", date: "2023-01-17 22:15:31 UTC", description: "Drop the separate `vdev check style` subcommand", pr_number: 15988, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 2, deletions_count: 16},
		{sha: "379b602717e35607f413d65af0b0a02f99d98c1e", date: "2023-01-18 16:18:02 UTC", description: "split each component validation run into its own test", pr_number: 15987, scopes: ["tests"], type: "enhancement", breaking_change: false, author: "Toby Lawrence", files_count: 10, insertions_count: 552, deletions_count: 430},
		{sha: "8f2aac8a44b4cb810f7f1bee4201fd67574d5414", date: "2023-01-18 17:17:32 UTC", description: "upgrade h2 and remove patch", pr_number: 15993, scopes: ["deps"], type: "chore", breaking_change: false, author: "Luke Steensen", files_count: 2, insertions_count: 4, deletions_count: 5},
		{sha: "c4a4e167a10dc178a3e35dd0265964ad25c02267", date: "2023-01-18 17:47:52 UTC", description: "skip pulsar 5.0.1", pr_number: 15994, scopes: ["deps"], type: "chore", breaking_change: false, author: "Luke Steensen", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c7a4d7b46a4b351f936b4ac675db3b12ece59734", date: "2023-01-18 19:53:02 UTC", description: "remove need to manually mark newtype fields as `derived`/`transparent`", pr_number: 15995, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 33, insertions_count: 253, deletions_count: 209},
		{sha: "2a7be394b4c447e048a5b415ff36f7ebf4d03934", date: "2023-01-18 19:45:24 UTC", description: "migrate away from deprecated chrono functions", pr_number: 15992, scopes: ["deps"], type: "chore", breaking_change: false, author: "Luke Steensen", files_count: 46, insertions_count: 518, deletions_count: 141},
		{sha: "13e6008768f0837db6eaf7555958c1195b009095", date: "2023-01-18 18:50:16 UTC", description: "Bump Vector version to 0.28.0", pr_number: 15996, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "9db4c5e2b52b845920f6d8daac92d6faa6dc2686", date: "2023-01-18 19:40:03 UTC", description: "Regenerate k8s manifests for v0.27.0", pr_number: 15998, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "614961588662ac4349fae9c3df6405c219a07107", date: "2023-01-18 22:57:24 UTC", description: "Spelling", pr_number: 15970, scopes: [], type: "chore", breaking_change: false, author: "Josh Soref", files_count: 243, insertions_count: 658, deletions_count: 807},
		{sha: "b0d9c3f96cdeccf5cde4a5d9f5d63a95be796353", date: "2023-01-18 21:46:33 UTC", description: "autogen docs", pr_number: 15956, scopes: ["host_metrics source"], type: "docs", breaking_change: false, author: "David Huie", files_count: 9, insertions_count: 338, deletions_count: 467},
		{sha: "964919a17e57e0b57d446826425ce03144dad5e1", date: "2023-01-19 04:39:36 UTC", description: "Add max_events option", pr_number: 14817, scopes: ["reduce transform"], type: "enhancement", breaking_change: false, author: "j chesley", files_count: 2, insertions_count: 165, deletions_count: 28},
		{sha: "f9c4006940d697a17e202be53e84bffdad0c4e70", date: "2023-01-19 19:24:49 UTC", description: "Use generated docs for configuration", pr_number: 15985, scopes: ["prometheus_remote_write source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 6, deletions_count: 14},
		{sha: "3e6a20bac7f5b0e446adc12d2c513ed0c875f9f9", date: "2023-01-19 15:14:06 UTC", description: "bump clap_complete from 4.0.7 to 4.1.0", pr_number: 16013, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 15, deletions_count: 15},
		{sha: "6081dd3e13cbe0cb53dc09bc83713e64715a8b8a", date: "2023-01-19 15:14:50 UTC", description: "bump tokio from 1.24.1 to 1.24.2", pr_number: 16012, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "73952f1e751527bf94f30a1f7cea4c544e91a6f3", date: "2023-01-19 15:15:38 UTC", description: "bump clap from 4.0.32 to 4.1.1", pr_number: 16011, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 6, deletions_count: 6},
		{sha: "da7f3491fc7aab702b5048d586c5f17b2dc57a54", date: "2023-01-19 15:18:19 UTC", description: "bump nix from 0.26.1 to 0.26.2", pr_number: 16010, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "b39044b2afb5c4d97d6c5e300410dbb031460318", date: "2023-01-19 15:30:11 UTC", description: "bump proc-macro2 from 1.0.49 to 1.0.50", pr_number: 16009, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 68, deletions_count: 68},
		{sha: "53fe4fe20884af937487c7ee6c8692ee0b82cf5f", date: "2023-01-19 16:26:50 UTC", description: "bump security-framework from 2.7.0 to 2.8.0", pr_number: 16006, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "10e4b98c07ce4d30e73fd524cae3b6a03178c467", date: "2023-01-19 21:58:53 UTC", description: "bump nom from 7.1.2 to 7.1.3", pr_number: 16005, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "83a51e49cfcdfa5a88e8e86217fb1b824d1f91d6", date: "2023-01-20 00:51:06 UTC", description: "Fix checksum calculation", pr_number: 15899, scopes: ["file source"], type: "fix", breaking_change: false, author: "Alex Savitskii", files_count: 2, insertions_count: 111, deletions_count: 5},
		{sha: "c6d81006ec03aa0db709559d5e08846f7a3b6caa", date: "2023-01-19 22:36:49 UTC", description: "Run `vdev` wrapped scripts through a shell on Windows", pr_number: 15999, scopes: ["dev"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 27, deletions_count: 3},
		{sha: "d79108abb9141a3497356dbb31c88f41c4520a4f", date: "2023-01-20 20:30:41 UTC", description: "added Log Namespacing tutorial", pr_number: 15954, scopes: [], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 436, deletions_count: 0},
		{sha: "5fc2f82860d950fb4cbc12820a05fbd7b9ce7950", date: "2023-01-20 15:32:07 UTC", description: "bump rustyline from 10.0.0 to 10.1.1", pr_number: 16029, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 5},
		{sha: "1c168452cc94c730b151417b56444c5de9a3efa7", date: "2023-01-20 15:41:00 UTC", description: "bump serde_yaml from 0.9.16 to 0.9.17", pr_number: 16042, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 7},
		{sha: "7c6af9d403d594ae288db11a0e5ca7322cd9ac11", date: "2023-01-20 15:41:30 UTC", description: "bump indicatif from 0.17.2 to 0.17.3", pr_number: 16043, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "786a376c4e3db33ca84ddab4af283fe5fd2a3327", date: "2023-01-20 15:42:19 UTC", description: "bump termcolor from 1.1.3 to 1.2.0", pr_number: 16046, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "85908debec7bd1d84d5c0a8555342a5537c90f70", date: "2023-01-20 15:43:02 UTC", description: "bump reqwest from 0.11.13 to 0.11.14", pr_number: 16044, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 19, deletions_count: 5},
		{sha: "3dd0fc5cb376be705e1a563e66dff23c4913635c", date: "2023-01-20 15:43:57 UTC", description: "bump async-recursion from 1.0.0 to 1.0.2", pr_number: 16041, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "39aeffca540282cf3c7ec960934672f4c059067f", date: "2023-01-20 22:21:03 UTC", description: "bump toml from 0.5.10 to 0.5.11", pr_number: 16040, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 8, deletions_count: 8},
		{sha: "1f55e68e3afb97370058c903da0beeec49db36ee", date: "2023-01-20 16:03:33 UTC", description: "autogen cue docs", pr_number: 16030, scopes: ["blackhole sink"], type: "feat", breaking_change: false, author: "David Huie", files_count: 7, insertions_count: 38, deletions_count: 41},
		{sha: "4b3c4c440db68c93dfc9f26c4b0dc64554fc6cc5", date: "2023-01-20 17:10:07 UTC", description: "support generating Linux-specific docs on all platforms", pr_number: 16033, scopes: ["host_metrics source"], type: "fix", breaking_change: false, author: "David Huie", files_count: 4, insertions_count: 132, deletions_count: 67},
		{sha: "eeb2bb5f846df4c22fc2a0180d387538249bacc2", date: "2023-01-20 20:01:18 UTC", description: "autogen cue docs", pr_number: 16051, scopes: ["console sink"], type: "docs", breaking_change: false, author: "David Huie", files_count: 3, insertions_count: 32, deletions_count: 22},
		{sha: "dffd4d0497837ff76b28ee1b70447720b12761e4", date: "2023-01-23 21:34:46 UTC", description: "Remove null fields from example configs on website", pr_number: 16071, scopes: [], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 4, deletions_count: 1},
		{sha: "5d4d3303a68f5d4b72dc1faad167414fe66f623d", date: "2023-01-23 16:33:27 UTC", description: "Rewrite `check-examples.sh` script in vdev", pr_number: 16034, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 49, deletions_count: 22},
		{sha: "83a6fc4acee7af671212bf4491ef615077ea90c6", date: "2023-01-23 17:07:32 UTC", description: "Add new `info` command to show overall vdev setup", pr_number: 16077, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 39, deletions_count: 2},
		{sha: "aff7df84999595932b2f06879a943ac2dbae8332", date: "2023-01-23 16:41:11 UTC", description: "use auto-generated config docs", pr_number: 16003, scopes: ["internal_metrics source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 6, insertions_count: 81, deletions_count: 98},
		{sha: "d2ad68c6d0a7bdfede41f50cadc069263c42c47c", date: "2023-01-23 16:41:24 UTC", description: "use auto-generated config docs", pr_number: 16038, scopes: ["http_server source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 4, insertions_count: 48, deletions_count: 106},
		{sha: "2722c9fea2bf737dcc3b1c0e5d170075da547a39", date: "2023-01-23 19:29:47 UTC", description: "Rewrite `scripts/check-scripts.sh` in vdev", pr_number: 16032, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 33, deletions_count: 19},
		{sha: "b72caa93f156681ed9eed152eb9faa6920e926b0", date: "2023-01-23 20:35:28 UTC", description: "Drop the `display!` macro", pr_number: 16083, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 7, insertions_count: 26, deletions_count: 38},
		{sha: "40e7313a2718c263fba50d2eab1177a0c5a497cd", date: "2023-01-23 21:27:48 UTC", description: "Rewrite `check-component-features` script into vdev", pr_number: 16080, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 102, deletions_count: 70},
		{sha: "5e6d29946a938a4543bbd97ec50df0d631b05dba", date: "2023-01-23 21:58:28 UTC", description: "use auto-generated config docs", pr_number: 16027, scopes: ["statsd", "socket source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 28, insertions_count: 331, deletions_count: 380},
		{sha: "c35e50d9b3ac113ab036e45deca035e457315742", date: "2023-01-23 23:53:33 UTC", description: "use auto-generated config docs ", pr_number: 16084, scopes: ["fluent source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 4, insertions_count: 30, deletions_count: 35},
		{sha: "6bcdde7ccb0a8b06a95a9fd8a56731ac82ca4078", date: "2023-01-24 00:20:06 UTC", description: "use auto-generated config docs", pr_number: 16057, scopes: ["logstash source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 24, deletions_count: 29},
		{sha: "9c5b0a137967986ccb11ee2675a7cbf4941398ad", date: "2023-01-24 15:46:05 UTC", description: "Use generated docs for configuration", pr_number: 15901, scopes: ["dnstap source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 36, deletions_count: 99},
		{sha: "bd17773f4ff441d746c7a7ab2d133ff9a302b2bf", date: "2023-01-24 23:58:24 UTC", description: "update source description", pr_number: 15871, scopes: ["statsd"], type: "docs", breaking_change: false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "de4fb54e46ab739feae6a3cc0d620807e23f7c4a", date: "2023-01-24 14:39:46 UTC", description: "use auto-generated config docs", pr_number: 16054, scopes: ["http_client source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 16, insertions_count: 285, deletions_count: 284},
		{sha: "ffb5c2ef49e761cc2c0bab293ccd4b2257d69234", date: "2023-01-24 14:54:44 UTC", description: "use auto-generated config docs", pr_number: 16052, scopes: ["datadog_agent source"], type: "docs", breaking_change: false, author: "neuronull", files_count: 21, insertions_count: 83, deletions_count: 78},
		{sha: "ad85402a82342a5f23b093469f5e3e340b76a604", date: "2023-01-24 17:06:09 UTC", description: "Add new issues and PRs to Gardener backlog for triage", pr_number: 15719, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 39, deletions_count: 0},
		{sha: "72219521788c2b5920dc895ca19d3b07666eb432", date: "2023-01-24 22:28:19 UTC", description: "Use generated docs for configuration", pr_number: 15983, scopes: ["nats source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 5, insertions_count: 40, deletions_count: 38},
		{sha: "4afcf3309c09331736b3d9725708ab7c580265d3", date: "2023-01-25 01:45:58 UTC", description: "add encode_gzip and decode_gzip functions", pr_number: 16039, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 8, insertions_count: 315, deletions_count: 0},
		{sha: "6d84f1cf426deb3cd40c9fe5c00441422fb43ac6", date: "2023-01-24 14:46:56 UTC", description: "bump async-trait from 0.1.61 to 0.1.63", pr_number: 16066, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "17f6846c9128e8396955bce81ac5ccaf8c56e89c", date: "2023-01-24 14:48:47 UTC", description: "bump redis from 0.22.2 to 0.22.3", pr_number: 16094, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c377d28bc84943c0115b370622c1d8cf7fa3c593", date: "2023-01-24 14:49:36 UTC", description: "bump clap_complete from 4.1.0 to 4.1.1", pr_number: 16092, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8881a362c334ce28e7e37b08102d8c2c89a237f1", date: "2023-01-24 14:50:02 UTC", description: "bump clap from 4.1.1 to 4.1.3", pr_number: 16091, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 16, deletions_count: 16},
		{sha: "3a726179afabdf60e6c888ebf29057e65aa9660e", date: "2023-01-24 14:50:29 UTC", description: "bump arbitrary from 1.2.2 to 1.2.3", pr_number: 16090, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "f054d3a4ec245ad5d4544399f3eec9cf83931c7a", date: "2023-01-24 14:50:52 UTC", description: "bump axum from 0.6.2 to 0.6.3", pr_number: 16069, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a6ba367e4f7277c8a4470e659ab7a4e912fc939c", date: "2023-01-24 14:51:20 UTC", description: "bump rust_decimal from 1.27.0 to 1.28.0", pr_number: 16067, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "c3b2d72121459ddbf279f6cb58461c25dca693e3", date: "2023-01-25 00:59:45 UTC", description: "Use generated docs for configuration", pr_number: 16074, scopes: ["prometheus_scrape source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 4, insertions_count: 31, deletions_count: 117},
		{sha: "3adc39b3d9a2118225b50cab9a344a9cd59f1481", date: "2023-01-24 20:00:51 UTC", description: "Fix GHA add-to-project version", pr_number: 16106, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "1bf3eae600577f1691989ba013a5c988af5b4c22", date: "2023-01-24 20:46:24 UTC", description: "Rework support for integration tests", pr_number: 16085, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 73, insertions_count: 1356, deletions_count: 304},
		{sha: "b6dcfcfe82675d33e4c8fac216d251154201d6ab", date: "2023-01-25 02:57:35 UTC", description: "bump bollard from 0.13.0 to 0.14.0", pr_number: 16068, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 13, deletions_count: 8},
		{sha: "2f3d155cfcfe7d85e1e3a117d8d90b300c7ef6c1", date: "2023-01-25 03:28:37 UTC", description: "Use generated docs for configuration ", pr_number: 16096, scopes: ["splunk_hec source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 5, deletions_count: 88},
		{sha: "a337b15e9dd40775206accb66876858fa75a1538", date: "2023-01-25 20:45:18 UTC", description: "Use generated docs for configuration", pr_number: 16102, scopes: ["syslog source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 24, deletions_count: 73},
		{sha: "76e9592e0561feb2c797e0b1c3173f2c4d874d2b", date: "2023-01-25 15:17:07 UTC", description: "Refactor test runners", pr_number: 16107, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 49, deletions_count: 97},
		{sha: "409eaa68c5ad5acd9dc3a25dfb35fa6adcf52f21", date: "2023-01-25 18:34:44 UTC", description: "Run some integration tests through vdev", pr_number: 16124, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 18, insertions_count: 86, deletions_count: 700},
		{sha: "d9da507d274a3d35af38c4337addfdde814dc954", date: "2023-01-25 17:14:21 UTC", description: "bump num_enum from 0.5.7 to 0.5.9", pr_number: 16117, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "20b4847421cce4fa2297c14cfbd6ec575976141f", date: "2023-01-25 17:14:41 UTC", description: "bump webbrowser from 0.8.4 to 0.8.5", pr_number: 16116, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 18},
		{sha: "c290491ab5e8365ccefd29d7f3c88eb8473e8e90", date: "2023-01-25 17:14:59 UTC", description: "bump axum from 0.6.3 to 0.6.4", pr_number: 16115, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a9304e2e1f1a71f2626ca3bf1221b830ce4303bf", date: "2023-01-25 17:15:15 UTC", description: "bump pest from 2.5.3 to 2.5.4", pr_number: 16114, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "072376e98c0d93fbe7f66fbaea785d793d4c5c4d", date: "2023-01-25 17:15:34 UTC", description: "bump clap from 4.1.3 to 4.1.4", pr_number: 16113, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 16, deletions_count: 16},
		{sha: "f3980bbfcfd33c4eadf2d43ed2949d88ba6ca84c", date: "2023-01-25 17:15:51 UTC", description: "bump aws-smithy-http-tower from 0.51.0 to 0.54.0", pr_number: 16112, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 62, deletions_count: 13},
		{sha: "dc473a4f9f08f6723596f653660fbe9378dc3050", date: "2023-01-25 17:16:06 UTC", description: "bump security-framework from 2.8.0 to 2.8.1", pr_number: 16110, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1b27f7704b27768aa3bd92c9b86d5ecf4f3b6cd4", date: "2023-01-25 19:56:30 UTC", description: "Fix the shutdown integration test to run in vdev", pr_number: 16128, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 1, deletions_count: 68},
		{sha: "5bfa0935c3c9f1298c89a830526a9f942f07868c", date: "2023-01-25 20:58:12 UTC", description: "Fix the chronicle integration test to run in vdev", pr_number: 16127, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 18, deletions_count: 51},
		{sha: "f7dfe052f81e977867df1314f0e1efe99d1d930d", date: "2023-01-25 20:59:21 UTC", description: "Run mongodb integration test through vdev", pr_number: 16129, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 0, deletions_count: 77},
		{sha: "12906ee63ce49900f063df1a8920aab91f992b0c", date: "2023-01-26 16:11:40 UTC", description: "added deprecated option to cue", pr_number: 16105, scopes: [], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 36, insertions_count: 135, deletions_count: 6},
		{sha: "c3d700198287c1f09f1466f3cf8d9243a9d0100a", date: "2023-01-27 00:10:21 UTC", description: "Separated healthcheck and regenerate token spawn", pr_number: 15827, scopes: ["gcp sink"], type: "fix", breaking_change: false, author: "Gleb Vazhenin", files_count: 5, insertions_count: 9, deletions_count: 17},
		{sha: "6392ed8576f9aa406a4e6d1f767824f6bfc3401f", date: "2023-01-26 18:05:50 UTC", description: " Use generated docs for config ", pr_number: 16099, scopes: ["file source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 8, insertions_count: 179, deletions_count: 318},
		{sha: "6dcdba3dca0dcba936f430206550410272a6f7c5", date: "2023-01-26 18:35:52 UTC", description: "Fix gardener PR workflow", pr_number: 16142, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "da676408f7206a62b688fa75becd8acf3ef7b5b1", date: "2023-01-26 20:15:13 UTC", description: "use auto-generated config docs ", pr_number: 16088, scopes: ["statsd sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 20, insertions_count: 57, deletions_count: 82},
		{sha: "f11a8033d22e6af061c89de3ba9bc6dcbd9b9dcb", date: "2023-01-26 20:16:28 UTC", description: "bump actions/github-script from 6.3.3 to 6.4.0", pr_number: 16143, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 7},
		{sha: "9e2afd711811315f6b01ea7461367466627a2ec8", date: "2023-01-26 20:16:51 UTC", description: "bump webbrowser from 0.8.5 to 0.8.6", pr_number: 16137, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "4ad0c4b293ca3689e7e8dc00abdb135073e63649", date: "2023-01-26 20:17:09 UTC", description: "bump pest_derive from 2.5.3 to 2.5.4", pr_number: 16135, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "25f089c814fcf00cfa096bec7abe3b33b1e47344", date: "2023-01-26 20:17:22 UTC", description: "bump aws-smithy-http-tower from 0.54.0 to 0.54.1", pr_number: 16133, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 11},
		{sha: "d502f1c75d36935ff0f611abd5d1dccac8cc394e", date: "2023-01-26 21:20:09 UTC", description: "Set `BatchConfig` defaults for auto generated cue docs", pr_number: 16108, scopes: ["sinks"], type: "docs", breaking_change: false, author: "neuronull", files_count: 38, insertions_count: 468, deletions_count: 220},
		{sha: "5a4f19707b3c42b50751eb622b48b86970dd3afa", date: "2023-01-26 22:05:30 UTC", description: "use auto-generated config docs", pr_number: 16151, scopes: ["vector sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 12, deletions_count: 21},
		{sha: "cb9a09d7e5c975bf801f822cf718fbd685a35d63", date: "2023-01-26 23:39:36 UTC", description: "Allow only a single running integration test environment", pr_number: 16152, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 112, deletions_count: 140},
		{sha: "81b82856a8ef9bd37018a4faa19db6eb323abe86", date: "2023-01-27 01:03:14 UTC", description: "Use generated docs for config", pr_number: 15836, scopes: ["aws_s3 source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 24, insertions_count: 599, deletions_count: 400},
		{sha: "a14cd28c54a1e0ca727483438434906a7f2e28ca", date: "2023-01-27 19:40:25 UTC", description: "Use generated docs for configuration", pr_number: 16076, scopes: ["redis source"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 10, deletions_count: 72},
		{sha: "4c8637501510f5ba0f64736831cd4662087aa79c", date: "2023-01-27 18:16:58 UTC", description: "Use autogenerated config docs", pr_number: 16165, scopes: ["aws_sqs source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 39, deletions_count: 73},
		{sha: "c7bba79860f2b9493241bca00bad2fb1f32b7b33", date: "2023-01-28 03:31:23 UTC", description: "Fix docker tag for distroless-libc image (In website documentation)", pr_number: 16140, scopes: ["docs"], type: "fix", breaking_change: false, author: "Max Mayorovsky", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "7d7ed58d117d71268227d8458d2e363376885a18", date: "2023-01-27 18:40:35 UTC", description: "use auto-generated config docs ", pr_number: 16157, scopes: ["websocket sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 4, insertions_count: 48, deletions_count: 55},
		{sha: "a1e59343ee1f253a9ffe7d40478ea5a9f7e93a53", date: "2023-01-27 22:25:48 UTC", description: "Clean up runner containers and networks on stop", pr_number: 16167, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 30, insertions_count: 22, deletions_count: 39},
		{sha: "d1cbd73e2163121bc9db08e3bf2346f083bd3799", date: "2023-01-27 23:12:08 UTC", description: "synchronous consumer offset commit on shutdown", pr_number: 15870, scopes: ["kafka"], type: "feat", breaking_change: false, author: "Adam Holmberg", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "a5692595ea705ef981bc3043546105e6449a67c8", date: "2023-01-28 00:18:52 UTC", description: "Fix the logstash integration test to run through vdev", pr_number: 16168, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 76, deletions_count: 74},
		{sha: "c96313e58c624b54ea9bd7c64447fa98e91e95e7", date: "2023-01-28 17:10:38 UTC", description: "Revert Fix the logstash integration test to run through vdev", pr_number: 16176, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 74, deletions_count: 76},
		{sha: "338642608ce66b25468c8749b2c3c10b2af55468", date: "2023-01-30 16:14:29 UTC", description: "Fix the logstash integration test to run through vdev", pr_number: 16191, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 77, deletions_count: 73},
		{sha: "06daa0e70db935b711b64df27b726ea297ba4273", date: "2023-01-30 16:24:24 UTC", description: "Fix the splunk integration tests to run in vdev", pr_number: 16174, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 28, deletions_count: 69},
		{sha: "660e87866ca66aa8fa02678a1ecb13a8a31bfa20", date: "2023-01-30 22:59:39 UTC", description: "Use generated docs for configuration", pr_number: 16163, scopes: ["gcp_chronicle sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 16, deletions_count: 56},
		{sha: "36e9c38bc6848f394dc7cda4a9ac8645ae525bb3", date: "2023-01-30 17:43:41 UTC", description: "Run shutdown integration test through vdev", pr_number: 16166, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 8, insertions_count: 79, deletions_count: 99},
		{sha: "4064a5f539744aeb4ce6c48c515acccb8f20b47b", date: "2023-01-31 00:42:12 UTC", description: "update deprecated message for source components", pr_number: 16138, scopes: [], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 12, insertions_count: 48, deletions_count: 42},
		{sha: "62e6737ed4d87f836544ad0968cc2f3cc883c2b2", date: "2023-01-30 18:17:05 UTC", description: " Adjust `TowerRequestConfig` for auto generated cue docs", pr_number: 16150, scopes: ["sinks"], type: "docs", breaking_change: false, author: "neuronull", files_count: 48, insertions_count: 1134, deletions_count: 416},
		{sha: "408f54b91452c34e9fca0a8c4c7f3c66b4071928", date: "2023-01-30 21:03:23 UTC", description: "Use generated docs for config", pr_number: 16192, scopes: ["docker_logs source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 62, deletions_count: 180},
		{sha: "12942d05ba378e5e4dcc93c5051d26f412d23620", date: "2023-01-30 21:37:55 UTC", description: "use auto-generated config docs", pr_number: 16170, scopes: ["socket sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 2, deletions_count: 29},
		{sha: "9449d179c05f506d008c153da0bca8937caafb71", date: "2023-01-30 23:35:52 UTC", description: "Fix prometheus integration test to work in `vdev`", pr_number: 16198, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 8, insertions_count: 104, deletions_count: 115},
		{sha: "65d7d7623ffc69bbf0b9d81bdc765b4e60013014", date: "2023-01-31 19:33:37 UTC", description: "Numerous fixes to make rustdocs compile.", pr_number: 16189, scopes: [], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 24, insertions_count: 44, deletions_count: 26},
		{sha: "38f0999396089b19f1b70d1c46d38c6ae5a7e33e", date: "2023-01-31 16:20:16 UTC", description: "Fix team PR check", pr_number: 16196, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 13, deletions_count: 3},
		{sha: "3773109ffeebd9ad40f121e7ef823b69914ec0b0", date: "2023-01-31 16:40:11 UTC", description: "fix improper expect during allocation group ID registration", pr_number: 16201, scopes: ["observability"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 11, deletions_count: 11},
		{sha: "0cb4dc228937f09838cea172035be8b2579e1ded", date: "2023-01-31 23:01:33 UTC", description: "Use generated docs for configuration", pr_number: 16202, scopes: ["gcp_stackdriver_metrics sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 9, insertions_count: 46, deletions_count: 105},
		{sha: "d137d063f9258fa839d0f02ba415bb13f64991af", date: "2023-02-01 03:06:48 UTC", description: "add encode_zlib and decode_zlib functions", pr_number: 16059, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 7, insertions_count: 313, deletions_count: 0},
		{sha: "108fe34de62e78bc03051e4bf8b6ee9c87d572f7", date: "2023-01-31 17:46:19 UTC", description: "enable auto-generated proxy settings", pr_number: 16200, scopes: ["sinks"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 58, deletions_count: 34},
		{sha: "228b3fd94ddbb86480576e7b138d91763c7a1bb7", date: "2023-01-31 19:59:01 UTC", description: "Use generated docs for config, remove deprecated `units` option", pr_number: 16194, scopes: ["journald source"], type: "docs", breaking_change: true, author: "Spencer Gilbert", files_count: 4, insertions_count: 168, deletions_count: 196},
		{sha: "876bccfc1218066e4f62b1194df61a32718d98af", date: "2023-01-31 17:20:37 UTC", description: "bump docker/build-push-action from 3.3.0 to 4.0.0", pr_number: 16217, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e4f1b2ee0d36ad7e2ed5d897556c4dea9bd0673c", date: "2023-01-31 20:33:57 UTC", description: "Use generated docs for config", pr_number: 15724, scopes: ["kubernetes_logs source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 486, deletions_count: 474},
		{sha: "97d46eda9b186014ca5ee54528927f92466fbf2b", date: "2023-01-31 19:34:10 UTC", description: "Make docker-logs integration run in `vdev`", pr_number: 16218, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 33, insertions_count: 204, deletions_count: 156},
		{sha: "6617316533ca73529b253483a66150a3a07df845", date: "2023-01-31 18:54:34 UTC", description: "use auto-generated config docs", pr_number: 16204, scopes: ["http sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 8, insertions_count: 120, deletions_count: 116},
		{sha: "90ab4698561fb5817af833304e78e51f08360309", date: "2023-01-31 19:14:03 UTC", description: "bump docker/setup-buildx-action from 2.2.1 to 2.4.0", pr_number: 16193, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "37c2141e94149b85a19f1655f7010fdc190fece7", date: "2023-01-31 21:10:09 UTC", description: "correct proxy doc comments", pr_number: 16220, scopes: ["docs"], type: "fix", breaking_change: false, author: "neuronull", files_count: 4, insertions_count: 21, deletions_count: 14},
		{sha: "bd70509cd24a1a218671eb392416f54408817c75", date: "2023-01-31 23:14:59 UTC", description: "use auto-generated config docs", pr_number: 16172, scopes: ["logdna sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 79, deletions_count: 107},
		{sha: "8c9c00821b16633b6e55816c6ea7d2d09a7f0a38", date: "2023-02-01 14:13:09 UTC", description: "Set up datadog-logs/metrics integrations to run in `vdev`", pr_number: 16213, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 11, insertions_count: 123, deletions_count: 113},
		{sha: "cdb3ae104d4b654aceff133342c77fbfb5234b4b", date: "2023-02-01 16:36:30 UTC", description: "Fix Windows compile problem", pr_number: 16234, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 9, deletions_count: 4},
		{sha: "c7e0b2ca94ffdeabfbdbae0c4a3a845c3752026b", date: "2023-02-01 15:55:11 UTC", description: "Improve Reduce Performance", pr_number: 9502, scopes: ["reduce transform"], type: "enhancement", breaking_change: false, author: "Danny Browning", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "c984f49da77b28bc4b563ec8de50cfb9d7ee6b79", date: "2023-02-01 18:17:04 UTC", description: "start marking certain shared config types as advanced", pr_number: 16235, scopes: ["config", "docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 6, insertions_count: 7, deletions_count: 0},
		{sha: "252d838ea79ec96579d5012936dfc2f27d0b4586", date: "2023-02-01 20:19:43 UTC", description: "bump futures from 0.3.25 to 0.3.26", pr_number: 16229, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 51, deletions_count: 51},
		{sha: "f07034e32efd3fe55a7bbc0c4e7d519b2ad1513f", date: "2023-02-01 20:19:57 UTC", description: "bump os_info from 3.5.1 to 3.6.0", pr_number: 16227, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4bfd4c327d3881121e825d0e6b2baa1f05b670dc", date: "2023-02-01 20:43:21 UTC", description: "bump zstd from 0.12.2+zstd.1.5.2 to 0.12.3+zstd.1.5.2", pr_number: 16228, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "dd143e3ebb371d24a074c97832dda015b8a6c48b", date: "2023-02-02 00:05:19 UTC", description: "RFC for Web Playground UI", pr_number: 15147, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jonathan Padilla", files_count: 1, insertions_count: 168, deletions_count: 0},
		{sha: "28b34d4e3df698286b94b1c293dcec3ec27958d7", date: "2023-02-02 14:27:01 UTC", description: "Set up fluent integration tests in vdev", pr_number: 16221, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 10, insertions_count: 115, deletions_count: 80},
		{sha: "b7f32cffc0b5ff7699e8b42e41eac23548a6dee3", date: "2023-02-02 16:02:25 UTC", description: "Fix the datadog-{agent,traces} integration tests to run in vdev", pr_number: 16130, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 0, deletions_count: 175},
		{sha: "7199346e55e46558a67d88c38dba20afe4c2adf2", date: "2023-02-02 15:32:02 UTC", description: "use auto-generated config docs", pr_number: 16238, scopes: ["datadog_traces sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 12, insertions_count: 124, deletions_count: 81},
		{sha: "3db9d4d6d1240983905b61f967b529b842c02895", date: "2023-02-02 22:34:06 UTC", description: "Use generated docs for configuration", pr_number: 16236, scopes: ["papertrail sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 28, deletions_count: 37},
		{sha: "518d62faf901bafbb1f7853a441d4c3943697364", date: "2023-02-02 20:24:27 UTC", description: "Fix up some subcommand text", pr_number: 16253, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "e6cefb1c5586c2053f216c373e975f3e3a8cc8de", date: "2023-02-02 20:55:42 UTC", description: "Modify eventstoredb integration tests to run in vdev", pr_number: 16255, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 1, deletions_count: 42},
		{sha: "2d0292145eb94367bfc4433c074c25b01573d489", date: "2023-02-02 20:13:59 UTC", description: "use auto-generated config docs ", pr_number: 16246, scopes: ["datadog_events sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 50, deletions_count: 20},
		{sha: "fabd1d78260910e4af94fb02687c1b493908ab44", date: "2023-02-02 21:21:58 UTC", description: "Modify humio integration test to run through vdev", pr_number: 16256, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 4, deletions_count: 41},
		{sha: "3d26c4f5a7640a5dffefe2a642d0f7e6ba3bd431", date: "2023-02-02 21:27:46 UTC", description: "Modify opentelemetry integration test to run in vdev", pr_number: 16258, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 5, deletions_count: 51},
		{sha: "0834c6904259882cf5e5d0777b84167b45ce4d14", date: "2023-02-02 20:29:34 UTC", description: "use auto-generated config docs", pr_number: 16242, scopes: ["datadog_metrics sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 55, deletions_count: 36},
		{sha: "3d7ccb6f9c7a5d9325a97f7362bbaf6fb8384954", date: "2023-02-02 20:32:26 UTC", description: "use auto-generated config docs", pr_number: 16244, scopes: ["datadog_logs sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 54, deletions_count: 22},
		{sha: "86ca03fa272a09b2988a5be1eea090322dfac357", date: "2023-02-02 21:45:32 UTC", description: "Modify nginx integration tests to run in vdev", pr_number: 16259, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 13, deletions_count: 66},
		{sha: "314e28e79cbff67e564dbdbce374c7d2c2982a4b", date: "2023-02-02 20:19:27 UTC", description: "bump notify from 5.0.0 to 5.1.0", pr_number: 16248, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "4139072c1a6e1541afab231a8f76c41d88ad09e1", date: "2023-02-02 20:19:55 UTC", description: "bump uuid from 1.2.2 to 1.3.0", pr_number: 16247, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "4ac7c032eea40f1000d21f599232e201903646e8", date: "2023-02-02 20:23:11 UTC", description: "bump security-framework from 2.8.1 to 2.8.2", pr_number: 16183, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a3b6086364d4b05b81bd96aec1555ab100473283", date: "2023-02-02 20:24:03 UTC", description: "bump crossterm from 0.25.0 to 0.26.0", pr_number: 16180, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 19, deletions_count: 3},
		{sha: "08cfcef02b74a44754caec3beb3f48bf8c182351", date: "2023-02-02 20:24:23 UTC", description: "bump indoc from 1.0.8 to 2.0.0", pr_number: 16179, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "0756ee2291cf440360b67524ad90876febcc8e9f", date: "2023-02-02 22:44:13 UTC", description: "Modify axiom integration tests to run in vdev", pr_number: 16260, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 4, deletions_count: 67},
		{sha: "aaf564a0f89e06d836344e37626749cef6fb158f", date: "2023-02-03 07:24:32 UTC", description: "add `seahash` function", pr_number: 16073, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Patryk Semeniuk", files_count: 7, insertions_count: 146, deletions_count: 0},
		{sha: "f66162923cb8a694cb990ed623c21f8202b81d5f", date: "2023-02-03 06:40:37 UTC", description: "bump bytes from 1.3.0 to 1.4.0", pr_number: 16226, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 13, insertions_count: 85, deletions_count: 85},
		{sha: "2c9cfb902ce2ebe698f267553e25fa603e8d2762", date: "2023-02-03 07:53:48 UTC", description: "bump crc from 3.0.0 to 3.0.1", pr_number: 16185, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3177208066d14372e0376f8eb87ea54ae1e4a9f0", date: "2023-02-03 08:04:47 UTC", description: "bump tokio from 1.24.2 to 1.25.0", pr_number: 16182, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "9333d0b65dee634c5ed22c35b3fc4f5535f6a919", date: "2023-02-03 17:10:31 UTC", description: "Use generated docs for configuration", pr_number: 16230, scopes: ["nats sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 4, insertions_count: 50, deletions_count: 143},
		{sha: "cf6392c8f6cf08061af8b1178ff4dea401b76333", date: "2023-02-03 11:45:25 UTC", description: "bump hyper from 0.14.23 to 0.14.24", pr_number: 16265, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "001f21cd46d993bd05ca19d56f6573d3c75a7783", date: "2023-02-03 11:45:39 UTC", description: "bump async-trait from 0.1.63 to 0.1.64", pr_number: 16266, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1b15e7a2a5e74daa6552d7d04ccde02115ed5096", date: "2023-02-03 11:45:55 UTC", description: "bump tracing-test from 0.2.3 to 0.2.4", pr_number: 16267, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 18, deletions_count: 72},
		{sha: "b9cd9c33f178c03e4e7c02347b5bdf149d03ae65", date: "2023-02-03 11:46:42 UTC", description: "bump encoding_rs from 0.8.31 to 0.8.32", pr_number: 16268, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ec0e0445fe8918ec82ad9864a3a0b945c33f6ebd", date: "2023-02-03 11:46:53 UTC", description: "bump quoted_printable from 0.4.6 to 0.4.7", pr_number: 16269, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "40cab72fdd5a0ba0267e6949183fb248be7f81d1", date: "2023-02-03 11:47:11 UTC", description: "bump bstr from 1.1.0 to 1.2.0", pr_number: 16270, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a4bad0e99808eb0816f27f17a9bb0349a3a4abfb", date: "2023-02-03 11:47:25 UTC", description: "bump wasm-bindgen from 0.2.83 to 0.2.84", pr_number: 16271, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 10, deletions_count: 10},
		{sha: "ccfb7aa65d32d33efef351c54ebd499fa11da424", date: "2023-02-03 11:47:35 UTC", description: "bump webbrowser from 0.8.6 to 0.8.7", pr_number: 16272, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "8d7ab2e07992195a48af012ced6a5ebcef3bf8df", date: "2023-02-03 11:48:06 UTC", description: "bump http-cache-semantics from 4.1.0 to 4.1.1 in /website", pr_number: 16262, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "364c9d109a176dfee47d50e5862c718bcbc62eef", date: "2023-02-03 14:22:35 UTC", description: "use auto-generated config docs ", pr_number: 16223, scopes: ["elasticsearch sink"], type: "docs", breaking_change: false, author: "neuronull", files_count: 29, insertions_count: 390, deletions_count: 532},
		{sha: "0861774e95f280491d416a88bff581313cd281d4", date: "2023-02-03 21:48:07 UTC", description: "Use generated docs for configuration", pr_number: 16211, scopes: ["humio_metrics sink", "humio_logs sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 7, insertions_count: 111, deletions_count: 37},
		{sha: "71662807dfb2c3f043b8a9fdfde23d19b1297fdd", date: "2023-02-03 18:34:44 UTC", description: "upgrade toml to 0.7.1", pr_number: 16239, scopes: [], type: "chore", breaking_change: false, author: "David Huie", files_count: 14, insertions_count: 92, deletions_count: 33},
		{sha: "0171719a6c1e4eee1866d8007f281c5364f35ccb", date: "2023-02-03 21:02:53 UTC", description: "Modify aws integration tests to run in vdev", pr_number: 16277, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 10, insertions_count: 19, deletions_count: 110},
		{sha: "f8f38df72d9533840cff90dd0d7e376c94ee050c", date: "2023-02-03 22:22:49 UTC", description: "Modify postgres integration tests to work with vdev", pr_number: 16279, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 15, deletions_count: 49},
		{sha: "50933e93a86a9e580bc02206c90e4c977d8233a0", date: "2023-02-04 00:55:36 UTC", description: "Modify dnstap integration test for vdev", pr_number: 16282, scopes: ["ci"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 34, deletions_count: 47},
		{sha: "71aac8396c2442fbdabf03d3bf6c128b1b16e3d3", date: "2023-02-06 23:37:00 UTC", description: "update documentation", pr_number: 16284, scopes: ["clickhouse"], type: "docs", breaking_change: false, author: "Alexander Zaitsev", files_count: 9, insertions_count: 16, deletions_count: 16},
		{sha: "dbb13784bacf2d45434d46f15ef3df5c586418a7", date: "2023-02-06 15:58:17 UTC", description: "Remove creation of debug tarballs", pr_number: 16307, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 31},
		{sha: "88b4c941af03c4218ab540ff968fbd6a43db288c", date: "2023-02-06 15:21:21 UTC", description: "Drop backwards compat hack for old integrations", pr_number: 16283, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 4, deletions_count: 74},
		{sha: "1c0804d3d18ce36ed978e5c3fe99d25420c3de69", date: "2023-02-06 19:22:37 UTC", description: "bump serde_bytes from 0.11.8 to 0.11.9", pr_number: 16290, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "115f6eb1c109bf97b407a6baf09e02265c01a13a", date: "2023-02-06 19:23:43 UTC", description: "bump anyhow from 1.0.68 to 1.0.69", pr_number: 16291, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "387a8e36ea9adb2e6ed46667fea47c143e97a6b1", date: "2023-02-06 19:25:19 UTC", description: "bump proc-macro2 from 1.0.50 to 1.0.51", pr_number: 16292, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 68, deletions_count: 68},
		{sha: "f30e6cf36d6344bb18eb0da389c89089cf04694e", date: "2023-02-06 21:26:09 UTC", description: "a bunch of fixes/additions to schema output", pr_number: 16240, scopes: ["config", "docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 26, insertions_count: 319, deletions_count: 23},
		{sha: "763c0930a56160778a9d5831c463e2e9f94f5036", date: "2023-02-06 19:27:12 UTC", description: "bump roxmltree from 0.17.0 to 0.18.0", pr_number: 16293, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4088499b1cf9900284d11d5c5028e688db13b1e8", date: "2023-02-06 19:29:15 UTC", description: "bump rust_decimal from 1.28.0 to 1.28.1", pr_number: 16294, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 11, deletions_count: 20},
		{sha: "84e1af0ef0637d02b9839255dd0299b135c6c168", date: "2023-02-06 20:47:18 UTC", description: "Export all config matrix settings as CONFIG_{VAR}", pr_number: 16311, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 31, insertions_count: 59, deletions_count: 61},
		{sha: "708acad76497c59f6d688fcfa39f667d2cdd95cc", date: "2023-02-07 04:32:09 UTC", description: "bump serde_json from 1.0.91 to 1.0.92", pr_number: 16296, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "ecb456c2472bb4fa3a5608077e36881a2659d771", date: "2023-02-06 22:49:58 UTC", description: "Clean up integration compose handling", pr_number: 16312, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 101, deletions_count: 76},
		{sha: "db998a62439fac2b84760796f6ae66ab6bf63a22", date: "2023-02-06 21:51:39 UTC", description: "bump proptest from 1.0.0 to 1.1.0", pr_number: 16295, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 14, deletions_count: 7},
		{sha: "61776ddd0bbe4cd373b3baf2531d49914b82793a", date: "2023-02-06 21:53:58 UTC", description: "bump docker/setup-buildx-action from 2.4.0 to 2.4.1", pr_number: 16310, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "2dd58fc84256bd3f0eb8ee6b9dee88fe7bd9b8fe", date: "2023-02-07 01:02:02 UTC", description: "Fix guard for prepare_compose_volumes on unix", pr_number: 16317, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 3, deletions_count: 5},
		{sha: "4ae71bc6c2d2d8a841c919cdd6eb8f2342eac8be", date: "2023-02-07 10:20:44 UTC", description: "add batch support", pr_number: 16063, scopes: ["pulsar"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 2, insertions_count: 27, deletions_count: 0},
		{sha: "a3ffa6b420f20272cbcc9bf1aa5d288b29cd49fb", date: "2023-02-07 01:07:33 UTC", description: "remove reference to non existent job", pr_number: 16319, scopes: ["fix publish workflow"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "2bfd63922e7d82f7616138b51251f27a3052c9b4", date: "2023-02-07 11:39:51 UTC", description: "add more configuration info", pr_number: 16254, scopes: ["kubernetes_logs source"], type: "docs", breaking_change: false, author: "David Huie", files_count: 2, insertions_count: 5, deletions_count: 0},
		{sha: "1410b0510620449dc5194f00f80436c137c1028a", date: "2023-02-07 22:54:34 UTC", description: "Run all checks when vdev is updated", pr_number: 16353, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 8, deletions_count: 0},
		{sha: "bed0b054a683f2a36e599bf0d9ae39c5281ddfd5", date: "2023-02-07 22:39:12 UTC", description: "Fix Windows compilation again", pr_number: 16354, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "a0f9d8299cab959a4e65c5f997116e13d99e0368", date: "2023-02-08 01:52:06 UTC", description: "Use autogenerated docs", pr_number: 16351, scopes: ["axiom sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 13, deletions_count: 46},
		{sha: "130ba91b8fcd519491484380be734875acff0bc9", date: "2023-02-08 01:52:35 UTC", description: "Polish docs and examples", pr_number: 16350, scopes: ["loki sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 11, deletions_count: 7},
		{sha: "41d6378a8d0ef8881448e9bb249287fbe9b795bf", date: "2023-02-08 02:31:04 UTC", description: "Use autogenerated docs", pr_number: 16352, scopes: ["honeycomb sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 8, deletions_count: 18},
		{sha: "1ea3164a95d7efba3765bb5678c5be6d35fb3158", date: "2023-02-08 17:15:46 UTC", description: "Use generated docs for configuration", pr_number: 16301, scopes: ["gcp_pubsub sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 37, deletions_count: 40},
		{sha: "dd6a372fb6ecc924944f5a901aaec146dc913b7d", date: "2023-02-08 18:06:00 UTC", description: "Use generated docs for configuration", pr_number: 16302, scopes: ["gcp_stackdriver_logs sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 63, deletions_count: 160},
		{sha: "e380ab4ad10a290ed1d63f62a866dd2184d7a940", date: "2023-02-08 19:27:23 UTC", description: "Use generated docs for configuration", pr_number: 16303, scopes: ["gcp_cloud_storage sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 9, insertions_count: 84, deletions_count: 132},
		{sha: "c9ff0912aceee7f43b001dabf4704f45e5bc1168", date: "2023-02-08 14:53:57 UTC", description: "Remove another reference to the debug build", pr_number: 16362, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 5},
		{sha: "f5e95ebe5b2eb8342d33ab03416eb8415884e847", date: "2023-02-08 14:01:36 UTC", description: "bump toml from 0.7.1 to 0.7.2", pr_number: 16358, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 16, deletions_count: 16},
		{sha: "ea0818410d542644e942e35d61d6249c5df7dcf1", date: "2023-02-08 14:55:05 UTC", description: "bump pest from 2.5.4 to 2.5.5", pr_number: 16360, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3c5e4f5e029dfb0c133ee4aeb1fe5d3f517036b1", date: "2023-02-08 17:55:51 UTC", description: "Switch to native-tls", pr_number: 16335, scopes: ["aws provider"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 28, deletions_count: 10},
		{sha: "4a5e85134f0b8736c3ba28203150c72fb5e02b60", date: "2023-02-08 22:03:36 UTC", description: "Run workflows that have required checks on the merge queue", pr_number: 16370, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 0},
		{sha: "236915172cdf95ad02ec53a06a8cac5e537f6fa9", date: "2023-02-08 22:38:55 UTC", description: "fix schema generation handling of base vs override metadata", pr_number: 16366, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 13, insertions_count: 400, deletions_count: 260},
		{sha: "5ca98d6e2a88817a68003cf2be8f26ca7e1fec93", date: "2023-02-09 00:06:23 UTC", description: "Use generated documentation", pr_number: 16357, scopes: ["file sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 68, deletions_count: 42},
		{sha: "31a82a5c3892da8bdcbffe76761da81094a1568e", date: "2023-02-09 08:08:58 UTC", description: "add again missing partition key", pr_number: 16286, scopes: ["kinesis"], type: "fix", breaking_change: false, author: "Alexander Zaitsev", files_count: 2, insertions_count: 58, deletions_count: 2},
		{sha: "d98ed324d110991afb8bdc39b9d8d0166c9b8221", date: "2023-02-09 09:30:21 UTC", description: "temporarily disable new aws integration test that is failing", pr_number: 16372, scopes: ["ci"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 56, deletions_count: 55},
		{sha: "87e009ec4cbab1ee6993f6368a1a027b9101f695", date: "2023-02-09 16:41:31 UTC", description: "Use generated docs for configuration", pr_number: 16330, scopes: ["redis sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 4, insertions_count: 37, deletions_count: 69},
		{sha: "085f092497814972bd442a2020b7b31f2730fab2", date: "2023-02-09 15:41:04 UTC", description: "Remove another debug build reference", pr_number: 16375, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 3},
		{sha: "61710fa2a9677b4876fa5e205518514f308b384f", date: "2023-02-09 22:31:13 UTC", description: "Use generated docs for configuration", pr_number: 16323, scopes: ["prometheus_exporter sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 7, deletions_count: 79},
		{sha: "a55bc3d9683bcf81531789613dfce84aeff7e78b", date: "2023-02-09 14:52:18 UTC", description: "autogen docs", pr_number: 16318, scopes: ["pulsar sink"], type: "docs", breaking_change: false, author: "David Huie", files_count: 3, insertions_count: 43, deletions_count: 108},
		{sha: "c48b6ab6451eeccd0a353bb4143f205f8895c2d2", date: "2023-02-09 16:36:31 UTC", description: "Add note about trace events to concepts page", pr_number: 16315, scopes: ["external docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "eb8ae9a201078fc04093742727df356f7f8e581e", date: "2023-02-10 00:28:59 UTC", description: "Use generated docs for configuration", pr_number: 16325, scopes: ["prometheus_remote_write sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 19, deletions_count: 109},
		{sha: "364da891449dcb7db1739d355efade50999c33cb", date: "2023-02-10 00:29:42 UTC", description: "Use generated docs for configuration", pr_number: 16334, scopes: ["splunk_hec_logs sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 57, deletions_count: 119},
		{sha: "ad61f3565bef74ff0e27aeb7abe7205e62b76240", date: "2023-02-10 02:02:51 UTC", description: "Use generated docs for configuration", pr_number: 16336, scopes: ["splunk_hec_metrics sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 6, insertions_count: 62, deletions_count: 129},
		{sha: "6e28862a1c9b92ef16bd64dc8fb2560971334d0b", date: "2023-02-09 19:19:01 UTC", description: "drop no-op track_caller annotations", pr_number: 16379, scopes: ["test_util"], type: "fix", breaking_change: false, author: "David Huie", files_count: 1, insertions_count: 0, deletions_count: 14},
		{sha: "0aa9a5ed843578628c02d642fbafa243db79121c", date: "2023-02-09 22:40:15 UTC", description: "bump serde_json from 1.0.92 to 1.0.93", pr_number: 16374, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "18fedcf1d4576ea93741159d9441f8437f930442", date: "2023-02-09 22:43:38 UTC", description: "bump pest_derive from 2.5.4 to 2.5.5", pr_number: 16373, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "f8a0b2af7b1fdbf963f7722ce931b051b323323c", date: "2023-02-10 03:07:31 UTC", description: "Remove deprecated metadata functions", pr_number: 14821, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 19, insertions_count: 45, deletions_count: 587},
		{sha: "e2c5cfe9ccb569a337d14af0777a36e456cf09b1", date: "2023-02-10 14:55:16 UTC", description: "autogen docs", pr_number: 16377, scopes: ["new_relic sink"], type: "docs", breaking_change: false, author: "David Huie", files_count: 7, insertions_count: 9, deletions_count: 183},
		{sha: "8bb4e09843b41372ea7df6444a94e2d95db0edc1", date: "2023-02-11 02:17:11 UTC", description: "Build rust-doc.vector.dev", pr_number: 16386, scopes: [], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 5, insertions_count: 19, deletions_count: 1},
		{sha: "5317e45575cca091c96b94dd4af5b785be593bc0", date: "2023-02-10 22:03:46 UTC", description: "autogen docs", pr_number: 16378, scopes: ["azure_blob sink"], type: "docs", breaking_change: false, author: "David Huie", files_count: 5, insertions_count: 45, deletions_count: 90},
		{sha: "ab4aca8541d5249beb420ed798329c4c92edf028", date: "2023-02-13 16:13:29 UTC", description: "Setup a redirect from `rust-doc.vector.dev/` to `rust-doc.vector.dev/vector`", pr_number: 16392, scopes: [], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "f4e90e19ca33a1a153c7df110d76222a3e77bd33", date: "2023-02-13 15:06:57 UTC", description: "Update aws-sdks to 0.23 and aws supporting crates to 0.53", pr_number: 16365, scopes: ["deps"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 21, insertions_count: 325, deletions_count: 373},
		{sha: "b46349eb78df439d38f0771b555ce0c924b93073", date: "2023-02-13 12:24:40 UTC", description: "add info about endpoints", pr_number: 16313, scopes: ["splunk_hec source"], type: "docs", breaking_change: false, author: "David Huie", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "665c891752c3798694af86f73b73ca06483be5a5", date: "2023-02-13 15:39:55 UTC", description: "Update install.sh from rustup script", pr_number: 16398, scopes: [], type: "chore", breaking_change: false, author: "Ben Johnson", files_count: 1, insertions_count: 255, deletions_count: 53},
		{sha: "3888f26e616291c2a5359b9b311c5305db27e141", date: "2023-02-13 15:54:20 UTC", description: "apply the field-level defaults to all ARC settings", pr_number: 16388, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "e2cb4de50baabdf2500ade643af9d706ac9407ad", date: "2023-02-13 16:09:08 UTC", description: "Clarify CODEOWNERS requirements and add additional owners", pr_number: 16404, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 83, deletions_count: 84},
		{sha: "9ffc11ec594dc28fe8e2e92db9ce0317f2713950", date: "2023-02-13 17:22:56 UTC", description: "Add @vectordotdev/ux-team CODEOWNERS", pr_number: 16407, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "7422f0521b18ba14b5d7272898ba4706461e61fa", date: "2023-02-13 17:52:17 UTC", description: "bump axum from 0.6.4 to 0.6.6", pr_number: 16401, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 5},
		{sha: "4d43147abf8dae915e5b10438aa731e0e185c53a", date: "2023-02-13 20:05:23 UTC", description: "Upload config schema as artifact", pr_number: 16409, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "4a1a2e95665d2cf7b9721a11b701ea300f5345e5", date: "2023-02-13 20:25:49 UTC", description: "Remove unused configuration", pr_number: 16411, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 0, deletions_count: 177},
		{sha: "c5268222ac4d903a9a0220a741efd4eb1a5d66fb", date: "2023-02-13 20:54:35 UTC", description: "annotate a bunch of types/fields with docs-specific metadata", pr_number: 16380, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 19, insertions_count: 69, deletions_count: 39},
		{sha: "9e1ab51192f582fc5f8c00533ca21d9e3925079a", date: "2023-02-13 22:46:32 UTC", description: "Add log namespace support to `datadog_logs` sink", pr_number: 15473, scopes: ["datadog_logs sink"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 99, deletions_count: 4},
		{sha: "74cc2de7fb8b61fa4ec97ac04e5dbbdfc783d1c2", date: "2023-02-14 00:51:48 UTC", description: "Use generated docs, unify region behavior", pr_number: 16405, scopes: ["sematext_logs", "sematext_metrics sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 72, deletions_count: 57},
		{sha: "6c6a3c0f4a8e71c88a769f36c1eaac9e7541cdf4", date: "2023-02-14 02:04:07 UTC", description: "Update transform descriptions", pr_number: 16414, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Lee Benson", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "e179e205379c0b32629254b28d805d5e58a2e6c3", date: "2023-02-14 14:21:37 UTC", description: "Use generated docs for configuration", pr_number: 16408, scopes: ["aws_cloudwatch_logs sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 48, deletions_count: 67},
		{sha: "350b57a6dbfe60327d7f0421c8db82f9c87f852a", date: "2023-02-14 14:23:44 UTC", description: "Remove strategy link from s3 source page", pr_number: 16417, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 10, deletions_count: 3},
		{sha: "bd61c69163883fec5585de1f6da8954b2ee82cac", date: "2023-02-14 15:26:01 UTC", description: "Use generated docs for configuration", pr_number: 16415, scopes: ["aws_cloudwatch_metrics sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 14, deletions_count: 29},
		{sha: "65959d41922b5ec4b04746165dc728fa71acfb93", date: "2023-02-14 16:02:54 UTC", description: "Mark advanced schema props", pr_number: 16420, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Lee Benson", files_count: 11, insertions_count: 49, deletions_count: 12},
		{sha: "71c871182b858628de5b9fe119ae08ca6b7ca01e", date: "2023-02-14 16:07:22 UTC", description: "bump rkyv from 0.7.39 to 0.7.40", pr_number: 16427, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "05ec702d62cc3fc23a81ef4a1ab1f8e622bdc909", date: "2023-02-14 17:30:09 UTC", description: "bump aws-smithy-http-tower from 0.53.1 to 0.54.1", pr_number: 16424, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 103, deletions_count: 54},
		{sha: "1aba8674267d2a85e345e5eab6204067979035d8", date: "2023-02-14 19:09:15 UTC", description: "bump nats from 0.23.1 to 0.24.0", pr_number: 16423, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "cf75bbeb030b281292896f6c00142297b151d033", date: "2023-02-14 19:10:36 UTC", description: "bump csv from 1.1.6 to 1.2.0", pr_number: 16422, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 17, deletions_count: 25},
		{sha: "b992e62c8ec54738ec165074e43e5e7b6a4dd75c", date: "2023-02-14 19:18:58 UTC", description: "bump test-case from 2.2.2 to 3.0.0", pr_number: 16421, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 19, deletions_count: 6},
		{sha: "fa724a78385bdd3523075cb9bcccfd972dbd6499", date: "2023-02-14 20:28:56 UTC", description: "Add `check rust` subcommand", pr_number: 16418, scopes: ["vdev"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 47, deletions_count: 2},
		{sha: "54b055165efd21fa6caac9ca95f90cd40d2a0173", date: "2023-02-14 20:06:34 UTC", description: "bump async-graphql from 5.0.5 to 5.0.6", pr_number: 16400, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 13, deletions_count: 28},
		{sha: "d3844c1e91876d328bef3fbd29417d33628dc991", date: "2023-02-14 22:24:54 UTC", description: "Use generated documentation", pr_number: 16433, scopes: ["aws_kinesis_firehose sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 7, deletions_count: 10},
		{sha: "e4f99460f40112be61b86d4872a5129c2020e0f9", date: "2023-02-14 22:25:44 UTC", description: "Use generated documentation", pr_number: 16434, scopes: ["aws_kinesis_streams sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 5, deletions_count: 18},
		{sha: "6cbc1a2a516a8ca6bc58494db0ab21caa02e4cdc", date: "2023-02-14 21:26:46 UTC", description: "Add support for adding features to `run` command", pr_number: 16437, scopes: ["vdev"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 13, deletions_count: 6},
		{sha: "fcf8f779fffff62d21a07795545655ffff2aee3b", date: "2023-02-14 21:16:16 UTC", description: "bump pulsar from 5.0.2 to 5.1.0", pr_number: 16425, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9314adadd325fdfd01dfe1b2ca3123ca4e5c5f21", date: "2023-02-14 20:17:30 UTC", description: "autogen docs", pr_number: 16419, scopes: ["azure_monitor_logs sink"], type: "docs", breaking_change: false, author: "David Huie", files_count: 3, insertions_count: 26, deletions_count: 46},
		{sha: "b6fdddca5997f82ee7fba5071b6ac7a7ea1bf780", date: "2023-02-14 23:19:34 UTC", description: "Update package verify OS versions", pr_number: 16440, scopes: [], type: "chore", breaking_change: false, author: "Ben Johnson", files_count: 1, insertions_count: 4, deletions_count: 1},
		{sha: "b81e34ae7744e10c45bde7824c5f8bf75d6b5529", date: "2023-02-15 17:18:05 UTC", description: "Add log namespacing support", pr_number: 16431, scopes: ["papertrail sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 7, deletions_count: 5},
		{sha: "2a877114a1d5b4466235c152f0c12362c9a0cc11", date: "2023-02-15 14:45:17 UTC", description: "Fix typedefs for operations that can short-circuit", pr_number: 16391, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 8, insertions_count: 223, deletions_count: 103},
		{sha: "e3f67e87c3f4764eb1c3fac37c6a57c53ba164d7", date: "2023-02-15 20:50:19 UTC", description: "Add Log Namespacing support", pr_number: 16451, scopes: ["pulsar sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 7, deletions_count: 4},
		{sha: "43a587bfb6a20a2cb69452c7dde2e598f837dd55", date: "2023-02-15 20:51:01 UTC", description: "Add log namespacing support ", pr_number: 16453, scopes: ["honeycomb sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 19, deletions_count: 4},
		{sha: "cbfdb56b792fc687d7d8b90e307801d4adaad5b9", date: "2023-02-15 22:39:19 UTC", description: "Add log namespacing support", pr_number: 16436, scopes: ["sematext_logs sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "55f746552ccd9fdae21bba266cf812a2064ab423", date: "2023-02-15 15:49:43 UTC", description: "bump async-graphql-warp from 5.0.5 to 5.0.6", pr_number: 16448, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 36, deletions_count: 4},
		{sha: "5bab78afd92e8ffb874c937638a45614ae69b24b", date: "2023-02-15 15:50:37 UTC", description: "bump once_cell from 1.17.0 to 1.17.1", pr_number: 16446, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "af03adb156109ab3989cef8ea9a1e1f7cb625a07", date: "2023-02-15 23:59:46 UTC", description: "Add log namespace support", pr_number: 16458, scopes: ["logdna sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 13, deletions_count: 4},
		{sha: "c4c03ca0c99f15eec4bbd211fee6d963eeefb418", date: "2023-02-15 19:42:05 UTC", description: "Add more examples for to_timestamp", pr_number: 16459, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 30, deletions_count: 2},
		{sha: "ba033047b72c27cde06ad8d88a6c68c33ac3189d", date: "2023-02-15 17:36:46 UTC", description: "autogen docs", pr_number: 16442, scopes: ["kafka sink"], type: "docs", breaking_change: false, author: "David Huie", files_count: 5, insertions_count: 104, deletions_count: 129},
		{sha: "3c66a3b42165697377feaf1739554576e717cd3f", date: "2023-02-15 17:43:52 UTC", description: "add example for producer_name", pr_number: 16461, scopes: ["pulsar sink"], type: "docs", breaking_change: false, author: "David Huie", files_count: 2, insertions_count: 2, deletions_count: 1},
		{sha: "fa751e25e994c6493a5829dc0c16e2579ceaae71", date: "2023-02-15 21:42:43 UTC", description: "mark structs as `additionalProperties: false`", pr_number: 16464, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 8, insertions_count: 13, deletions_count: 18},
		{sha: "14e1173ce6a7f278b962fd33e8f5f9589b843f41", date: "2023-02-15 21:06:48 UTC", description: "Replace `Serialize` with new `ToValue` trait in config schemas", pr_number: 16460, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 21, insertions_count: 349, deletions_count: 115},
		{sha: "b7ec52ab71869efc3b769ce9bd4aa9def97d3949", date: "2023-02-16 14:33:09 UTC", description: "Add log namespace support to `log_to_metric` transform", pr_number: 15526, scopes: ["log_to_metric transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 2, deletions_count: 4},
		{sha: "45d760cb14d928c209fbad573eac7ffda499cd35", date: "2023-02-16 22:31:10 UTC", description: "add namespaced timestamp integration test", pr_number: 16473, scopes: ["loki sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 83, deletions_count: 9},
		{sha: "87c8e7b24106d759d4766c36e5c3989372186e6b", date: "2023-02-16 16:02:55 UTC", description: "bump inherent from 1.0.3 to 1.0.4", pr_number: 16471, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "8cb86a2f9c743448282dab8744da5713f0967ec2", date: "2023-02-16 16:03:20 UTC", description: "bump clap_complete from 4.1.1 to 4.1.2", pr_number: 16470, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "09f1fb6226adb48e3dea5061dcd134f3989787bc", date: "2023-02-16 16:03:56 UTC", description: "bump clap from 4.1.4 to 4.1.6", pr_number: 16469, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 16, deletions_count: 16},
		{sha: "9c7a1059d8e1d6218f8bdac37d608bbbc35c885d", date: "2023-02-16 18:46:01 UTC", description: "Add missing check_proc function to install script", pr_number: 16476, scopes: ["distribution"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 9, deletions_count: 1},
		{sha: "6c56b2584154e00a2eaa969c1b640fe8a146e098", date: "2023-02-16 19:29:11 UTC", description: "Remove unused test-harness workflow", pr_number: 16477, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 0, deletions_count: 146},
		{sha: "b4cfba1b6cfcc89a986a28d11b73b009fea90247", date: "2023-02-16 20:29:43 UTC", description: "add metadata support to semantic meaning", pr_number: 16455, scopes: ["core"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 26, insertions_count: 451, deletions_count: 160},
		{sha: "fc94ce63463176f7d923e13197bea0b1c9f1b40e", date: "2023-02-16 18:53:32 UTC", description: "Add clarifying example for GCP Storage key_prefix usage", pr_number: 16478, scopes: ["external docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 27, deletions_count: 0},
		{sha: "0c9a16047d9a409883c2b76d41c02c74d206da44", date: "2023-02-16 22:39:04 UTC", description: "`doc::label` meta for components, updated descriptions", pr_number: 16467, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Lee Benson", files_count: 3, insertions_count: 197, deletions_count: 91},
		{sha: "b843042aa3e9acf893dd5e436c32120f50b649fa", date: "2023-02-16 22:39:43 UTC", description: "Use generated documentation", pr_number: 16481, scopes: ["aws_sqs sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 13, deletions_count: 47},
		{sha: "d563cd049328296dffa4b724b2effadc4bd10751", date: "2023-02-16 22:51:37 UTC", description: "Use generated documentation", pr_number: 16480, scopes: ["aws_s3 sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 176, deletions_count: 270},
		{sha: "7be94e4fe289d7d98826a5d6bf4da0ac78c9a2e4", date: "2023-02-17 14:46:05 UTC", description: "Add log namespace support", pr_number: 16483, scopes: ["aws_cloudwatch_logs sink"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 12, deletions_count: 9},
		{sha: "85f2b1cc14f996ead65d29496d7e9b68eab2d9bb", date: "2023-02-17 20:02:18 UTC", description: "add `#[deny(missing_docs)]` to root", pr_number: 16429, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 48, insertions_count: 82, deletions_count: 3},
		{sha: "ef4c5c07a0c24f2ff46defd2997f3908fe9322ac", date: "2023-02-17 20:03:05 UTC", description: "Add log namespace support to GCP sinks", pr_number: 16403, scopes: ["gcp_chronicle sink", "gcp_stackdriver_logs sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 16, deletions_count: 6},
		{sha: "faa96f80224e5b92578d9bf82ba9c7845bb46d84", date: "2023-02-17 21:01:45 UTC", description: "remove semantic encoder for the Datadog Logs sink", pr_number: 16496, scopes: ["datadog_logs sink"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 23, deletions_count: 168},
		{sha: "59ab0daacf8ed687c90a1cd2146d9f50e7ab06b0", date: "2023-02-17 20:06:24 UTC", description: "Store default value in a box", pr_number: 16487, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 27, deletions_count: 28},
		{sha: "0d084c2f26f992ac77ab14ab646e05cdba20c765", date: "2023-02-17 19:28:25 UTC", description: "upgrade AWS crates to v0.54.1", pr_number: 16443, scopes: ["deps"], type: "chore", breaking_change: false, author: "neuronull", files_count: 21, insertions_count: 262, deletions_count: 236},
		{sha: "b443913a9efe5ec7c26404d1b6e5d84839332c2d", date: "2023-02-17 21:53:28 UTC", description: "Drop generic bound on config `Metadata`", pr_number: 16497, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 17, insertions_count: 95, deletions_count: 112},
		{sha: "927e44beab709a0d260552d183c323c1c09767a4", date: "2023-02-17 23:21:00 UTC", description: "Use log namespacing and semantic meaning", pr_number: 16499, scopes: ["datadog_events sink"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 19, deletions_count: 18},
		{sha: "3e88ed61c6ffea9dc5c7abb71a19e84ac6e172dc", date: "2023-02-17 20:43:28 UTC", description: "test file_to_blackhole egress throughput", pr_number: 16468, scopes: ["ci"], type: "chore", breaking_change: false, author: "Geoffrey Oxberry", files_count: 3, insertions_count: 8, deletions_count: 2},
		{sha: "f544eaa5f948b8513957b38c482ba845552a55a8", date: "2023-02-17 22:16:45 UTC", description: "adjust file_to_blackhole Vector config", pr_number: 16503, scopes: ["ci"], type: "fix", breaking_change: false, author: "Geoffrey Oxberry", files_count: 1, insertions_count: 5, deletions_count: 3},
		{sha: "a9eae44ce35bb46a49937d3c8325721c2730486d", date: "2023-02-20 16:34:44 UTC", description: "bump aws-smithy-http-tower from 0.54.1 to 0.54.3", pr_number: 16445, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 9, deletions_count: 9},
		{sha: "a293800ac4d617dc88e9969f18485c3341cad3f8", date: "2023-02-20 16:37:58 UTC", description: "bump aws-smithy-async from 0.53.1 to 0.54.3", pr_number: 16447, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2247777cd0222829151e943dd234b817aad3dd35", date: "2023-02-20 16:45:35 UTC", description: "bump async-stream from 0.3.3 to 0.3.4", pr_number: 16511, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 8, deletions_count: 7},
		{sha: "745ccdb2c92d11e3651276e2b3d58239c9a7120a", date: "2023-02-20 16:46:25 UTC", description: "bump memmap2 from 0.5.8 to 0.5.9", pr_number: 16512, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1f0530cd3e54ceac7d01f1f8957a369ed2ae7483", date: "2023-02-20 16:47:08 UTC", description: "bump axum from 0.6.6 to 0.6.7", pr_number: 16515, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9e130fcfd52aa836af21e3f88f70bade0911a04b", date: "2023-02-20 16:48:46 UTC", description: "bump num_enum from 0.5.9 to 0.5.10", pr_number: 16516, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "4d5d281e5a76caacfbc539cdb7b8e487939e1a11", date: "2023-02-20 18:28:56 UTC", description: "bump aws-smithy-client from 0.54.2 to 0.54.3", pr_number: 16514, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "2b28408eae2b3c43a9e88d701e57fc50cd25fad7", date: "2023-02-20 18:30:30 UTC", description: "bump rustyline from 10.1.1 to 11.0.0", pr_number: 16517, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 30},
		{sha: "7cd20be2f1fa459817f2f49c133d46e9f863f2c3", date: "2023-02-21 09:49:02 UTC", description: "Fixes GcpSeries serialization format without breaking config.", pr_number: 16394, scopes: ["gcp_stackdriver_metrics"], type: "fix", breaking_change: false, author: "Jason Hills", files_count: 2, insertions_count: 71, deletions_count: 4},
		{sha: "4fb3c647c024da0e136ff2587180ff03361af747", date: "2023-02-21 17:38:12 UTC", description: "bump http from 0.2.8 to 0.2.9", pr_number: 16510, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "203c08c3eb6d132dd1778e03a343e5d8604ea020", date: "2023-02-21 18:42:15 UTC", description: "bump tokio-stream from 0.1.11 to 0.1.12", pr_number: 16522, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "c8870af7195b4875f95a487ebee0c3e9f00522ae", date: "2023-02-21 22:49:00 UTC", description: "add encode_zstd and decode_zstd functions", pr_number: 16060, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 10, insertions_count: 314, deletions_count: 3},
		{sha: "8b8ee04bc7e28d6da3edc74a6340bb54075d6e80", date: "2023-02-21 17:36:00 UTC", description: "revert bump rustyline from 10.1.1 to 11.0.0", pr_number: 16530, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 30, deletions_count: 7},
		{sha: "add750972be1ee55d484b249a69dfa67e4edf516", date: "2023-02-21 19:57:30 UTC", description: "Regererate Cargo.lock", pr_number: 16528, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5d7d46d9dc2bd9ceb396536d44ec1d8a137436e5", date: "2023-02-21 20:42:36 UTC", description: "remove the sink", pr_number: 16533, scopes: ["apex sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 18, insertions_count: 9, deletions_count: 623},
		{sha: "6c7200e69bdfa86761942025b5a872b48f059277", date: "2023-02-21 23:16:35 UTC", description: "Hardcode timestamp-field header to @timestamp", pr_number: 16536, scopes: ["axiom sink"], type: "feat", breaking_change: true, author: "Spencer Gilbert", files_count: 2, insertions_count: 9, deletions_count: 9},
		{sha: "859b27ae2df87c1754ffd15273a93db7e675e932", date: "2023-02-21 23:17:08 UTC", description: "Use log namespacing and semantic meaning", pr_number: 16532, scopes: ["elasticsearch sink"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 16, deletions_count: 10},
		{sha: "8a1035b53567ce38ece2137737dc412f0fe1fd36", date: "2023-02-22 18:37:01 UTC", description: "bump bstr from 1.2.0 to 1.3.0", pr_number: 16521, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "9fa1886274ddcab82a071693afec630c2f0f3b57", date: "2023-02-22 19:51:04 UTC", description: "correct storage for event token", pr_number: 16520, scopes: ["splunk_hec_logs sink"], type: "docs", breaking_change: false, author: "Harald Gutmann", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "18d4683470acccdfa08ef1052b97e2bd618e73fb", date: "2023-02-22 14:37:18 UTC", description: "Fix typo in 0.28 upgrade guide", pr_number: 16545, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e939bf1ba38a9791753199a0c952535d81ccf18a", date: "2023-02-22 13:53:18 UTC", description: "Move the `SchemaGenerator` into a `RefCell`", pr_number: 16538, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 16, insertions_count: 92, deletions_count: 56},
		{sha: "91ef7172c335cfc9eb1687725ec576d4d01b3508", date: "2023-02-22 16:18:04 UTC", description: "Fix `LogEvent::source_type_path`", pr_number: 16541, scopes: ["core"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 4, deletions_count: 6},
		{sha: "4be5dd7176188b53d1ea845fd5f872df23bbe199", date: "2023-02-22 21:53:26 UTC", description: "generate component docs for splunk sink", pr_number: 16547, scopes: ["splunk_hec sink"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "547257b96073d308ec511dbc403d2e72e35fb0e5", date: "2023-02-23 02:46:03 UTC", description: "add consumer_lag metrics to Kafka", pr_number: 15106, scopes: ["kafka"], type: "feat", breaking_change: false, author: "Alexander Zaitsev", files_count: 7, insertions_count: 66, deletions_count: 6},
		{sha: "848c36cdef07f2d2aef6c8b5fa7cc12f37b0842d", date: "2023-02-22 19:19:41 UTC", description: "vendor `schemars` and slim it down", pr_number: 16540, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 28, insertions_count: 865, deletions_count: 77},
		{sha: "e667e9da1fe26c5673395e66e5a5877a1accd4d4", date: "2023-02-22 19:21:52 UTC", description: "Use log namespacing and semantic meaning", pr_number: 16537, scopes: ["kafka sink"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 8, deletions_count: 10},
		{sha: "03b500bc55b1e303ddf672600226d83501c8b6ff", date: "2023-02-22 19:39:59 UTC", description: "Drop `const NAME` from `trait NamedComponent`", pr_number: 16550, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 56, insertions_count: 55, deletions_count: 75},
		{sha: "06553b063a43e2f48736daaf39c4346fd01718bc", date: "2023-02-22 18:47:28 UTC", description: "extract common config struct attributes to shared struct", pr_number: 16280, scopes: ["datadog sinks"], type: "enhancement", breaking_change: false, author: "neuronull", files_count: 15, insertions_count: 230, deletions_count: 324},
		{sha: "edb6be03c8366b5ba63185ec13135247380c0810", date: "2023-02-22 21:03:39 UTC", description: "Include AXIOM_TOKEN in workflow env", pr_number: 16552, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "3ab2fed51655c6b93f9ef8b3e4bb37f808f0f39d", date: "2023-02-22 21:51:25 UTC", description: "Upgrade to cross 0.2.5", pr_number: 16527, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 9, insertions_count: 10, deletions_count: 10},
		{sha: "121d06a8c0e8ab47dcde4a3e675e20e80a2a50e7", date: "2023-02-22 21:17:19 UTC", description: "Drop unused `ToValue` trait bound", pr_number: 16554, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 9, deletions_count: 9},
		{sha: "eab2c1c08812262abd6c3124833134a145d792e7", date: "2023-02-22 22:03:07 UTC", description: "Drop `build-ci-docker-images` make rule", pr_number: 16553, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 0, deletions_count: 4},
		{sha: "e8f4c468a0c476627b897f763d15017576af222d", date: "2023-02-23 18:00:54 UTC", description: "bump syn from 1.0.107 to 1.0.108", pr_number: 16558, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 72, deletions_count: 72},
		{sha: "fbe834f5789404c0461c6e2593c1c4a377daefb9", date: "2023-02-23 18:01:24 UTC", description: "bump memmap2 from 0.5.9 to 0.5.10", pr_number: 16559, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "84155fb14956510d37b37ec9fc567e202f5fdd19", date: "2023-02-23 20:31:36 UTC", description: "fix high cpu on redis intermittent connection issues", pr_number: 16518, scopes: ["redis source"], type: "fix", breaking_change: false, author: "Harald Gutmann", files_count: 1, insertions_count: 23, deletions_count: 3},
		{sha: "e59246e4078a5195f0270d631e2f953bff8a606f", date: "2023-02-23 15:37:52 UTC", description: "Add issue template for Vector minor releases", pr_number: 16563, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 60, deletions_count: 0},
		{sha: "49a33ea3aed9cf4a4418dfbe0a4944ed78748210", date: "2023-02-23 15:42:16 UTC", description: "Correct label in issue template", pr_number: 16566, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9ab2701b6ec38e9d2d26aea23b514386206db5f0", date: "2023-02-24 16:13:52 UTC", description: "revert make consumer group instance id configurable", pr_number: 16580, scopes: ["kafka"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 0, deletions_count: 13},
		{sha: "d4f234700e0e279f74bce4bd70c7194fdddfc2e9", date: "2023-02-24 20:33:53 UTC", description: "Rename batch size option", pr_number: 16582, scopes: ["pulsar sink"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 5, deletions_count: 5},
	]
}
