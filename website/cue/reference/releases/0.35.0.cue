package metadata

releases: "0.35.0": {
	date:     "2024-01-08"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.35.0!

		Be sure to check out the [upgrade guide](/highlights/2023-12-19-0-35-0-upgrade-guide) for
		breaking changes in this release.

		In addition to the usual enhancements and bug fixes, this release also includes

		- The ability to use VRL to specify inputs for [unit
		  tests](/docs/reference/configuration/unit-tests)
		- A new `avro` decoder that can used to decode [AVRO](https://avro.apache.org/) data in
		  sources

		This release is also the first release only published to the new `apt.vector.dev` and
		`yum.vector.dev` OS package repositories and not to the deprecated `repositories.timber.io`.
		A reminder that the `repositories.timber.io` package repositories will be decommissioned on
		February 28th, 2024. Please see the [release
		highlight](/highlights/2023-11-07-new-linux-repos) for details about this change and
		instructions on how to migrate.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["kafka source", "kafka sink"]
			description: """
				The `kafka` source and sink now add component tags to published Kafka consumer and
				producer metrics.
				"""
			pr_numbers: [19082]
		},
		{
			type: "feat"
			scopes: ["aws_cloudwatch_logs sink"]
			description: """
				The `aws_cloudwatch_logs` sink now allows for the log group retention to be
				configured for any log groups created by Vector via the new `retention` options.
				"""
			contributors: ["AndrewChubatiuk"]
			pr_numbers: [18865]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				The `heroku_logs`, `http_server`, `prometheus_remote_write`, and `splunk_hec`
				sources now correctly report decompressed bytes, rather than compressed bytes, for
				the `component_received_bytes_total` internal metric.
				"""
			pr_numbers: [19048]
		},
		{
			type: "fix"
			scopes: ["elasticsearch sink"]
			description: """
				Memory use by the `elasticsearch` sink was improved through reduced buffering.
				"""
			pr_numbers: [18699]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				The `appsignal`, `datadog_metrics`, `greptimedb`, `gcp_stackdriver`, `honeycomb`,
				and `http` sinks now correctly report uncompressed bytes, rather than compressed
				bytes, for the `component_sent_bytes_total` internal metric.
				"""
			pr_numbers: [19060]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				A new component-level internal metric, `buffer_send_duration_max_seconds`, was added
				to measure the time that a component spends waiting to push events to downstream
				components. This is a useful metric to use to identify back pressure in your topology.
				"""
			pr_numbers: [19022]
		},
		{
			type: "enhancement"
			scopes: ["observability", "throttle transform"]
			breaking: true
			description: """
				For the `throttle` transform, make the `key` tag added to `events_discarded_total`
				opt-in. This key can be of unbounded cardinality so should only be opted in if you
				are confident the cardinality is bounded to avoid runaway memory growth.

				See [upgrade
				guide](/highlights/2023-12-19-0-35-0-upgrade-guide#events-discarded-total-opt-in)
				for details.
				"""
			pr_numbers: [19083]
		},
		{
			type: "enhancement"
			scopes: ["observability", "throttle transform"]
			breaking: true
			description: """
				File-based components (`file` source,  `kubernetes_logs` source, `file` sink) now
				include a `internal_metrics.include_file_tag` config option that determines whether
				the `file` tag is included on the component's corresponding internal metrics. This
				config option defaults to `false`, as this `tag` is likely to be of high
				cardinality.

				See [upgrade
				guide](/highlights/2023-12-19-0-35-0-upgrade-guide#file-tag-opt-in)
				for details.
				"""
			pr_numbers: [19145]
		},
		{
			type: "enhancement"
			scopes: ["file sink", "aws_s3 sink", "gcp_cloud_storage sink"]
			description: """
				The `file`, `aws_s3`, and `gcp_cloud_storage` sink now use the configured timezone
				when templating out timestamps as part of creating object key names. It will use the
				globally configured `timezone` option or the newly added `timezone` option on each
				of these sinks. Previously it always used UTC when templating timestamps.
				"""
			contributors: ["kates"]
			pr_numbers: [18506]
		},
		{
			type: "enhancement"
			scopes: ["networking", "sinks"]
			description: """
				Sinks with retries now add jitter to the retries to spread out retries. This
				behavior can be disabled by setting `request.retry_jitter_mode` to `none`.
				"""
			pr_numbers: [19106]
		},
		{
			type: "enhancement"
			scopes: ["networking", "sinks"]
			description: """
				Sink request behavior was improved by:

				- Capping the retry duration at 30 seconds by default for faster recovery when
				  downstream services recover, rather than the previous default of an hour. This can
				  be configured via `request.retry_max_duration_secs`
				- Ensuring defaults are correctly applied as documented
				- Adding a `request.max_concurrency_limit` that can be used to cap the maximum
				  number of concurrent requests when adaptive request concurrency is in-use
				"""
			pr_numbers: [19101]
		},
		{
			type: "enhancement"
			scopes: ["http source"]
			description: """
				HTTP server-based sources include a new `keepalive.max_connection_age_secs`
				configuration option, which defaults to 5 minutes (300 seconds). When enabled, this
				closes incoming TCP connections that reach the maximum age by sending a `Connection:
				close` header in the response. While this parameter is crucial for managing the
				lifespan of persistent, incoming connections to Vector and for effective load
				balancing, it can be disabled by setting `keepalive.max_connection_age_secs` to
				 a large number like `100000000`.
				"""
			pr_numbers: [19141]
		},
		{
			type: "feat"
			scopes: ["log_to_metric transform"]
			description: """
				The `log_to_metric` now has the ability to convert logs that have the same structure
				as metrics directly into metrics rather than only deriving metrics from logs. This
				"mode" can be enabled by setting the `all_metrics` configuration option. Incoming
				metrics should match the structure described by the [native
				codec](https://github.com/vectordotdev/vector/blob/aa6fd40ae9fda3279cbfd4f4ec3bdbb7debde691/lib/codecs/tests/data/native_encoding/schema.cue).
				"""
			contributors: ["dygfloyd"]
			pr_numbers: [19160]
		},
		{
			type: "feat"
			scopes: ["unit test"]
			description: """
				Vector configuration [unit tests](/docs/reference/configuration/unit-tests) now have the
				ability to use VRL to specify the input to each test case rather than needing to
				specify the input as structure directly in the configuration file (via
				`log_fields`). See [unit tests](/docs/reference/configuration/unit-tests) for
				details.
				"""
			contributors: ["MichaHoffmann"]
			pr_numbers: [19107]
		},
		{
			type: "fix"
			scopes: ["kafka source", "kafka sink"]
			description: """
				The `kafka` source and sink now correctly propagate the component-level
				`tls.verify_certificate` setting. Previously this was always set to `true`.
				"""
			contributors: ["zjj"]
			pr_numbers: [19117]
		},
		{
			type: "enhancement"
			scopes: ["splunk_hec_logs sink", "splunk_hec_metrics sink", "humio sink"]
			description: """
				The `splunk_hec_logs`, `splunk_hec_metrics`, and `humio` sinks now allow accessing
				event metadata when specifying `host_key` and `timestamp_key` when [log
				namespacing](https://vector.dev/blog/log-namespacing/) is enabled.
				"""
			contributors: ["sbalmos"]
			pr_numbers: [19086]
		},
		{
			type: "fix"
			scopes: ["cli", "performance"]
			description: """
				`vector tap` now performs better by not recompiling glob matches on each fetch interval.
				"""
			contributors: ["aholmberg"]
			pr_numbers: [19356]
		},
		{
			type: "fix"
			scopes: ["tag_cardinality_limit transform"]
			description: """
				The `tag_cardinality_limit` transform has improved performance in `probabilistic`
				mode via caching the count of entries in the bloom filter.
				"""
			pr_numbers: [19281]
		},
		{
			type: "fix"
			scopes: ["remap transform"]
			description: """
				The `remap` transform no longer emits errors or increments
				`component_discarded_events_total` when `reroute_dropped` is true and events error
				during processing as the events are not actually dropped, but instead routed to the
				`dropped` output.
				"""
			pr_numbers: [19296]
		},
		{
			type: "fix"
			scopes: ["file source"]
			description: """
				The `file` source now emits logs with the correct `offset` field when aggregating
				multiline events.
				"""
			contributors: ["jches"]
			pr_numbers: [19065]
		},
		{
			type: "enhancement"
			scopes: ["http_server source"]
			description: """
				The `http_server` source now allows a glob wildcard to be used when specifying the
				headers to capture to use as fields to received events. For example, setting
				`headers` to `["X-*"]` will capture all headers starting with `X-` and add them as
				fields on the event (or in the metadata when [log
				namespacing](https://vector.dev/blog/log-namespacing/) is enabled).
				"""
			contributors: ["sonnens"]
			pr_numbers: [18922]
		},
		{
			type: "enhancement"
			scopes: ["datadog provider"]
			description: """
				The `datadog_logs`,`datadog_metrics`, and `datadog_traces` sinks now default the
				values of the `default_api_key` and `site` configuration options to the values of
				environment variables `DD_API_KEY` and `DD_SITE`, respectively.
				"""
			pr_numbers: [18929]
		},
		{
			type: "enhancement"
			scopes: ["performance"]
			description: """
				The `jemalloc` memory allocator, which Vector uses on Linux systems, is now also
				used by any native dependencies, like `librdkafka`, on Linux systems as well. This
				results in improved memory use by, for example, the `kafka` source and sink.
				"""
			contributors: ["Ilmarii"]
			pr_numbers: [19340]
		},
		{
			type: "fix"
			scopes: ["aws_kinesis_firehose sink"]
			description: """
				The `aws_kinesis_firehose` sink now has a `partition_key_field` that can be used to
				configure a log event field to use as the Kinesis partition key. By default, Kinesis
				will use a unique identifier.
				"""
			contributors: ["gromnsk"]
			pr_numbers: [19108]
		},
		{
			type: "fix"
			scopes: ["security", "remap transform"]
			description: """
				The `remap` transform now filters out the source contents from error messages when
				the VRL program is read from a `file`. This removes the ability to use Vector to
				execute an attack to read files that the user wouldn't otherwise have permissions to
				(e.g. `/etc/passwd`).
				"""
			pr_numbers: [19356]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				Running Vector with `-v` and `-vv` to output `debug` and `trace` logs, respectively,
				or `-q` and `-qq` to output `warn` and `fatal` logs, respectively, now behaves the
				same as setting `VECTOR_LOG` to `debug`, `trace`, `warn`, and `fatal`, respectively.
				Previously the CLI flags would only apply to some of Vector's internal modules and
				dependencies unlike `VECTOR_LOG` which applied to everything.
				"""
			pr_numbers: [19359]
		},
		{
			type: "feat"
			scopes: ["vrl"]
			description: """
				VRL was updated to 0.9.1. This includes the following changes:

				- `parse_regex_all` pattern parameter can now be resolved from a variable
				- fixed `parse_json` data corruption issue for numbers greater or equal to `i64::MAX`
				- support timestamp comparison using operators <, <=, >, >=
				"""
			pr_numbers: [19368]
		},
		{
			type: "feat"
			scopes: ["codecs"]
			description: """
				Support for decoding AVRO data in sources was added via a new codec configurable by
				setting `decoding.codec` to `avro` on components that support it. Additional
				AVRO-specific codec options are configurable via `decoding.avro`.
				"""
			contributors: ["Ion-manden"]
			pr_numbers: [19342]
		},
		{
			type: "chore"
			scopes: ["observability"]
			breaking: true
			description: """
				The `requests_completed_total`, `request_duration_seconds`, and
				`requests_received_total` internal metrics were removed in the 0.35.0 release.


				See [upgrade
				guide](/highlights/2023-12-19-0-35-0-upgrade-guide#remove-obsolete-http-metrics)
				for details.
				"""
			pr_numbers: [19447]
		},
		{
			type: "chore"
			scopes: ["cli", "config"]
			description: """
				Vector now has the ability to turn all undefined variable warnings into errors by
				using the `--strict-env-vars` flag (or `VECTOR_STRICT_ENV_VARS` environment
				variable) when running Vector. If any environment variables that are used in
				[configurations](/docs/reference/configuration/#environment-variables) are
				undefined, Vector will raise an error rather than a silent warning.

				In a future release, this will "strict environment variable" mode will be the
				default. This release deprecates the current behavior of only outputting a warning
				for undefined variables.

				See [upgrade guide](/highlights/2023-12-19-0-35-0-upgrade-guide#strict-env-vars) for
				details.
				"""
			pr_numbers: [19393]
		},
	]

	commits: [
		{sha: "2913cfea3f279cb12f66465ad7003cf3cf9770b7", date: "2023-11-08 02:34:40 UTC", description: "Bump toml from 0.8.6 to 0.8.8", pr_number: 19070, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 25, deletions_count: 14},
		{sha: "49a27375b50e7a97d3dcb114dc1074a7f9ca3d5e", date: "2023-11-08 02:35:26 UTC", description: "Bump serde from 1.0.190 to 1.0.192", pr_number: 19071, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "1baadd6a6f77e58cac0d31df86aa328a4cb206c7", date: "2023-11-08 04:41:59 UTC", description: "Bump async-graphql from 6.0.9 to 6.0.10", pr_number: 19053, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 10},
		{sha: "b68362c9462a2c032a8319173f6e579b75757961", date: "2023-11-08 02:17:55 UTC", description: "rustc warnings", pr_number: 19075, scopes: ["config"], type: "fix", breaking_change: false, author: "Aaron Andersen", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "da06f2a308e375a2dc4f4bef5b711954220ba3ce", date: "2023-11-08 02:45:22 UTC", description: "Bump Vector to v0.35.0", pr_number: 19077, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "71fe94c1998812fee04d4ba7a147127c5a693a1b", date: "2023-11-08 10:15:42 UTC", description: "Small fix to function call", pr_number: 19079, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "70669785a817070966efc6e71a20ed222326c2d7", date: "2023-11-09 02:41:50 UTC", description: "Bump async-nats from 0.32.1 to 0.33.0", pr_number: 19091, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "db66247ed48f760807a9e0c23fd1d45105b578eb", date: "2023-11-09 02:45:20 UTC", description: "Bump rdkafka from 0.34.0 to 0.35.0", pr_number: 19090, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "e9ec7835a55678631cf4ba73d45aae5bf766edff", date: "2023-11-09 02:45:53 UTC", description: "Bump getrandom from 0.2.10 to 0.2.11", pr_number: 19089, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "3902b2365c0b1660307e3032a5cc4538ca78bd52", date: "2023-11-09 02:46:39 UTC", description: "Bump async-graphql-warp from 6.0.9 to 6.0.10", pr_number: 19088, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2a0404adc35517b3b002e736cb3046557c1e0c02", date: "2023-11-09 02:42:23 UTC", description: "Simplify a few tiny import issues in `vector-lib`", pr_number: 19066, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 11, insertions_count: 16, deletions_count: 24},
		{sha: "515ce43d6562f896a4ed3e710ea77e95a5f2ea91", date: "2023-11-09 07:29:17 UTC", description: "propagate span for internal events", pr_number: 19082, scopes: ["kafka source", "kafka sink"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 3, insertions_count: 17, deletions_count: 3},
		{sha: "9733dd639fdaccbbf4df16715a3f03b61d3df5f5", date: "2023-11-09 08:22:02 UTC", description: "Update VRL to use `KeyString` type wrapper", pr_number: 19069, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 81, insertions_count: 719, deletions_count: 749},
		{sha: "ec31d03d1b5630a49a4852cf66a747a4b14db75c", date: "2023-11-09 06:30:23 UTC", description: "Replace trust_dns_proto with hickory_proto", pr_number: 19095, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 8, insertions_count: 45, deletions_count: 44},
		{sha: "10b3ae7c0491d59b024b8ddfdbc3013d3ce663ba", date: "2023-11-10 06:57:46 UTC", description: "make `file` internal metric tag opt-out", pr_number: 19084, scopes: ["file source", "kubernetes_logs source", "file sink"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 10, insertions_count: 315, deletions_count: 81},
		{sha: "90e5044c7c073545b37c573ce715b7e0544095cc", date: "2023-11-11 05:00:36 UTC", description: "Bump smallvec from 1.11.1 to 1.11.2", pr_number: 19113, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "c4ed54d216c5cefbbf60ccefe4a14f13c44abac4", date: "2023-11-11 05:01:13 UTC", description: "Bump bstr from 1.7.0 to 1.8.0", pr_number: 19111, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "473b720876bc0886ea2518d86b4a05a6d9bd43c7", date: "2023-11-11 05:06:00 UTC", description: "Bump tokio from 1.33.0 to 1.34.0", pr_number: 19112, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 15, deletions_count: 15},
		{sha: "104984f0718ac48725aa0c93356fc07ccb197f54", date: "2023-11-11 07:41:28 UTC", description: "add configurable log retention", pr_number: 18865, scopes: ["aws_cloudwatch_logs sink"], type: "feat", breaking_change: false, author: "Andrii Chubatiuk", files_count: 6, insertions_count: 132, deletions_count: 8},
		{sha: "b0c09e1ce16e9afaa1a13a4ba249c90d7c5453df", date: "2023-11-11 09:52:55 UTC", description: "List checks to be run prior to submitting a PR in `CONTRIBUTING.md`", pr_number: 19118, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 21, deletions_count: 0},
		{sha: "40305f164fb4e8f6976b63d6723c50d67d6b1af6", date: "2023-11-11 06:30:32 UTC", description: "always emit HttpBytesReceived after decompression", pr_number: 19048, scopes: ["sources"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 6, insertions_count: 39, deletions_count: 29},
		{sha: "dcd994201a6e91a8af3daf2d5e129c64c19dd062", date: "2023-11-11 08:33:46 UTC", description: "set fixed buffer size for distributed service", pr_number: 18699, scopes: ["sinks"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 5, insertions_count: 67, deletions_count: 2},
		{sha: "ca64f310c18d9636edc78644df2797296fdf4b0a", date: "2023-11-11 10:43:55 UTC", description: "use uncompressed body size for bytes sent", pr_number: 19060, scopes: ["sinks"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 6, insertions_count: 11, deletions_count: 6},
		{sha: "c68b2b83b181b16cf467f6d40ca471f932d4ed10", date: "2023-11-14 04:15:56 UTC", description: "add buffer `buffer_send_duration_seconds` metric", pr_number: 19022, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Doug Smith", files_count: 21, insertions_count: 283, deletions_count: 130},
		{sha: "678605ce32005f37e908803606986251f8c7adb7", date: "2023-11-14 04:26:29 UTC", description: "make `events_discarded_total` internal metric with `key` tag opt-in", pr_number: 19083, scopes: ["throttle transform"], type: "fix", breaking_change: true, author: "Doug Smith", files_count: 4, insertions_count: 81, deletions_count: 20},
		{sha: "8dcb7db9c03610802b59a90e973c86f19c985099", date: "2023-11-14 04:44:04 UTC", description: "fix playground vrl version and link", pr_number: 19119, scopes: ["playground"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 68, deletions_count: 29},
		{sha: "30295584f8382d4b194dca70616b060eeb7929ee", date: "2023-11-14 05:11:45 UTC", description: "Fix commenting step on workflow", pr_number: 19134, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 1, insertions_count: 5, deletions_count: 2},
		{sha: "6b53a067b570fd18736f1c96fc8bafc58d7afa2c", date: "2023-11-14 02:29:56 UTC", description: "Bump bufbuild/buf-setup-action from 1.27.2 to 1.28.0", pr_number: 19137, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4a61e3650d94853d271cd6a2dbcb936ad8101f75", date: "2023-11-14 02:49:32 UTC", description: "Bump the clap group with 1 update", pr_number: 19127, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 20, deletions_count: 20},
		{sha: "7c6c0d1e6a65f32abdb47fc3b43bcdcaf63f5b34", date: "2023-11-14 12:39:05 UTC", description: "Bump proptest from 1.3.1 to 1.4.0", pr_number: 19131, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 8, deletions_count: 8},
		{sha: "913c2ae9bc9a722516ced689d8b5a61963a1b221", date: "2023-11-14 12:49:24 UTC", description: "Bump env_logger from 0.10.0 to 0.10.1", pr_number: 19130, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "bd7df4a7b8ac25a874ec3d2cef394eaaf375628d", date: "2023-11-14 15:45:56 UTC", description: "Bump hdrhistogram from 7.5.2 to 7.5.3", pr_number: 19129, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5a5556717c37dd2e5c426ef6d48d1a269e440ba3", date: "2023-11-15 10:42:54 UTC", description: "configurable filename timezone", pr_number: 18506, scopes: ["file sink", "aws_s3 sink", "gcp_cloud_storage"], type: "enhancement", breaking_change: false, author: "Kates Gasis", files_count: 14, insertions_count: 204, deletions_count: 50},
		{sha: "c668defa04ab8f499fd24ff3ba1c7bf8932122d3", date: "2023-11-15 04:46:22 UTC", description: "add full jitter to retry backoff policy", pr_number: 19106, scopes: ["networking", "sinks"], type: "enhancement", breaking_change: false, author: "Doug Smith", files_count: 46, insertions_count: 1076, deletions_count: 63},
		{sha: "e75062257ffd277c5dedc65b652f5dda6860246d", date: "2023-11-16 06:07:44 UTC", description: "Bump async-compression from 0.4.4 to 0.4.5", pr_number: 19153, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c089b8f0c44999bc46322bcfcf0c97783702f051", date: "2023-11-16 07:29:23 UTC", description: "Bump itertools from 0.11.0 to 0.12.0", pr_number: 19152, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 16, deletions_count: 7},
		{sha: "fdf37425541ea19d95163501587c2ba2d61d6642", date: "2023-11-16 07:31:57 UTC", description: "Bump tracing-subscriber from 0.3.17 to 0.3.18", pr_number: 19144, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 11, deletions_count: 11},
		{sha: "98100712673b313c5bd435822a977f43dda0bde0", date: "2023-11-16 10:12:18 UTC", description: "Bump bufbuild/buf-setup-action from 1.28.0 to 1.28.1", pr_number: 19157, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "59e6d36862f13ff21902610e574fb2d83890ab47", date: "2023-11-16 07:50:53 UTC", description: "Add `keepalive.max_connection_age_secs` config option to HTTP-server sources", pr_number: 19141, scopes: ["sources"], type: "enhancement", breaking_change: false, author: "Doug Smith", files_count: 22, insertions_count: 723, deletions_count: 13},
		{sha: "a6399284fb944e22b8d3174988c56f4d2a1df94c", date: "2023-11-16 07:13:46 UTC", description: "Bump syslog_loose from 0.19.0 to 0.21.0", pr_number: 19143, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 3},
		{sha: "dcb40f62d0a8c28244b5fa259bbf130b5fe019de", date: "2023-11-17 00:03:05 UTC", description: "Bump opendal from 0.41.0 to 0.42.0", pr_number: 19169, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 6},
		{sha: "aa2d3603bc0784279e408e3907ffe94851727b6d", date: "2023-11-17 06:03:38 UTC", description: "Bump manifests to v0.29.0 of Helm chart", pr_number: 19178, scopes: ["releasing", "kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "5e0d64136ecad49668f904bcd8c218a0cdc205ee", date: "2023-11-18 00:54:19 UTC", description: "Bump the prost group with 3 updates", pr_number: 19180, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 35, deletions_count: 35},
		{sha: "9692ee0a5e5b393008d7273abbb485f3aa6052e7", date: "2023-11-18 06:55:37 UTC", description: "Bump nkeys from 0.3.2 to 0.4.0", pr_number: 19181, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 19, deletions_count: 3},
		{sha: "d391c43366de80224a07b04a7482e6e70c7ae233", date: "2023-11-18 03:32:13 UTC", description: "Bump docker/build-push-action from 5.0.0 to 5.1.0", pr_number: 19185, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "114715eaf5584d191e8a44688d40ff5fc6079147", date: "2023-11-18 06:29:11 UTC", description: "dynamically convert all logs to metrics", pr_number: 19160, scopes: ["log_to_metric"], type: "feat", breaking_change: false, author: "Steve Steward", files_count: 4, insertions_count: 1018, deletions_count: 104},
		{sha: "b595fb4c6b3f31e4738f6cec1c8021d6cdb5c79f", date: "2023-11-18 14:57:36 UTC", description: "add vrl as test input", pr_number: 19107, scopes: ["unit tests"], type: "feat", breaking_change: false, author: "Michael Hoffmann", files_count: 4, insertions_count: 71, deletions_count: 2},
		{sha: "d4189e06bd7f8efa1b0daf68f5e1a57eb5b489af", date: "2023-11-18 23:12:42 UTC", description: "fixed kafka tls config", pr_number: 19117, scopes: ["kafka source", "kafka sink", "auth"], type: "fix", breaking_change: false, author: "zjj", files_count: 1, insertions_count: 14, deletions_count: 0},
		{sha: "50eb79c5e5338f34a40a1e8891f1d3b81981ace7", date: "2023-11-21 00:19:40 UTC", description: "Bump hdrhistogram from 7.5.3 to 7.5.4", pr_number: 19194, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "3ca8c1cdaa567dfc6240be29eb13919b660ca0dd", date: "2023-11-21 06:20:19 UTC", description: "Bump uuid from 1.5.0 to 1.6.0", pr_number: 19195, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "5a5ea193d6e17fefee95571dc5732c5aa20d6f8d", date: "2023-11-21 10:12:38 UTC", description: "Bump actions/github-script from 6.4.1 to 7.0.1", pr_number: 19200, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "20ff4c8aee5048dcd50629e3acadb9a41e97998c", date: "2023-11-21 06:25:23 UTC", description: "Remove build files", pr_number: 19199, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 3, insertions_count: 0, deletions_count: 77},
		{sha: "2d1523fb3610feb910172648829bc5a8564e0ea7", date: "2023-11-21 08:39:22 UTC", description: "Bump async-graphql from 6.0.10 to 6.0.11", pr_number: 19196, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 10},
		{sha: "3d284a69dece45a1699770b7087584784bf18f30", date: "2023-11-21 23:04:37 UTC", description: "Bump serde from 1.0.192 to 1.0.193", pr_number: 19207, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "ef8c6023f99e7f49e06abc15bce85d1e152169c2", date: "2023-11-21 23:05:37 UTC", description: "Bump uuid from 1.6.0 to 1.6.1", pr_number: 19206, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "13021327b149b3f41bbdcab3bb2595fc05a57e41", date: "2023-11-22 05:07:05 UTC", description: "Bump async-graphql-warp from 6.0.10 to 6.0.11", pr_number: 19205, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "6ee00fa69a4ad372239434f5a715677af3c5f930", date: "2023-11-22 23:57:41 UTC", description: "Bump the prost group with 3 updates", pr_number: 19213, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 35, deletions_count: 35},
		{sha: "5e2e4a07856a74f508b37a923ce29925d198fbbd", date: "2023-11-23 00:04:53 UTC", description: "Bump data-encoding from 2.4.0 to 2.5.0", pr_number: 19216, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5a0966ad9ca4ad6a69b3a3b86cdfb92edd634c77", date: "2023-11-23 06:19:19 UTC", description: "Bump lru from 0.12.0 to 0.12.1", pr_number: 19215, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "24034c720845fe784a4ad807b52ea386fb58a992", date: "2023-11-24 03:51:49 UTC", description: "Update OpenTelemetry Protobuf Definitions to v1.0.0", pr_number: 19188, scopes: [], type: "chore", breaking_change: false, author: "Harold Dost", files_count: 14, insertions_count: 1235, deletions_count: 15},
		{sha: "d2d2ad0ad51d79bc9b193f2aef16efc53fd703cc", date: "2023-11-24 03:59:27 UTC", description: "Bump cargo_toml from 0.17.0 to 0.17.1", pr_number: 19225, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "86689c7f76d15a726bfc0141f5aebad26b2c796d", date: "2023-11-24 10:01:24 UTC", description: "Bump openssl from 0.10.59 to 0.10.60", pr_number: 19226, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "c8a0d41a765115b1c0270453485627b43bc5d39a", date: "2023-11-24 10:02:33 UTC", description: "Bump mlua from 0.9.1 to 0.9.2", pr_number: 19227, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 24, deletions_count: 11},
		{sha: "54c8c92ec2af7f177a77e355299e7983ac202483", date: "2023-11-24 10:03:10 UTC", description: "Bump percent-encoding from 2.3.0 to 2.3.1", pr_number: 19228, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d5f6d074fb550b9bbb8b4eae3a44c64f1353065b", date: "2023-11-24 07:43:32 UTC", description: "Bump h2 from 0.3.21 to 0.4.0", pr_number: 19168, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 92, deletions_count: 62},
		{sha: "5f0b2e87d7e1f096e32c2f383fb7cdbdea120668", date: "2023-11-25 00:39:00 UTC", description: "Bump url from 2.4.1 to 2.5.0", pr_number: 19232, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 18, deletions_count: 8},
		{sha: "02c09a417e4c165ccc7746b47b27d1234a5429ca", date: "2023-11-28 01:02:37 UTC", description: "Bump proc-macro2 from 1.0.69 to 1.0.70", pr_number: 19238, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 70, deletions_count: 70},
		{sha: "e1d97de41056ba899fe0060164e8f82a1ab88fe4", date: "2023-11-28 07:03:29 UTC", description: "Bump hashbrown from 0.14.2 to 0.14.3", pr_number: 19239, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "a9b97052970be51448682a27bcb1a513b79dd2a7", date: "2023-11-28 00:23:50 UTC", description: "Updated Fedora versions used in testing to latest", pr_number: 19242, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 3},
		{sha: "4aaf4c28995d24e9c3a5179a5c6c4f3f11332b34", date: "2023-11-28 02:23:49 UTC", description: "re-enable int test for partition_key", pr_number: 19220, scopes: ["aws_kinesis_streams sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 75, deletions_count: 61},
		{sha: "32245e0e760e70f5a1746c93fac25149aba13cfc", date: "2023-11-28 04:53:04 UTC", description: "Fix building the benches", pr_number: 19235, scopes: ["dev"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 16, insertions_count: 110, deletions_count: 96},
		{sha: "cfd9e5e64ebb21ebb241f262de2ec5cd5f64cc57", date: "2023-11-29 06:29:55 UTC", description: "Bump wasm-bindgen from 0.2.88 to 0.2.89", pr_number: 19248, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 10, deletions_count: 10},
		{sha: "f5fe318581b3b820f94f2d1b2a9a0b455a960482", date: "2023-11-29 06:30:49 UTC", description: "Bump the clap group with 1 update", pr_number: 19247, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 20, deletions_count: 20},
		{sha: "6262eb9f2af823b726a70c90963770452598510b", date: "2023-11-29 05:28:06 UTC", description: "Bump cloudsmith-io/action from 0.5.3 to 0.5.4", pr_number: 19254, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "8053443752d9b01a13b0efb322a1c52a3e0b940f", date: "2023-11-29 04:05:25 UTC", description: "Remove stale docs content around schemas", pr_number: 19256, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 4},
		{sha: "d7453ca03dbffecf0ca54a7bff8788838f78ac60", date: "2023-11-29 06:10:32 UTC", description: "avoid compiling globs frequently during tap", pr_number: 19255, scopes: ["tap"], type: "perf", breaking_change: false, author: "Adam Holmberg", files_count: 1, insertions_count: 30, deletions_count: 20},
		{sha: "b58b0d43d5bc20664cd016f125ceab9331d6e5cf", date: "2023-11-29 05:12:31 UTC", description: "Update minor release template step for updating vector.dev", pr_number: 19109, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 4},
		{sha: "ce615d0d23d0a468721a26988a77c166b3288506", date: "2023-11-30 00:39:46 UTC", description: "Update lading to 0.20.0", pr_number: 19259, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "92d4102df59f3239b9aa062caa28e19ead7f3c57", date: "2023-11-30 22:32:48 UTC", description: "Revert update lading to 0.20.0", pr_number: 19259, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "9195f7555abd3f11bdd244576958092ba9a83d79", date: "2023-12-01 10:50:22 UTC", description: "cargo test compilation error", pr_number: 19268, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "87951086c06ab7e7bab7799ec6e559c4009a7c92", date: "2023-12-01 09:28:14 UTC", description: "Bump redis from 0.23.3 to 0.23.4", pr_number: 19240, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 22, deletions_count: 22},
		{sha: "d7edc74623ce9e3b4948727aca2f382a9bc7b6de", date: "2023-12-01 03:23:39 UTC", description: "Update smp to 0.11.0", pr_number: 19270, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "46fcbf481e93d044275fd48ccecd369b242235b7", date: "2023-12-01 07:29:31 UTC", description: "Use welch consignor", pr_number: 19273, scopes: ["performance"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "c2c2dbd7f86112ce8a0aa6a92ee92ee7e3a023e4", date: "2023-12-02 09:41:37 UTC", description: "peg pulsar docker image for int tests to stable image", pr_number: 19287, scopes: ["ci"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "286745878e261ff6c61e191e2b3a2a607ae3fa7a", date: "2023-12-02 11:41:47 UTC", description: "Use correct concurrency group settings for comment trigger & PR commit workflows", pr_number: 19283, scopes: ["ci"], type: "fix", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 9, deletions_count: 2},
		{sha: "ab54db131e330e01268426d611c3df361e4b8224", date: "2023-12-02 18:41:57 UTC", description: "Bump docker/metadata-action from 5.0.0 to 5.2.0", pr_number: 19282, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "ccedb8c09177e64ebbe0b7a557acd951700a883e", date: "2023-12-02 18:42:20 UTC", description: "Bump cidr-utils from 0.5.11 to 0.6.1", pr_number: 19276, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 22, deletions_count: 4},
		{sha: "f009bc1ae40719598f59dba5cd6f12844655012b", date: "2023-12-02 18:42:26 UTC", description: "Bump wiremock from 0.5.21 to 0.5.22", pr_number: 19275, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "79f2d2231633945f6b243fa3d1282ff8bb2254a9", date: "2023-12-02 14:05:19 UTC", description: "Second batch of editorial edits for the Functions doc", pr_number: 19284, scopes: ["external docs"], type: "chore", breaking_change: false, author: "May Lee", files_count: 43, insertions_count: 137, deletions_count: 136},
		{sha: "7a39ae92ff39b26a3ce51f335a8f45d5bc20d74b", date: "2023-12-04 17:39:00 UTC", description: "Bump cargo-deb to 2.0.2", pr_number: 19288, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "8abe8e5a4bb27f4191a67ce5c3869ca75e09f2e1", date: "2023-12-05 03:56:39 UTC", description: "add readme and refactor protobuf test fixtures", pr_number: 19277, scopes: [], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 18, insertions_count: 57, deletions_count: 13},
		{sha: "d703e92a7b415368ce4e351f0c4850cbe5fb3d31", date: "2023-12-05 09:48:42 UTC", description: "do not emit error/discarded metrics for re-routed events", pr_number: 19296, scopes: ["remap"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 48, deletions_count: 9},
		{sha: "b0aa8a0e729fbb0ef6c7997aecc17819ef63fd70", date: "2023-12-05 10:33:17 UTC", description: "update nextest version to 0.9.64", pr_number: 19292, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "c663c354fef5878bf35a3a9afb7bd64053437848", date: "2023-12-05 03:50:15 UTC", description: "emit the correct start offset for multiline aggregated lines", pr_number: 19065, scopes: ["file source"], type: "fix", breaking_change: false, author: "j chesley", files_count: 4, insertions_count: 115, deletions_count: 72},
		{sha: "17b672a5b7ac403e847c40216c5d0ef2b9729646", date: "2023-12-05 06:01:15 UTC", description: "revert peg pulsar docker image for int tests to stable image", pr_number: 19297, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 3},
		{sha: "8f16a00636508a8f44f42bdf9a6ecba0ec6b0d60", date: "2023-12-05 07:10:28 UTC", description: "add all headers to the namespace metadata", pr_number: 18922, scopes: ["http_server source"], type: "feat", breaking_change: false, author: "John Sonnenschein", files_count: 3, insertions_count: 119, deletions_count: 15},
		{sha: "b297070f9ab5f90d6fdb7ffe36d7d59a86cf6f1d", date: "2023-12-06 01:57:55 UTC", description: "Add datadog global config options", pr_number: 18929, scopes: ["datadog service"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 43, insertions_count: 920, deletions_count: 243},
		{sha: "93219494a2d816e0e2c8469cb6ffdbfb8c92bd4f", date: "2023-12-06 17:33:31 UTC", description: "don't compile `secret-backend-example` by default", pr_number: 19317, scopes: ["dev"], type: "fix", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 5, deletions_count: 1},
		{sha: "59dd1fad4b2ea53e47c17baabeb7ab9b922460fc", date: "2023-12-07 02:35:26 UTC", description: "remove component-validation-runner feature from defaults", pr_number: 19324, scopes: ["dev"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "4503ed62fdb99d079f072dd685c8110faf058ac9", date: "2023-12-07 18:00:32 UTC", description: "unused enum variants", pr_number: 19321, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 47, deletions_count: 47},
		{sha: "63ea8a5bd1ee2f79bbc8ca3fc81c40c49656400d", date: "2023-12-08 06:20:02 UTC", description: "Bump openssl from 0.10.60 to 0.10.61", pr_number: 19306, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "32e99eb69aa491855dc001d8595fc391ff5d7002", date: "2023-12-08 16:57:42 UTC", description: "component validation runner and 'sinks::datadog::test_utils' feature gates", pr_number: 19334, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 11, deletions_count: 3},
		{sha: "a006053c944061c5ebd8cb3be93f4438a03008c6", date: "2023-12-09 02:42:57 UTC", description: "Bump once_cell from 1.18.0 to 1.19.0", pr_number: 19339, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "33eef4c3bea59d06c05c62ce07ae8bc035d5da75", date: "2023-12-09 03:17:36 UTC", description: "Bump the clap group with 1 update", pr_number: 19305, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 20, deletions_count: 20},
		{sha: "1bdbf6ddd5d10d3c23559eb8b89e2ec3af68e768", date: "2023-12-09 02:40:30 UTC", description: "Bump ordered-float from 4.1.1 to 4.2.0", pr_number: 19307, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 13, deletions_count: 13},
		{sha: "09126f9435bec9872ae0f90f7aa209a8ce75ed7b", date: "2023-12-09 02:41:01 UTC", description: "Bump redis from 0.23.4 to 0.24.0", pr_number: 19319, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d16a0a584e0ae903830a45fb766d608d3bde154b", date: "2023-12-09 02:51:38 UTC", description: "Bump actions/labeler from 4 to 5", pr_number: 19300, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c3eaf28d34fcbdf90b2cfa318d0e0f50b7781beb", date: "2023-12-09 05:15:47 UTC", description: "Bump snap from 1.1.0 to 1.1.1", pr_number: 19318, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "0ac650b6a3917c1648768674ce39219cd672f26a", date: "2023-12-09 05:21:53 UTC", description: "Bump docker/metadata-action from 5.2.0 to 5.3.0", pr_number: 19299, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "77353fc188db935a335f69e1aae7381fc70f1411", date: "2023-12-09 06:47:01 UTC", description: "Bump opendal from 0.42.0 to 0.43.0", pr_number: 19338, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "1d5b8811c7cdf08a525bf64d8015fb0f688d945e", date: "2023-12-12 05:08:26 UTC", description: "replace anymap with a simple hashmap", pr_number: 19335, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 4, insertions_count: 74, deletions_count: 35},
		{sha: "d2e9f65a25f490a54c24c5f57d9c205df19aa231", date: "2023-12-12 12:09:52 UTC", description: "Enable jemallocator for all non rust-code", pr_number: 19340, scopes: ["performance"], type: "enhancement", breaking_change: false, author: "Alex Savitskii", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6d49346be0ceb72845eacadcc536a5ddfef5d9c6", date: "2023-12-12 16:24:40 UTC", description: "add workaround for batching", pr_number: 19108, scopes: ["aws_kinesis_firehose sink"], type: "fix", breaking_change: false, author: "Damir Sultanov", files_count: 7, insertions_count: 143, deletions_count: 23},
		{sha: "2ad7097b10112f1bd086d6a58c3bce47eb5652ae", date: "2023-12-13 10:17:55 UTC", description: "filter out file contents from error logs", pr_number: 19356, scopes: ["remap"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 42, deletions_count: 4},
		{sha: "0803faaff978ca593e88c372a406a564affebaae", date: "2023-12-13 06:23:29 UTC", description: "Bump mongodb from 2.7.1 to 2.8.0", pr_number: 19365, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "28c70a0296aa3ab0e6d78a991d572f7003fe13e3", date: "2023-12-13 11:25:19 UTC", description: "Bump openssl-src from 300.1.6+3.1.4 to 300.2.1+3.2.0", pr_number: 19364, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "55ec7f1d3aa18081cfe44dcadf6384c0e86c8abc", date: "2023-12-13 05:28:51 UTC", description: "Simplify the default log targets selection", pr_number: 19359, scopes: ["observability"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 15},
		{sha: "0e600ec5cbcf3b74cfdeddd1a1afc88afce00b93", date: "2023-12-14 05:01:40 UTC", description: "update VRL to v0.9.0", pr_number: 19368, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 38, deletions_count: 60},
		{sha: "c5d6917a079f5a203b5665b598cbf0e52bbc9dfb", date: "2023-12-14 12:30:48 UTC", description: "add timestamp comparison example", pr_number: 19266, scopes: ["docs"], type: "chore", breaking_change: false, author: "maksimtor", files_count: 1, insertions_count: 11, deletions_count: 4},
		{sha: "cbf3b783f44a952ac5d57d38e92b3df78e196274", date: "2023-12-14 07:46:27 UTC", description: "document new snappy vrl functions", pr_number: 19081, scopes: ["docs"], type: "chore", breaking_change: false, author: "Michael Hoffmann", files_count: 2, insertions_count: 64, deletions_count: 0},
		{sha: "632fe210ce28d178e22d8d35f4fdce348b63c212", date: "2023-12-14 02:09:12 UTC", description: "Add datadog-signing-keys package as recommended", pr_number: 19369, scopes: ["apt platform"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 5},
		{sha: "dff8ca3aee4a628885765f3fe1d2af70d2f4b969", date: "2023-12-14 02:27:11 UTC", description: "Remove references to CloudSmith from docs", pr_number: 19377, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 6, insertions_count: 14, deletions_count: 24},
		{sha: "290a635e2f50e83c5b9dad5df5d8f3187878b4d9", date: "2023-12-14 02:42:14 UTC", description: "Remove Cloudsmith package publishing", pr_number: 19378, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 126},
		{sha: "e82ac3702305c1de8c8163b5b5f31067e1262d4c", date: "2023-12-14 08:09:07 UTC", description: "Bump typetag from 0.2.13 to 0.2.14", pr_number: 19353, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "6e9bb20e6fe95452bfd62b42bb862180d8ba101b", date: "2023-12-14 07:05:52 UTC", description: "Bump syn from 2.0.39 to 2.0.41", pr_number: 19373, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 45, deletions_count: 45},
		{sha: "07cdf75150b7b1587e8ff43dbc3d42ff909e56e0", date: "2023-12-14 13:12:02 UTC", description: "Bump tokio-openssl from 0.6.3 to 0.6.4", pr_number: 19366, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "5e0dc25a291278bca4e8ca7ee7f93830b95f380a", date: "2023-12-15 05:19:40 UTC", description: "Bump actions/labeler from 4 to 5", pr_number: 19358, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 25, deletions_count: 15},
		{sha: "b7b8081b63ce4fbcc74f4cb31e9f5827b6594b3a", date: "2023-12-15 07:41:27 UTC", description: "group artifact dependabot upgrades", pr_number: 19390, scopes: ["ci"], type: "enhancement", breaking_change: false, author: "Doug Smith", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "bd2cff83a6df2e0e287c40000bd9d7e9d24a59dc", date: "2023-12-16 02:59:04 UTC", description: "introduce avro", pr_number: 19342, scopes: ["new codecs"], type: "feat", breaking_change: false, author: "Ion-manden", files_count: 65, insertions_count: 1620, deletions_count: 1},
		{sha: "17fd152743d914a4d2bcad8cc5194c8c1eea5e7e", date: "2023-12-16 07:11:37 UTC", description: "Bump the artifact group with 2 updates", pr_number: 19391, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 56, deletions_count: 56},
		{sha: "d9de79728e266c6cd1c3750aab82b57639e540bf", date: "2023-12-16 04:56:33 UTC", description: "close code block", pr_number: 19389, scopes: ["releases website"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3fb9922c648f867166bd065793a573c4fb65ed83", date: "2023-12-16 06:39:08 UTC", description: "specify that the unix mode supports stream sockets only", pr_number: 19399, scopes: ["syslog source"], type: "docs", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 10, deletions_count: 4},
		{sha: "2277b58bc89950f03b89882c64985e1c5dd988e6", date: "2023-12-16 07:25:19 UTC", description: "Bump crossbeam-utils from 0.8.16 to 0.8.17", pr_number: 19381, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "09978c93aa3d8c759f35154f58a9b899c1a46875", date: "2023-12-19 07:25:19 UTC", description: "Revert bump the artifact group with 2 updates", pr_number: 19416, scopes: ["ci"], type: "chore", breaking_change: false, author: "Doug Smith", files_count: 6, insertions_count: 56, deletions_count: 56},
		{sha: "6b190157346735fbf8b2c8e4ad60450d32ec0303", date: "2023-12-20 01:12:26 UTC", description: "Bump tokio from 1.34.0 to 1.35.0", pr_number: 19348, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 13, deletions_count: 13},
		{sha: "0f4098970cc8eb06847527695eb0ea50f0b2b4f1", date: "2023-12-20 04:17:39 UTC", description: "Fix test of install script", pr_number: 19425, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a2b92f4a0c171ea677f3f2d55730e7e5be5f045b", date: "2023-12-20 03:45:25 UTC", description: "Bump cargo_toml from 0.17.1 to 0.17.2", pr_number: 19422, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1fc7a147be02103ea2df6c76cf4bd9f5e95c0c26", date: "2023-12-20 03:46:45 UTC", description: "Bump reqwest from 0.11.22 to 0.11.23", pr_number: 19421, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "27ccf524db221c4adc23846808a6f0942da16a3b", date: "2023-12-20 11:49:44 UTC", description: "Bump hyper from 0.14.27 to 0.14.28", pr_number: 19419, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "7448d3febff368fe893ff13bb2c729ad0b526dd4", date: "2023-12-20 11:50:52 UTC", description: "Bump docker/metadata-action from 5.3.0 to 5.4.0", pr_number: 19414, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c35831b2bb34460ff40f79beeb4827436c86d926", date: "2023-12-20 07:00:43 UTC", description: "Add ignored branch check to workflow", pr_number: 19427, scopes: ["website"], type: "fix", breaking_change: false, author: "Devin Ford", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "f5fd79f76734470656e6e8c1ccffd4f86944d204", date: "2023-12-20 12:11:02 UTC", description: "Bump stream-cancel from 0.8.1 to 0.8.2", pr_number: 19407, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "03cbc2ea081eab14a2adf8b1a2125b2b6e11f15b", date: "2023-12-20 12:11:54 UTC", description: "Bump thiserror from 1.0.50 to 1.0.51", pr_number: 19406, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "f124f86766031863a6a14be1031d112a7b5c48f7", date: "2023-12-20 12:13:24 UTC", description: "Bump memmap2 from 0.9.0 to 0.9.2", pr_number: 19404, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0a71d9ed167c99544e1ec28df8068f6621faea2c", date: "2023-12-20 12:14:45 UTC", description: "Bump rkyv from 0.7.42 to 0.7.43", pr_number: 19403, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 5},
		{sha: "62817ea1176a969aa93c20d055253aca359049ae", date: "2023-12-20 12:15:13 UTC", description: "Bump the clap group with 1 update", pr_number: 19402, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "20682e7724c907b676d96ff7660d1c675e7fb56a", date: "2023-12-20 20:34:00 UTC", description: "Allow downloading specific versions of Vector", pr_number: 19408, scopes: ["install vector"], type: "enhancement", breaking_change: false, author: "Suika", files_count: 2, insertions_count: 8, deletions_count: 1},
		{sha: "5eab9528d68f68108feac5385aa4e24cdf3f1560", date: "2023-12-20 23:22:46 UTC", description: "Configure spellchecker to ignore avro files", pr_number: 19432, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "bf2b65c151be8ef83b240e53a15256224116b1e2", date: "2023-12-21 05:32:45 UTC", description: "Bump memmap2 from 0.9.2 to 0.9.3", pr_number: 19431, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5ef73ee665a395af23deb93a8138038745038c92", date: "2023-12-21 05:33:02 UTC", description: "Bump tokio from 1.35.0 to 1.35.1", pr_number: 19430, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 13, deletions_count: 13},
		{sha: "35acd3f428193462530373cf955f2fed017f208b", date: "2023-12-22 03:59:10 UTC", description: "Bump syn from 2.0.41 to 2.0.42", pr_number: 19441, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 45, deletions_count: 45},
		{sha: "b22dc2557a1c88548e8ce0562fbde3bf202b1f8b", date: "2023-12-22 09:59:59 UTC", description: "Bump async-trait from 0.1.74 to 0.1.75", pr_number: 19440, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7ef8ccb69c90a20017126517786667473653df0f", date: "2023-12-22 10:00:23 UTC", description: "Bump serde_yaml from 0.9.27 to 0.9.28", pr_number: 19439, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 10},
		{sha: "e350c6b318e238c6ab5f26071322104f85b94be2", date: "2023-12-22 10:02:01 UTC", description: "Bump anyhow from 1.0.75 to 1.0.76", pr_number: 19437, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "05d827d0e35029d305cfb9f56af036e46b185c8a", date: "2023-12-22 04:35:53 UTC", description: "Update the version of cue we are using to 0.7.0", pr_number: 19449, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 24, insertions_count: 276, deletions_count: 276},
		{sha: "af4de5eae6ad454fccd47fc933ac02bafa579446", date: "2023-12-22 07:00:13 UTC", description: "Remove deprecated HTTP metrics", pr_number: 19447, scopes: ["observability"], type: "chore", breaking_change: true, author: "Jesse Szwedko", files_count: 27, insertions_count: 55, deletions_count: 281},
		{sha: "406ec497488c90d7008b9764ce529f29e0d0656b", date: "2023-12-22 22:46:12 UTC", description: "Bump serde_yaml from 0.9.28 to 0.9.29", pr_number: 19451, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "82c4e50a16530f1cc1516cc5d682a892c1d6958f", date: "2023-12-22 22:46:23 UTC", description: "Bump inventory from 0.3.13 to 0.3.14", pr_number: 19452, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0a567da3c732f43473c815b04019deb62a6645dc", date: "2023-12-23 06:46:45 UTC", description: "Bump temp-dir from 0.1.11 to 0.1.12", pr_number: 19454, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3191397115a63ed7efb70a21035bae3bfeee96c9", date: "2023-12-22 23:36:38 UTC", description: "Update VRL to 0.9.1", pr_number: 19455, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "930611fe3430b63c043739add7d4f1011e1dfafd", date: "2023-12-23 08:17:38 UTC", description: "Bump ratatui from 0.24.0 to 0.25.0", pr_number: 19420, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 44, deletions_count: 32},
		{sha: "88f5c23614d2197a4eaad1c8bca993250782e566", date: "2023-12-23 08:45:10 UTC", description: "Bump owo-colors from 3.5.0 to 4.0.0", pr_number: 19438, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "c7b1cabc42b9fdbfca474a14aa2185fd5a4e4403", date: "2023-12-23 08:46:32 UTC", description: "Bump libc from 0.2.150 to 0.2.151", pr_number: 19349, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2caf6e215e679eda807340a96c6723ac7ec05a85", date: "2023-12-23 08:49:01 UTC", description: "Bump colored from 2.0.4 to 2.1.0", pr_number: 19350, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 4},
		{sha: "2d24e7d7e75463bf72087dfc09b78fd38d5f6158", date: "2023-12-23 08:49:50 UTC", description: "Bump ryu from 1.0.15 to 1.0.16", pr_number: 19351, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "3a08e64c45c1125f9ef21e9cf0ee173e1069585e", date: "2023-12-23 08:50:15 UTC", description: "Bump serde-wasm-bindgen from 0.6.1 to 0.6.3", pr_number: 19354, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "06a924882bb900ac980bd333dfdb938aa316e706", date: "2023-12-23 09:12:55 UTC", description: "Bump proc-macro2 from 1.0.70 to 1.0.71", pr_number: 19453, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 69, deletions_count: 69},
		{sha: "9d164b66122dd059c51651a174cb6a9cb62827a4", date: "2024-01-03 03:43:17 UTC", description: "fix mismatch in config vs config file name", pr_number: 19469, scopes: [], type: "docs", breaking_change: false, author: "Jeff Byrnes", files_count: 1, insertions_count: 18, deletions_count: 16},
		{sha: "cbafbc5af24fb8be7046f32287e2c2d3827e9e4a", date: "2024-01-03 03:09:59 UTC", description: "update tests for 2024", pr_number: 19493, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "deb31e2de75f9d5a1ba8713985416c4b8049e654", date: "2024-01-03 02:53:02 UTC", description: "Update manifests to chart v0.29.1", pr_number: 19494, scopes: ["kubernetes"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "84cf99a171632036581992d63012a99c5c33869f", date: "2024-01-03 04:11:42 UTC", description: "Bump bstr from 1.8.0 to 1.9.0", pr_number: 19477, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "b00d4e33c63a8554769cb1b85cec1acb01f0736f", date: "2024-01-03 12:12:51 UTC", description: "Bump the futures group with 1 update", pr_number: 19461, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 55, deletions_count: 55},
		{sha: "2ae64f3b20bd0305383ef7339a1ee07cc8ec62c5", date: "2024-01-03 12:13:01 UTC", description: "Bump openssl from 0.10.61 to 0.10.62", pr_number: 19462, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "92fc72646bb9606ac9a456df78f4032732582852", date: "2024-01-03 12:15:10 UTC", description: "Bump crossbeam-utils from 0.8.17 to 0.8.18", pr_number: 19465, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "a8629afeda956045c3ef87cab8ad34ab2a779632", date: "2024-01-03 12:15:42 UTC", description: "Bump tempfile from 3.8.1 to 3.9.0", pr_number: 19474, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 90, deletions_count: 24},
		{sha: "32ac1cba5e75c88774f016cb6aa94da5291fa4a7", date: "2024-01-03 12:15:46 UTC", description: "Bump schannel from 0.1.22 to 0.1.23", pr_number: 19475, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "f6b8cb24fc24b013a76bb83e67c8dd3cbcd70c35", date: "2024-01-03 12:55:50 UTC", description: "Bump chrono-tz from 0.8.4 to 0.8.5", pr_number: 19479, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "074c257aa1767c27416f0d00c0f45c746a4dcc35", date: "2024-01-03 12:58:55 UTC", description: "Bump quanta from 0.12.1 to 0.12.2", pr_number: 19486, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 5},
		{sha: "42ad075c215cdba5b7a88a812b9f60a40d9a30fd", date: "2024-01-03 12:59:22 UTC", description: "Bump serde_json from 1.0.108 to 1.0.109", pr_number: 19489, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "84e1ac11d3d221710b30030224b7928d9c56c998", date: "2024-01-03 16:47:52 UTC", description: "Bump opendal from 0.43.0 to 0.44.0", pr_number: 19483, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "dad8a850db0d4a845f0e8d3b506524a8a0617311", date: "2024-01-03 16:49:04 UTC", description: "Bump proc-macro2 from 1.0.71 to 1.0.74", pr_number: 19496, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 69, deletions_count: 69},
		{sha: "0f29afa442f7e9a08809867d38bf47593e1925c9", date: "2024-01-03 17:35:01 UTC", description: "Bump the clap group with 2 updates", pr_number: 19501, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 23, deletions_count: 23},
		{sha: "f4661777b33ca61fa6935afa798cbd2d88a62daf", date: "2024-01-03 15:10:18 UTC", description: "Add option to turn missing env vars in config into an error", pr_number: 19393, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 8, insertions_count: 111, deletions_count: 47},
		{sha: "80d3bf20a9b2c9263fab1f9cd02d6f447dfa306a", date: "2024-01-04 02:23:19 UTC", description: "Bump serde from 1.0.193 to 1.0.194", pr_number: 19506, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 128, deletions_count: 128},
		{sha: "9bdaaa27948057c53e9a107a92b454657a67d5f7", date: "2024-01-04 08:23:59 UTC", description: "Bump typetag from 0.2.14 to 0.2.15", pr_number: 19504, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "981fb8c08f0cd74440bac6710c893d99b0c698c9", date: "2023-11-15 06:08:14 UTC", description: "make file internal metric tag opt-in", pr_number: 19145, scopes: ["file source", "kubernetes_logs source", "file sink"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 8, insertions_count: 31, deletions_count: 28},
	]
}
