package metadata

releases: "0.18.0": {
	date:     "2021-11-18"
	codename: ""

	description: """
		The Vector team is pleased to announce version 0.18.0!

		Be sure to check out the [upgrade guide](/highlights/2021-11-18-0-18-0-upgrade-guide) for breaking changes in this release.

		In case you missed it, we've also released a new unified `vector` helm chart! This new chart can deployed as either
		as either an agent or aggregator role and so deprecates our existing `vector-agent` and `vector-aggregator` charts.
		See the [chart upgrade
		guide](https://github.com/vectordotdev/helm-charts/blob/develop/charts/vector/README.md#upgrading) for how to
		transition from the old charts.
		"""

	known_issues: [
		"The `elasticsearch` sink incorrectly prints a message for each delivered event. Fixed in v0.18.1.",
		"A change to internal telemetry causes aggregated histograms emitted by the `prometheus_exporter` and `prometheus_remote_write` sinks to be incorrectly tallied. Fixed in v0.18.1.",
		"The new automatic namespacing feature broke running Vector from the published RPM due to it trying to load directories from `/etc/vector` that are not valid. Fixed in v0.18.1.",
		"The new `reroute_dropped` feature of `remap` always creates the `dropped` output even if `reroute_dropped = false`. Fixed in v0.18.1.",
		"The `headers_key` option for the `kafka` sink was inadvertantly changed to `headers_field`. Fixed in v0.19.0.",
		"If `--config-dir` is used, Vector incorrectly tries to load files with unknown extensions. Fixed in v0.19.0.",
		"""
			`encoding.only_fields` failed to deserialize correctly for sinks that used fixed encodings (i.e. those that don't have `encoding.codec`). Fixed in v0.19.2. As a workaround, you can split the paths up in your configuration like:

			```toml
			encoding.only_fields = ["message", "foo.bar"]
			```

			to

			```toml
			encoding.only_fields = [["message"], ["foo", "bar"]]
			```

			You will need to convert it back to its original representation when upgrading to >= v0.19.2.
			""",
	]

	changelog: [
		{
			type: "feat"
			scopes: ["remap transform", "config"]
			description: """
				Initial support for routing failed events from transforms has
				been added, starting with the `remap` transform. See [the
				highlight](/highlights/2021-11-18-failed-event-routing) for
				more.
				"""
		},
		{
			type: "feat"
			scopes: ["enrichment"]
			description: """
				Initial support for enriching events from external data sources has been
				added via a new Vector concept, enrichment tables. To start, we've added
				support for enriching events with data from a CSV file. See [the
				highlight](/highlights/2021-11-18-csv-enrichment) for more.
				"""
		},
		{
			type: "feat"
			scopes: ["new transform"]
			description: """
				A new `throttle` transform has been added for controlling costs. See [the
				highlight](/highlights/2021-11-12-event-throttle-transform) for more.
				"""
		},
		{
			type: "feat"
			scopes: ["config"]
			description: """
				Better support for breaking up Vector configuration into multiple files
				was added via deriving configuration from file and directory names. See
				[the highlight](/highlights/2021-11-18-implicit-namespacing) for more.
				"""
		},
		{
			type: "feat"
			scopes: ["new source"]
			description: """
				A new `aws_sqs` source was added for consuming messages from AWS SQS as log
				events.
				"""
		},
		{
			type: "enhancement"
			scopes: ["buffers", "observability"]
			description: """
				Instrumentation has been added to sink buffers to help give more visibility into
				their operation. The following metrics have been added:

				 - `buffer_byte_size` (disk buffer only): The number of bytes in the buffer
				 - `buffer_events` (in-memory buffer only): The number of events in the buffer
				 - `buffer_received_event_bytes_total`: The number of bytes that have been
				   received by this buffer. This count does not include discarded events.
				 - `buffer_sent_event_bytes_total`: The number of bytes that have been sent
				   from the buffer to its associated sink.
				 - `buffer_received_events_total`: The number of events that have been received
				   by this buffer. This count does not include discarded events.
				 - `buffer_sent_events_total`: The number of events that have been sent from
				   the buffer to its associated sink.
				 - `buffer_discarded_events_total`: The number of events that
				   have been discarded from the buffer because it is full
				   (relevant when `when_full` is `drop_newest`)
				"""
		},
		{
			type: "enhancement"
			scopes: ["config"]
			description: """
				The `$LOG` environment variable for configuring the Vector log level has
				been renamed to `$VECTOR_LOG`. `$LOG` is still also accepted for backwards
				compatibility. This change makes logging configuration more in-line with
				Vector's other environment variable based options, and isolates Vector from
				being affected by other generic environment variables.
				"""
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				VRL diagnostic error messages have been improved to suggest `null`, `true`, or
				`false` for undefined variables. This helps guide users to realize when they
				are trying to use a keyword like `nil` that doesn't actually exist in VRL.
				"""
		},
		{
			type: "enhancement"
			scopes: ["log_to_metric transform"]
			description: """
				The `log_to_metric` transform now also allows emitting absolute counters
				in addition to relative counters via `kind = "absolute"`.
				"""
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			breaking: true
			description: """
				The `status` tag for the `http_client_responses_total` internal
				metric was updated to be just the integer (e.g. `200`) rather
				than including the text portion of the HTTP response code (e.g.
				`200 OK`).
				"""
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source now annotates logs with the `pod_owner` when
				available.
				"""
		},
		{
			type: "enhancement"
			scopes: ["papertrail sink"]
			description: """
				The `papertrail` sink now allows `process` field to be set to a event field
				value the templatable `process` key.
				"""
		},
		{
			type: "enhancement"
			scopes: ["aws_s3 sink"]
			description: """
				The `aws_s3` sink now has less connections terminated prematurely as it
				optimistically terminates connections before AWS's timeout.
				"""
		},
		{
			type: "enhancement"
			scopes: ["prometheus_exporter sink"]
			description: """
				The `prometheus_exporter` now expires metrics that haven't been seen since the
				last flush (controlled by `flush_interval_secs`) to avoid holding onto stale
				metrics indefinitely and consuming increasing amounts of memory.
				"""
		},
		{
			type: "enhancement"
			scopes: ["aws_kinesis_firehose source", "journald source", "file sink"]
			description: """
				Added support for end-to-end acknowledgements to the `aws_kinesis_firehose`
				source, `journald` source, and `file` sink.
				"""
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				The `utilization` metric for most transforms no longer count time spent
				blocked on downstream components as busy. This means they should more
				accurately represent the time spent in that specific transform and require
				less interpretation to find bottlenecks.
				"""
		},
		{
			type: "enhancement"
			scopes: ["datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now supports sending distribution data to Datadog
				like histograms and aggregated samples.
				"""
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source has been updated to be less demanding on the
				Kubernetes API server (and backing etcd cluster) by allowing for slightly
				stale data to be used for log enrichment rather than always requesting the
				most-up-to-date metadata.
				"""
		},
		{
			type: "enhancement"
			scopes: ["generator source"]
			description: """
				The `generator` source has been renamed to `demo_logs`. We feel this name
				better reflects the intent of the source. An alias has been added to maintain
				compatibility.
				"""
		},
		{
			type: "enhancement"
			scopes: ["codecs", "heroku_logs source"]
			description: """
				The `framing` and `decoding` options are now available on `heroku_logs`
				source. See the [framing and decoding highlight from
				v0.17.0](/highlights/2021-10-06-source-codecs/) for more about this new source
				feature.
				"""
		},
		{
			type: "enhancement"
			scopes: ["metric_to_log transform"]
			breaking: true
			description: """
				 The `upper_limit` field for aggregated summaries from the
				 `metric_to_log` transform has been renamed to `q` which is
				 a common shorthand for `quantile`.
				"""
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				We have continued to add additional instrumentation to
				components with the goal of having them all match the [Component
				Specification](https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md#instrumentation).
				Once we have finished this we will post a highlight outlining
				all of the added metrics.
				"""
		},
		{
			type: "fix"
			scopes: ["performance"]
			description: """
				Configuring the number of threads (via `--threads`) for Vector now actually
				takes effect again rather than it always using the number of available cores.
				This was a regression in v0.13.
				"""
		},
		{
			type: "fix"
			scopes: ["config", "reload"]
			description: """
				Vector no longer crashes when configuration was reloaded that include changes
				to both the order of inputs for a component and configuration of one of those
				inputs.
				"""
		},
		{
			type: "fix"
			scopes: ["exec source"]
			breaking: true
			description: """
				The obsolete `event_per_line` configuration option was removed from
				the `exec` source. This option became non-functional in 0.17.0 but
				was left available to be configured. Instead, the new `framing`
				option can be used to choose between interpreting th e output of the
				subcommand as an event per line or all at once as one event.
				"""
		},
		{
			type: "fix"
			scopes: ["aws_s3 sink"]
			description: """
				Fix regression in `v0.17.0` for `aws_s3` sink where it would add
				a `/` to the prefix provided. The sink no longer adds this `/` to
				replace previous behavior.
				"""
		},
		{
			type: "fix"
			scopes: ["windows platform"]
			description: """
				Fix memory leak that occurred when using Vector as a Windows service.
				"""
		},
		{
			type: "fix"
			scopes: ["aws_s3 sink", "loki sink", "datadog_logs sink"]
			description: """
				Fix lock-ups with the `aws_s3`, `loki`, and `datadog_logs` sinks.
				"""
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				Fix naming of the VRL `compact` function's `object` argument to match the
				docs. This was incorrectly implemented named `map` in the implementation.
				"""
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				The `component_sent_bytes_total` internal metric is now reported _after_
				events are successfully sent to HTTP-based sinks rather than before they are
				sent.
				"""
		},
		{
			type: "fix"
			scopes: ["influxdb_logs sink", "influxdb_metrics sink"]
			description: """
				The `influxdb_metrics` and `influxdb_logs` sinks now use `/ping` for
				healthchecks rather than `/health` to work with Influx DB 2 Cloud.
				"""
		},
		{
			type:     "deprecation"
			breaking: true
			scopes: ["sinks"]
			description: """
				The deprecated `batch.max_size` parameter has been removed in
				this release.  See the [upgrade
				guide](/highlights/2021-11-18-0-18-0-upgrade-guide) for more.
				"""
		},
		{
			type:     "deprecation"
			breaking: true
			scopes: ["sinks"]
			description: """
				The deprecated `request.in_flight_limit` has been removed in
				this release.  See the [upgrade
				guide](/highlights/2021-11-18-0-18-0-upgrade-guide) for more.
				"""
		},
		{
			type:     "deprecation"
			breaking: true
			scopes: ["datadog_metrics sink"]
			description: """
				The deprecated `host` and `namespace` field on the `datadog_metrics`
				sink has been removed. See the [upgrade
				guide](/highlights/2021-11-18-0-18-0-upgrade-guide) for more.
				"""
		},
	]

	whats_next: [
		{
			title:       "Component metric standardization"
			description: """
				We are in the process of ensuring that all Vector components report a consistent set of metrics to make
				it easier to monitor the performance of Vector.  These metrics are outlined in this new [instrumentation
				specification](\(urls.specs_instrumentation)).
				"""
		},
		{
			title:       "VRL iteration support"
			description: """
				A common request from users when the incoming log event shape is unknown, to be able to iterate over the
				keys and values in those log events. We recently published an [RFC](\(urls.vector_rfc_8381)) for this and
				expect to implement this support this quarter.
				"""
		},
	]

	commits: [
		{sha: "8b3287132f1a13dafb57bd1e1ee78b0d8cf6291a", date: "2021-10-07 17:55:15 UTC", description: "Add metrics recorder and tracing to buffer benchmarks", pr_number: 9483, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Will", files_count: 5, insertions_count: 24, deletions_count: 2},
		{sha: "d4bb4ab5e02578b32dfbd1668c7d00db5ff5b8b8", date: "2021-10-07 22:51:41 UTC", description: "Adjust component spec and sinks to reflect sent bytes and events only after sending successfully", pr_number: 9503, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 124, deletions_count: 40},
		{sha: "7b538d38bf0aa1fdd2d52cde1add80b201f3d747", date: "2021-10-08 06:12:23 UTC", description: "bump reqwest from 0.11.4 to 0.11.5", pr_number: 9507, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 7},
		{sha: "69fb78888687efc14ee1d019379f78d2ed42da76", date: "2021-10-08 00:31:11 UTC", description: "Fix double emit of EventsSent in HttpSink", pr_number: 9508, scopes: ["observability", "sinks"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 6, deletions_count: 15},
		{sha: "d849e22f8154442f276757f597e374673b67fb00", date: "2021-10-08 01:40:03 UTC", description: "add transforms with multiple outputs", pr_number: 9169, scopes: ["topology"], type: "feat", breaking_change: false, author: "Luke Steensen", files_count: 26, insertions_count: 1227, deletions_count: 596},
		{sha: "7dbd7c3ffddbf549d947250b62d8f7f4a54a55be", date: "2021-10-08 07:35:16 UTC", description: "bump assert_cmd from 2.0.1 to 2.0.2", pr_number: 9513, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7e20c2c1da6bba2618afea85d39f8906eb7acade", date: "2021-10-08 20:18:41 UTC", description: "suggest null, true or false for undefined variables", pr_number: 9517, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Jean Mertz", files_count: 2, insertions_count: 43, deletions_count: 0},
		{sha: "d0a8cba6c1fc26cccd328696d1e4793acf41c410", date: "2021-10-08 19:21:01 UTC", description: "enable enrichment file reload on SIGHUP", pr_number: 9371, scopes: ["enriching"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 7, insertions_count: 257, deletions_count: 33},
		{sha: "10046ce0725b87f6c45fa0bcb678d5f59a6c5f43", date: "2021-10-08 20:59:03 UTC", description: "add absolute_kind to log_to_metric counters", pr_number: 9463, scopes: ["log_to_metric transform"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 55, deletions_count: 1},
		{sha: "676b42132b868dcd1b889a01ab18a9a31ef58400", date: "2021-10-08 17:30:17 UTC", description: "convert datadog logs sink to streaming model + a whole bunch of other stuff", pr_number: 9499, scopes: ["datadog_logs sink"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 49, insertions_count: 1152, deletions_count: 766},
		{sha: "9c1e1fbd511612c71b9b7bfdc990a710a5549738", date: "2021-10-08 17:19:04 UTC", description: "Remove pipelines", pr_number: 9509, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 51, insertions_count: 72, deletions_count: 1174},
		{sha: "92565bbe297e1b8effc10ba04424ada8d3701e22", date: "2021-10-09 00:41:48 UTC", description: "Integrate `Decoder`/`DecodingConfig` with `heroku_logs` source", pr_number: 9432, scopes: ["codecs", "heroku_logs source"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count: 3, insertions_count: 71, deletions_count: 34},
		{sha: "74c3f06f6e4eb63793e6a32c83f7996e17b6fa65", date: "2021-10-08 22:53:52 UTC", description: "actually apply threads option", pr_number: 9527, scopes: ["cli"], type: "fix", breaking_change: false, author: "Luke Steensen", files_count: 1, insertions_count: 6, deletions_count: 7},
		{sha: "89d53c8163080726bcb7e2a78ad6a64fb4f593ed", date: "2021-10-09 00:05:55 UTC", description: "Label HTTP response metrics with status code", pr_number: 9510, scopes: ["observability"], type: "fix", breaking_change: true, author: "Jesse Szwedko", files_count: 2, insertions_count: 17, deletions_count: 4},
		{sha: "ff2d1bcdc2b4fdf5c490820860c6438f04ca9010", date: "2021-10-09 03:01:16 UTC", description: "Bump version in Cargo.toml", pr_number: 9529, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "9774570530a953aec31bcc6915ef34ac72859812", date: "2021-10-11 22:17:46 UTC", description: "rewrite kafka sink with new style", pr_number: 9361, scopes: ["kafka sink"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 17, insertions_count: 1076, deletions_count: 1131},
		{sha: "0dffc5197426bbf207567643b63e8eafe7f70f87", date: "2021-10-12 23:37:37 UTC", description: "Adding additional metadata from kubernetes", pr_number: 9505, scopes: ["kubernetes_logs source"], type: "enhancement", breaking_change: false, author: "Nikolay Bogdanov", files_count: 2, insertions_count: 18, deletions_count: 0},
		{sha: "810f4c91e0ffe3dc7bd9c40471931062ea2328f5", date: "2021-10-12 16:54:40 UTC", description: "Document usage of capture groups with replace()", pr_number: 9518, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 14, deletions_count: 0},
		{sha: "bb98c5311ae97ce4410d11912f68ecc56be4dbba", date: "2021-10-12 17:06:21 UTC", description: "bump strum_macros from 0.21.1 to 0.22.0", pr_number: 9552, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "027facc57109975121eff5e8990cfbb396791286", date: "2021-10-12 17:06:42 UTC", description: "bump thiserror from 1.0.29 to 1.0.30", pr_number: 9553, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "06b540f3e6ffa7d5c100818df2e369da4486fbda", date: "2021-10-12 17:07:03 UTC", description: "bump k8s-openapi from 0.13.0 to 0.13.1", pr_number: 9554, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "17d9af99443665c02ef20aa20504eb4782047e72", date: "2021-10-12 19:23:16 UTC", description: "Add RFC for `throttle` transform", pr_number: 9381, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 171, deletions_count: 0},
		{sha: "c1d9c727e9d584ca22ecf2d51b8e0908ef20f0c7", date: "2021-10-12 18:01:35 UTC", description: "Emit EventsSent in batch sinks", pr_number: 9504, scopes: ["observability", "sinks"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 38, insertions_count: 306, deletions_count: 147},
		{sha: "c878c50fd905048d28742443142aad10fad0620f", date: "2021-10-12 18:05:18 UTC", description: "Add event processing metrics to TcpSource sources", pr_number: 9540, scopes: ["observability", "sources"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 12, insertions_count: 143, deletions_count: 18},
		{sha: "cd5238afb4c2fd210d67e52cf00325a16b42996d", date: "2021-10-13 00:21:14 UTC", description: "Remove non-working tests", pr_number: 9576, scopes: ["fluent source", "logstash source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 0, deletions_count: 12},
		{sha: "18c3b27cea7f348d1df9be2fb20da11b6decbaff", date: "2021-10-13 01:26:39 UTC", description: "handle rebuilding connected components", pr_number: 9536, scopes: ["topology"], type: "fix", breaking_change: false, author: "Luke Steensen", files_count: 3, insertions_count: 106, deletions_count: 6},
		{sha: "16a16be7b8c8fdf3e3b69b38f5ac2ec0d7ef6016", date: "2021-10-13 05:25:20 UTC", description: "Instrument buffer total bytes/events received, sent, dropped", pr_number: 9327, scopes: ["buffers"], type: "enhancement", breaking_change: false, author: "Will", files_count: 21, insertions_count: 482, deletions_count: 136},
		{sha: "f9f43b436af10defcf274f99d0e255bc1a34ff90", date: "2021-10-13 17:12:44 UTC", description: "bump mlua from 0.6.5 to 0.6.6", pr_number: 9581, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "14dbd9a7ed42beb8b5207c39c1c5289174faaf48", date: "2021-10-13 19:27:37 UTC", description: "satisfy markdownlint on throttle rfc", pr_number: 9585, scopes: ["ci"], type: "fix", breaking_change: false, author: "Luke Steensen", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "cc2e849d05b01b4979c97781313f13ff80528fbf", date: "2021-10-13 21:11:23 UTC", description: "Fix missing increment in batch counter and add component tests to HttpSink sinks", pr_number: 9525, scopes: ["observability", "sinks"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 16, insertions_count: 150, deletions_count: 116},
		{sha: "09cfd0b422df06cff5a85715713966f1289cdb71", date: "2021-10-14 00:13:32 UTC", description: "bump mongodb from 2.0.0 to 2.0.1", pr_number: 9580, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "6969d1fb96c1a42d1be7d4eae21a305b3c11300f", date: "2021-10-14 00:42:13 UTC", description: "Add missing `protocol` tag to HTTP sink tags", pr_number: 9590, scopes: ["observability"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "eaf548abe33eb667ff4343538049289e1fb10dc6", date: "2021-10-14 03:29:08 UTC", description: "Update to comply with component spec", pr_number: 9596, scopes: ["apache_metrics source", "observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 121, deletions_count: 35},
		{sha: "de4c6a8327386b8ae14f17e94eecabe8eed62606", date: "2021-10-14 17:52:42 UTC", description: "Revert to _bytes naming convention for buffer metrics", pr_number: 9591, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Will", files_count: 4, insertions_count: 17, deletions_count: 8},
		{sha: "0e4ba6505184b30a21c9f67d9aabc757dde1a41c", date: "2021-10-14 23:22:28 UTC", description: "Resolve markdown lint error in buffer specification", pr_number: 9607, scopes: ["ci"], type: "fix", breaking_change: false, author: "Will", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "d0c30e33bfc758ae3e1c066da59325357696af3a", date: "2021-10-14 22:13:19 UTC", description: "Unify component/trace test init", pr_number: 9606, scopes: ["unit tests"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 10, insertions_count: 24, deletions_count: 55},
		{sha: "a7cf0144940afdfbc7f17acff53083d65a113e70", date: "2021-10-14 22:56:57 UTC", description: "Fix compilation error in buffers soak example", pr_number: 9609, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "72545b310c8f362dd692e26120a3f14c257c9617", date: "2021-10-15 00:22:13 UTC", description: "Fix markdown check guard", pr_number: 9610, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "58dfee0443511c4e51378cb33e22408cbe705556", date: "2021-10-15 17:57:13 UTC", description: "bump cached from 0.25.0 to 0.25.1", pr_number: 9630, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 49},
		{sha: "b1f83d5d8ccc1827295f0a11ba11653f75ff9eff", date: "2021-10-15 19:53:36 UTC", description: "bump redis from 0.21.2 to 0.21.3", pr_number: 9635, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "560e3ab2102e0d71bf824f12abd728f0c13236ed", date: "2021-10-16 02:11:51 UTC", description: "bump tower from 0.4.8 to 0.4.9", pr_number: 9638, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "6fa232458ee375720163575052bd2045beb13689", date: "2021-10-16 02:26:34 UTC", description: "bump actions/checkout from 2.3.4 to 2.3.5", pr_number: 9639, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 56, deletions_count: 56},
		{sha: "23231a4694d46bed2ae799631ad4c7d63489e7f1", date: "2021-10-15 23:06:52 UTC", description: "Instrument buffer max size and disk buffer initial size", pr_number: 9584, scopes: ["buffers"], type: "enhancement", breaking_change: false, author: "Will", files_count: 6, insertions_count: 64, deletions_count: 24},
		{sha: "1ada00235d4aedd125f19b70c0e5b52ae1ca2be8", date: "2021-10-15 22:17:50 UTC", description: "Simplify calling size_of over vectors", pr_number: 9627, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 7, insertions_count: 10, deletions_count: 14},
		{sha: "e3b3fe14cab2bc1149fe5981a598ff250cd6be01", date: "2021-10-16 01:16:42 UTC", description: "Instrument splunk_hec source according to component spec", pr_number: 9586, scopes: ["observability", "splunk_hec source"], type: "enhancement", breaking_change: false, author: "Will", files_count: 11, insertions_count: 98, deletions_count: 92},
		{sha: "6683e2713d4cffd55978ea7ad8b34354381a70ac", date: "2021-10-18 20:29:38 UTC", description: "Fix split return type", pr_number: 9672, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "cfd66ce9949b519e60e49d385459daccbce83caf", date: "2021-10-19 00:57:30 UTC", description: "Add event processing metrics", pr_number: 9589, scopes: ["file sink", "observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 58, deletions_count: 11},
		{sha: "b100f786f6d0c625a3985ef70b649972c50eba04", date: "2021-10-19 02:10:04 UTC", description: "Fix compilation of tests", pr_number: 9684, scopes: ["file sink"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "39435401299fb0fff98f23b8387b2ea5f5963cd7", date: "2021-10-19 05:14:34 UTC", description: "Add PowerPC build scaffolding", pr_number: 9629, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 6, deletions_count: 0},
		{sha: "a8bee332c0f0109bf296d39a8f7c512a1d2e3055", date: "2021-10-19 10:29:22 UTC", description: "bump indexmap from 1.6.2 to 1.7.0", pr_number: 9512, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 10, deletions_count: 20},
		{sha: "3de42ccb2321e922959fa3d41566a7180d17f61d", date: "2021-10-19 11:54:25 UTC", description: "bump libc from 0.2.103 to 0.2.104", pr_number: 9693, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "6e608dd9de83bf9e86ad716b4daea1c20e08eac7", date: "2021-10-19 13:19:53 UTC", description: "bump reqwest from 0.11.5 to 0.11.6", pr_number: 9694, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "5074e161580a749be9b96333c26177ec6f482eea", date: "2021-10-19 13:44:16 UTC", description: "bump headers from 0.3.4 to 0.3.5", pr_number: 9695, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "0f93d507f09d704d33c095ae12768ba1416c3799", date: "2021-10-19 13:51:58 UTC", description: "bump structopt from 0.3.23 to 0.3.25", pr_number: 9696, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a1347f25efceb5dccd7c89e996265c92724ca0ff", date: "2021-10-19 16:49:26 UTC", description: "create rfc for automatic namespacing", pr_number: 9571, scopes: ["architecture"], type: "docs", breaking_change: false, author: "Jérémie Drouet", files_count: 1, insertions_count: 133, deletions_count: 0},
		{sha: "483e455564f38456b168aa396208311ec7458f47", date: "2021-10-19 17:59:38 UTC", description: "Add event processing metrics", pr_number: 9614, scopes: ["observability", "aws_s3 sink"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 13, insertions_count: 183, deletions_count: 117},
		{sha: "9ff369db28616135570970b485f393c4dccb59a1", date: "2021-10-19 20:00:53 UTC", description: "Upgrade rdkafka and simplify storing offset", pr_number: 9681, scopes: ["kafka source"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 15, deletions_count: 15},
		{sha: "c5e0ca92092a938888f906f94aa7068a1c9c3d55", date: "2021-10-19 21:16:30 UTC", description: "Allow `process` to be set", pr_number: 9685, scopes: ["papertrail"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 47, deletions_count: 4},
		{sha: "7f34438841261a87ad8b22c1db6fb5222d7a1d18", date: "2021-10-19 22:47:22 UTC", description: "Fix diff for VRL error 630", pr_number: 9706, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "88008074cada58ebc947e2467b18b19ff4ecfc91", date: "2021-10-20 00:10:32 UTC", description: "Add powerpc64le-unknown-linux-gnu as a target", pr_number: 9690, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 6, deletions_count: 0},
		{sha: "23611d469843d11753d7f4b9aabd80083787cb66", date: "2021-10-20 16:57:36 UTC", description: "bump tokio-openssl from 0.6.2 to 0.6.3", pr_number: 9711, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "868c86ab9a80edc7bc4e0c9b2fbdac7e61a9b1fc", date: "2021-10-20 17:00:58 UTC", description: "bump tokio-postgres from 0.7.3 to 0.7.4", pr_number: 9713, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8f3e63a816da123ee504d967569ac0614fe260c7", date: "2021-10-20 21:10:29 UTC", description: "rewrite elasticsearch sink with new style", pr_number: 9611, scopes: ["elasticsearch sink"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 21, insertions_count: 2134, deletions_count: 1518},
		{sha: "7e61cce1dbdd62d0e245a62f69664044d43454c2", date: "2021-10-21 06:44:51 UTC", description: "close idle connections for aws s3 sinks", pr_number: 9703, scopes: ["aws_s3 sink"], type: "fix", breaking_change: false, author: "Vladimir Zhuk", files_count: 3, insertions_count: 28, deletions_count: 3},
		{sha: "a558a9814d83a531aa551b314e7b95e964bf64ef", date: "2021-10-21 00:23:37 UTC", description: "Add batch configuration docs", pr_number: 9725, scopes: ["vector sink"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "29234094a55cafbb6a2cb18d3deca370e8897ed4", date: "2021-10-21 01:07:37 UTC", description: "bump tower from 0.4.9 to 0.4.10", pr_number: 9712, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "bc20bc6f329a02903d0a9465816b20c70f5c6783", date: "2021-10-21 02:22:41 UTC", description: "bump encoding_rs from 0.8.28 to 0.8.29", pr_number: 9663, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "db560ab9580a476422c9115d4df74c3679f486d0", date: "2021-10-21 02:29:58 UTC", description: "Make more concurrency defaults adaptive", pr_number: 9726, scopes: ["sinks"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 10, insertions_count: 12, deletions_count: 30},
		{sha: "aa331a60cf33dde2d998836a4ce251fb69a68801", date: "2021-10-21 03:30:06 UTC", description: "Allow for localhost soak testing", pr_number: 9699, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 44, insertions_count: 1565, deletions_count: 1},
		{sha: "9604a20b93d385b14f68d559c12da3681a0455e5", date: "2021-10-21 17:13:42 UTC", description: "bump cached from 0.25.1 to 0.26.2", pr_number: 9730, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 16, deletions_count: 5},
		{sha: "0567ca4ca4907d1841a56a07ab9076c3ee11af3f", date: "2021-10-21 18:42:55 UTC", description: "buffer improvements RFC", pr_number: 9645, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 294, deletions_count: 0},
		{sha: "c2b11f6a52008aad5df32a808bca692210226ff9", date: "2021-10-21 22:05:33 UTC", description: "Update onig to 6.3.0", pr_number: 9736, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "8b282cdc01ec5d6d37e4a67bc927eafb187e2387", date: "2021-10-21 23:34:00 UTC", description: "Add a guideline about log schemas", pr_number: 9721, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Ben Johnson", files_count: 1, insertions_count: 21, deletions_count: 0},
		{sha: "742bc406fcd6b55e550e70b1c875c38473c11fec", date: "2021-10-21 21:50:48 UTC", description: "Introduce a 'syslog -> loki' soak", pr_number: 9729, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 23, insertions_count: 306, deletions_count: 236},
		{sha: "c05a775a53450789551ed171f3655de1568cdc93", date: "2021-10-22 17:20:38 UTC", description: "bump rust_decimal from 1.16.0 to 1.17.0", pr_number: 9755, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "ff64ebf3da876ca64fdcdf718090ac0671dbb17c", date: "2021-10-22 18:41:53 UTC", description: "Fix documentation of `end` parameter for `slice`", pr_number: 9763, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1e251a8c7c33b2e57836b8346fa40940dfcc2200", date: "2021-10-22 19:04:28 UTC", description: "Bump onig to 6.3.1", pr_number: 9762, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "40bf8d392374fa61a8324789a57e68f1a1b4fb14", date: "2021-10-22 18:43:51 UTC", description: "Show release notes on /releases page", pr_number: 9731, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count: 10, insertions_count: 49, deletions_count: 15},
		{sha: "e8987631ea2ceab1fff5089318ee569a05b92abb", date: "2021-10-22 22:51:21 UTC", description: "Fix version in Cargo.toml", pr_number: 9770, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "896c258ef0e32814121f92b9c060d86aa1adb787", date: "2021-10-22 22:33:53 UTC", description: "Lighthouse scores for the website", pr_number: 9753, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count: 26, insertions_count: 749, deletions_count: 37},
		{sha: "b71029c2f4aedd614b81028f57e4b62eb0146c92", date: "2021-10-23 01:43:46 UTC", description: "Enable the \"union\" feature on SmallVec", pr_number: 9772, scopes: ["performance"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "cb4e346374faa045969e66a2024e6b15d25e156b", date: "2021-10-23 01:31:48 UTC", description: "Netlify Lighthouse plugin", pr_number: 9775, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count: 8, insertions_count: 651, deletions_count: 39},
		{sha: "eee3168449436dd0ac328bc1e51c26e85873187e", date: "2021-10-23 05:10:15 UTC", description: "Make more sinks default to adaptive concurrency", pr_number: 9774, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 6, insertions_count: 14, deletions_count: 20},
		{sha: "3b1ee2a36a012f91ce5862e2e4d0565d799b3761", date: "2021-10-26 01:10:37 UTC", description: "use /ping endpoint to check health of InfluxDB 2", pr_number: 9781, scopes: [], type: "chore", breaking_change: false, author: "Jakub Bednář", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "ff7db0bef783c0ab873246be6e713c6c10e1dbb2", date: "2021-10-25 22:31:53 UTC", description: "Rename LOG env var to VECTOR_LOG", pr_number: 9743, scopes: ["cli"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 8, insertions_count: 65, deletions_count: 33},
		{sha: "ba349eeab82f8c4fadba20b1f63c8f3199ab0555", date: "2021-10-25 22:32:50 UTC", description: "Minikube Mount Host Filesystem ", pr_number: 9773, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 23, insertions_count: 540, deletions_count: 89},
		{sha: "8e654298317e37ada3e40bbab6ca025a0ecb1724", date: "2021-10-26 01:47:22 UTC", description: "Use OS X compatible option in soak test script", pr_number: 9788, scopes: ["ci"], type: "fix", breaking_change: false, author: "Will", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "924d0025ad0e7e1a90106e63efbed8163135dbf2", date: "2021-10-26 03:30:24 UTC", description: "Remove `-Clink-self-contained=no` from musl builds", pr_number: 9771, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 1, deletions_count: 5},
		{sha: "d8a39860ec88763ead48e51c64b8414072794e5f", date: "2021-10-26 07:14:26 UTC", description: "Upgrade to Rust 1.56.0", pr_number: 9747, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 3, deletions_count: 5},
		{sha: "b73b19b367c154c37f717783f044a4da9e6aeb35", date: "2021-10-26 14:35:34 UTC", description: "Upgrade to Rust edition 2021", pr_number: 9761, scopes: ["deps"], type: "chore", breaking_change: false, author: "Lee Benson", files_count: 29, insertions_count: 33, deletions_count: 33},
		{sha: "653be97452e2d408ea3373910b3960d1ac1a41a4", date: "2021-10-27 02:29:44 UTC", description: "rewrite using stream", pr_number: 9506, scopes: ["loki sink"], type: "feat", breaking_change: false, author: "Jérémie Drouet", files_count: 15, insertions_count: 1677, deletions_count: 1363},
		{sha: "b7e1dfcb21506d58777cd2b0dcb44ae451ed47b2", date: "2021-10-26 23:58:45 UTC", description: "Rewrite splunk_hec logs sink in new style", pr_number: 9738, scopes: ["splunk_hec sink"], type: "chore", breaking_change: false, author: "Will", files_count: 45, insertions_count: 2089, deletions_count: 1081},
		{sha: "668f0c2b2f0b884f8032119d8c86a264b2dfff91", date: "2021-10-27 06:04:33 UTC", description: "Add SHA256 hash to config for enterprise reporting", pr_number: 9575, scopes: ["config"], type: "enhancement", breaking_change: false, author: "Lee Benson", files_count: 9, insertions_count: 165, deletions_count: 14},
		{sha: "f726dc6ecc03a16077c7ea24c2d3bf7d0e38b0ff", date: "2021-10-27 15:37:55 UTC", description: "Bump main Cargo.toml to 2021 edition", pr_number: 9800, scopes: ["deps"], type: "fix", breaking_change: false, author: "Lee Benson", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "7b6a5bf5af1d8c57a8d660f92f02d080eeb51b62", date: "2021-10-27 21:20:06 UTC", description: "Adds host_metrics reporting to DD Pipelines", pr_number: 9784, scopes: ["pipelines"], type: "enhancement", breaking_change: false, author: "Lee Benson", files_count: 3, insertions_count: 49, deletions_count: 4},
		{sha: "c5326a11407632be6f66cacd03a993c977401374", date: "2021-10-27 20:05:36 UTC", description: "Wire up humio metrics soak correctly", pr_number: 9810, scopes: ["ci"], type: "fix", breaking_change: false, author: "Will", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "9055e734549ee81b291a8fabd5563a495b3d8991", date: "2021-10-27 17:13:45 UTC", description: "Initial website perf improvements", pr_number: 9795, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count: 6, insertions_count: 7, deletions_count: 8},
		{sha: "87cfdf783c2c2223f5dae2fe2ad9c1b5f500afe2", date: "2021-10-27 20:29:27 UTC", description: "enforce `EventsSent` instrumentation for new-style sinks", pr_number: 9801, scopes: ["sinks"], type: "enhancement", breaking_change: false, author: "Nathan Fox", files_count: 40, insertions_count: 338, deletions_count: 159},
		{sha: "760cc936b3bc7922a9396fa88350a4fc128f4563", date: "2021-10-27 23:31:26 UTC", description: "fix bug in Driver that causes hang", pr_number: 9804, scopes: ["architecture"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 6, insertions_count: 590, deletions_count: 77},
		{sha: "9ef6501e161e9ec003ea4cdda299e54051c8e224", date: "2021-10-28 02:19:22 UTC", description: "Rewrite Splunk HEC metrics sink into the new style", pr_number: 9813, scopes: ["splunk_hec sink"], type: "chore", breaking_change: false, author: "Will", files_count: 29, insertions_count: 1281, deletions_count: 1106},
		{sha: "ea932dbd7e68b1dff402fc8433c59b4486e3c4b6", date: "2021-10-28 17:34:06 UTC", description: "Rewrite datadog_events logs sink in new style", pr_number: 9748, scopes: ["datadog_events sink"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 15, insertions_count: 765, deletions_count: 486},
		{sha: "93eb0b7fd97dfcc2620819060cf813a2657f3aab", date: "2021-10-29 01:43:10 UTC", description: "Initial `throttle` transform", pr_number: 9378, scopes: ["new transform"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 9, insertions_count: 665, deletions_count: 7},
		{sha: "4db23b396508d6b26374a6f831ba5f390f504288", date: "2021-10-29 23:21:46 UTC", description: "rewrite to the new model + add sketch support", pr_number: 9178, scopes: ["datadog_metrics sink"], type: "enhancement", breaking_change: false, author: "Toby Lawrence", files_count: 66, insertions_count: 5355, deletions_count: 1327},
		{sha: "654674624f76c6824e04882fef70347f4afdf03e", date: "2021-10-30 01:52:54 UTC", description: "Add note about ARC change to upgrade guide", pr_number: 9840, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 15, deletions_count: 0},
		{sha: "d314b733ce4c50fde763be9b5adf8d7186e16ed9", date: "2021-11-01 20:52:01 UTC", description: "Test `ConfigBuilderHash` serialization ordering", pr_number: 9807, scopes: ["config"], type: "enhancement", breaking_change: false, author: "Lee Benson", files_count: 2, insertions_count: 84, deletions_count: 20},
		{sha: "655310db39181ed5c2c94f87552ba9d8a8dc328b", date: "2021-11-01 15:45:21 UTC", description: "bump hyper from 0.14.13 to 0.14.14", pr_number: 9779, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2556e2702d5e4a197975b61090e8a0d3d15e4674", date: "2021-11-01 15:54:20 UTC", description: "bump docker/metadata-action from 3.3.0 to 3.6.0", pr_number: 9802, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "12b09aba37b56e2feb302207785433f193b57fe1", date: "2021-11-01 16:57:56 UTC", description: "Fix GitHub release asset upload", pr_number: 9833, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a720c634a4bd3883e82f75f6a9cafd9accff04c4", date: "2021-11-01 19:18:57 UTC", description: "Update S3 source example", pr_number: 9838, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Max Knee", files_count: 1, insertions_count: 30, deletions_count: 28},
		{sha: "8e6a31c3d565a68c2efd6c62d0dcce909dc2073f", date: "2021-11-01 17:25:40 UTC", description: "Remove obsolete `event_per_line` documentation", pr_number: 9839, scopes: ["exec source"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 13, deletions_count: 6},
		{sha: "fdb01c0434d9a70c8f3f02e2544367c10e41b346", date: "2021-11-01 19:30:56 UTC", description: "Run soaks in CI ", pr_number: 9818, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 20, insertions_count: 448, deletions_count: 263},
		{sha: "7796b3e766085225d2ebbe698a43d4015fe303c5", date: "2021-11-02 06:54:24 UTC", description: "Remove unintentional prefix", pr_number: 9848, scopes: ["aws_s3 sink"], type: "fix", breaking_change: false, author: "Will", files_count: 2, insertions_count: 41, deletions_count: 3},
		{sha: "7d8781a53f1e944deb577526dce131ed9dcba9b2", date: "2021-11-02 17:49:58 UTC", description: "add `BytesSent` metric", pr_number: 9835, scopes: ["kafka sink"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 10, insertions_count: 87, deletions_count: 61},
		{sha: "f1d5d9842f88d74bab6e893346eca58a5a7d2055", date: "2021-11-02 15:52:22 UTC", description: "Fix soak test Rust version", pr_number: 9860, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a981c0f96dd030cb3feaaf14039f33e74d00a250", date: "2021-11-02 19:14:55 UTC", description: "rewrite the aws_kinesis_streams sink in the new style", pr_number: 9825, scopes: ["aws_kinesis_streams sink"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 11, insertions_count: 648, deletions_count: 521},
		{sha: "ec5949f238890f89c261043f508aed9607542f35", date: "2021-11-02 18:19:09 UTC", description: "Have soak test workflow compute soaks", pr_number: 9854, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 53, insertions_count: 65, deletions_count: 93},
		{sha: "5022a45ca365938a12439e0aa3b8443448114176", date: "2021-11-02 18:25:42 UTC", description: "Add event processing metric", pr_number: 9830, scopes: ["observability", "vector sink"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 94, deletions_count: 27},
		{sha: "e1b3f308b960c2a3276b5f6cf9f3a3492876badf", date: "2021-11-02 19:03:17 UTC", description: "Fix build error when repl is disabled", pr_number: 9859, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 8, deletions_count: 6},
		{sha: "a1cbf954aa3ec145a533035b80d5d128466409aa", date: "2021-11-02 18:43:52 UTC", description: "Introduce new datadog-agent -> vrl -> blackhole soak", pr_number: 9849, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 7, insertions_count: 167, deletions_count: 8},
		{sha: "7ab19ad899bce81f223794d2d290ad1392aa3d9b", date: "2021-11-02 20:38:04 UTC", description: "bump docker/metadata-action from 3.3.0 to 3.6.0", pr_number: 9865, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "cb688946354fd11cd1d5d9d258ab6af951ee0a0c", date: "2021-11-02 21:07:37 UTC", description: "Instrument with event processing metrics", pr_number: 9683, scopes: ["observability", "vector source"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 161, deletions_count: 67},
		{sha: "a67755afe1dd01620156eed444a42b70dabb506f", date: "2021-11-03 00:58:59 UTC", description: "Fix sink compression options", pr_number: 9869, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Will", files_count: 1, insertions_count: 16, deletions_count: 10},
		{sha: "28bae91e9fbd4c6b4086e2946b45da94ce98a6c9", date: "2021-11-03 22:25:34 UTC", description: "Implicit namespacing based on the config directory structure", pr_number: 9701, scopes: ["config"], type: "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count: 22, insertions_count: 540, deletions_count: 36},
		{sha: "242917af56d435b73b857a82ec5dd6c82eca2d49", date: "2021-11-03 19:23:08 UTC", description: "RFC #9480 - Processing Arrays of Events", pr_number: 9776, scopes: [], type: "docs", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 321, deletions_count: 0},
		{sha: "3e79d524f23db87166abf494db6d4055fa3a005d", date: "2021-11-03 19:55:01 UTC", description: "bump woothee from 0.12.1 to 0.13.0", pr_number: 9846, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "cfb8135edca7291b9cc7bbeb07cc741216a09504", date: "2021-11-03 19:55:27 UTC", description: "bump libc from 0.2.104 to 0.2.106", pr_number: 9843, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c89ce2fa270f3eab8b702e3cef967e32ef1cfce6", date: "2021-11-04 02:16:28 UTC", description: "bump actions/checkout from 2.3.5 to 2.4.0", pr_number: 9882, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 12, insertions_count: 63, deletions_count: 63},
		{sha: "42bebf4a2195230d27862fdacf57ac2d84b57877", date: "2021-11-04 02:26:33 UTC", description: "Rename `config_hash` -> `version`", pr_number: 9845, scopes: ["pipelines"], type: "chore", breaking_change: false, author: "Lee Benson", files_count: 6, insertions_count: 30, deletions_count: 32},
		{sha: "c9577e25deb9be347da1a332d0d552ed38dbc94e", date: "2021-11-03 20:24:47 UTC", description: "Supply defaults for target triples", pr_number: 9885, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count: 105, insertions_count: 11, deletions_count: 1039},
		{sha: "544e4c74bc53e97fc98315dfcd8842187bc3e305", date: "2021-11-04 04:32:52 UTC", description: "Report `datadog.configuration` as a metrics tag for DD Pipelines", pr_number: 9850, scopes: ["pipelines"], type: "enhancement", breaking_change: false, author: "Lee Benson", files_count: 5, insertions_count: 56, deletions_count: 33},
		{sha: "f249bbb37358b32465522659b0f65483efb286e8", date: "2021-11-03 21:39:48 UTC", description: "Use defaults more extensively in CUE", pr_number: 9836, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count: 121, insertions_count: 75, deletions_count: 1001},
		{sha: "53862fb53e04f8198f44e4928c2dccb550ae4108", date: "2021-11-04 02:37:53 UTC", description: "expire metrics", pr_number: 9769, scopes: ["prometheus_exporter sink"], type: "enhancement", breaking_change: false, author: "Luke Steensen", files_count: 1, insertions_count: 95, deletions_count: 11},
		{sha: "9cb91d13c9740a521250580b57b8459cb03f0825", date: "2021-11-04 05:37:13 UTC", description: "Upgrade to Rust 1.56.1", pr_number: 9858, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 7, insertions_count: 8, deletions_count: 10},
		{sha: "54deadeb41a51fcbc1ac7f5dcc254615721295ed", date: "2021-11-04 22:06:46 UTC", description: "load and handle pipelines transforms", pr_number: 9733, scopes: ["pipelines transform"], type: "feat", breaking_change: false, author: "Jérémie Drouet", files_count: 14, insertions_count: 1028, deletions_count: 42},
		{sha: "216ca7db3d58925ee407641d65640bd6867efa01", date: "2021-11-04 22:11:17 UTC", description: "refactor loading for loading recursively", pr_number: 9881, scopes: ["config"], type: "feat", breaking_change: false, author: "Jérémie Drouet", files_count: 1, insertions_count: 128, deletions_count: 120},
		{sha: "433914545e6ac2bc27d8553027b4bbf8261348b1", date: "2021-11-04 19:43:30 UTC", description: "bump wiremock from 0.5.7 to 0.5.8", pr_number: 9886, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ea8193b6886e979b64f524f22bf9ba63a0b6db76", date: "2021-11-04 18:22:39 UTC", description: "Add support for end-to-end acknowledgements", pr_number: 9891, scopes: ["aws_kinesis_firehose source"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 175, deletions_count: 107},
		{sha: "945545ffa0e14cd25378580a6486aab9210b7074", date: "2021-11-05 01:38:32 UTC", description: "Add `gcp_cloud_storage` support", pr_number: 9403, scopes: ["datadog_archives sink"], type: "feat", breaking_change: false, author: "Vladimir Zhuk", files_count: 21, insertions_count: 869, deletions_count: 451},
		{sha: "72f65af7f2b4819bd8d692eea1cb306f85508410", date: "2021-11-05 00:55:28 UTC", description: "bump test-case from 1.2.0 to 1.2.1", pr_number: 9887, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "ca7807fad5faf2efc6ffaffe43190b8b5040cdbb", date: "2021-11-04 20:56:35 UTC", description: "enhance PRs with basic automated labeling", pr_number: 9906, scopes: ["ci"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 3, insertions_count: 53, deletions_count: 0},
		{sha: "ec18c59de1a9547c5295602aaa0448f94b98ba5b", date: "2021-11-04 21:00:32 UTC", description: "rewrite the aws_kinesis_firehose sink in the new style #9825", pr_number: 9861, scopes: ["aws_kinesis_firehose sink"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 10, insertions_count: 685, deletions_count: 548},
		{sha: "a73cce5e1e613f09e8de788a7cef012cde339141", date: "2021-11-04 20:26:19 UTC", description: "Add `tally_value` function", pr_number: 9890, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 103, deletions_count: 0},
		{sha: "72f4db10c63418b4375c3be8c52188a4be1398d0", date: "2021-11-05 02:28:29 UTC", description: "bump tokio from 1.12.0 to 1.13.0", pr_number: 9907, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "5b51015cb052572046d43f4bfb113b3c083918f8", date: "2021-11-04 21:44:22 UTC", description: "Rework acknowledgement config", pr_number: 9883, scopes: ["config", "sources"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 13, insertions_count: 46, deletions_count: 35},
		{sha: "d230d8accc0a17119739fd40ecae61597febb6a7", date: "2021-11-04 21:55:21 UTC", description: "Add support for end-to-end acknowledgements", pr_number: 9892, scopes: ["file sink"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 10, deletions_count: 3},
		{sha: "ac2b1f04fe12251b9c24af58461891100cc0ffe2", date: "2021-11-04 21:06:08 UTC", description: "Analyze all captures at once", pr_number: 9888, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 4, insertions_count: 110, deletions_count: 76},
		{sha: "97d68e3d35f0176e0cc241854105b1143f4b7479", date: "2021-11-05 18:28:15 UTC", description: "new `BufferSender<T>`/`BufferReceiver<T>` + buffer topology builder", pr_number: 9915, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 21, insertions_count: 1514, deletions_count: 26},
		{sha: "9fd723fe38bd6fa2d6c5af1ff017831e663f18b6", date: "2021-11-05 16:07:43 UTC", description: "Add soak analysis as PR comment", pr_number: 9925, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 3, insertions_count: 104, deletions_count: 48},
		{sha: "8b2565713c4c7da8509e008dc323ba704835396a", date: "2021-11-06 00:34:35 UTC", description: "Support DataDog grok parser - parsing DD grok rules ", pr_number: 8850, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Vladimir Zhuk", files_count: 11, insertions_count: 1091, deletions_count: 2},
		{sha: "c4c8278e0430a27b952add9000a2d4eba84148b8", date: "2021-11-05 19:22:41 UTC", description: "bump redis from 0.21.3 to 0.21.4", pr_number: 9931, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b35b7a6dff1274bb7345d2b53ee42257e6b400c9", date: "2021-11-05 19:23:11 UTC", description: "bump tokio-stream from 0.1.7 to 0.1.8", pr_number: 9909, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "4503f2b4d52d9eebc621616ca80306c5347990f9", date: "2021-11-05 18:41:15 UTC", description: "Add missing environment variables to CLI docs", pr_number: 9913, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Luc Perkins", files_count: 7, insertions_count: 210, deletions_count: 105},
		{sha: "ca228bf998156de412f516c995d26b3c8c392707", date: "2021-11-05 20:15:42 UTC", description: "Add support for end-to-end acknowledgements", pr_number: 9893, scopes: ["journald source"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 103, deletions_count: 31},
		{sha: "1333fa30e5b2d26db41a99bd5ec2331de9756b67", date: "2021-11-06 02:56:25 UTC", description: "bump anyhow from 1.0.44 to 1.0.45", pr_number: 9933, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e3af5a2d99f9431fb9ef10cf580c75be1ea4a05e", date: "2021-11-05 21:44:52 UTC", description: "Fix older vector soak builds", pr_number: 9936, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "1dc68baca579345204734b1f2a8be5ccd01d0666", date: "2021-11-06 01:06:02 UTC", description: "improve transform utilization metrics", pr_number: 9828, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Luke Steensen", files_count: 2, insertions_count: 84, deletions_count: 45},
		{sha: "520018ede08fe2a2e3f9623ae1431751dd1f3983", date: "2021-11-06 00:40:23 UTC", description: "Replace index of soak tests", pr_number: 9943, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 7},
		{sha: "499be1a198cd047987f8d1238c11eadb9288d608", date: "2021-11-06 01:03:36 UTC", description: "Implement component spec", pr_number: 9637, scopes: ["journald source", "observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 98, deletions_count: 51},
		{sha: "70b10290da6a0c78a339c4a6c273c834fce00e4c", date: "2021-11-06 01:48:54 UTC", description: "Update how soak tests determine SHA", pr_number: 9944, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "f71b8a7ee2f75e4d9e5507c2cfc0dd3daf0548a0", date: "2021-11-06 19:35:35 UTC", description: "add azure_blog_storage support", pr_number: 9495, scopes: ["datadog_archives sink"], type: "feat", breaking_change: false, author: "Vladimir Zhuk", files_count: 10, insertions_count: 742, deletions_count: 402},
		{sha: "9f789e085cbe7a9cfe4f621f0444eca986fb8ed1", date: "2021-11-08 17:43:46 UTC", description: "bump nom from 7.0.0 to 7.1.0", pr_number: 9932, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 12, deletions_count: 12},
		{sha: "2b58a1df5065cc50859d8e614f21436d209a5275", date: "2021-11-09 02:45:14 UTC", description: "call `Application::run` when running vector as a service on windows instead of manually handling the topology life cycle.", pr_number: 9950, scopes: ["windows platform"], type: "fix", breaking_change: false, author: "Mathieu Stefani", files_count: 1, insertions_count: 38, deletions_count: 45},
		{sha: "077a7a3f8ba0dfa9c3196b4875e5a903a0566219", date: "2021-11-09 03:05:47 UTC", description: "bump paste from 1.0.5 to 1.0.6", pr_number: 9953, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "374dc3a43e72dd1e99c5a04407ebd185e390f04d", date: "2021-11-08 22:57:21 UTC", description: "Fix VRL example on unit testing page", pr_number: 9955, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e67521ca1391ee3ad5977062012c5ea9308f14b1", date: "2021-11-09 02:16:44 UTC", description: "Add Splunk HEC indexer acknowledgement RFC", pr_number: 9863, scopes: [], type: "chore", breaking_change: false, author: "Will", files_count: 1, insertions_count: 407, deletions_count: 0},
		{sha: "cf20fa1ead9027a46b6695e8020f834dde50d739", date: "2021-11-09 06:45:27 UTC", description: "add configuration support for multi-stage buffers", pr_number: 9959, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 49, insertions_count: 630, deletions_count: 450},
		{sha: "d3a4492ee7f250c30c6e26592a7d8d4fdb3de70f", date: "2021-11-09 18:30:32 UTC", description: "Allow CPU/memory targets to be set by user in soaks", pr_number: 9962, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 24, insertions_count: 425, deletions_count: 93},
		{sha: "99e7549ab099b4ac31a2e4590b059ea4f51435eb", date: "2021-11-09 21:35:09 UTC", description: "Fix naming of `compact`'s `object` argument", pr_number: 9972, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 17, deletions_count: 13},
		{sha: "f485cbde2a92f8e5d6646af2d7c1a93761459c2a", date: "2021-11-10 07:00:13 UTC", description: "fix some doc wording in pipelines transform", pr_number: 9965, scopes: ["doc"], type: "chore", breaking_change: false, author: "Jérémie Drouet", files_count: 2, insertions_count: 7, deletions_count: 6},
		{sha: "3b7912c57e885cf9ff7c82ed94844476e508d949", date: "2021-11-09 23:05:31 UTC", description: "Update `flush_period_secs` docs", pr_number: 9976, scopes: ["prometheus_exporter sink"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c818644d13e3a2bf8d72b3d9da3bb8cbd69ab6f5", date: "2021-11-09 23:39:49 UTC", description: "Introduce splunk_hec -> route -> s3 soak", pr_number: 9942, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 9, insertions_count: 167, deletions_count: 3},
		{sha: "a9c310c5dd91c61b6d7cd36aac3b579b833429c2", date: "2021-11-10 23:58:43 UTC", description: "fix transform name in example config", pr_number: 9988, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "6cfb28d54c0347744739e5cde1fefa3ef81713b3", date: "2021-11-11 20:24:41 UTC", description: "Use resource_version of 0 to use cache", pr_number: 9974, scopes: ["kubernetes_logs source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 17, deletions_count: 12},
		{sha: "ed044b35d21e24c009cf327d475157c9652828e4", date: "2021-11-12 03:24:05 UTC", description: "Fix soak test artifact uploading", pr_number: 10008, scopes: ["ci"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "254937976fbe9a55e7c650fb5aded11d0d8a8c6c", date: "2021-11-12 01:31:29 UTC", description: "Introduce fluent -> elasticsearch soak", pr_number: 9997, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 7, insertions_count: 128, deletions_count: 4},
		{sha: "b273b7835f964fd9d7155d002a93e66f5ef90b1f", date: "2021-11-12 18:56:31 UTC", description: "add AWS SQS source", pr_number: 9968, scopes: ["new source"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 42, insertions_count: 1259, deletions_count: 188},
		{sha: "c7e963469f8776da6564165dd0f2e0f5dfba0bff", date: "2021-11-12 22:54:27 UTC", description: "revamp `BatchConfig`/`BatchSettings`", pr_number: 10006, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 60, insertions_count: 882, deletions_count: 672},
		{sha: "3c4e3f37ce03f0d39c33f058f81470e8c5a92572", date: "2021-11-13 00:48:40 UTC", description: "Fix artifact uploading for soak tests", pr_number: 10013, scopes: ["ci"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 35, deletions_count: 7},
		{sha: "ab0744ae8ea1557166257fd1308fac585f751c60", date: "2021-11-13 06:38:13 UTC", description: "added docs for enrichment tables", pr_number: 8817, scopes: ["enriching"], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 9, insertions_count: 344, deletions_count: 44},
		{sha: "f74302c7c1c65bcdd6b7f2e91a478e9874d9385b", date: "2021-11-13 00:50:39 UTC", description: "add error output stream", pr_number: 9417, scopes: ["remap transform"], type: "feat", breaking_change: false, author: "Luke Steensen", files_count: 13, insertions_count: 647, deletions_count: 203},
		{sha: "c2c05b31add2b33363cde334babe558870cb5f5b", date: "2021-11-13 04:05:06 UTC", description: "Remove unused 'test_name' variable in vector module", pr_number: 10003, scopes: [], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 13, insertions_count: 0, deletions_count: 20},
		{sha: "f1cf1c23af8e540bdd9b2e747b80527aa1b50270", date: "2021-11-15 20:39:56 UTC", description: "Add `client_concurrency` to sqs source", pr_number: 10029, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 12, deletions_count: 13},
		{sha: "8258c2e2e2113b82f731fa0a1efc3d195f94fdfa", date: "2021-11-16 02:02:43 UTC", description: "Fix topology query in GraphQL API blog post", pr_number: 10030, scopes: ["blog website"], type: "fix", breaking_change: false, author: "Lee Benson", files_count: 1, insertions_count: 49, deletions_count: 15},
		{sha: "2b6cb3c2fbd4397ea52127bc2ae5955fc50772fd", date: "2021-11-15 21:15:07 UTC", description: "Add docs for no_outputs_from in unit tests", pr_number: 10023, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count: 1, insertions_count: 51, deletions_count: 0},
		{sha: "70dc6a6b1bcb495ba1011aca57d4885628f91ea8", date: "2021-11-15 21:24:15 UTC", description: "This parameter was dropped", pr_number: 10033, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 0, deletions_count: 2},
		{sha: "80f6f5c716ed48c762f7e82a049ae712ac59bc07", date: "2021-11-15 21:38:26 UTC", description: "Rename generator source to demo_logs source", pr_number: 9979, scopes: ["generator source"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 32, insertions_count: 157, deletions_count: 133},
		{sha: "106ebf5e6afaff0be1b5e2432119ea754f8351ff", date: "2021-11-16 02:13:45 UTC", description: "Swap out DCO for CLA", pr_number: 10036, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 7, deletions_count: 67},
		{sha: "2f9d536cdae1f2924958b4b537476a0b5d259b59", date: "2021-11-16 03:08:12 UTC", description: "Fix link to aliased `generator` source", pr_number: 10038, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b53f2bbe602cef541de9d466efba60fd03239940", date: "2021-11-16 19:24:06 UTC", description: "Add Marketo Tracking to Vector.dev", pr_number: 9977, scopes: ["javascript website"], type: "chore", breaking_change: false, author: "David Weid II", files_count: 2, insertions_count: 25, deletions_count: 0},
		{sha: "10ac8cc45f04aefd4a09c0b2a4f3abc41a54185a", date: "2021-11-16 20:59:10 UTC", description: "Survey batch configurations", pr_number: 10039, scopes: ["sinks"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 22, insertions_count: 36, deletions_count: 23},
		{sha: "24ea6de3a7646062df9639364776ff0ff298d3da", date: "2021-11-17 02:21:57 UTC", description: "Add configurable delay to deletion of k8s metadata", pr_number: 10031, scopes: ["kubernetes_logs source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 34, deletions_count: 3},
		{sha: "4dddc6ef3127724d894dc545774aac1163a02d71", date: "2021-11-17 21:05:28 UTC", description: "Add lading splunk_hec soak test modules", pr_number: 10047, scopes: [], type: "chore", breaking_change: false, author: "Will", files_count: 6, insertions_count: 252, deletions_count: 0},
		{sha: "da47e9e813882150890d8cc8436716c476745a14", date: "2021-11-17 21:32:58 UTC", description: "Document timezone configuration", pr_number: 9889, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Will", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "9e006961d3faf3cb83654e075b6725d3461009ad", date: "2021-11-18 01:29:27 UTC", description: "soak-observer CI improvements", pr_number: 10089, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 13, deletions_count: 4},
		{sha: "412eea329d511fed0f0c61b2d49f4d0e258e6461", date: "2021-11-18 01:54:01 UTC", description: "Update batch defaults", pr_number: 10088, scopes: ["loki sink"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 13, deletions_count: 5},
		{sha: "529905972669bf7a878e1e2d2f6e5004e80fdcfe", date: "2021-11-18 02:03:45 UTC", description: "Remove breaking change note for `VECTOR_LOG`", pr_number: 10091, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 9},
		{sha: "5218fb83699c521479b7a12d8bdaa5c3d82f86d7", date: "2021-11-18 03:01:35 UTC", description: "Add highlight for new `throttle` transform", pr_number: 10040, scopes: ["throttle transform"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 72, deletions_count: 0},
		{sha: "b05f5d80a7aec0f14f7d7f240fb20cb8fdf707e2", date: "2021-11-18 03:27:18 UTC", description: "Added automatic namespacing article", pr_number: 10048, scopes: ["config"], type: "docs", breaking_change: false, author: "Barry Eom", files_count: 1, insertions_count: 92, deletions_count: 0},
		{sha: "cf4bcf29d0f38eb4a4e9600154a5cf332bcbbe32", date: "2021-11-18 03:31:11 UTC", description: "Try disabling soak workflow another way", pr_number: 10093, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 331, deletions_count: 336},
		{sha: "b949d93ff8da4948346334e4f11e5d9d9cdd866e", date: "2021-11-18 03:32:37 UTC", description: "Added failed route event routing highlight article", pr_number: 10052, scopes: ["transforms"], type: "docs", breaking_change: false, author: "Barry Eom", files_count: 1, insertions_count: 133, deletions_count: 0},
		{sha: "358301daf84a4d39d5cf6d80fbb90c9a7a9eeb1e", date: "2021-11-18 04:01:47 UTC", description: "added csv enrichment highlight article", pr_number: 10041, scopes: [], type: "docs", breaking_change: false, author: "Barry Eom", files_count: 1, insertions_count: 90, deletions_count: 0},
		{sha: "1fbadaf475292d743774ccc9100d14174c190eb4", date: "2021-11-18 04:10:36 UTC", description: "Rename `window` parameter", pr_number: 10095, scopes: ["throttle transform"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "7d235b90aeda90079e7827f20e32987209dfd7e5", date: "2021-11-18 20:11:29 UTC", description: "Prepare 0.18.0 release", pr_number: 10092, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 363, deletions_count: 2},

	]
}
