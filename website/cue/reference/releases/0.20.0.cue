package metadata

releases: "0.20.0": {
	date:     "2022-02-08"
	codename: ""

	known_issues: [
		"When unit testing targets that have multiple outputs, Vector logs a warning for untested outputs. Will be fixed in `0.20.1`.",
		"If nonexistent extract_from/no_outputs_from targets are included in unit testing configurations, `vector test` will panic. Will be fixed in `0.20.1`.",
	]

	description: """
		The Vector team is pleased to announce version 0.20.0!

		In addition to the new features, enhancements, and fixes listed below, this release includes a new opt-in disk
		buffer implementation that we hope will provide users with faster, more consistent, and lower resource usage
		buffer performance. We encourage you to opt-in during this beta period and [give us feedback](/community/). See
		[the beta disk buffer highlight article](/highlights/2022-02-08-disk-buffer-v2-beta) for more details including
		how to opt-in.

		We also made additional performance improvements this release increasing the average throughput by 10-20% for
		common topologies (see our [soak test
		framework](https://github.com/vectordotdev/vector/tree/master/soaks/tests)).

		Be sure to check out the [upgrade guide](/highlights/2022-02-08-0-20-0-upgrade-guide) for breaking changes in
		this release.
		"""

	changelog: [
		{
			type: "enhancement"
			scopes: ["unit tests"]
			description: """
				Support for unit testing task transforms and named outputs was added. See the [highlight
				article](/highlights/2022-01-12-vector-unit-test-improvements) for more details.
				"""
			pr_numbers: [10540]
		},
		{
			type: "enhancement"
			scopes: ["route transform"]
			description: """
				The `route` transform was refactored to rely on Vector's new concept of named outputs for components.
				The behavior is the same, but the metrics emitted by the transform have been updated as noted in the
				[highlight article for named output metrics](/highlights/2022-01-19-component-sent-metrics-output-tag).
				"""
			pr_numbers: [10738]
		},
		{
			type: "fix"
			scopes: ["prometheus_exporter sink"]
			description: """
				Fixed runaway memory growth when the `prometheus_exporter` sink was used with distributions due to the
				sink holding onto all of the samples it had seen.

				This required a breaking change as documented in the [upgrade
				guide](/highlights/2022-02-08-0-20-0-upgrade-guide#prom-exporter-set-expiration).
				"""
			pr_numbers: [10741]
		},
		{
			type: "enhancement"
			scopes: ["datadog_agent source"]
			description: """
				A `multiple_outputs` option was added to the `datadog_agent` source. When set to `true`, rather than
				emitting both logs and metrics to any components using the component id of the source in `inputs`, the
				events will be split into two separate streams, `<component_id>.logs` and `<component_id>.metrics`, so
				that logs and metrics from the Datadog agent can be processed separately.

				When set to `true`, the internal metrics from this source are also updated as mentioned in the
				[highlight article for named output metrics](/highlights/2022-01-19-component-sent-metrics-output-tag).

				`multiple_outputs` defaults to `false` for compatibility with existing configurations.
				"""
			pr_numbers: [10776]
		},
		{
			type: "feat"
			scopes: ["new_relic sink"]
			description: """
				A new `new_relic` sink was added that can handle both logs and metrics. It can also send logs to New
				Relic as New Relic events. It replaces the existing `new_relic_logs` sink that has been deprecated.

				Thanks to [@Andreu](https://github.com/asllop) for this contribution!
				"""
			pr_numbers: [9746]
		},
		{
			type: "fix"
			scopes: ["splunk_hec source"]
			description: """
				The `splunk_hec` source now accepts invalid UTF-8 bytes, matching Splunk's HEC behavior. It replaces
				them with the UTF-8 replacement character: �
				"""
			pr_numbers: [10858]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				A few fixes were made to `parse_groks`:

				- The `array` filter now supports arrays without brackets, if empty string is
				  provided as brackets. For example: `%{data:field:array("", "-")} will parse "a-b" into ["a", "b"].
				- The `keyvalue` filter now supports nested paths as keys. For example:

				  ```coffeescript
					parse_groks("db.name=my_db,db.operation=insert",
					  patterns: ["%{data::keyvalue}"]
					)
				  ```

				  will yield

				  ```json
					{
						"db" : {
							"name" : "my_db",
							"operation" : "insert",
						}
					}
				  ```
				- The `date` filter now supports the `d` and `y` shorthands. For example:

				  ```coffeescript
				    parse_groks("Nov 16 2020 13:41:29 GMT", patterns: ["%{date("MMM d y HH:mm:ss z"):field}"] )
				  ```

				  Will yield:

				  ```json
				  1605534089000
				  ```
				- Numeric values are now returned as integers when there is no loss in precision; otherwise they
				  continue to be returned as floats.
				- Aliases that match multiple fields now correctly extract as an array of values.
				  extracted values.
				- Aliases with a filter now correctly extract.
				- The `keyvalue` filter now correctly ignores keys without values.
				- To match the beginning and the end of lines, `\\A` and `\\Z` must be used rather than `^` and $`
				- The `keyvalue` filter now correctly handles parsing keys that start with a number
				"""
			pr_numbers: [10537, 10954, 10958, 10961, 10938, 10956, 10538]
		},
		{
			type: "enhancement"
			scopes: ["observability", "route transform", "remap transform", "datadog_agent source"]
			description: """
				The telemetry for a few components was updated as a result expanding the use of named outputs in Vector for components with multiple output streams:

				- `route`
				- `datadog_agent` (when `multiple_outputs` is `true`)
				- `remap` (when `reroute_dropped` is `true`)

				These components now add an `output` tag to their metrics. See [highlight article for named output
				metrics](/highlights/2022-01-19-component-sent-metrics-output-tag).
				"""
			pr_numbers: [10869]
		},
		{
			type: "enhancement"
			scopes: ["observability", "vrl"]
			description: """
				The `abort` function can now take an optional string message to include in logs and the metadata for
				dropped events. For example:

				```coffeescript
				if .foo == 5 {
					abort "foo is " + .foo + "!"
				}
				```

				The `foo is 5!` message will appear in Vector's internal logs as well as being used as
				`metadata.message` for rerouted dropped events (if `reroute_dropped` is `true`).
				"""
			pr_numbers: [10997]
		},
		{
			type: "enhancement"
			scopes: ["observability", "remap transform"]
			description: """
				If a custom message is passed to `assert` or `assert_eq`, this message is now used as `metadata.message`
				rather than the default VRL error message which included `function call error: ...`. This allows cleaner
				handling of different assertion errors if routing dropped events to another component (when
				`reroute_dropped` is `true`).
				"""
			pr_numbers: [10914]
		},
		{
			type: "enhancement"
			scopes: ["vector source", "vector sink"]
			description: """
				The `vector` source and `vector` sink now default to version 2 of the protocol for inter-Vector
				communication. See the [upgrade guide](/highlights/2022-02-08-0-20-0-upgrade-guide#deprecate-v1) for more
				details.
				"""
			pr_numbers: [11023]
		},
		{
			type: "enhancement"
			scopes: ["observability", "sources"]
			description: """
				We are in the process of updating all Vector components with consistent instrumentation as described in
				[Vector's component
				specification](\(urls.specs_instrumentation)).

				With this release we have instrumented the following sources with these new metrics:

				- `aws_ecs_metrics`
				- `aws_kinesis_firehose`
				- `aws_s3`
				- `datadog_agent`
				- `demo_logs`
				- `dnstap`
				- `docker_logs`
				- `eventstoredb_metrics`
				- `exec`
				- `host_metrics`
				- `internal_logs`
				- `internal_metrics`
				- `kafka`
				- `kubernetes_logs`
				- `nats`
				- `nginx_metrics`
				- `prometheus_scrape`
				- `stdin`
				- `vector`

				And these transforms:

				- `aws_ec2_metadata`
				"""
			pr_numbers: [10861, 10879, 10932, 11017, 10880, 11130, 10890, 10893, 11018, 11035, 11038, 11081, 11120, 11121, 11123, 11126, 11059, 11132, 11034, 11220]
		},
		{
			type: "fix"
			scopes: ["azure_blob sink"]
			description: """
				The `azure_blob` sink now correctly parses connection strings including SAS tokens.
				"""
			pr_numbers: [11030]
		},
		{
			type: "fix"
			scopes: ["buffers"]
			description: """
				Report correct `buffer_events_total` metric `drop_newest` is used for buffers. Previously, this was
				counting discarded events.
				"""
			pr_numbers: [11030]
		},
		{
			type: "enhancement"
			scopes: ["loki sink"]
			description: """
				The `loki` sink now supports the `compression` option. For now, the only available compression is `gzip`.
				"""
			pr_numbers: [10953]
		},
		{
			type: "enhancement"
			scopes: ["observability", "loki sink"]
			description: """
				The internal telemetry for the `loki` sink has been improved by:

				- The addition of `component_discarded_events_total` metric for discarded events
				- The addition of the `rewritten_timestamp_events_total` metric to count events whose timestamp was rewritten
				- The log messages for out-of-order events were dropped in severity from `warn` to `debug` since
				  out-of-order events are a common occurrence, something the sink is designed to handle, and the handling
				  behavior is explicitly configured by the user

				See the [upgrade guide](/highlights/2022-02-08-0-20-0-upgrade-guide#events-loki-sink) for more details.
				"""
			pr_numbers: [10971]
		},
		{
			type: "fix"
			scopes: ["aws_sqs source"]
			description: """
				Support for HTTP(S) proxies was added to the `aws_sqs` source. This can be configured the usual way
				through the [`proxy` configuration field](/docs/reference/configuration/sources/aws_sqs/#proxy) or
				[environment variables](/docs/reference/configuration/sources/aws_sqs/#https_proxy).
				"""
			pr_numbers: [11042]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				When a condition (for example for the `route` transform) that was written with VRL fails to execute, the
				reason for failure is now output in the logs.
				"""
			pr_numbers: [11044]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				`vector top` has been updated to show the event metrics per output when a component has multiple outputs.
				"""
			pr_numbers: [11085]
		},
		{
			type: "enhancement"
			scopes: ["sources"]
			description: """
				TCP-based sources like `fluent`, `syslog`, and `socket` now handle back-pressure better. Previously,
				back-pressure to these sources from downstream components would cause runaway resource growth as the
				source would continue to accept connections even though it couldn't handle them right away. A new
				algorithm has been introduced to start applying back-pressure to clients once ~100k events are buffered
				in the source. The source will then start closing new connections, rather than accepting them, until it
				has flushed events downstream.
				"""
			pr_numbers: [10962]
		},
		{
			type: "chore"
			scopes: ["kubernetes_logs source"]
			description: """
				The minimal supported Kubernetes version was bumped from v1.15 to v1.19.

				Older versions are likely to still work, we just only test against v1.19+.
				"""
			pr_numbers: [11118]
		},
		{
			type: "feat"
			scopes: ["buffers"]
			description: """
				This release includes a new opt-in disk buffer implementation that we hope will provide users with
				faster, more consistent, and lower resource usage buffer performance. We encourage you to opt-in during
				this beta period and [give us feedback](/community/). See [the beta disk buffer highlight
				article](/highlights/2022-02-08-disk-buffer-v2-beta) for more details including how to opt-in.
				"""
			pr_numbers: [9476]
		},
		{
			type: "fix"
			scopes: ["codecs"]
			description: """
				`encoding.only_fields` now correctly deserializes again for sinks that used fixed encodings (i.e. those
				that don't have `encoding.codec`). This was a regression in `v0.18.0`.
				"""
			pr_numbers: [11198]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			breaking: true
			description: """
				Predicates in VRL `if` conditions now correctly check for fallibility. This is a breaking change. See
				the [upgrade guide](/highlights/2022-02-08-0-20-0-upgrade-guide#vrl-fallible-predicates) for more
				details.
				"""
			pr_numbers: [11172]
		},
		{
			type: "fix"
			scopes: ["sources", "codecs"]
			description: """
				Continue to process data when decoding fails in a source that is using `decoding.codec`.
				"""
			pr_numbers: [11254]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			breaking: true
			description: """
				Division by a literal nonzero number in `VRL` is no longer fallible. See the [upgrade
				guide](/highlights/2022-02-08-0-20-0-upgrade-guide#vrl-fallible-predicates) for more details.
				"""
			pr_numbers: [10339]
		},
		{
			type: "fix"
			scopes: ["transforms"]
			description: """
				All transforms now correctly publish metrics with their component span tags (like `component-id`). This
				was a regression in `v0.19.0`.
				"""
			pr_numbers: [11241]
		},
		{
			type: "fix"
			scopes: ["codecs"]
			description: """
				The `syslog`  decoder (`encoding.codec` on sources) now correctly errors if the incoming data is not
				actually syslog data. Previously it would pass through the invalid data. In the future, we will likely
				add a way to route invalid events.
				"""
			pr_numbers: [11244]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				The `vector tap` command has had two enhancements:

				- It can now tap metric events in addition to log events
				- It has a new `logfmt` option that can be used to format the tapped output as logfmt data
				"""
			pr_numbers: [11234, 11201]
		},
	]

	whats_next: [
		{
			title: "Faster disk buffers stabilization"
			description: """
				After the beta period, we plan to make the new disk buffer implementation the default.
				"""
		},
		{
			title:       "Component metric standardization"
			description: """
				We continue to be in the process of ensuring that all Vector components report a consistent set of
				metrics to make it easier to monitor the performance of Vector.  These metrics are outlined in this new
				[instrumentation specification](\(urls.specs_instrumentation)).
				"""
		},
		{
			title: "Official release of end-to-end acknowledgements feature"
			description: """
				We have started to add support for end-to-end acknowledgements from sources to sinks where sources will
				not ack data until the data has been processed by all associated sinks. It is usable by some components
				now, but we expect to officially release this feature after some final revisions, testing, and documentation.
				"""
		},
	]

	commits: [
		{sha: "62bcd4d76696da022dfd64511b62dd9b28a7f14d", date: "2021-12-29 03:12:03 UTC", description: "Escape \\ more in 0.19.0 release changelog", pr_number:                                                                         10609, scopes: [], type:                               "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "b05ac65254cfdf969489015fa715d10d861fd1d3", date: "2021-12-29 03:22:03 UTC", description: "Bump Cargo.toml version to 0.20.0", pr_number:                                                                                  10607, scopes: ["releasing"], type:                    "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:   2, deletions_count:    2},
		{sha: "ebc8ae6572a0ed2b79b3edf79298526123b0987e", date: "2021-12-29 23:22:35 UTC", description: "bump lru from 0.7.1 to 0.7.2", pr_number:                                                                                       10617, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "69a949f04970aaeed52313ba03bd5d75e25202b8", date: "2021-12-29 23:25:57 UTC", description: "bump governor from 0.3.2 to 0.4.0", pr_number:                                                                                  10618, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   13, deletions_count:   32},
		{sha: "9fecdc8b5c45c613de2d01d4d2aee22be3a2e570", date: "2021-12-30 02:42:56 UTC", description: "Speed up handling larger arrays of finalizers", pr_number:                                                                      10613, scopes: ["performance"], type:                  "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   8, deletions_count:    43},
		{sha: "e666b4299a0d3f509d49d9dae80ac160a747d67f", date: "2021-12-31 00:33:10 UTC", description: "Document that topic is templatable", pr_number:                                                                                 10628, scopes: ["kafka sink"], type:                   "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      1, insertions_count:   1, deletions_count:    0},
		{sha: "eaa0690048bd4e262fa8766bc6831b4138a5d89e", date: "2021-12-31 04:21:24 UTC", description: "bump async-graphql from 3.0.18 to 3.0.19", pr_number:                                                                           10630, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "6708f12e9ad495827486c2a04b6a721027df44af", date: "2021-12-31 04:21:42 UTC", description: "bump tracing-subscriber from 0.3.4 to 0.3.5", pr_number:                                                                        10629, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   8, deletions_count:    8},
		{sha: "2fd29abbba9e64bcbe83f537e3c169650767e1b5", date: "2021-12-31 05:13:17 UTC", description: "Fix typo in docker compose integration tests", pr_number:                                                                       10638, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "c5fea0383a55d913873f57ca215cce9518819b73", date: "2021-12-31 05:18:59 UTC", description: "Stop running soak jobs for dependabot PRs", pr_number:                                                                          10636, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   6, deletions_count:    1},
		{sha: "6b46b08435639bdce6786ce09fa9237d3b89e75e", date: "2021-12-31 13:28:22 UTC", description: "refactor makefile integration tests command", pr_number:                                                                        10632, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     1, insertions_count:   9, deletions_count:    12},
		{sha: "1d8d43c5dc8a62fb48f2c5fb15984cac07b788a1", date: "2022-01-04 03:15:51 UTC", description: "bump http from 0.2.5 to 0.2.6", pr_number:                                                                                      10643, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   5, deletions_count:    5},
		{sha: "1362de2514bc1537f0a29045865f7f669f1842d1", date: "2022-01-04 03:17:08 UTC", description: "bump async-graphql-warp from 3.0.18 to 3.0.19", pr_number:                                                                      10644, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "f0f2b1def91e6ff4df819dc0ed1115f4907d7040", date: "2022-01-04 03:17:44 UTC", description: "bump atomig from 0.3.2 to 0.3.3", pr_number:                                                                                    10645, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "9a86b4ba82989a4adfe13a725b225216c03aa42b", date: "2022-01-04 09:43:39 UTC", description: "bump pin-project from 1.0.9 to 1.0.10", pr_number:                                                                              10646, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   24, deletions_count:   24},
		{sha: "e17d2ffb086ece5bbcb9f3d6558a8c295ac3a056", date: "2022-01-04 11:09:39 UTC", description: "bump serde from 1.0.132 to 1.0.133", pr_number:                                                                                 10649, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:   11, deletions_count:   11},
		{sha: "730ce9a704b28f1674cb9237e688711692f6d938", date: "2022-01-04 13:51:22 UTC", description: "bump clap from 2.34.0 to 3.0.0", pr_number:                                                                                     10647, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   44, deletions_count:   14},
		{sha: "6836461df86f88357c88db706a25a1d30fe15bd1", date: "2022-01-05 00:09:06 UTC", description: "add integration test with real endpoint", pr_number:                                                                            10464, scopes: ["datadog_logs sink"], type:            "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     6, insertions_count:   55, deletions_count:   1},
		{sha: "dc274bd3ade525eed530aacb99d5d48db265dc10", date: "2022-01-05 03:20:37 UTC", description: "Improved performance of starts_with", pr_number:                                                                                10296, scopes: ["vrl"], type:                          "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:     3, insertions_count:   112, deletions_count:  20},
		{sha: "95b9a2f99c8690f880dd42d8984455f021ab852a", date: "2022-01-05 00:31:42 UTC", description: "Remove old helm charts and associated tooling", pr_number:                                                                      10639, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    85, insertions_count:  3, deletions_count:    4760},
		{sha: "b5f0efa9f43c6374dec1d72fb362dc3e27369e0f", date: "2022-01-05 01:09:30 UTC", description: "bump ordered-float from 2.8.0 to 2.9.0", pr_number:                                                                             10663, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "eeca067e46b2dfe68ce13c05d5ea4f4a6797fe58", date: "2022-01-05 01:09:59 UTC", description: "bump clap from 3.0.0 to 3.0.1", pr_number:                                                                                      10664, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "9040efc02b6315f8c955ebc47c8afb34c8cfb2fa", date: "2022-01-05 07:37:29 UTC", description: "move nginx integration test to docker-compose", pr_number:                                                                      10621, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     5, insertions_count:   86, deletions_count:   89},
		{sha: "cb69b08ec18f0f9b76a26f69501e7316859830ef", date: "2022-01-05 03:19:49 UTC", description: "Update kustomization resources to new Helm chart", pr_number:                                                                   10641, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    7, insertions_count:   301, deletions_count:  829},
		{sha: "89afe9038e8388abdf62639c3539b55cf49e4973", date: "2022-01-05 09:20:31 UTC", description: "move logstash integration tests to use docker-compose", pr_number:                                                              10679, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     6, insertions_count:   102, deletions_count:  90},
		{sha: "86548fd2e3972620d2dc4dc0ec17cf91808b8269", date: "2022-01-05 03:55:31 UTC", description: "bump wiremock from 0.5.8 to 0.5.9", pr_number:                                                                                  10662, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "34119d10bda05f1da829adea7eed18fee68359d2", date: "2022-01-05 02:27:13 UTC", description: "Leverage existing topology logic for unit test internals", pr_number:                                                           10540, scopes: ["unit tests"], type:                   "chore", breaking_change:       false, author: "Will", files_count:               13, insertions_count:  2216, deletions_count: 1828},
		{sha: "24d1813be8285ce9c1210aad060bd62dc8c6f61b", date: "2022-01-05 07:04:21 UTC", description: "Add RFC for health endpoint improvements", pr_number:                                                                           10297, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   117, deletions_count:  0},
		{sha: "0f1aa9a6bd7886360dde8e223b079a2d98688a8b", date: "2022-01-05 22:47:52 UTC", description: "Separate default consumer/producer configs", pr_number:                                                                         10680, scopes: ["kafka sink"], type:                   "fix", breaking_change:         false, author: "everpcpc", files_count:           1, insertions_count:   68, deletions_count:   61},
		{sha: "9955c459bef9bc4b9a93f8a6b30a116369fe1c18", date: "2022-01-06 02:55:49 UTC", description: "move es integration tests to docker-compose", pr_number:                                                                        10603, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     11, insertions_count:  205, deletions_count:  115},
		{sha: "5d1447f0e9af0f178672da37076e0370c38d53e1", date: "2022-01-06 02:19:33 UTC", description: "bump clap from 3.0.1 to 3.0.4", pr_number:                                                                                      10689, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "54264c0a7c0d249616f03fa6b8051b22502e6ed4", date: "2022-01-06 08:39:25 UTC", description: "migrating aws integration tests to docker-compose", pr_number:                                                                  10725, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     14, insertions_count:  163, deletions_count:  132},
		{sha: "328cc0e856b73881175692075ea971614a0478fe", date: "2022-01-06 07:58:48 UTC", description: "Add acknowledgement config to globals", pr_number:                                                                              10374, scopes: ["config"], type:                       "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:      20, insertions_count:  151, deletions_count:  79},
		{sha: "9b28326ef9acd318e910b65cb9fb57c66b0e860a", date: "2022-01-06 23:35:51 UTC", description: "bump async-graphql from 3.0.19 to 3.0.20", pr_number:                                                                           10732, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "f01f8016c173ffa1151b026ea8ff5923ff1a6e82", date: "2022-01-06 23:36:21 UTC", description: "bump clap from 3.0.4 to 3.0.5", pr_number:                                                                                      10733, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "dc9d914a7bec5da891c1978469e3dae636c0c466", date: "2022-01-06 23:36:50 UTC", description: "bump ordered-float from 2.9.0 to 2.10.0", pr_number:                                                                            10731, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "81851e73b1a22cf1077d83bf9ab0e49accc262f0", date: "2022-01-07 06:13:18 UTC", description: "bump serde_json from 1.0.73 to 1.0.74", pr_number:                                                                              10648, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:   9, deletions_count:    9},
		{sha: "2d2479d0de9ae131051b47427ac5f8f8849cbb1d", date: "2022-01-07 06:14:07 UTC", description: "bump num_enum from 0.5.5 to 0.5.6", pr_number:                                                                                  10650, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    6},
		{sha: "8bd3eeaa0e7bc2b0ae5334485fb8cc144e6699ec", date: "2022-01-07 09:58:14 UTC", description: "prep for additional branching work", pr_number:                                                                                 10549, scopes: ["sources"], type:                      "chore", breaking_change:       false, author: "Luke Steensen", files_count:      66, insertions_count:  869, deletions_count:  871},
		{sha: "77430772f778b912d390e882dcf326f9a21d7ef2", date: "2022-01-08 02:17:40 UTC", description: "migration clickhouse integration tests to docker-compose", pr_number:                                                           10736, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     5, insertions_count:   47, deletions_count:   63},
		{sha: "b09cde087470834c9ddc1bcd5f6d70850039885f", date: "2022-01-08 02:45:38 UTC", description: "bump async-graphql-warp from 3.0.19 to 3.0.20", pr_number:                                                                      10750, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "7f20047fe5951e14acfb4c93b390e75c127e36c5", date: "2022-01-08 04:07:53 UTC", description: "branching without defaults", pr_number:                                                                                         10640, scopes: ["topology"], type:                     "enhancement", breaking_change: false, author: "Luke Steensen", files_count:      94, insertions_count:  777, deletions_count:  451},
		{sha: "c2b538b06a56880a29c4cdf12fbd6ee8aa632d2d", date: "2022-01-08 05:31:10 UTC", description: "native event encoding RFC", pr_number:                                                                                          9935, scopes: ["rfc"], type:                           "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      1, insertions_count:   186, deletions_count:  0},
		{sha: "701c40e2bc5e5d7166b7dc086b9a2f5106b2aec3", date: "2022-01-11 01:31:06 UTC", description: "Update component spec for multiple outputs", pr_number:                                                                         10687, scopes: ["internal docs"], type:                "docs", breaking_change:        false, author: "Will", files_count:               1, insertions_count:   3, deletions_count:    0},
		{sha: "61bdf2d1425f7175988f9d01f163154c68cefefa", date: "2022-01-11 02:56:56 UTC", description: "bump sha2 from 0.10.0 to 0.10.1", pr_number:                                                                                    10749, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "fea76d8416d10da449573350d65946ffffe06f81", date: "2022-01-11 03:10:35 UTC", description: "bump wiremock from 0.5.9 to 0.5.10", pr_number:                                                                                 10759, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   14, deletions_count:   3},
		{sha: "3ce45b409f649da484e219303bda3da6df4cca6e", date: "2022-01-11 04:11:31 UTC", description: "Use multiple outputs for route", pr_number:                                                                                     10738, scopes: ["route transform"], type:              "chore", breaking_change:       false, author: "Will", files_count:               3, insertions_count:   159, deletions_count:  95},
		{sha: "ab4fa32830d2c5ff139f36dbfb3d418daf58c760", date: "2022-01-11 09:22:25 UTC", description: "bump redis from 0.21.4 to 0.21.5", pr_number:                                                                                   10768, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "322222640704dc62bcf741e5d2bc2b2fb8f197b6", date: "2022-01-11 09:23:32 UTC", description: "bump memmap2 from 0.5.0 to 0.5.2", pr_number:                                                                                   10769, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "cd3409b59838fb09762474051aa7b28a3b97503a", date: "2022-01-11 04:57:43 UTC", description: "bump infer from 0.5.0 to 0.6.0", pr_number:                                                                                     10757, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "6a393921691768a987a99e4a1ec2851347731b44", date: "2022-01-11 11:04:23 UTC", description: "migrate loki to use docker-compose for integration tests", pr_number:                                                           10753, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     5, insertions_count:   68, deletions_count:   85},
		{sha: "340f9390332ca4dbd4e575dc04f23be61879f83e", date: "2022-01-11 11:04:52 UTC", description: "migrate docker integration tests", pr_number:                                                                                   10766, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     3, insertions_count:   31, deletions_count:   5},
		{sha: "7a4eed99893dfa1f155af87a41c15ac50ed18d54", date: "2022-01-11 10:41:26 UTC", description: "bump rkyv from 0.7.28 to 0.7.29", pr_number:                                                                                    10773, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "ed7fcc6dbec9f5a0e7304209d5a9f2c19af852db", date: "2022-01-11 10:48:12 UTC", description: "bump crossbeam-utils from 0.8.5 to 0.8.6", pr_number:                                                                           10774, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "24a6d4b759c52ceadd19cd5857ccdd0742f6c83e", date: "2022-01-11 09:26:43 UTC", description: "kubernetes_logs rewrite RFC", pr_number:                                                                                        10301, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   138, deletions_count:  0},
		{sha: "d466eb75455cc28e9e8cfc97a879d1ccc256cd62", date: "2022-01-11 23:33:30 UTC", description: "migrate prometheus integration tests", pr_number:                                                                               10765, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     6, insertions_count:   85, deletions_count:   64},
		{sha: "816ff24d21ed6ff4dcd4ab544d98212411a09afa", date: "2022-01-12 00:11:19 UTC", description: "bump indexmap from 1.7.0 to 1.8.0", pr_number:                                                                                  10758, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   7, deletions_count:    7},
		{sha: "5351eca3f00b04c6d4a5f431159bd3b1d62ccdb5", date: "2022-01-12 00:12:15 UTC", description: "bump reqwest from 0.11.8 to 0.11.9", pr_number:                                                                                 10782, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   8, deletions_count:    7},
		{sha: "7301856eb375f66de1a0c610953de0db3ddd070f", date: "2022-01-12 00:13:16 UTC", description: "bump rust_decimal from 1.19.0 to 1.20.0", pr_number:                                                                            10781, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "2f2733b276cde4a9401679fd7f3ac454ce786453", date: "2022-01-12 06:53:56 UTC", description: "Add RFC for \"Framing and Codecs - Sinks\"", pr_number:                                                                         9864, scopes: ["codecs"], type:                        "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      1, insertions_count:   130, deletions_count:  0},
		{sha: "2c487f8bea1ee2826814f769f2139afe81a0d587", date: "2022-01-12 03:58:50 UTC", description: "distributions should not grow unbounded over time", pr_number:                                                                  10741, scopes: ["prometheus_exporter sink"], type:     "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      25, insertions_count:  1289, deletions_count: 588},
		{sha: "ea0d002f4f26522764314f176af6a0e6c3adc28c", date: "2022-01-12 09:49:50 UTC", description: "bump snafu from 0.6.10 to 0.7.0", pr_number:                                                                                    10665, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    80, insertions_count:  324, deletions_count:  277},
		{sha: "3c91bc5e4970defa5f04d806b1b62112084894ea", date: "2022-01-12 05:59:12 UTC", description: "Update transforms descriptions", pr_number:                                                                                     10802, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    3, insertions_count:   3, deletions_count:    1},
		{sha: "f238b79467207df746c61ea2b34d502969061696", date: "2022-01-12 03:56:14 UTC", description: "Address sources of non-vector variability", pr_number:                                                                          10730, scopes: [], type:                               "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 56, insertions_count:  508, deletions_count:  377},
		{sha: "904431d30dc00229d07e13f21d8e2d502f75fe62", date: "2022-01-12 04:18:36 UTC", description: "bump clap from 3.0.5 to 3.0.6", pr_number:                                                                                      10789, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "426ff2950aa7740700c0dc75870e8ce0b48620d2", date: "2022-01-12 04:18:52 UTC", description: "bump async-graphql from 3.0.20 to 3.0.21", pr_number:                                                                           10790, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "6dbcbc51b13cb9d76cdad194aa2ce81ea8fa33e5", date: "2022-01-12 07:27:17 UTC", description: "Fix type in nginx_metrics source documenatation", pr_number:                                                                    10799, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "e936db348a5e5bac32a070c423637b88db04e8ed", date: "2022-01-12 07:54:10 UTC", description: "Fix get_gnu_musl_glibc function", pr_number:                                                                                    10804, scopes: ["setup"], type:                        "fix", breaking_change:         false, author: "Spencer Gilbert", files_count:    1, insertions_count:   11, deletions_count:   9},
		{sha: "9c136c7cddd48f0768c1e0134411a55607457333", date: "2022-01-12 08:08:22 UTC", description: "lint/fmt/clippy issues due to a bad merge", pr_number:                                                                          10805, scopes: ["prometheus_exporter sink"], type:     "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      2, insertions_count:   56, deletions_count:   57},
		{sha: "95bdb12745415cf4a8f0e3ecc3aae722bbc93dde", date: "2022-01-12 09:43:04 UTC", description: "Add script to regenerate Kubernetes manifests based on latest Helm chart", pr_number:                                           10744, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    26, insertions_count:  697, deletions_count:  355},
		{sha: "13bc473884bcfc544472b57419b7a0da1b789aac", date: "2022-01-13 00:12:52 UTC", description: "migrate fluent integration test", pr_number:                                                                                    10796, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     3, insertions_count:   33, deletions_count:   5},
		{sha: "a29192fa072cd2963ee08532b802563c6d4f5410", date: "2022-01-13 00:18:13 UTC", description: "migrate datadog-metrics integration tests", pr_number:                                                                          10794, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     3, insertions_count:   32, deletions_count:   5},
		{sha: "cf1714bef5029f0bf46b9199a65b0e1737296855", date: "2022-01-13 01:39:16 UTC", description: "migrate influxdb integration test", pr_number:                                                                                  10793, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     10, insertions_count:  192, deletions_count:  93},
		{sha: "f9e114284eb65367dab7e46624e579539e4afc4c", date: "2022-01-13 01:48:35 UTC", description: "migrate dnstap integration tests", pr_number:                                                                                   10762, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     6, insertions_count:   145, deletions_count:  118},
		{sha: "308bfc8afc52a71a1584d543626512d9d75c9481", date: "2022-01-13 02:57:42 UTC", description: "migrate eventstore integration test", pr_number:                                                                                10795, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     4, insertions_count:   42, deletions_count:   46},
		{sha: "095edb770ef405a9db2eeb7175b77db0dfd6640f", date: "2022-01-13 03:44:20 UTC", description: "add an option to route metrics and logs to different outputs", pr_number:                                                       10776, scopes: ["datadog_agent source"], type:         "enhancement", breaking_change: false, author: "Pierre Rognant", files_count:     4, insertions_count:   245, deletions_count:  38},
		{sha: "5247b9e4dc608d7662a6440b027859e9772343f2", date: "2022-01-13 04:41:18 UTC", description: "migrate gcp integration test", pr_number:                                                                                       10798, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     5, insertions_count:   45, deletions_count:   63},
		{sha: "4872d58285004172dbe14ff5f6100828352927a5", date: "2022-01-13 02:25:08 UTC", description: "bump assert_cmd from 2.0.2 to 2.0.3", pr_number:                                                                                10812, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "ea435375e2cfca64db27a7c2e5fd40fb3e097cf9", date: "2022-01-13 02:29:11 UTC", description: "bump async-graphql-warp from 3.0.20 to 3.0.21", pr_number:                                                                      10813, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "ad5340b2c809f700fcd21ad874e519b78d754731", date: "2022-01-13 03:42:31 UTC", description: "Add component outputs as structured cue data", pr_number:                                                                       10616, scopes: [], type:                               "docs", breaking_change:        false, author: "Will", files_count:               11, insertions_count:  73, deletions_count:   17},
		{sha: "07e00a0c4598dca93d00f61b55ac25006af1a465", date: "2022-01-13 10:30:34 UTC", description: "Implement encoding support structures", pr_number:                                                                              10108, scopes: ["codecs"], type:                       "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      9, insertions_count:   554, deletions_count:  37},
		{sha: "f69f93130a571a8b118820d34cce06eea5f422e4", date: "2022-01-13 06:14:36 UTC", description: "Document outputs for datadog agent source", pr_number:                                                                          10821, scopes: ["datadog_agent source"], type:         "docs", breaking_change:        false, author: "Will", files_count:               1, insertions_count:   21, deletions_count:   0},
		{sha: "2fa8c7d304581ab925f81ec5e8f6ca7762033b95", date: "2022-01-13 08:08:37 UTC", description: "Remove lading_common dependency", pr_number:                                                                                    10824, scopes: [], type:                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    2, insertions_count:   0, deletions_count:    65},
		{sha: "a35abf9690670b4e0457092b5d147ea87f7db9a7", date: "2022-01-13 08:39:16 UTC", description: "fix shutdown bug with disk_v2 buffer", pr_number:                                                                               10808, scopes: ["buffers"], type:                      "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      4, insertions_count:   114, deletions_count:  40},
		{sha: "66a4989eca2b39140c29c2bacf5444f3193f922a", date: "2022-01-13 16:09:26 UTC", description: "Implement adapter for seamless migration from legacy `EncodingConfiguration` to `FramingConfig`/`SerializerConfig`", pr_number: 10253, scopes: ["codecs"], type:                       "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      2, insertions_count:   345, deletions_count:  11},
		{sha: "7acbdbd3eeb842349633b31340687bab46b9156e", date: "2022-01-13 09:32:18 UTC", description: "Fix issue in quickstart identified by check-markdown", pr_number:                                                               10830, scopes: ["docs"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   3, deletions_count:    0},
		{sha: "9583b338f2f20848e17a0e0176c2011b05721f08", date: "2022-01-13 11:59:32 UTC", description: "Update debug output for a disconnected topology", pr_number:                                                                    10831, scopes: ["unit tests"], type:                   "chore", breaking_change:       false, author: "Will", files_count:               1, insertions_count:   26, deletions_count:   15},
		{sha: "f56c56100949a026b522717abe50f3ac606c7c02", date: "2022-01-13 21:26:32 UTC", description: "bump clap from 3.0.6 to 3.0.7", pr_number:                                                                                      10837, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "cf34f9de8df276340b15e582d2cd53ffe70e0fd2", date: "2022-01-13 21:29:38 UTC", description: "bump openssl-probe from 0.1.4 to 0.1.5", pr_number:                                                                             10838, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "5daaac83befec8ad39932a6d50f880d188b46f1f", date: "2022-01-14 01:58:56 UTC", description: "migrate humio integration test", pr_number:                                                                                     10815, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     4, insertions_count:   76, deletions_count:   63},
		{sha: "0ba3f41ce779c2cc9d6478aae9a9e378cd496259", date: "2022-01-14 04:01:57 UTC", description: "Handle `framing.character_delimiter.delimiter` as `char` in config serialization", pr_number:                                   10829, scopes: ["codecs"], type:                       "fix", breaking_change:         false, author: "Pablo Sichert", files_count:      2, insertions_count:   64, deletions_count:   0},
		{sha: "30448230d2ac418acfaa87841659918d33953590", date: "2022-01-14 05:15:13 UTC", description: "Implement `JsonSerializer`", pr_number:                                                                                         10729, scopes: ["codecs"], type:                       "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      3, insertions_count:   68, deletions_count:   6},
		{sha: "b389de4566994eb726d0fc0ef04572766b9a8641", date: "2022-01-14 07:12:56 UTC", description: "migrate pulsar integration test", pr_number:                                                                                    10841, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     5, insertions_count:   45, deletions_count:   60},
		{sha: "7885ca6e5635db2245b01cf855635999b299fa86", date: "2022-01-14 08:30:31 UTC", description: "migrate postgres integration test", pr_number:                                                                                  10840, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     9, insertions_count:   183, deletions_count:  104},
		{sha: "7dc1d2a26ccd0bc4e91fd99e007e3895f624dd5b", date: "2022-01-14 02:13:04 UTC", description: "Add base support for arrays of events to sinks", pr_number:                                                                     10809, scopes: ["core"], type:                         "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:      72, insertions_count:  343, deletions_count:  117},
		{sha: "2c02b4742f8974d05ea32a76bab13b0e0a532e75", date: "2022-01-14 09:31:26 UTC", description: "New Relic sink for Events, Metrics and Logs", pr_number:                                                                        9764, scopes: ["new sink"], type:                      "feat", breaking_change:        false, author: "Andreu", files_count:             17, insertions_count:  1121, deletions_count: 0},
		{sha: "bd95bdcca1d69a64966ac181ad1660f3bd08c1cf", date: "2022-01-14 03:58:25 UTC", description: "Update Splunk HEC integration tests", pr_number:                                                                                10819, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Will", files_count:               8, insertions_count:   121, deletions_count:  94},
		{sha: "78ef1fc6d23baebc4c814a546535a18a423e5652", date: "2022-01-14 05:26:43 UTC", description: "Fix `new_relic` sink definition", pr_number:                                                                                    10851, scopes: ["new_relic sink"], type:               "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      2, insertions_count:   2, deletions_count:    2},
		{sha: "cdfa82d32be84c4b140f2d2971cee7f9c6610878", date: "2022-01-14 09:46:22 UTC", description: "correctly migrate old disk v1 buffer data dir when possible", pr_number:                                                        10826, scopes: ["buffers"], type:                      "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      8, insertions_count:   503, deletions_count:  20},
		{sha: "5cb9ba95c9aa533447e8d72dd3f8ff75ff997bc9", date: "2022-01-15 03:48:49 UTC", description: "Add a workflow for opening issues for security vulnerabilities", pr_number:                                                     10865, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   17, deletions_count:   0},
		{sha: "316089253076748428012292be39771b197fc4ff", date: "2022-01-15 06:30:35 UTC", description: "Sort soaks by mean change %, not absolute change", pr_number:                                                                   10857, scopes: ["performance"], type:                  "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   2, deletions_count:    2},
		{sha: "48757c17f11cd162cbef4abbc8ea7505bb9c1d2e", date: "2022-01-18 00:33:26 UTC", description: "bump smallvec from 1.7.0 to 1.8.0", pr_number:                                                                                  10859, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "48374545f99647d6a990774935e743bbdb29c1fa", date: "2022-01-19 02:25:45 UTC", description: "Fix `TapSink` not flushing properly", pr_number:                                                                                10866, scopes: ["graphql api"], type:                  "fix", breaking_change:         false, author: "Nathan Fox", files_count:         11, insertions_count:  150, deletions_count:  68},
		{sha: "16709905fec0b972f184bbcb1d09f551a7b98cc5", date: "2022-01-19 03:55:13 UTC", description: "Add output tag to route event discarded metric", pr_number:                                                                     10834, scopes: ["route transform"], type:              "chore", breaking_change:       false, author: "Will", files_count:               5, insertions_count:   40, deletions_count:   5},
		{sha: "04e0f66d3040786b75d06dbe3568c5257af1be88", date: "2022-01-19 09:11:39 UTC", description: "bump docker/build-push-action from 2.7.0 to 2.8.0", pr_number:                                                                  10894, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   3, deletions_count:    3},
		{sha: "f2c7f65a4ae604bcd84e4b10cfd8ba5f44d0e3dd", date: "2022-01-19 04:47:02 UTC", description: "bump rmp-serde from 0.15.5 to 1.0.0", pr_number:                                                                                10874, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "660d1f670b88830ba5b385dea3a5c82bfb86ae2e", date: "2022-01-19 04:47:53 UTC", description: "bump goauth from 0.10.0 to 0.11.0", pr_number:                                                                                  10876, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "f76c72181a4c5c3c8e64a3696907a47eec950ff6", date: "2022-01-19 05:24:04 UTC", description: "detect partially written data file + wait on writer/reader file lap", pr_number:                                                10868, scopes: ["buffers"], type:                      "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      8, insertions_count:   639, deletions_count:  152},
		{sha: "85edc36d518308a04711632bc785f789f6855344", date: "2022-01-19 07:10:03 UTC", description: "Add highlight article for Vector unit testing updates", pr_number:                                                              10827, scopes: ["highlights website"], type:           "docs", breaking_change:        false, author: "Will", files_count:               1, insertions_count:   48, deletions_count:   0},
		{sha: "c0da3b314f70c6dde177b86b8e4022947b797f89", date: "2022-01-19 04:26:23 UTC", description: "bump mlua from 0.7.1 to 0.7.2", pr_number:                                                                                      10896, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "a32e51ff98224d006a3d1c22605ec3ff89e68a61", date: "2022-01-19 07:32:38 UTC", description: "bump structopt from 0.3.25 to 0.3.26", pr_number:                                                                               10897, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "a4c45e22004bc7e4fee154f1addc7b9d07209a07", date: "2022-01-19 06:32:38 UTC", description: "revert bump mlua from 0.7.1 to 0.7.2", pr_number:                                                                               10902, scopes: ["deps"], type:                         "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      3, insertions_count:   4, deletions_count:    4},
		{sha: "899370b513da069f3b464a1023658268c576cb3c", date: "2022-01-19 06:54:49 UTC", description: "bump assert_cmd from 2.0.3 to 2.0.4", pr_number:                                                                                10860, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "7faaf65d8b2130aeb6d49541d98d1e4b0976a7d8", date: "2022-01-19 16:20:32 UTC", description: "bump serde_json from 1.0.74 to 1.0.75", pr_number:                                                                              10901, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:   9, deletions_count:    9},
		{sha: "28df74839a3ea6a9ddcecc6d751964de90f2fc09", date: "2022-01-19 16:24:02 UTC", description: "bump clap from 3.0.7 to 3.0.10", pr_number:                                                                                     10905, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "1bb7cbdcf5936276a772d735f9d42928a9fb28a8", date: "2022-01-19 18:01:13 UTC", description: "bump async-graphql from 3.0.21 to 3.0.22", pr_number:                                                                           10908, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "0bb68d80cb39000254b537d9e7b2c9f46716b48e", date: "2022-01-19 18:12:57 UTC", description: "bump console-subscriber from 0.1.0 to 0.1.1", pr_number:                                                                        10909, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   6, deletions_count:    5},
		{sha: "dc1ad916ca351792646100fed99201137f68d2e7", date: "2022-01-19 12:25:55 UTC", description: "Move the conversion to EventArray out of VectorSink", pr_number:                                                                10889, scopes: ["core"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      41, insertions_count:  288, deletions_count:  170},
		{sha: "3ffedf65122e07255ea27ae0f7ea7b9345c8f81c", date: "2022-01-19 14:30:17 UTC", description: "try tokio mpsc for SourceSender", pr_number:                                                                                    10907, scopes: ["performance"], type:                  "chore", breaking_change:       false, author: "Luke Steensen", files_count:      2, insertions_count:   10, deletions_count:   7},
		{sha: "cd5abcaec91649c22d3a51a7a116b33fa2f17b31", date: "2022-01-19 23:47:33 UTC", description: "bump uaparser from 0.4.0 to 0.5.0", pr_number:                                                                                  10910, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    14},
		{sha: "a1158637a6675053245f99ebe0655ba88ae85eb9", date: "2022-01-19 23:48:23 UTC", description: "bump async-graphql-warp from 3.0.21 to 3.0.22", pr_number:                                                                      10911, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "4ba29c1360201d58eddc218789b23fdb1f2d09c6", date: "2022-01-20 09:07:30 UTC", description: "bump rand_distr from 0.4.2 to 0.4.3", pr_number:                                                                                10916, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "b958323269e7d0aaeb210b0180f2da6b024ea52f", date: "2022-01-20 04:19:08 UTC", description: "Build soak image for PRs", pr_number:                                                                                           10919, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   4, deletions_count:    1},
		{sha: "9bf32cf6095d951b45f43a9f30df390874f23951", date: "2022-01-20 07:21:19 UTC", description: "Flatten out core-common and buffers", pr_number:                                                                                10918, scopes: ["core"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      88, insertions_count:  128, deletions_count:  125},
		{sha: "5da3fc8fde893fe82b0c4408301f63b3e664afe9", date: "2022-01-20 09:30:22 UTC", description: "Fix handling of non UTF8 payloads", pr_number:                                                                                  10858, scopes: ["splunk_hec source"], type:            "chore", breaking_change:       false, author: "Will", files_count:               1, insertions_count:   35, deletions_count:   4},
		{sha: "20274b7a7641530b1cba9e8c1e0f730ffc6eed8c", date: "2022-01-20 14:30:57 UTC", description: "bump aws crates from 0.3.0 to 0.4.1", pr_number:                                                                                10785, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   45, deletions_count:   38},
		{sha: "c30da6ca0ee88b88e294bbc952afec4ed1894c7a", date: "2022-01-20 06:40:07 UTC", description: "Add condition config examples", pr_number:                                                                                      10328, scopes: ["external docs"], type:                "enhancement", breaking_change: false, author: "Luc Perkins", files_count:        3, insertions_count:   152, deletions_count:  71},
		{sha: "a72008618c99550ebcc6b6f1794cc74e87be6983", date: "2022-01-20 11:18:49 UTC", description: "Simplify encode/decode traits", pr_number:                                                                                      10921, scopes: ["buffers"], type:                      "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      10, insertions_count:  28, deletions_count:   38},
		{sha: "366de1bc8419f0684b46db1dc99e1ce4e76dc1ac", date: "2022-01-21 02:27:07 UTC", description: "bump tracing-subscriber from 0.3.5 to 0.3.6", pr_number:                                                                        10915, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   8, deletions_count:    8},
		{sha: "02a64edb742210b25a7c685c78cf4b6b43829e58", date: "2022-01-21 04:37:09 UTC", description: "bump mlua from 0.7.1 to 0.7.3", pr_number:                                                                                      10920, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "b81889dfcc62c6a1f16c03630e0e29b95457c191", date: "2022-01-21 06:27:56 UTC", description: "Pass through jemalloc page size env vars", pr_number:                                                                           10940, scopes: ["platforms"], type:                    "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   2, deletions_count:    0},
		{sha: "66cf650aa15f6446f4a66827ab4b8ac8a17d1d5d", date: "2022-01-21 15:42:15 UTC", description: "bump libc from 0.2.112 to 0.2.113", pr_number:                                                                                  10935, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "09d31a29e872ed5c7cc322ae493803ab0b7d2cf8", date: "2022-01-21 16:01:32 UTC", description: "bump aws-types from 0.4.1 to 0.5.1", pr_number:                                                                                 10924, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   61, deletions_count:   35},
		{sha: "31d2aa80a142a420e52ec49878fc9266b0b4075c", date: "2022-01-21 17:18:45 UTC", description: "bump socket2 from 0.4.2 to 0.4.3", pr_number:                                                                                   10944, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   9, deletions_count:    9},
		{sha: "b9ae50b9c259cbf58c2b19f8fae1692a478ed1cf", date: "2022-01-21 17:27:58 UTC", description: "bump goauth from 0.11.0 to 0.11.1", pr_number:                                                                                  10945, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "c8c8c9c4e549236361b584c04478663498f84693", date: "2022-01-22 04:04:25 UTC", description: "update `array` filter for `parse_groks` function to suppo…", pr_number:                                                         10537, scopes: ["remap"], type:                        "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      3, insertions_count:   49, deletions_count:   43},
		{sha: "a2f434a4563ac2a04a1a19c2a6560943574dd2ac", date: "2022-01-21 21:01:17 UTC", description: "Add known issue for 0.19.0 for character delimiter", pr_number:                                                                 10846, scopes: [], type:                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   5, deletions_count:    4},
		{sha: "671109076d7bb8f061d5537308b99e66c55f08ba", date: "2022-01-22 01:25:44 UTC", description: "tcp source backpressure RFC", pr_number:                                                                                        10803, scopes: ["sources"], type:                      "chore", breaking_change:       false, author: "Nathan Fox", files_count:         1, insertions_count:   160, deletions_count:  0},
		{sha: "4976ff21347e44496f3b3090d11b6f9ca38a1bec", date: "2022-01-22 00:10:52 UTC", description: "bump async-graphql from 3.0.22 to 3.0.23", pr_number:                                                                           10949, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "0402c8d25ae3f439f2ab8292bc660dd5c3f5a576", date: "2022-01-22 00:11:43 UTC", description: "bump aws-sdk-sqs from 0.5.0 to 0.5.2", pr_number:                                                                               10950, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   27, deletions_count:   27},
		{sha: "419b3b57167ee119f73b287755d40520f68107da", date: "2022-01-22 02:32:35 UTC", description: "ensure fanout is flushed on idle", pr_number:                                                                                   10948, scopes: ["topology"], type:                     "fix", breaking_change:         false, author: "Luke Steensen", files_count:      4, insertions_count:   72, deletions_count:   7},
		{sha: "a500a16fd1a1c030d369eacc25be52e369c02db5", date: "2022-01-22 09:29:28 UTC", description: "bump governor from 0.4.0 to 0.4.1", pr_number:                                                                                  10966, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "cbf03059fa6995f140a8b27bbeada47ec2621cbf", date: "2022-01-22 09:40:00 UTC", description: "bump crc32fast from 1.3.0 to 1.3.1", pr_number:                                                                                 10967, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "1ff4b4ce6a1c559d5041977c55c37e3ee0885f33", date: "2022-01-22 09:43:16 UTC", description: "bump async-graphql-warp from 3.0.22 to 3.0.23", pr_number:                                                                      10968, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "7ae197ea16e809885382f0f7c4440502d485aa76", date: "2022-01-22 09:52:02 UTC", description: "bump aws-config from 0.5.1 to 0.5.2", pr_number:                                                                                10947, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   9, deletions_count:    9},
		{sha: "8f357be293876842aef3fca9034c53e3ba7d96e0", date: "2022-01-22 02:46:34 UTC", description: "Fix links for RELEASES.md", pr_number:                                                                                          10845, scopes: [], type:                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   7, deletions_count:    2},
		{sha: "3080030539e5e70860436779e6f8c05c4d6cfeea", date: "2022-01-22 03:22:39 UTC", description: "bump serde from 1.0.133 to 1.0.134", pr_number:                                                                                 10970, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:   11, deletions_count:   11},
		{sha: "560e3b4a1559a57c0c39c68476dd8a9eaea774fc", date: "2022-01-22 12:43:19 UTC", description: "bump lalrpop-util from 0.19.6 to 0.19.7", pr_number:                                                                            10936, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "d190e257f6a8ab953444c107e3683738216113a5", date: "2022-01-22 14:19:52 UTC", description: "bump lalrpop from 0.19.6 to 0.19.7", pr_number:                                                                                 10973, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   5, deletions_count:    5},
		{sha: "439b2677cfbdf462f12731c3de31111adb7ecbba", date: "2022-01-22 09:51:13 UTC", description: "Add unit tests for vector tap on multiple outputs", pr_number:                                                                  10972, scopes: ["api"], type:                          "chore", breaking_change:       false, author: "Will", files_count:               1, insertions_count:   128, deletions_count:  0},
		{sha: "84b6663e71d3e41150e07351c686891638cf361a", date: "2022-01-22 09:55:35 UTC", description: "Update EventsSent and component instrumentation for multiple outputs", pr_number:                                               10869, scopes: ["observability"], type:                "chore", breaking_change:       false, author: "Will", files_count:               39, insertions_count:  294, deletions_count:  40},
		{sha: "78e2d53365e2bf361e55e4f6a065e254accb21cf", date: "2022-01-25 03:08:32 UTC", description: "bump nanoid from 3.1.28 to 3.2.0 in /website", pr_number:                                                                       10981, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   3, deletions_count:    3},
		{sha: "062641407e9a32d4395ee660e406eae6ba2f13e8", date: "2022-01-25 06:40:39 UTC", description: "support nested paths in `keyvalue` filter for `parse_groks`", pr_number:                                                        10954, scopes: ["vrl"], type:                          "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      2, insertions_count:   31, deletions_count:   13},
		{sha: "498488b3a36b0a253e72834d9b4f1476ba55cb06", date: "2022-01-24 23:27:28 UTC", description: "bump serde_json from 1.0.75 to 1.0.78", pr_number:                                                                              10992, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:   10, deletions_count:   10},
		{sha: "1ca17fa9a8ae05d6f8ad093715904cfeba616ddd", date: "2022-01-24 23:30:26 UTC", description: "bump approx from 0.5.0 to 0.5.1", pr_number:                                                                                    10995, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "e28a84c997279244f68d7f5713ab7d807ce50666", date: "2022-01-25 09:43:16 UTC", description: "support `d` and `y` shorthands in `date` matcher of `parse_groks` function", pr_number:                                         10958, scopes: ["vrl"], type:                          "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      2, insertions_count:   14, deletions_count:   2},
		{sha: "29a055daa8a8be1af1a4d1abaa0ee7b504e2be1b", date: "2022-01-25 05:34:23 UTC", description: "Add non-interpolated VRL error message to dropped event", pr_number:                                                            10914, scopes: ["remap transform"], type:              "chore", breaking_change:       false, author: "Will", files_count:               4, insertions_count:   120, deletions_count:  21},
		{sha: "2e04be130ac8d0246c4c95ac7e03e2d1846d5aa8", date: "2022-01-25 12:52:06 UTC", description: "convert floats to integers when possible in `parse_groks`", pr_number:                                                          10961, scopes: ["vrl"], type:                          "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      3, insertions_count:   50, deletions_count:   38},
		{sha: "eeea28f6eec525ac3479671603308668ce569d16", date: "2022-01-25 12:58:52 UTC", description: "bump clap from 3.0.10 to 3.0.11", pr_number:                                                                                    11003, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "39b455fe90df19165758a122493eb27b753fbded", date: "2022-01-25 14:41:22 UTC", description: "Add socket -> socket soak test", pr_number:                                                                                     10960, scopes: ["tests"], type:                        "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      9, insertions_count:   260, deletions_count:  1},
		{sha: "4ee7b3cc50fa0f827585e8f047353b7a3218ebc9", date: "2022-01-25 14:39:10 UTC", description: "bump listenfd from 0.3.5 to 0.5.0", pr_number:                                                                                  10999, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "a90ec91438fbe9d717ea9b05d9f47caa3301ffe7", date: "2022-01-25 14:56:00 UTC", description: "bump async-graphql from 3.0.23 to 3.0.24", pr_number:                                                                           10998, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "573cda346af0782fbe76c4d8975be8c4b70ea84f", date: "2022-01-25 15:21:32 UTC", description: "bump anyhow from 1.0.52 to 1.0.53", pr_number:                                                                                  10994, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "b69cadc8cc445fcb50dfa843861b0f67fa992f5f", date: "2022-01-25 11:36:16 UTC", description: "Fix flakey vector tap test", pr_number:                                                                                         11008, scopes: ["api"], type:                          "chore", breaking_change:       false, author: "Will", files_count:               1, insertions_count:   2, deletions_count:    2},
		{sha: "8b7d988706ded4f9475e4277e6a27be34e188b83", date: "2022-01-25 17:13:38 UTC", description: "bump clap from 3.0.11 to 3.0.12", pr_number:                                                                                    11010, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "ce1d364d4b0d491a01ee548c80bcefb0860b17e5", date: "2022-01-25 18:27:56 UTC", description: "bump serde from 1.0.134 to 1.0.135", pr_number:                                                                                 10996, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:   11, deletions_count:   11},
		{sha: "f91af07a74073c2d0ec39fd827544406b7b98ffa", date: "2022-01-25 18:48:04 UTC", description: "bump async-graphql-warp from 3.0.23 to 3.0.24", pr_number:                                                                      11011, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "3ed8b79265a848838b89d373320c3f62c395db32", date: "2022-01-26 04:24:50 UTC", description: "correct alias resolution for `parse_groks`", pr_number:                                                                         10938, scopes: ["vrl"], type:                          "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      2, insertions_count:   290, deletions_count:  298},
		{sha: "56c23083134715a66f2fe1bee4da157e49579f4b", date: "2022-01-26 05:22:22 UTC", description: "support cross-type numerical comparisons for match_datadog_query", pr_number:                                                   10952, scopes: ["vrl"], type:                          "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      2, insertions_count:   59, deletions_count:   0},
		{sha: "ad6ae66b2dd6968a763fced17462acb28be8e424", date: "2022-01-26 06:28:05 UTC", description: "support empty values in `keyvalue` filter of `parse_groks`", pr_number:                                                         10956, scopes: ["vrl"], type:                          "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      2, insertions_count:   8, deletions_count:    1},
		{sha: "7d0530c536c7db84b88fc7a0c820049b7cb1d693", date: "2022-01-26 01:05:04 UTC", description: "Set up task transforms to accept event arrays", pr_number:                                                                      10974, scopes: ["transforms"], type:                   "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      17, insertions_count:  133, deletions_count:  77},
		{sha: "075328e965d3788fd5bbdb430cd13dc545325df1", date: "2022-01-26 01:46:03 UTC", description: "Add histogram to soak test artifacts", pr_number:                                                                               11012, scopes: ["performance"], type:                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   17, deletions_count:   2},
		{sha: "e5460ecf0f4d24abeb9fbf3a3c1fce3ed226c9f7", date: "2022-01-26 12:05:59 UTC", description: "bump async-graphql from 3.0.24 to 3.0.25", pr_number:                                                                           11014, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   15, deletions_count:   15},
		{sha: "4ebe2c47f2cffd690438e12c349b7e6f5a0705c2", date: "2022-01-26 04:16:58 UTC", description: "Default to version 2", pr_number:                                                                                               11023, scopes: ["vector source", "vector sink"], type: "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      3, insertions_count:   22, deletions_count:   3},
		{sha: "6c45afbee0d1ff45b1d4225e1fb70f30d6ac6b57", date: "2022-01-26 06:26:12 UTC", description: "Fix example in CSV enrichment highlight", pr_number:                                                                            11026, scopes: ["enrichment"], type:                   "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      1, insertions_count:   2, deletions_count:    2},
		{sha: "708f597db18a81a1f908b09e5c7654a9a1ff9352", date: "2022-01-26 09:37:28 UTC", description: "Remove documented elasticsearch sink options that have been removed from code", pr_number:                                      11028, scopes: ["external docs"], type:                "fix", breaking_change:         false, author: "Spencer Gilbert", files_count:    1, insertions_count:   2, deletions_count:    27},
		{sha: "3baa701820c626270318619091f402551e51e5af", date: "2022-01-26 16:59:20 UTC", description: "bump libc from 0.2.113 to 0.2.114", pr_number:                                                                                  11027, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "f97495bd7d308180ee355af9cf7d3bffc5b165d3", date: "2022-01-26 18:22:03 UTC", description: "bump serde from 1.0.135 to 1.0.136", pr_number:                                                                                 11031, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:   11, deletions_count:   11},
		{sha: "5cd701e258c848ef73f96b3bb315c3c1b6139b35", date: "2022-01-26 18:26:49 UTC", description: "bump tracing-subscriber from 0.3.6 to 0.3.7", pr_number:                                                                        11032, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   8, deletions_count:    8},
		{sha: "c8df50f88f48fa0fe1a30c8540dee21a06dd91ed", date: "2022-01-27 00:01:59 UTC", description: "comply with component spec", pr_number:                                                                                         10861, scopes: ["aws_ecs_metrics source"], type:       "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     5, insertions_count:   123, deletions_count:  48},
		{sha: "1612ff2e31827c5c4f949eee1127d98415f2e049", date: "2022-01-27 08:36:28 UTC", description: "comply with component spec", pr_number:                                                                                         11017, scopes: ["demo_logs source"], type:             "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     2, insertions_count:   21, deletions_count:   11},
		{sha: "35d2a7e06d5b8ad8b59cba1cee7e1c95919f0138", date: "2022-01-27 00:51:59 UTC", description: "Document unsupported version 2 options", pr_number:                                                                             11040, scopes: ["vector source", "vector sink"], type: "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      2, insertions_count:   19, deletions_count:   0},
		{sha: "cf3e30b9ccb564325d635f2c03d4faada95e4fa6", date: "2022-01-27 06:37:57 UTC", description: "spelling and grammar fixes for the native event encoding RFC.", pr_number:                                                      11045, scopes: ["rfc"], type:                          "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      1, insertions_count:   13, deletions_count:   16},
		{sha: "d81f52d8f406f98cfeb83f41a423a9ca24a29659", date: "2022-01-27 05:30:08 UTC", description: "Update revision used for the Azure SDK/chase changes as necessary", pr_number:                                                  11030, scopes: ["azure service"], type:                "chore", breaking_change:       false, author: "Arch Oversight", files_count:     6, insertions_count:   73, deletions_count:   146},
		{sha: "70f69351a53813c969844290ffdc9b5aef048d32", date: "2022-01-27 05:22:58 UTC", description: "bump k8s-openapi from 0.13.1 to 0.14.0", pr_number:                                                                             10993, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   5, deletions_count:    5},
		{sha: "f4e56c120208cffc8e677e3196a1197db8e1905c", date: "2022-01-27 08:36:20 UTC", description: "add support for codec/schema evolution", pr_number:                                                                             11039, scopes: ["buffers"], type:                      "enhancement", breaking_change: false, author: "Toby Lawrence", files_count:      33, insertions_count:  795, deletions_count:  145},
		{sha: "7c364fba165ff3b1810a12c3a2a9cd356513a1d7", date: "2022-01-27 13:55:35 UTC", description: "bump async-graphql from 3.0.25 to 3.0.26", pr_number:                                                                           11046, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "edaa963f5b69b57458c266bef6f352ad21951890", date: "2022-01-27 14:19:01 UTC", description: "bump rkyv from 0.7.29 to 0.7.30", pr_number:                                                                                    11048, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "7d089adc653266dcec1152a9936b3d81e68ccdec", date: "2022-01-27 15:24:28 UTC", description: "bump async-graphql-warp from 3.0.25 to 3.0.26", pr_number:                                                                      11051, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "1e94aaa4714608f6b7fe51665c1dd57d4c64f3ca", date: "2022-01-27 15:50:00 UTC", description: "bump clap from 3.0.12 to 3.0.13", pr_number:                                                                                    11052, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "8160b5e7423715823664fe501e9234560f811e1e", date: "2022-01-27 16:51:30 UTC", description: "bump aws-types from 0.5.2 to 0.6.0", pr_number:                                                                                 11047, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   37, deletions_count:   37},
		{sha: "8f5d56c4d438d14fba8d025449a94be057f81bbd", date: "2022-01-27 12:40:23 UTC", description: "harden buffer directory name generation for disk buffer v1", pr_number:                                                         11054, scopes: ["buffers"], type:                      "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      6, insertions_count:   248, deletions_count:  3},
		{sha: "f7916d1637b3633ebe4a5e902cafdb57b62d3b3b", date: "2022-01-27 17:51:38 UTC", description: "bump rkyv from 0.7.30 to 0.7.31", pr_number:                                                                                    11055, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "371b99d48fea6790168bc0d0594d74477c9702c0", date: "2022-01-27 14:59:09 UTC", description: "Introduce transform output buffer abstraction", pr_number:                                                                      11019, scopes: ["transforms"], type:                   "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      39, insertions_count:  208, deletions_count:  142},
		{sha: "81e42eb4c8b8decf43c2a6b6cadab230c7935ab0", date: "2022-01-27 14:32:58 UTC", description: "Ensure we collect at least 200 `bytes_written` samples", pr_number:                                                             11043, scopes: [], type:                               "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 3, insertions_count:   19, deletions_count:   16},
		{sha: "3da3b0f3a589c69ea1ad26bb401dd9ef0a317d5d", date: "2022-01-28 00:32:21 UTC", description: "comply with component spec", pr_number:                                                                                         10880, scopes: ["dnstap source"], type:                "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     4, insertions_count:   40, deletions_count:   22},
		{sha: "bd2de20bad261ee37c76996430fad098af99c31d", date: "2022-01-28 00:37:27 UTC", description: "comply with component spec", pr_number:                                                                                         10893, scopes: ["exec source"], type:                  "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     3, insertions_count:   76, deletions_count:   28},
		{sha: "8ce2c6788dabdde69d6acd1daf0550d56d7c9268", date: "2022-01-28 01:42:44 UTC", description: "comply with component spec", pr_number:                                                                                         10932, scopes: ["datadog_agent source"], type:         "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     2, insertions_count:   6, deletions_count:    5},
		{sha: "e14c483ea84190b4a449801d308331131f4a05a9", date: "2022-01-28 01:48:40 UTC", description: "comply with component spec", pr_number:                                                                                         10890, scopes: ["eventstoredb_metrics source"], type:  "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     3, insertions_count:   44, deletions_count:   32},
		{sha: "02482a6dd89860c4445a57d225790bd9c139a1e2", date: "2022-01-28 05:11:05 UTC", description: "update `parse_groks` to support DOTALL mode", pr_number:                                                                        10538, scopes: ["vrl"], type:                          "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      5, insertions_count:   73, deletions_count:   5},
		{sha: "d7527ca1f364acde9fa9fa80566d61382b5a8ed5", date: "2022-01-28 08:00:53 UTC", description: "comply with component spec", pr_number:                                                                                         11038, scopes: ["kafka source"], type:                 "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     3, insertions_count:   74, deletions_count:   24},
		{sha: "2c06286cc74fd2df6f78552a05a1dbb5d12a399d", date: "2022-01-28 12:32:56 UTC", description: "skaffold: Update `.metadata.name` to fix local vector binary build", pr_number:                                                 11004, scopes: ["dev"], type:                          "fix", breaking_change:         false, author: "Pranjal Gupta", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "cbd5d3bdb350888585245eee106acfbd6ea16fa1", date: "2022-01-28 13:29:40 UTC", description: "Add compression to loki sink", pr_number:                                                                                       10953, scopes: ["loki sink"], type:                    "feat", breaking_change:        false, author: "3JIou_from_home", files_count:    4, insertions_count:   27, deletions_count:   15},
		{sha: "3e166af773981a69b0ba20f206c35b801338b3ad", date: "2022-01-28 04:43:46 UTC", description: "Internal Events updates", pr_number:                                                                                            10971, scopes: ["loki sink"], type:                    "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count:    3, insertions_count:   31, deletions_count:   8},
		{sha: "5203f178ee47555fe761491ddf2e835b77f64691", date: "2022-01-28 07:05:55 UTC", description: "add proxy support to aws_sqs source", pr_number:                                                                                11042, scopes: ["aws_sqs source"], type:               "fix", breaking_change:         false, author: "Nathan Fox", files_count:         4, insertions_count:   60, deletions_count:   37},
		{sha: "d19f8bc2087ae324b7636f19ee47a2299e49659f", date: "2022-01-28 13:22:29 UTC", description: "bump socket2 from 0.4.3 to 0.4.4", pr_number:                                                                                   11067, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   9, deletions_count:    9},
		{sha: "713064e269be3407cb8dbbe734e42e78f449a92c", date: "2022-01-29 02:20:57 UTC", description: "Include VRL error in log when condition execution fails", pr_number:                                                            11044, scopes: ["observability"], type:                "enhancement", breaking_change: false, author: "Will", files_count:               2, insertions_count:   10, deletions_count:   5},
		{sha: "26efeb372cbfcb01a5dd8994aa71a724fde25349", date: "2022-01-28 23:53:44 UTC", description: "bump async-graphql from 3.0.26 to 3.0.27", pr_number:                                                                           11074, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "579cf13b922e8331f542b26f62f228b26bae0562", date: "2022-01-29 05:47:33 UTC", description: "Support optional user-provided message with abort expression", pr_number:                                                       10997, scopes: ["vrl"], type:                          "enhancement", breaking_change: false, author: "Will", files_count:               12, insertions_count:  298, deletions_count:  14},
		{sha: "a28a4aebe4426fc6ba21461ca9dabf8556866db7", date: "2022-01-29 06:02:13 UTC", description: "Upgrade tokio to 1.16.1", pr_number:                                                                                            11078, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      9, insertions_count:   11, deletions_count:   11},
		{sha: "e980b70ad4e276710438f62175b35212ef00316f", date: "2022-01-29 17:14:50 UTC", description: "bump security-framework from 2.3.1 to 2.5.0", pr_number:                                                                        11090, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   26, deletions_count:   37},
		{sha: "d09e04029d0b880c32345d5307a8236e31865e5c", date: "2022-01-31 06:00:07 UTC", description: "fix flaky test in disk buffer v2", pr_number:                                                                                   11073, scopes: ["buffers"], type:                      "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      2, insertions_count:   3, deletions_count:    12},
		{sha: "76d38f1865d14ab2623217d6a664680a8f344623", date: "2022-01-31 23:30:31 UTC", description: "bump rustyline from 9.0.0 to 9.1.2", pr_number:                                                                                 11096, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   7, deletions_count:    20},
		{sha: "0c6ae560bbf25391faa9a947a9b2d1948190383a", date: "2022-02-01 02:30:54 UTC", description: "Add component multiple outputs to the API", pr_number:                                                                          10964, scopes: ["api"], type:                          "enhancement", breaking_change: false, author: "Will", files_count:               8, insertions_count:   500, deletions_count:  43},
		{sha: "1c6490da26a9aacacdf8a2e8cdd05037fdf26af7", date: "2022-01-31 23:31:15 UTC", description: "bump libc from 0.2.114 to 0.2.116", pr_number:                                                                                  11088, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "1affbf94bb63c4b3d3e853d1c55db7cbe070bc57", date: "2022-01-31 23:39:47 UTC", description: "bump tui from 0.16.0 to 0.17.0", pr_number:                                                                                     11093, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   20, deletions_count:   11},
		{sha: "d077393db7d4f68faf3569439c102194e522b837", date: "2022-01-31 23:41:46 UTC", description: "bump crossterm from 0.21.0 to 0.22.1", pr_number:                                                                               11092, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    30},
		{sha: "828b060fdc13f9533c072073811ed6938062127e", date: "2022-01-31 23:42:32 UTC", description: "bump headers from 0.3.5 to 0.3.6", pr_number:                                                                                   11097, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "cce51c2cd74b97556b975a218b07878af2087ae6", date: "2022-02-01 01:33:50 UTC", description: "bump parking_lot from 0.11.2 to 0.12.0", pr_number:                                                                             11089, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   88, deletions_count:   22},
		{sha: "2b73f00932d11645c83ecab96fedd7e8dc81e8d2", date: "2022-02-01 01:35:16 UTC", description: "bump tempfile from 3.2.0 to 3.3.0", pr_number:                                                                                  11091, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   7, deletions_count:    7},
		{sha: "0736db51653aa128017a377c4ac8ed312038a25c", date: "2022-02-01 01:36:56 UTC", description: "bump security-framework from 2.5.0 to 2.6.0", pr_number:                                                                        11107, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "f5e8e5d734bf3fac096a6ba7f3ed7a484c91a3a8", date: "2022-02-01 10:06:16 UTC", description: "bump async-graphql-warp from 3.0.26 to 3.0.27", pr_number:                                                                      11077, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "ab2141c2c619dd0db9f6e40b5f0706246d6eca56", date: "2022-02-01 12:18:11 UTC", description: "Add RFC for \"LLVM Backend for VRL\"", pr_number:                                                                               10518, scopes: ["performance", "vrl"], type:           "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      1, insertions_count:   815, deletions_count:  0},
		{sha: "0e140510090e6576d6ef6c6012f5084f2e8eff14", date: "2022-02-01 04:30:43 UTC", description: "Use one more thread for merge_and_fork test", pr_number:                                                                        11112, scopes: ["tests"], type:                        "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:   2, deletions_count:    2},
		{sha: "b8cab8a1fcf85f08d64963a0c5112e56d0cc229a", date: "2022-02-01 05:44:03 UTC", description: "bump async-graphql from 3.0.27 to 3.0.28", pr_number:                                                                           11106, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "69ed8ca2acbfb43ba142c3bcd79d519e938506e8", date: "2022-02-01 05:44:26 UTC", description: "bump EmbarkStudios/cargo-deny-action from 1.2.9 to 1.2.10", pr_number:                                                          11110, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "a398504fe21ff64a065f97b203539ce986e4e889", date: "2022-02-01 08:04:17 UTC", description: "Merge shared and vector-common crates", pr_number:                                                                              11087, scopes: ["core"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      127, insertions_count: 363, deletions_count:  674},
		{sha: "153a6fd9cdd28213ea2c3f49f5245ffa23df1849", date: "2022-02-01 15:24:20 UTC", description: "bump async-graphql-warp from 3.0.27 to 3.0.28", pr_number:                                                                      11116, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "a3849398b601cb06c0f934f60e47ecc570bc1ad5", date: "2022-02-01 15:29:13 UTC", description: "bump rust_decimal from 1.20.0 to 1.21.0", pr_number:                                                                            11115, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "04d376b79a6b0b6581cefd5704b3e9160378a780", date: "2022-02-01 08:17:44 UTC", description: "Minor refactor in datadog-grok", pr_number:                                                                                     11086, scopes: [], type:                               "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 31, insertions_count:  850, deletions_count:  80},
		{sha: "0845dc86a14f74e23d15b4fd15d45de8fb86088e", date: "2022-02-01 23:38:06 UTC", description: "update error tags", pr_number:                                                                                                  11101, scopes: ["kafka source"], type:                 "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:     1, insertions_count:   3, deletions_count:    0},
		{sha: "37b5c59d3b0b0624ef029546ab38dd130850ba21", date: "2022-02-02 01:23:54 UTC", description: "comply with component spec", pr_number:                                                                                         11081, scopes: ["kubernetes_logs source"], type:       "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     4, insertions_count:   109, deletions_count:  35},
		{sha: "be03df2a19f423c6ec5e5a5dc290c73dd744ea6a", date: "2022-02-02 03:42:24 UTC", description: "fix parsing numbers for `keyvalue` filter in `parse_groks`", pr_number:                                                         10955, scopes: ["vrl"], type:                          "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      2, insertions_count:   45, deletions_count:   16},
		{sha: "ee6771146836c53cd13f8260b29bc678e61b6330", date: "2022-02-02 05:22:08 UTC", description: "Implement a VM for VRL", pr_number:                                                                                             9829, scopes: ["vrl"], type:                           "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:     89, insertions_count:  2932, deletions_count: 544},
		{sha: "d5e6a50071dd38f4945085a41375d9321249fa4f", date: "2022-02-01 23:19:42 UTC", description: "bump pretty_assertions from 1.0.0 to 1.1.0", pr_number:                                                                         11129, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:   6, deletions_count:    6},
		{sha: "4629f4ad8c5e9d124f29249835b3c7a0592f4f25", date: "2022-02-02 02:50:10 UTC", description: "Implement TCP source backpressure RFC", pr_number:                                                                              10962, scopes: ["sources"], type:                      "chore", breaking_change:       false, author: "Nathan Fox", files_count:         5, insertions_count:   474, deletions_count:  302},
		{sha: "17a832df4f257548042dad05d3acf26c9090ff05", date: "2022-02-02 01:27:45 UTC", description: "bump docker/build-push-action from 2.8.0 to 2.9.0", pr_number:                                                                  11136, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   5, deletions_count:    5},
		{sha: "7817cef54fe94cc5051021a7d097ec13eb08db46", date: "2022-02-02 02:23:49 UTC", description: "Bump the k8s versions we test against", pr_number:                                                                              11118, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      4, insertions_count:   14, deletions_count:   8},
		{sha: "078c316c7b5149f45178514f1c2836a69a7cbd52", date: "2022-02-02 06:46:12 UTC", description: "Display component output streams in vector top", pr_number:                                                                     11085, scopes: ["observability"], type:                "enhancement", breaking_change: false, author: "Will", files_count:               8, insertions_count:   232, deletions_count:  67},
		{sha: "3f314690b768af2b4ae25355db6c214fcf09e449", date: "2022-02-02 04:38:33 UTC", description: "Rename average to median in soak test output", pr_number:                                                                       10867, scopes: ["performance"], type:                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   3, deletions_count:    3},
		{sha: "427264989b413cde783e88cfccfa22c0e761f327", date: "2022-02-02 07:16:40 UTC", description: "Update rpm tests", pr_number:                                                                                                   11140, scopes: ["releasing"], type:                    "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      2, insertions_count:   28, deletions_count:   6},
		{sha: "47913187db384a1cb7f6250829469832c976336f", date: "2022-02-02 10:47:30 UTC", description: "try to fix random lockup", pr_number:                                                                                           11139, scopes: ["aws_ec2_metadata transform"], type:   "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      4, insertions_count:   66, deletions_count:   62},
		{sha: "251a7b436c67f1e87eb7d320205c3f6fe4fcf931", date: "2022-02-02 10:48:01 UTC", description: "add highlight article for beta release of disk buffer v2", pr_number:                                                           11113, scopes: ["highlights website"], type:           "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      1, insertions_count:   97, deletions_count:   0},
		{sha: "89f023c6216e6a20d9cdc948f4e8d43852e68fea", date: "2022-02-03 02:06:50 UTC", description: "comply with component spec", pr_number:                                                                                         10879, scopes: ["aws_kinesis_firehose source"], type:  "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     5, insertions_count:   50, deletions_count:   35},
		{sha: "6863e5528ab61339300c48de900bc43aa1c910f7", date: "2022-02-03 02:07:25 UTC", description: "add back metrics for deprecation", pr_number:                                                                                   11100, scopes: ["aws_ecs_metrics source"], type:       "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:     2, insertions_count:   35, deletions_count:   5},
		{sha: "73d32bb8d089c86026f9d866f101bed6a168e132", date: "2022-02-03 00:00:51 UTC", description: "Remove legacy TCP integration tests", pr_number:                                                                                11119, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   0, deletions_count:    276},
		{sha: "5437b7b0f7cd12a543cb6163fa740689f2ce8b29", date: "2022-02-03 03:39:05 UTC", description: "bump clap from 3.0.13 to 3.0.14", pr_number:                                                                                    11143, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "86bfec8d70cb30158964a0208d37b30723be3e8f", date: "2022-02-03 08:13:05 UTC", description: "fix buffer metrics when using DropNewest", pr_number:                                                                           11159, scopes: ["buffers"], type:                      "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      12, insertions_count:  134, deletions_count:  47},
		{sha: "5fcef193f14f9eb4f3074a523ae207db5302618f", date: "2022-02-04 03:06:56 UTC", description: "update `http_pipelines_blackhole` soak with the latest config", pr_number:                                                      11145, scopes: ["performance"], type:                  "chore", breaking_change:       false, author: "Vladimir Zhuk", files_count:      1, insertions_count:   1864, deletions_count: 1213},
		{sha: "11f71d897cbbed3170a3dbd6ea79f750f3a211f0", date: "2022-02-04 08:57:54 UTC", description: "comply with component spec", pr_number:                                                                                         11126, scopes: ["stdin source"], type:                 "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     6, insertions_count:   44, deletions_count:   14},
		{sha: "1ccdb9f8fd8e2dfde24eae7110495b8e552e355f", date: "2022-02-04 08:58:40 UTC", description: "comply with component spec", pr_number:                                                                                         11130, scopes: ["docker_logs source"], type:           "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     3, insertions_count:   100, deletions_count:  49},
		{sha: "38a3998c977250c13f9b17bc3ba98dc0d298fc9a", date: "2022-02-04 09:04:26 UTC", description: "comply with component spec ", pr_number:                                                                                        11018, scopes: ["host_metrics source"], type:          "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     4, insertions_count:   20, deletions_count:   20},
		{sha: "487b49d4387d58992eae4a2f9dfc1d832ab31870", date: "2022-02-04 09:22:01 UTC", description: "comply with component spec", pr_number:                                                                                         11120, scopes: ["nats source"], type:                  "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     3, insertions_count:   32, deletions_count:   18},
		{sha: "9c33519ddaaeee7c11f838f9d33dd31ab7294b86", date: "2022-02-04 02:45:26 UTC", description: "fix typos + grammar in upgrade guide", pr_number:                                                                               11175, scopes: [], type:                               "chore", breaking_change:       false, author: "Steve Hall", files_count:         1, insertions_count:   2, deletions_count:    2},
		{sha: "fce3b5fcda4589598d3427238df8ddc57d97da7c", date: "2022-02-04 04:18:49 UTC", description: "try ref counting LogEvent's top-level Value", pr_number:                                                                        11166, scopes: ["performance"], type:                  "chore", breaking_change:       false, author: "Luke Steensen", files_count:      3, insertions_count:   27, deletions_count:   18},
		{sha: "d0f782de957377c6555f2225f45ca4d594ecfcd1", date: "2022-02-04 13:48:44 UTC", description: "comply with component spec", pr_number:                                                                                         11059, scopes: ["vector source"], type:                "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     4, insertions_count:   42, deletions_count:   11},
		{sha: "ae93d9cfb302dc290adada9283ec1509560d31f3", date: "2022-02-04 15:12:49 UTC", description: "comply with component spec", pr_number:                                                                                         11121, scopes: ["nginx_metrics source"], type:         "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     2, insertions_count:   57, deletions_count:   5},
		{sha: "ec0e91bc98bc4111092d03e3f511adfd85932dc7", date: "2022-02-04 06:21:17 UTC", description: "bump trust-dns-proto from 0.20.3 to 0.20.4", pr_number:                                                                         11158, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "f1400d99cdbdab1dd68d1216b952a235c1dc4b21", date: "2022-02-04 14:26:38 UTC", description: "bump libc from 0.2.116 to 0.2.117", pr_number:                                                                                  11163, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "b736aee48ba00cc8f2225858c30b3809e12bccc2", date: "2022-02-05 01:56:42 UTC", description: "update sources to comply with intrumentation", pr_number:                                                                       11171, scopes: ["sources"], type:                      "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:     10, insertions_count:  58, deletions_count:   27},
		{sha: "5f8a3a76003bdf44bebd6c5e78bfb34d12313518", date: "2022-02-05 03:34:12 UTC", description: "comply with component spec", pr_number:                                                                                         11123, scopes: ["prometheus_scrape source"], type:     "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     3, insertions_count:   76, deletions_count:   33},
		{sha: "eb8df46cb51b8f4ff231730af034bae0e1cca2a9", date: "2022-02-05 02:10:19 UTC", description: "Fix flakey vector tap integration test", pr_number:                                                                             11186, scopes: ["api"], type:                          "chore", breaking_change:       false, author: "Will", files_count:               1, insertions_count:   30, deletions_count:   11},
		{sha: "11e85d30aca4de1628cc57bfb777ed0280f424e1", date: "2022-02-05 07:24:05 UTC", description: "fallible predicates should error at compile time.", pr_number:                                                                  11172, scopes: ["vrl"], type:                          "fix", breaking_change:         true, author:  "Stephen Wakely", files_count:     4, insertions_count:   91, deletions_count:   0},
		{sha: "9572c7d0a37d54447d14335fcc058a010a98c9e9", date: "2022-02-04 23:27:53 UTC", description: "bump prettydiff from 0.5.1 to 0.6.0", pr_number:                                                                                11191, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "eea8808e30f555e47d74278e0a7da4160058b1b2", date: "2022-02-05 12:10:36 UTC", description: "Split up codec hierarchy into `(de|en)coding::{format, framing}`", pr_number:                                                   11134, scopes: ["codecs"], type:                       "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      21, insertions_count:  518, deletions_count:  436},
		{sha: "a03ef1aab852fa0187f1060edb7df48cead41c0c", date: "2022-02-05 09:26:10 UTC", description: "Signup Functionality", pr_number:                                                                                               10904, scopes: ["website"], type:                      "feat", breaking_change:        false, author: "David Weid II", files_count:      10, insertions_count:  311, deletions_count:  28},
		{sha: "06c04b803e8bfbdf8d18d4c923bb7e3b3857faa0", date: "2022-02-05 06:35:55 UTC", description: "Fix deserialization of `only_fields` for fixed encodings", pr_number:                                                           11198, scopes: ["codecs"], type:                       "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      3, insertions_count:   30, deletions_count:   18},
		{sha: "a796d772f0b4127978b6ec62f9fe2ba5283994bf", date: "2022-02-06 02:22:34 UTC", description: "bump rustyline from 9.0.0 to 9.1.2", pr_number:                                                                                 11202, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   6, deletions_count:    19},
		{sha: "4252f1b72ec9d33815e26e687599e04bd1485cba", date: "2022-02-06 02:22:48 UTC", description: "bump trust-dns-proto from 0.20.3 to 0.20.4", pr_number:                                                                         11203, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "b536c969abc8bf753b395572baad51f8718b8579", date: "2022-02-06 02:23:02 UTC", description: "bump rust_decimal from 1.19.0 to 1.21.0", pr_number:                                                                            11204, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "21ac08690133d83935f91c8e54df0547919c7225", date: "2022-02-06 02:23:18 UTC", description: "bump smallvec from 1.7.0 to 1.8.0", pr_number:                                                                                  11205, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "24ba9057c5e4eae5c881efab4381a645cac97b50", date: "2022-02-06 11:39:47 UTC", description: "bump crossbeam-utils from 0.8.6 to 0.8.7", pr_number:                                                                           11207, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "8ff868992497eae0c3600e3e64fc5073f0594041", date: "2022-02-06 11:45:43 UTC", description: "bump semver from 1.0.4 to 1.0.5", pr_number:                                                                                    11208, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "39594ee956248bdabdbbfcbb0b322034c1e1fea2", date: "2022-02-06 12:06:30 UTC", description: "bump tracing from 0.1.29 to 0.1.30", pr_number:                                                                                 11189, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    7, insertions_count:   63, deletions_count:   56},
		{sha: "ec40a28de6d70d5c7087317fb2aa6827392c1b07", date: "2022-02-06 11:51:58 UTC", description: "bump tracing-subscriber from 0.3.7 to 0.3.8", pr_number:                                                                        11210, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   10, deletions_count:   10},
		{sha: "a1189122c4706ee13abc909154a8bdf35a6a78ca", date: "2022-02-06 20:58:09 UTC", description: "bump security-framework from 2.6.0 to 2.6.1", pr_number:                                                                        11209, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "31b4d35da93e7dcb205a98a559128e46d2fa10f4", date: "2022-02-08 04:30:21 UTC", description: "Use `enum` delegation when decoding rather than dynamic dispatch", pr_number:                                                   11162, scopes: ["codecs"], type:                       "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      30, insertions_count:  428, deletions_count:  199},
		{sha: "ed98f241e04b8e5f24ca15b0710fb868b3ca8790", date: "2022-02-08 04:30:40 UTC", description: "Use `enum` delegation when encoding rather than dynamic dispatch", pr_number:                                                   11194, scopes: ["codecs"], type:                       "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      7, insertions_count:   191, deletions_count:  82},
		{sha: "8491c7572bda04193f61a1e1447568bf691dbff6", date: "2022-02-08 01:29:51 UTC", description: "bump futures-util from 0.3.19 to 0.3.21", pr_number:                                                                            11211, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   16, deletions_count:   16},
		{sha: "6bfe81b7eda1fa02440870159bf6d376d4ef6681", date: "2022-02-08 01:30:01 UTC", description: "bump test-case from 1.2.1 to 1.2.2", pr_number:                                                                                 11212, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "aba072802074cd5401a4dcd551b5fe97684f064e", date: "2022-02-08 01:30:31 UTC", description: "bump async-graphql from 3.0.28 to 3.0.29", pr_number:                                                                           11214, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   10, deletions_count:   10},
		{sha: "02bdaf0c02bf9df218736c640caed6eeaa245213", date: "2022-02-08 01:30:42 UTC", description: "bump dashmap from 5.0.0 to 5.1.0", pr_number:                                                                                   11215, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   8, deletions_count:    8},
		{sha: "d7bcb24fc36d4e36514fd5306282f26fa624fd56", date: "2022-02-08 06:12:10 UTC", description: "Marketo Styles (website)", pr_number:                                                                                           11228, scopes: ["website"], type:                      "fix", breaking_change:         false, author: "David Weid II", files_count:      3, insertions_count:   80, deletions_count:   65},
		{sha: "0ba0d97f5dd116e37b369f3adc755d53ab74d936", date: "2022-02-08 11:31:09 UTC", description: "bump crossterm from 0.22.1 to 0.23.0", pr_number:                                                                               11216, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   22, deletions_count:   6},
		{sha: "8a225ec181abefa916284a98c09a37916987c308", date: "2022-02-08 12:10:08 UTC", description: "fix markdownlint-cli to version 0.30.", pr_number:                                                                              11224, scopes: ["ci"], type:                           "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     1, insertions_count:   6, deletions_count:    1},
		{sha: "f253ec068c927cdb23ee9532c80bef61dbece184", date: "2022-02-08 12:54:00 UTC", description: "bump futures from 0.3.19 to 0.3.21", pr_number:                                                                                 11213, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   42, deletions_count:   42},
		{sha: "6bcfae177711fd586135f88bf489fb68c719365a", date: "2022-02-08 14:08:50 UTC", description: "Use bsd-compatible cp in makefile", pr_number:                                                                                  11161, scopes: ["ci"], type:                           "fix", breaking_change:         false, author: "Filip Pytloun", files_count:      1, insertions_count:   3, deletions_count:    3},
		{sha: "1eab953748e07da3c4d51531773bb7ae5d98870f", date: "2022-02-08 07:53:36 UTC", description: "Add a `EventCount` trait to bufferable types", pr_number:                                                                       11227, scopes: ["core"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      7, insertions_count:   61, deletions_count:   7},
		{sha: "e3f043c552fbe9633b524ae8292f7150cc2726c4", date: "2022-02-08 06:42:44 UTC", description: "Clarify `component_discarded_events_total`", pr_number:                                                                         11184, scopes: ["internal docs"], type:                "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   8, deletions_count:    2},
		{sha: "1c0c15c4d2aba0f045ca84149d75f175676f9ecd", date: "2022-02-08 06:52:52 UTC", description: "bump async-graphql-warp from 3.0.28 to 3.0.29", pr_number:                                                                      11229, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "e8a2d6afffe3ef709b6f6bbd1b21ee7e92c039aa", date: "2022-02-08 17:06:33 UTC", description: "bump console-subscriber from 0.1.1 to 0.1.2", pr_number:                                                                        11235, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "2e19f617eda0c9dc671077e704c72cce3abb1712", date: "2022-02-08 17:28:14 UTC", description: "bump serde_with from 1.11.0 to 1.12.0", pr_number:                                                                              11233, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "72c93f717fe8b55b681c5ecb58033f13fc6edec8", date: "2022-02-08 18:26:46 UTC", description: "bump crc32fast from 1.3.1 to 1.3.2", pr_number:                                                                                 11236, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "1bd55260169ce15be33ca45ca2305cfb7790259a", date: "2022-02-08 11:17:17 UTC", description: "Fix conversion of distribution to aggregated histogram", pr_number:                                                             11231, scopes: ["metrics"], type:                      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      2, insertions_count:   30, deletions_count:   11},
		{sha: "cc146cc8b16019036e61548e59b162813e88ca6f", date: "2022-02-08 12:53:53 UTC", description: "Remove ignore for RUSTSEC-2022-0002", pr_number:                                                                                11237, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   0, deletions_count:    4},
		{sha: "ce21724b99c67238eb4d0a52871764680f7feb78", date: "2022-02-09 01:22:36 UTC", description: "updates aws_* to comply with component spec", pr_number:                                                                        11220, scopes: ["transforms"], type:                   "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     4, insertions_count:   32, deletions_count:   13},
		{sha: "f049246db7e3025aa55eb1e160bd3669b485887d", date: "2022-02-09 03:34:30 UTC", description: "add new `value` crate, including `Kind` type", pr_number:                                                                       10906, scopes: ["vrl", "schemas"], type:               "chore", breaking_change:       false, author: "Jean Mertz", files_count:         20, insertions_count:  5934, deletions_count: 0},
		{sha: "9e7682a2d9bce1f0cad4e9107e43a4af2bd5dd95", date: "2022-02-08 22:25:46 UTC", description: "Propagate concurrent transform spans", pr_number:                                                                               11241, scopes: ["observability"], type:                "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:   2, deletions_count:    1},
		{sha: "68307c237dd166715a7165ff61453c9686a57907", date: "2022-02-09 07:33:10 UTC", description: "comply with component spec", pr_number:                                                                                         11034, scopes: ["internal_logs source"], type:         "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     4, insertions_count:   53, deletions_count:   17},
		{sha: "6aad4c173bf7056f532834edf48972402681fcca", date: "2022-02-09 07:35:07 UTC", description: "comply with component spec", pr_number:                                                                                         11132, scopes: ["aws_s3 source"], type:                "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     3, insertions_count:   115, deletions_count:  35},
		{sha: "a438ec243d98a33c992df413c7f59ebc9b103e05", date: "2022-02-09 07:44:12 UTC", description: "infallible division by literal float/int", pr_number:                                                                           10339, scopes: ["vrl"], type:                          "enhancement", breaking_change: false, author: "Jean Mertz", files_count:         16, insertions_count:  98, deletions_count:   36},
		{sha: "ed0ca37a4cda6d108fd092e019ee09aed57c128f", date: "2022-02-09 00:15:35 UTC", description: "Remove explicit lazy_static in project", pr_number:                                                                             11243, scopes: [], type:                               "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 40, insertions_count:  244, deletions_count:  264},
		{sha: "d802cf2ee3bcab24ddd737910bb0e72c4dacd86a", date: "2022-02-09 09:49:38 UTC", description: "use constant for error stage in internal metrics", pr_number:                                                                   11250, scopes: ["observability"], type:                "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     24, insertions_count:  139, deletions_count:  109},
		{sha: "798b3e595b2dcd911a564a78b0be615c3496afed", date: "2022-02-09 01:54:02 UTC", description: "continue on error", pr_number:                                                                                                  11254, scopes: ["codecs"], type:                       "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      3, insertions_count:   15, deletions_count:   8},
		{sha: "1b9abb892436345f23e129066726d6410f8a988b", date: "2022-02-09 11:52:59 UTC", description: "add missing BytesReceived", pr_number:                                                                                          11252, scopes: ["internal_logs source"], type:         "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     2, insertions_count:   19, deletions_count:   2},
		{sha: "6faa5944fa5b67d82594160f6dd5cbab2fd293e5", date: "2022-02-09 07:10:53 UTC", description: "Tabs Wrap, Not Overflow (website)", pr_number:                                                                                  11259, scopes: ["website"], type:                      "fix", breaking_change:         false, author: "David Weid II", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "a39a7a04fde1abb694edf944c2becf136f6788e6", date: "2022-02-09 04:17:35 UTC", description: "bump test-case from 1.2.2 to 1.2.3", pr_number:                                                                                 11255, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "1ea6f17d4c5d82bf0e0d9970a955d0ddc7252f29", date: "2022-02-09 06:24:03 UTC", description: "Have encode_logfmt take the `BTreeMap` by reference", pr_number:                                                                11262, scopes: ["codecs"], type:                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      5, insertions_count:   20, deletions_count:   20},
		{sha: "7f3775b7f1e762e9d7f115d508e71b437febd99a", date: "2022-02-09 06:25:10 UTC", description: "switch to in-memory v2 by default, remove in-memory v1", pr_number:                                                             11080, scopes: ["buffers"], type:                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 16, insertions_count:  420, deletions_count:  577},
		{sha: "468d525756b9e7801ccdb288638bf2a70228c021", date: "2022-02-09 11:35:19 UTC", description: "Support metric events in vector tap", pr_number:                                                                                11201, scopes: ["api"], type:                          "enhancement", breaking_change: false, author: "Will", files_count:               10, insertions_count:  461, deletions_count:  33},
		{sha: "1711a8bdeeb96da915edb1b463143d5347aeb8f3", date: "2022-02-10 08:27:54 UTC", description: "use new `value` crate for type checking in VRL", pr_number:                                                                     11222, scopes: ["vrl"], type:                          "chore", breaking_change:       false, author: "Jean Mertz", files_count:         167, insertions_count: 2518, deletions_count: 3836},
		{sha: "691c8534f91af0f0379868a5d927c67803a50396", date: "2022-02-10 03:28:38 UTC", description: "Support logfmt formatting option in vector tap", pr_number:                                                                     11234, scopes: ["api"], type:                          "enhancement", breaking_change: false, author: "Will", files_count:               6, insertions_count:   29, deletions_count:   1},
		{sha: "ceac9251e2234660df55dc5af103ec836ebc64df", date: "2022-02-10 02:53:36 UTC", description: "Remove more references to timberio", pr_number:                                                                                 11111, scopes: [], type:                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      143, insertions_count: 361, deletions_count:  361},
		{sha: "87759f1f4d185bdeb298fb813ea787f1061eee02", date: "2022-02-10 11:38:46 UTC", description: "error if syslog decoder doesn't parse successfully.", pr_number:                                                                11244, scopes: ["codecs"], type:                       "fix", breaking_change:         true, author:  "Stephen Wakely", files_count:     2, insertions_count:   10, deletions_count:   1},
		{sha: "b585edac55e59deb08a9ef818b9d436564371d1e", date: "2022-02-10 14:22:36 UTC", description: "wrap `DataType` in `Input` in preparation for internal schema support", pr_number:                                              11268, scopes: ["schemas", "topology"], type:          "chore", breaking_change:       false, author: "Jean Mertz", files_count:         101, insertions_count: 398, deletions_count:  313},
		{sha: "3de73432532adad288cfaee4cf3ac0ce25e34946", date: "2022-02-10 14:10:48 UTC", description: "bump trust-dns-proto from 0.20.3 to 0.20.4", pr_number:                                                                         11280, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "40bfd5187a1b153a059195613da64681b2dedbb9", date: "2022-02-10 14:21:18 UTC", description: "bump rustyline from 9.0.0 to 9.1.2", pr_number:                                                                                 11281, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   6, deletions_count:    19},
		{sha: "6e463d00eb814b1e98175a46a26088bad6f9de02", date: "2022-02-10 14:31:33 UTC", description: "bump test-case from 1.2.1 to 1.2.3", pr_number:                                                                                 11282, scopes: ["deps"], type:                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "a7aadeec39c90a99ab928394bf96c184dd1ecc56", date: "2022-02-10 08:01:14 UTC", description: "revert use new `value` crate for type checking in VRL", pr_number:                                                              11287, scopes: ["vrl"], type:                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      167, insertions_count: 3836, deletions_count: 2518},
	]
}
