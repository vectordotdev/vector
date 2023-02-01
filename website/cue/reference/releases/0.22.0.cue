package metadata

releases: "0.22.0": {
	date:     "2022-05-24"
	codename: ""

	whats_next: [
		{
			title: "Removal of legacy buffers"
			description: """
				With this release of v0.22.0, [we've switched the default for disk buffers to the new `v2`
				implementation](/highlights/2022-04-06-disk-buffer-v2-stable). This means if you set `type
				= "disk"` you will get the new buffer implementation. In a future release, we will remove the legacy
				disk buffers. To continue using the `v1` disk buffers, for now, set `type = "disk_v1"`.
				"""
		},
	]

	known_issues: [
		"The `journald` source deadlocks almost immediately ([#12966](https://github.com/vectordotdev/vector/issues/12966)). Fixed in v0.22.1.",
		"The `kubernetes_logs` source does not work with k3s/k3d ([#12989](https://github.com/vectordotdev/vector/issues/12989)). Fixed in v0.22.1.",
		"Vector would panic when reloading configuration using the `compression` or `concurrency` options due to a deserialization failure ([#12919](https://github.com/vectordotdev/vector/issues/12919)). Fixed in v0.22.1.",
		"When using a component that creates a unix socket, `vector validate` no longer creates the socket ([#13018](https://github.com/vectordotdev/vector/issues/13018)). This causes the default SystemD unit file to fail to start Vector since it runs `vector validate` before starting Vector. Fixed in v0.22.1.",
		"VRL sometimes miscalculates type definitions when conditionals are used causing later usages of values assigned in conditionals to not require type coercion as they should ([#12948](https://github.com/vectordotdev/vector/issues/12948)). Fixed in v0.22.1.",
		"Metrics from AWS components were tagged with an `endpoint` including the full path of the request. For the `aws_s3` sink this caused cardinality issues since the AWS S3 key is included in the URL. Fixed in v0.22.3.",
		"The `gcp_pubsub` source would log errors due to attempting to fetch too quickly when it has no acknowledgements to pass along. Fixed in v0.22.3.",
		"Vector shuts down when a configured source codec (`decoding.codec`) receives invalid data. Fixed in v0.23.1.",
	]

	description: """
		The Vector team is pleased to announce version 0.22.0!

		Be sure to check out the [upgrade guide](/highlights/2022-05-03-0-22-0-upgrade-guide) for breaking changes in
		this release.

		**Important**: as part of this release, we have promoted the new implementation of disk buffers (`buffer.type
		= "disk_v2"`) to the default implementation (`buffer.type = "disk"`). Any existing disk buffers (`disk_v1`
		or `disk`) will be automatically migrated. We have rigorously tested this migration, but recommend making
		a back up of the disk buffers (in the configured `data_dir`, typically in `/var/lib/vector`) to roll back if
		necessary. Please see the [release highlight](/highlights/2022-04-06-disk-buffer-v2-stable) for additional
		updates about this migration.

		In addition to the new features, enhancements, and fixes listed below, this release adds:

		- [Support for iteration has landed in VRL](/highlights/2022-05-18-vrl-iteration-support). Now you can
		  dynamically map unknown key/value pairs in objects and items in arrays. This replaces some common use cases for
		  the `lua` transform with the much more performant [`remap`](\(urls.vector_remap_transform)) transform.
		- [New native event codecs](/highlights/2022-03-31-native-event-codecs) for Vector. We are still rolling out the
		  new codec support to all sinks, but this will allow sending events (logs, metrics, and traces) between Vector
		  instances via transports like `kafka` rather than being limited to the gRPC `vector` source and sink.
		- A new GCP PubSub (`gcp_pubsub`) source to consume events from GCP PubSub.
		- A new `websocket` sink was added to send events to a remote websocket listener.
		- [New VRL functions for encrypting and decrypting data.](/highlights/2022-05-24-vrl-encryption)

		We also made additional performance improvements this release increasing the average throughput by up to 50% for
		common topologies (see our [soak test
		framework](https://github.com/vectordotdev/vector/tree/master/soaks/tests)).

		| experiment                            | Δ mean   |   Δ mean % | confidence   |
		|---------------------------------------|----------|------------|--------------|
		| splunk_transforms_splunk3             | 5.98MiB  |      58.22 | 100.00%      |
		| datadog_agent_remap_blackhole         | 20.1MiB  |      43.44 | 100.00%      |
		| splunk_hec_route_s3                   | 5.28MiB  |      35.34 | 100.00%      |
		| syslog_regex_logs2metric_ddmetrics    | 1.84MiB  |      15.62 | 100.00%      |
		| syslog_log2metric_splunk_hec_metrics  | 2.52MiB  |      15.59 | 100.00%      |
		| datadog_agent_remap_datadog_logs      | 9.6MiB   |      15.02 | 100.00%      |
		| http_to_http_json                     | 2.78MiB  |      13.19 | 100.00%      |
		| syslog_humio_logs                     | 1.91MiB  |      12.23 | 100.00%      |
		| syslog_splunk_hec_logs                | 1.85MiB  |      12.11 | 100.00%      |
		| syslog_loki                           | 1.42MiB  |       9.53 | 100.00%      |
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["splunk_hec source", "delivery"]
			description: """
				The `splunk_hec` source now correctly handles negative acknowledgements from sinks. Previously it would
				mark the request including the rejected events as delivered. In Splunk's acknowledgement protocol, this
				means returning `true` for the `ackID` for the request, but now it correctly returns `false`, indicating
				the request is not acknowledged.
				"""
			pr_numbers: [12125]
		},
		{
			type: "chore"
			scopes: ["buffers"]
			description: """
				Vector now automatically migrates disk buffers from `disk_v1` to `disk_v2` as described in [Switching
				the default implementation of disk buffers to
				`disk_v2`](/highlights/2022-04-06-disk-buffer-v2-stable). In a future release, support for `v1` disk
				buffers will be dropped.
				"""
			pr_numbers: [12069]
		},
		{
			type: "fix"
			scopes: ["gcp_stackdriver_metrics sink"]
			breaking: true
			description: """
				The `gcp_stackdriver_metrics` sink now requires configuration of labels at the top-level to match the
				`gcp_stackdriver_logs` sink. Previously these were nested under `.labels`.

				See [the upgrade guide](/highlights/2022-05-03-0-22-0-upgrade-guide#stackdriver-metrics) for more
				details.
				"""
			pr_numbers: [12124]
		},
		{
			type: "feat"
			scopes: ["vrl"]
			description: """
				VRL now includes two new functions for encrypting and decrypting field values:
				[encrypt](/docs/reference/vrl/functions/#encrypt) and [decrypt](/docs/reference/vrl/functions/#decrypt).
				A [random_bytes](/docs/reference/vrl/functions/#random_bytes) function was added to make it easy to
				generate initialization vectors for the `encrypt` function.

				See the [highlight](/highlights/2022-05-24-vrl-encryption) for more details about this new
				functionality.
				"""
			pr_numbers: [12090]
		},
		{
			type: "fix"
			scopes: ["new_relic sink"]
			description: """
				The `new_relic` sink health check now considers any 200-level response a success. It used to require
				a 200 which did not match what New Relic actually returns: 202.
				"""
			pr_numbers: [12168]
		},
		{
			type: "feat"
			scopes: ["socket source", "syslog source"]
			description: """
				The `socket` and `syslog` sources now allow configuration of the permissions to use when creating a unix
				socket via `socket_file_mode` when `mode = "unix"` is used.
				"""
			pr_numbers: [12115]
			contributors: ["@Sh4d1"]
		},
		{
			type: "enhancement"
			scopes: ["journald source"]
			description: """
				The `journald` source now processes data more efficiently by continuing to read new data while waiting
				for read data to be processed by Vector.
				"""
			pr_numbers: [12209]
		},
		{
			type: "enhancement"
			scopes: ["config"]
			description: """
				Vector's configuration interpolation of environment variables has been enhanced to both allow setting of
				default values and returning an error message if an expected environment variable is unset or empty. The
				syntax matches bash interpolation syntax:

				- `${VARIABLE:-default}` evaluates to default if VARIABLE is unset or empty in the environment.
				- `${VARIABLE-default}` evaluates to default only if VARIABLE is unset in the environment.
				- `${VARIABLE:?err}` exits with an error message containing err if VARIABLE is unset or empty in the environment.
				- `${VARIABLE?err}` exits with an error message containing err if VARIABLE is unset in the environment.
				"""
			pr_numbers: [12150]
			contributors: ["@hhromic"]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				When using Vector's ability to load configuration from a directory (`--config-dir`), Vector now ignores
				subdirectories starting with a `.`.
				"""
			pr_numbers: [12239]
		},
		{
			type: "fix"
			scopes: ["aws_s3 sink"]
			description: """
				The `aws_s3` sink now only sets the `x-amz-tagging` header if tags are being applied. Specifying an
				empty value was incompatible with Ceph.
				"""
			pr_numbers: [12027]
		},
		{
			type: "enhancement"
			scopes: ["datadog provider"]
			description: """
				The Datadog sinks now retry requests that failed due to an invalid API key. This avoids data loss in the
				case that an API key is revoked.
				"""
			pr_numbers: [12291]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				The VRL type definition for `.` and `parse_xml` was corrected to be a map of any field/value rather than
				specifically an empty map. This could cause later false positives with type issues during VRL compilation.
				"""
			pr_numbers: [12333]
		},
		{
			type: "feat"
			scopes: ["vrl"]
			description: """
				VRL now allows for a simple form of string templating via `{{ some_variable }}` syntax. We will be
				expanding support for templating over time. This does mean that any strings that had `{{ }}` in them
				already now need to be escaped. See the [upgrade
				guide](/highlights/2022-05-03-0-22-0-upgrade-guide#vrl-template-strings) for details.
				"""
			pr_numbers: [12180]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				VRL now correctly updates the type definition of variables defined in one scope, that are mutated in another.

				For example:

				```coffeescript
				foo = 1
				{ foo = "bar" }
				upcase(foo)
				```

				Would previously fail to compile because VRL thinks `foo` is an integer when, in fact, it has been
				reassigned to a string.
				"""
			pr_numbers: [12383]
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source", "observability"]
			description: """
				The `kubernetes_logs` source now tags emitted internal metrics with `pod_namespace`.
				"""
			pr_numbers: [12403]
		},
		{
			type: "fix"
			scopes: ["internal_metrics source"]
			description: """
				The `internal_metrics` source now correctly tags emitted metrics with `host` and `pid` when `host_key`
				and `pid_key` are configured, respectively, on the `internal_metrics` source.
				"""
			pr_numbers: [12320]
		},
		{
			type: "fix"
			scopes: ["socket source"]
			description: """
				The `socket` source now discards UDP frames greater than the configured `max_length` (when `mode
				= "udp"`). Previously these were truncated rather than discarded, which did not match the behavior when
				`mode = "tcp"`. All `socket` source modes are now consistent with dropping messages greater than
				`max_length`.
				"""
			pr_numbers: [12023]
		},
		{
			type: "feat"
			scopes: ["codecs"]
			description: """
				Vector has two new codecs that can be used on sources and sinks to encode as Vector's native
				representation: `native` and `native_json`. This makes it easier to send events between Vector instances
				on transports like `kafka`. It also makes it possible to send metrics to Vector from an external process
				(such as when using the `exec` source) without needing to use the `lua` transform to convert logs to
				metrics. Previously, these generic sources (like `exec` or `http`) could only receive logs. See the
				[release highlight](/highlights/2022-03-31-native-event-codecs) for more about this new feature and how
				to use it.
				"""
			pr_numbers: [12048]
		},
		{
			type: "enhancement"
			scopes: ["datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now supports sending aggregated summary metrics (typically scraped from
				a Prometheus exporter) to Datadog. Previously these metrics were dropped at the sink.
				"""
			pr_numbers: [12436]
		},
		{
			type: "enhancement"
			scopes: ["datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now supports sending aggregated summary metrics (typically scraped from
				a Prometheus exporter) to Datadog. Previously these metrics were dropped at the sink.
				"""
			pr_numbers: [12436]
		},
		{
			type: "fix"
			scopes: ["internal_logs source"]
			description: """
				The `internal_logs` source occasionally missed some events generated early in Vector's start-up, before
				the component was initialized. This was remedied so that the `internal_logs` source more reliably
				captures start-up events.
				"""
			pr_numbers: [12411]
		},
		{
			type: "feat"
			scopes: ["gcp_pubsub source", "sources"]
			description: """
				A new `gcp_pubsub` source was added for consuming events from [GCP PubSub](https://cloud.google.com/pubsub).
				"""
			pr_numbers: [12057]
		},
		{
			type: "feat"
			scopes: ["websocket sink", "sinks"]
			description: """
				A new `websocket` sink was added for sending events to a remote websocket listener.
				"""
			pr_numbers: [9632]
			contributors: ["@zshell31"]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				The `parse_ruby_hash` VRL function can now parse hashes that contain a symbol as the value, such as `{ "key" => :foo }`.
				"""
			pr_numbers: [12514]
		},
		{
			type: "enhancement"
			scopes: ["releasing"]
			description: """
				The RPM package now adds the created `vector` user to the `systemd-journal-remote` group to be able to
				consume journald events from a remote system. This matches the Debian package.
				"""
			pr_numbers: [12563]
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source now allows configuration of `extra_namespace_label_selector` which Vector
				will to use select the pods to capture the logs of, if set, based on labels attached to the pod
				namespace. This is similar to the `extra_label_selector` option which applies to pod labels.
				"""
			contributors: ["@anapsix"]
			pr_numbers: [12438]
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source now reads events in order whenever a pod log file rotates. Previously
				Vector could start reading the new file before it finished processing the previous one, resulting in the
				logs being out-of-order.
				"""
			pr_numbers: [12330]
			contributors: ["@sillent"]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				The `parse_json` function now takes an optional `max_depth` parameter to control how far it will recurse
				when deserializing the event. Once the depth limit is hit, the remainder of the fields is left as raw
				JSON in the deserialized event.

				For example:

				```coffeescript
				parse_json!("{\"1\": {\"2\": {\"3\": {\"4\": {\"5\": {\"6\": \"finish\"}}}}}}", max_depth: 5)
				```

				Yields:

				```json
				{ "1": { "2": { "3": { "4": { "5": "{\"6\": \"finish\"}" } } } } }
				```

				The default remains no max depth limit.
				"""
			pr_numbers: [12545]
			contributors: ["@nabokihms"]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				A new `component_received_events_count` histogram metric was added to record the sizes of event batches
				passed around in Vector's internal topology. Note that this is different than sink-level batching. It is
				mostly useful for debugging performance issues in Vector due to low internal batching.
				"""
			pr_numbers: [11290]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				The `log` function in VRL no longer wraps logged string values in quotes. This was causing double
				quoting for sink encodings like `json`.
				"""
			pr_numbers: [12609]
			contributors: ["@nabokihms"]
		},
		{
			type: "fix"
			scopes: ["aws_s3 source"]
			description: """
				The `aws_s3` source now handles S3 object keys that contain spaces. Previously Vector would encounter
				a 404 when querying for objects due to not decoding spaces correctly from the SQS object notification.
				"""
			pr_numbers: [12664]
		},
		{
			type: "fix"
			scopes: ["gcp provider"]
			description: """
				GCP sinks now correctly handle authentication token refreshing from the metadata service when the health
				check fails.
				"""
			pr_numbers: [12645]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				The `http` config provider now correctly repolls when an error is encountered.
				"""
			pr_numbers: [12580]
			contributors: ["@jorgebay"]
		},
		{
			type: "enhancement"
			scopes: ["http source"]
			description: """
				The `http` source now allows configuration of the HTTP method to expect requests with via the new
				`method` option. Previously it only allowed POST requests.
				"""
			pr_numbers: [12424]
			contributors: ["@r3b-fish"]
		},
		{
			type: "fix"
			scopes: ["codecs", "vrl"]
			description: """
				The `logfmt` sink codec and as the `encode_logfmt` function now correctly wrap values that contain
				quotes (`"`) in quotes and escape the inner quote.
				"""
			pr_numbers: [12700]
			contributors: ["@jalaziz"]
		},
		{
			type: "feat"
			scopes: ["vrl"]
			description: """
				A new `is_json` function was added to VRL. This allows more efficient checking of whether the incoming
				value is JSON vs. trying to parse it using `parse_json` and checking if there was an error.
				"""
			pr_numbers: [12747]
			contributors: ["@nabokihms"]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				All components now emit consistent metrics in accordance with Vector's [component
				specification](https://github.com/vectordotdev/vector/blob/927fd2eeb4ee15b10f8c046f0f9347789ca1c356/docs/specs/component.md).
				"""
			pr_numbers: [12572, 12668, 12755]
		},
	]

	commits: [
		{sha: "6a930b5d1f57c3c40b80d5a881982190286e4966", date: "2022-04-08 07:54:26 UTC", description: "Fix handling of acknowledgements", pr_number:                                                12125, scopes: ["splunk_hec source"], type:                   "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      2, insertions_count:    85, deletions_count:   30},
		{sha: "6d5c63311e76e78e24f630696b8a3b9a48ba4f81", date: "2022-04-09 04:49:39 UTC", description: "String interpolation RFC", pr_number:                                                        8467, scopes: ["vrl"], type:                                  "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     1, insertions_count:    273, deletions_count:  0},
		{sha: "6102577eaf97651fb4c41501325e155873ad6e12", date: "2022-04-09 04:34:17 UTC", description: "add automatic migration of disk v1 buffers to disk v2, make disk v2 default", pr_number:     12069, scopes: ["buffers"], type:                             "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      26, insertions_count:   857, deletions_count:  62},
		{sha: "fb769eb7ddd0dba25aed95b44740bee6808b9ecf", date: "2022-04-09 03:05:43 UTC", description: "Separate out note about _default route", pr_number:                                          12144, scopes: ["route transform"], type:                     "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      1, insertions_count:    5, deletions_count:    4},
		{sha: "9d861d191ec6bcd3cafa0579994cc84c9a5149a1", date: "2022-04-09 06:06:37 UTC", description: "bump actions/download-artifact from 2 to 3", pr_number:                                      12140, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    84, deletions_count:   84},
		{sha: "962c5aeb081cec821fe0f40a2bd01734614c4385", date: "2022-04-09 06:06:46 UTC", description: "bump actions/upload-artifact from 2 to 3", pr_number:                                        12139, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    40, deletions_count:   40},
		{sha: "eb99b2cadc3896089eeed3fa12dcfe8cfb9849f2", date: "2022-04-09 04:02:53 UTC", description: "bump libc from 0.2.121 to 0.2.122", pr_number:                                               12113, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "904f5e4eeea5cdd785d4d4171f39392d1f030bdd", date: "2022-04-09 04:03:06 UTC", description: "bump serde_with from 1.12.0 to 1.12.1", pr_number:                                           12130, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "994973df033c8f6a2dd46932d4a37b85ca8971a3", date: "2022-04-09 04:03:19 UTC", description: "bump bollard from 0.11.1 to 0.12.0", pr_number:                                              12091, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    128, deletions_count:  39},
		{sha: "046d2229e632227228d0381efdc027df2b8fa0a5", date: "2022-04-09 04:03:26 UTC", description: "bump encoding_rs from 0.8.30 to 0.8.31", pr_number:                                          12092, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "d3875e224ff607667a7c62b2d260a4434e589eb1", date: "2022-04-09 04:03:40 UTC", description: "bump docker/metadata-action from 3.6.2 to 3.7.0", pr_number:                                 12108, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:    4, deletions_count:    4},
		{sha: "b9f661b8b6066774cae34595f1b6231dc8d4586f", date: "2022-04-09 04:03:57 UTC", description: "bump indexmap from 1.8.0 to 1.8.1", pr_number:                                               12015, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:    7, deletions_count:    7},
		{sha: "0a946cf7719de3a45b7e17723a638e8dab5a8309", date: "2022-04-09 04:30:13 UTC", description: "Move soak build jobs to test runners", pr_number:                                            12146, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    6, deletions_count:    2},
		{sha: "15e0867ca9cfe9e97ea4c92daceaad52b5fea2cb", date: "2022-04-09 13:46:33 UTC", description: "bump crossterm from 0.23.1 to 0.23.2", pr_number:                                            12074, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    8, deletions_count:    7},
		{sha: "a0ab382f86c3baabe6a816c3d0f09bfd51d0cee0", date: "2022-04-09 09:12:36 UTC", description: "Clean up a couple pieces of tech debt", pr_number:                                           12148, scopes: ["journald source"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:    25, deletions_count:   48},
		{sha: "74b7da00a0a7c5baf1518cb5619fb134f60d1cd8", date: "2022-04-09 23:42:14 UTC", description: "bump tonic from 0.6.2 to 0.7.0", pr_number:                                                  12062, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    259, deletions_count:  92},
		{sha: "e2530d8d821d088aaf56882e938df8f14516894f", date: "2022-04-10 06:17:17 UTC", description: "flatten the resource.labels to match stackdriver logs", pr_number:                           12124, scopes: ["gcp_stackdriver_metrics sink"], type:        "fix", breaking_change:         true, author:  "Spencer Gilbert", files_count:    2, insertions_count:    66, deletions_count:   0},
		{sha: "7fd758a749cfb42d538d17e6f24054261c97075b", date: "2022-04-12 07:55:01 UTC", description: "mention ytt as another available templating tool", pr_number:                                12167, scopes: ["config"], type:                              "docs", breaking_change:        false, author: "Hugo Hromic", files_count:        3, insertions_count:    6, deletions_count:    4},
		{sha: "d1e4a7cc9a650e53a3e3fa5b8922b65ae1ee5418", date: "2022-04-12 00:25:59 UTC", description: "bump tracing from 0.1.32 to 0.1.33", pr_number:                                              12152, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:    47, deletions_count:   47},
		{sha: "cd85b43a2faeb9bcb5838e638662729efae97a3f", date: "2022-04-12 11:55:01 UTC", description: "Move soak build jobs back to scoped runners runners", pr_number:                             12164, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    2, deletions_count:    6},
		{sha: "4ad156fb7bf5af917e18dea823499368f3a96d4e", date: "2022-04-13 05:57:43 UTC", description: "Add encrypt / decrypt / random_bytes functions", pr_number:                                  12090, scopes: ["vrl"], type:                                 "feat", breaking_change:        false, author: "Nathan Fox", files_count:         17, insertions_count:   1152, deletions_count: 7},
		{sha: "8e10aeb7170be9213eec1fb9e4b54e78676afe96", date: "2022-04-13 05:35:04 UTC", description: "Fix healthcheck response status check", pr_number:                                           12168, scopes: ["new_relic sink"], type:                      "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "87de95792b69fbcb86646103131d85bd12c4e5e0", date: "2022-04-13 05:42:04 UTC", description: "bump console-subscriber from 0.1.3 to 0.1.4", pr_number:                                     12174, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    132, deletions_count:  12},
		{sha: "6a6271a56a0769d07d6ff11eef6eae940ada6fbc", date: "2022-04-13 05:42:21 UTC", description: "bump flate2 from 1.0.22 to 1.0.23", pr_number:                                               12173, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    14, deletions_count:   5},
		{sha: "a95488df1c3149e01ae98221d59af2ef83150056", date: "2022-04-13 13:23:51 UTC", description: "bump async-graphql from 3.0.37 to 3.0.38", pr_number:                                        12154, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    11, deletions_count:   11},
		{sha: "78346070937c89f1295c94160f9362081b7a168a", date: "2022-04-13 07:08:58 UTC", description: "Add microbenchmarks to lib/datadog/grok", pr_number:                                         12172, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 7, insertions_count:    128, deletions_count:  24},
		{sha: "e02f1200ff0b0147fed654b2cc8795d6cbddc7b9", date: "2022-04-13 14:33:09 UTC", description: "bump enumflags2 from 0.7.4 to 0.7.5", pr_number:                                             12153, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "07ff4ee8b7dffe112fdff8bcc98a81d39f4eef2e", date: "2022-04-13 17:13:48 UTC", description: "allow custom permissions on socket", pr_number:                                              12115, scopes: ["socket source", "syslog source"], type:      "feat", breaking_change:        false, author: "Patrik", files_count:             12, insertions_count:   188, deletions_count:  23},
		{sha: "3f21c911152c8b560b01ad5926cf889471c3402f", date: "2022-04-13 15:14:50 UTC", description: "bump tracing-subscriber from 0.3.10 to 0.3.11", pr_number:                                   12155, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:    8, deletions_count:    8},
		{sha: "e0aecd844e0acb72931bed20afbe57bb2da7e68c", date: "2022-04-13 23:37:33 UTC", description: "Implement additional `Encoder`/`EncodingConfig` without framing", pr_number:                 12156, scopes: ["codecs"], type:                              "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      6, insertions_count:    266, deletions_count:  64},
		{sha: "8227a614948b9e331e98071b99fba049e53a4ae1", date: "2022-04-14 08:23:34 UTC", description: "Implement `StandardEncodingsMigrator`", pr_number:                                           12157, scopes: ["codecs"], type:                              "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      2, insertions_count:    51, deletions_count:   0},
		{sha: "a79e6e212d72a2459168c7911dbfbe10633fb4d1", date: "2022-04-13 23:43:56 UTC", description: "Tidy up our soak runs", pr_number:                                                           12189, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 1, insertions_count:    5, deletions_count:    4},
		{sha: "41015b7612ce78ba41195b2528603c4c9f781d0f", date: "2022-04-14 04:11:16 UTC", description: "Add target specification", pr_number:                                                        11477, scopes: [], type:                                      "chore", breaking_change:       false, author: "Ben Johnson", files_count:        1, insertions_count:    183, deletions_count:  0},
		{sha: "33366b9ab6c458a2602ae7a235b9021fb89cbda0", date: "2022-04-14 10:22:44 UTC", description: "bump kube from 0.70.0 to 0.71.0", pr_number:                                                 12192, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    9, deletions_count:    9},
		{sha: "44fa7e08030f23c15a670c2fbca549a8990db054", date: "2022-04-14 11:10:11 UTC", description: "bump tracing-core from 0.1.24 to 0.1.25", pr_number:                                         12191, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:    15, deletions_count:   15},
		{sha: "08e305bb10931a803c043f875ebd3d9ca4743fec", date: "2022-04-14 11:35:27 UTC", description: "bump async-graphql-warp from 3.0.37 to 3.0.38", pr_number:                                   12193, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "10c2bda1cebf0e9151eb157e6d17ba1fe607e5c0", date: "2022-04-14 11:46:22 UTC", description: "bump libc from 0.2.122 to 0.2.123", pr_number:                                               12194, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "2b308641712539cdb7e54df46f4b89fafe6cb8ee", date: "2022-04-14 12:08:41 UTC", description: "Use single task for finalization", pr_number:                                                12138, scopes: ["aws_sqs source"], type:                      "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:      6, insertions_count:    79, deletions_count:   56},
		{sha: "c1dbaa5f39187fed7b11b1b82c2840ab21dcdb2c", date: "2022-04-15 07:50:58 UTC", description: "document latest VRL iteration design changes", pr_number:                                    12199, scopes: ["internal docs"], type:                       "chore", breaking_change:       false, author: "Jean Mertz", files_count:         21, insertions_count:   681, deletions_count:  221},
		{sha: "7a50e86ab5e2b92d2be95524069a0e88e7b53826", date: "2022-04-15 02:05:39 UTC", description: "Add new configurations for agents and aggregators", pr_number:                               12202, scopes: ["config"], type:                              "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:    4, insertions_count:    142, deletions_count:  0},
		{sha: "7ef198558edbbf785fb84a8bb9043d95efe216ec", date: "2022-04-15 00:27:32 UTC", description: "Fix release artifact upload filename", pr_number:                                            12216, scopes: ["ci"], type:                                  "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:    1, deletions_count:    1},
		{sha: "06aca83182ae6129000023b0709e217a454e0d0b", date: "2022-04-15 02:02:50 UTC", description: "Handle acknowledgements asynchronously", pr_number:                                          12209, scopes: ["journald source"], type:                     "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:      3, insertions_count:    106, deletions_count:  45},
		{sha: "d8f74c4e1fef1664199f3d86ea4e56d2737a53c1", date: "2022-04-15 13:57:07 UTC", description: "Integrate `encoding::Encoder` with `socket` sink", pr_number:                                10684, scopes: ["socket sink"], type:                         "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      20, insertions_count:   594, deletions_count:  288},
		{sha: "ebac7fb7f01ece6d1a4150ba1719e8f6b4b25ada", date: "2022-04-15 05:30:58 UTC", description: "Update k8s manifests for 0.21.0", pr_number:                                                 12226, scopes: ["releasing"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      17, insertions_count:   21, deletions_count:   21},
		{sha: "593932149fe40b133c5416b44044c9a6720f5849", date: "2022-04-15 07:53:35 UTC", description: "start to encapsulate config representation", pr_number:                                      12101, scopes: ["topology"], type:                            "chore", breaking_change:       false, author: "Luke Steensen", files_count:      8, insertions_count:    123, deletions_count:  126},
		{sha: "656316f1adf604f8a852da8c9a8c1eab5ef3adc0", date: "2022-04-16 09:53:18 UTC", description: "extend environment variables interpolation syntax", pr_number:                               12150, scopes: ["config"], type:                              "enhancement", breaking_change: false, author: "Hugo Hromic", files_count:        4, insertions_count:    104, deletions_count:  34},
		{sha: "64a0a927a65bb3e9d97ba17e957d343b9a27e707", date: "2022-04-20 02:57:51 UTC", description: "add VrlImmutableTarget for conditions.", pr_number:                                          11916, scopes: ["vrl"], type:                                 "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:     5, insertions_count:    162, deletions_count:  66},
		{sha: "b29c727c41c4f5b56a51b253e66c64ee81c3a50d", date: "2022-04-20 01:55:46 UTC", description: "Remove unmaintained nixpkg, note that nixpkg is community maintained", pr_number:            12261, scopes: [], type:                                      "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    3, insertions_count:    4, deletions_count:    107},
		{sha: "6d007e2d63f6d08357098f94ce3a645123002674", date: "2022-04-20 07:59:42 UTC", description: "ignore directories starting with a dot", pr_number:                                          12239, scopes: ["config"], type:                              "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:     1, insertions_count:    9, deletions_count:    1},
		{sha: "018780694e14bc299b4a18764707e727928c8d53", date: "2022-04-20 08:39:47 UTC", description: "correct conversion of datetime patterns to strftime format", pr_number:                      12241, scopes: ["vrl"], type:                                 "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      4, insertions_count:    130, deletions_count:  28},
		{sha: "0a01b3b59906593b3a31f4bc8708cd14079e1c96", date: "2022-04-20 09:14:16 UTC", description: "bump tracing from 0.1.33 to 0.1.34", pr_number:                                              12232, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:    48, deletions_count:   48},
		{sha: "28624bdf7f7a961944e4f383e719cca25a1ea79e", date: "2022-04-20 03:16:17 UTC", description: "Bump version to 0.22.0", pr_number:                                                          12285, scopes: ["releasing"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:    2, deletions_count:    2},
		{sha: "06e05be09945c69650874a5da7f6ff684dcb2277", date: "2022-04-20 03:32:04 UTC", description: "Conditionally set tagging", pr_number:                                                       12027, scopes: ["aws_s3 sink"], type:                         "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:    5, deletions_count:    5},
		{sha: "7a997179a93f4f572eebb2759de43f2813522299", date: "2022-04-20 13:22:27 UTC", description: "bump nats from 0.18.1 to 0.19.0", pr_number:                                                 12287, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    8, deletions_count:    43},
		{sha: "546aca62d51da7b93df98cd0bb9a9cefc5aa10ac", date: "2022-04-20 15:19:42 UTC", description: "bump tracing-core from 0.1.25 to 0.1.26", pr_number:                                         12294, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:    17, deletions_count:   17},
		{sha: "b762e833d9b5541394a1fd340c6ea9bc09e643f2", date: "2022-04-20 11:16:49 UTC", description: "remove some seemingly unused dependencies", pr_number:                                       12268, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "Luke Steensen", files_count:      20, insertions_count:   10, deletions_count:   108},
		{sha: "ed5b29a77cf40b39c46f60e23b0a0c6ea8b13a7a", date: "2022-04-20 16:52:00 UTC", description: "bump mongodb from 2.1.0 to 2.2.0", pr_number:                                                12234, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    64, deletions_count:   102},
		{sha: "c41e3e95b4052691012a71cf67a7f92271b98591", date: "2022-04-21 01:41:58 UTC", description: "Use multiple of CHUNK_SIZE for SOURCE_SENDER_BUFFER_SIZE", pr_number:                        11732, scopes: ["performance"], type:                         "chore", breaking_change:       false, author: "Will", files_count:               3, insertions_count:    9, deletions_count:    7},
		{sha: "f6e55f5f69235a5413e70a617b3821f280ec6757", date: "2022-04-20 22:08:05 UTC", description: "bump clap from 3.1.8 to 3.1.10", pr_number:                                                  12298, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    20, deletions_count:   14},
		{sha: "7b5aff41a7aec16c445c1fa406b210181b354092", date: "2022-04-20 21:10:25 UTC", description: "bump wiremock from 0.5.12 to 0.5.13", pr_number:                                             12301, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "d9f92b70bd82fb8cd0cd873f8652bd46fbdce520", date: "2022-04-20 21:11:36 UTC", description: "bump tracing-test from 0.1.0 to 0.2.1", pr_number:                                           12296, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    16, deletions_count:   46},
		{sha: "ef786a87774144e7151c1ff382857c24e15c511f", date: "2022-04-20 21:12:06 UTC", description: "bump rmp-serde from 1.0.0 to 1.1.0", pr_number:                                              12295, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    6, deletions_count:    5},
		{sha: "d57ab2ab18d04b86fe460df77426f469a4ee6d1a", date: "2022-04-21 00:11:33 UTC", description: "Upgrade AWS SDK to 0.10.1", pr_number:                                                       12292, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      2, insertions_count:    64, deletions_count:   62},
		{sha: "4b5359fdc7756069e6b1599dfeffbaa979d21962", date: "2022-04-21 08:15:25 UTC", description: "use array instead of map for pipelines", pr_number:                                          12305, scopes: ["pipelines transform"], type:                 "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:     14, insertions_count:   727, deletions_count:  954},
		{sha: "89e5f8075aaa6fd3448e01dc296650ef7f99bfcb", date: "2022-04-21 06:26:11 UTC", description: "bump toml from 0.5.8 to 0.5.9", pr_number:                                                   12297, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    5, deletions_count:    5},
		{sha: "bf0ce0c1346b16ce06a16336ca22234b728b347f", date: "2022-04-21 03:25:29 UTC", description: "Fix links to default configurations in target spec", pr_number:                              12312, scopes: [], type:                                      "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "1052e95bb1534e606d8756bcf138df1261687de0", date: "2022-04-21 08:17:23 UTC", description: "bump libc from 0.2.123 to 0.2.124", pr_number:                                               12308, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "ff7317efc022096721d140bad38cfb216d887014", date: "2022-04-21 03:41:07 UTC", description: "bump prost to 0.10.1 and tonic to 0.7.1", pr_number:                                         12222, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      6, insertions_count:    119, deletions_count:  168},
		{sha: "ed303fdb1ab672ed2efc55065e2f75d7226e3b41", date: "2022-04-21 06:55:05 UTC", description: "update link to releases", pr_number:                                                         12316, scopes: [], type:                                      "chore", breaking_change:       false, author: "Johan Bergström", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "307164460f44f79b93903bdd3db28c3bb54fe911", date: "2022-04-21 04:38:43 UTC", description: "Retry forbidden requests", pr_number:                                                        12291, scopes: ["datadog provider"], type:                    "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:      4, insertions_count:    31, deletions_count:   14},
		{sha: "3224b5d794ee972d2653ef67e8bef637e4f89374", date: "2022-04-21 23:41:51 UTC", description: "Add region and endpoint to [enterprise] configuration", pr_number:                           12205, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Will", files_count:               3, insertions_count:    35, deletions_count:   8},
		{sha: "d7a854af54a77115363bed003fc425cb2db22c16", date: "2022-04-22 00:59:43 UTC", description: "update comment not being up to date", pr_number:                                             12335, scopes: ["pipelines transform"], type:                 "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:     1, insertions_count:    73, deletions_count:   26},
		{sha: "575d74ba67e15f080214731a13aaf7d0d96c0edc", date: "2022-04-22 06:10:20 UTC", description: "return correct type def for root path", pr_number:                                           12333, scopes: ["vrl"], type:                                 "fix", breaking_change:         false, author: "Jean Mertz", files_count:         3, insertions_count:    75, deletions_count:   6},
		{sha: "f6ea9ca83f0b16bc5d639085264991d8e0955257", date: "2022-04-22 06:41:06 UTC", description: "improve collection \"kind\" display", pr_number:                                             12318, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         2, insertions_count:    176, deletions_count:  0},
		{sha: "d3a6193c16fb81359e56ff552ac27fbb7ee78391", date: "2022-04-21 23:08:10 UTC", description: "Add protoc program to Ubuntu and MacOS bootstrap images", pr_number:                         12331, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      2, insertions_count:    2, deletions_count:    1},
		{sha: "7ddd86954294573f1e4886fbf3aade84b7d192ad", date: "2022-04-22 07:02:09 UTC", description: "bump nix from 0.23.1 to 0.24.0", pr_number:                                                  12332, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    17, deletions_count:   5},
		{sha: "9907d3c032a39de0cc8724f4a89e31c4615ab4d8", date: "2022-04-22 02:18:18 UTC", description: "Deny unknown options in TLS settings", pr_number:                                            12341, scopes: ["config"], type:                              "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      1, insertions_count:    1, deletions_count:    0},
		{sha: "cff389c904e1da6df22a90120ec3c9984e714e13", date: "2022-04-22 09:35:19 UTC", description: "implement template strings in VRL", pr_number:                                               12180, scopes: ["vrl"], type:                                 "enhancement", breaking_change: true, author:  "Stephen Wakely", files_count:     15, insertions_count:   602, deletions_count:  154},
		{sha: "02efb43479d3f838cbfd3a1550087a56a3976ecc", date: "2022-04-22 06:32:04 UTC", description: "Revert prost/tonic upgrade", pr_number:                                                      12350, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      8, insertions_count:    169, deletions_count:  121},
		{sha: "f3c8c50be907ef5a7d394405c72b6e9257309c3c", date: "2022-04-23 01:47:45 UTC", description: "bump anyhow from 1.0.56 to 1.0.57", pr_number:                                               12359, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "1819698cd8c44e454511fc1d50cb69be1a078116", date: "2022-04-23 01:48:11 UTC", description: "bump tracing-log from 0.1.2 to 0.1.3", pr_number:                                            12360, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "d16e339b9510e0cf432852e5fa202485ca8035ce", date: "2022-04-23 01:49:26 UTC", description: "bump clap from 3.1.10 to 3.1.11", pr_number:                                                 12358, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    10, deletions_count:   10},
		{sha: "7e9cae82cd8b00a87bc61198cd6742be18a42eda", date: "2022-04-23 04:54:26 UTC", description: "allow ignoring cue during test runs", pr_number:                                             12334, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         2, insertions_count:    10, deletions_count:   2},
		{sha: "6e47fe94a8ff78626e9d082c22d56c8874207963", date: "2022-04-23 05:18:43 UTC", description: "add syntax support for function closures", pr_number:                                        12336, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         6, insertions_count:    163, deletions_count:  5},
		{sha: "7cea91a50a476a64e85610cb2037def9dac9c848", date: "2022-04-23 04:39:19 UTC", description: "added documentation for template strings", pr_number:                                        12352, scopes: ["external docs"], type:                       "docs", breaking_change:        false, author: "Stephen Wakely", files_count:     1, insertions_count:    18, deletions_count:   1},
		{sha: "d5d4727ef83d75108b78cf55e14e3ab93ed85413", date: "2022-04-23 05:52:40 UTC", description: "Integrate `encoding::Encoder` with `kafka` sink", pr_number:                                 12133, scopes: ["kafka sink", "codecs"], type:                "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      5, insertions_count:    36, deletions_count:   23},
		{sha: "6ece04de8d77a4795f1834088b946bcf22805070", date: "2022-04-23 06:29:21 UTC", description: "Integrate `encoding::Encoder` with `aws_kinesis_firehose` sink", pr_number:                  12176, scopes: ["aws_kinesis_firehose sink", "codecs"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      5, insertions_count:    34, deletions_count:   15},
		{sha: "1c5a71d9babc5eb164156eb52f05eede40d1ec02", date: "2022-04-22 22:50:04 UTC", description: "bump goauth from 0.11.1 to 0.12.0", pr_number:                                               12361, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    14, deletions_count:   134},
		{sha: "d8b78e65d02906fedaa19b2f729b4da162c9822e", date: "2022-04-23 07:47:02 UTC", description: "Integrate `encoding::Encoder` with `aws_cloudwatch_logs` sink", pr_number:                   12175, scopes: ["aws_cloudwatch_logs sink", "codecs"], type:  "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      5, insertions_count:    45, deletions_count:   34},
		{sha: "8287b09672cacd7f921fd6f8c2c5474b5f7f2ac8", date: "2022-04-23 09:30:42 UTC", description: "RFC for an Opentelemetry traces source", pr_number:                                          11802, scopes: ["new source"], type:                          "feat", breaking_change:        false, author: "Pierre Rognant", files_count:     1, insertions_count:    402, deletions_count:  0},
		{sha: "f6e6d3ee79454eff519691c6983a6069f6cf234b", date: "2022-04-23 11:01:04 UTC", description: "remove it", pr_number:                                                                       12370, scopes: ["compound transform"], type:                  "chore", breaking_change:       false, author: "Pierre Rognant", files_count:     10, insertions_count:   2, deletions_count:    431},
		{sha: "c6fd8151a7716f38d1bec12664253c9561e5f9ad", date: "2022-04-24 15:41:54 UTC", description: "fix typo in 0.21.1 release note", pr_number:                                                 12380, scopes: [], type:                                      "docs", breaking_change:        false, author: "W.T. Chang", files_count:         1, insertions_count:    2, deletions_count:    2},
		{sha: "329e3bfffc8b1b0e55e5bb3201450ddede1d7253", date: "2022-04-25 22:11:14 UTC", description: "ensure the example folder is validated with namespacing", pr_number:                         12367, scopes: ["ci"], type:                                  "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:     2, insertions_count:    8, deletions_count:    4},
		{sha: "0af1ef8a68e49010af042bdc1e9dccc962ccbfb4", date: "2022-04-25 22:46:25 UTC", description: "bump nix from 0.24.0 to 0.24.1", pr_number:                                                  12395, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    4, deletions_count:    4},
		{sha: "1e51c409c03f1b82e54a10bfafb26d6fbf70cfc9", date: "2022-04-25 22:47:06 UTC", description: "bump mongodb from 2.2.0 to 2.2.1", pr_number:                                                12394, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    5, deletions_count:    5},
		{sha: "0aa528239e88de8fca44f50280cadd0b46073f59", date: "2022-04-25 22:48:44 UTC", description: "bump clap from 3.1.11 to 3.1.12", pr_number:                                                 12392, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    10, deletions_count:   10},
		{sha: "97282389017b51ac93351d841bb37b185728fa61", date: "2022-04-25 22:49:26 UTC", description: "bump tui from 0.17.0 to 0.18.0", pr_number:                                                  12389, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    5, deletions_count:    35},
		{sha: "9f66419712eb0ef39446d3ba8ff28bde4c036359", date: "2022-04-25 22:50:55 UTC", description: "bump crc from 2.1.0 to 3.0.0", pr_number:                                                    12393, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    19, deletions_count:   4},
		{sha: "8e259dd9e7a4b8511212d4e98bb79656f8fc7fff", date: "2022-04-25 23:51:03 UTC", description: "bump webbrowser from 0.6.0 to 0.7.0", pr_number:                                             12390, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "0239e9988711805e78935cded29ba08cf7626da1", date: "2022-04-25 23:00:33 UTC", description: "bump serde_with from 1.12.1 to 1.13.0", pr_number:                                           12391, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    11, deletions_count:   11},
		{sha: "0200742e68eb4d1f6e475f8f3257e2004ef45d10", date: "2022-04-26 01:01:14 UTC", description: "make source output types depend on codec", pr_number:                                        12229, scopes: ["codecs"], type:                              "chore", breaking_change:       false, author: "Luke Steensen", files_count:      17, insertions_count:   85, deletions_count:   36},
		{sha: "5b3ee8e9e74593022542605679f00d1ad1753d89", date: "2022-04-26 11:21:50 UTC", description: "update type definition of parent scopes when needed", pr_number:                             12383, scopes: ["vrl"], type:                                 "fix", breaking_change:         false, author: "Jean Mertz", files_count:         3, insertions_count:    35, deletions_count:   9},
		{sha: "59cfccc8af46130bfb3ad6350c9ccf44ac82cce8", date: "2022-04-26 04:03:09 UTC", description: "bump prost to 0.10.1 and tonic to 0.7.1", pr_number:                                         12357, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      8, insertions_count:    137, deletions_count:  166},
		{sha: "f65b1f2aca93c2798ce7565c68b4a73d0449eded", date: "2022-04-26 05:35:11 UTC", description: "Rename TLS configuration structs for clarity", pr_number:                                    12404, scopes: ["config"], type:                              "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      65, insertions_count:   237, deletions_count:  216},
		{sha: "4e01dd9724b3bbffe5d2bb4d719274eedef4b14c", date: "2022-04-26 08:26:51 UTC", description: "Include pod_namespace tag on emitted metrics", pr_number:                                    12403, scopes: ["kubernetes_logs source"], type:              "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:    2, insertions_count:    20, deletions_count:   8},
		{sha: "324bd2c9a675d00cef5783e1819fddcb94ce0f27", date: "2022-04-26 07:41:23 UTC", description: "Fix setting of host/pid tags", pr_number:                                                    12320, scopes: ["internal_metrics source"], type:             "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:    42, deletions_count:   17},
		{sha: "e4124d6461ca7e1ce0226a71d16a4083288e1de5", date: "2022-04-26 23:06:24 UTC", description: "report internal logs to Datadog for enterprise", pr_number:                                  12307, scopes: ["observability"], type:                       "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:     5, insertions_count:    81, deletions_count:   6},
		{sha: "51cc14c29a5549b4649a3f55020729943a62e1ca", date: "2022-04-27 07:52:36 UTC", description: "add `into_iter` to Value type", pr_number:                                                   12372, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         2, insertions_count:    468, deletions_count:  0},
		{sha: "b94149c0be20cdf2aee57a936872d82bafbc228c", date: "2022-04-27 07:53:09 UTC", description: "add compiler support for function closures", pr_number:                                      12338, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         14, insertions_count:   869, deletions_count:  50},
		{sha: "f6b31a0eda26cc19e7f78301bb876d209484d4c3", date: "2022-04-27 01:36:27 UTC", description: "Detect rust-toolchain.toml changes", pr_number:                                              12416, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    1, deletions_count:    1},
		{sha: "fe0810031869afa00f1e5475ecc309414338cd5a", date: "2022-04-27 10:43:51 UTC", description: "Implement `sinks::util::encoding::Encoder` for `crate::codecs::Encoder`", pr_number:         12178, scopes: ["codecs"], type:                              "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      2, insertions_count:    273, deletions_count:  14},
		{sha: "26fd6f9b638459a301dad079d19b8e626c3e19ee", date: "2022-04-27 11:03:07 UTC", description: "add `for_each` enumeration function", pr_number:                                             12382, scopes: ["vrl"], type:                                 "feat", breaking_change:        false, author: "Jean Mertz", files_count:         12, insertions_count:   177, deletions_count:  26},
		{sha: "88367ad280d6058367f02696f47db8fdb5374ac1", date: "2022-04-27 05:23:12 UTC", description: "bump async from 2.6.3 to 2.6.4 in /website", pr_number:                                      12419, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    3, deletions_count:    3},
		{sha: "40ceae30c2f79fd147a927703fda5701a04c2518", date: "2022-04-27 08:52:53 UTC", description: "Drop global `allow(clippy::too_many_arguments)`", pr_number:                                 12420, scopes: [], type:                                      "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      19, insertions_count:   300, deletions_count:  427},
		{sha: "dcc7bd215ad087927017e3200316eff2c45fb218", date: "2022-04-27 17:54:13 UTC", description: "allow compiler to emit non-fatal errors", pr_number:                                         12412, scopes: ["vrl"], type:                                 "feat", breaking_change:        false, author: "Jean Mertz", files_count:         35, insertions_count:   196, deletions_count:  129},
		{sha: "90bf0c2bda65f2bda6a17265e2414deca32039d2", date: "2022-04-28 01:58:41 UTC", description: "Integrate `encoding::Encoder` with `aws_kinesis_streams` sink", pr_number:                   12177, scopes: ["aws_kinesis_streams sink", "codecs"], type:  "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      3, insertions_count:    27, deletions_count:   19},
		{sha: "8e860a9fe9f03cdc94af977e600fb529d00c6743", date: "2022-04-28 01:53:00 UTC", description: "discard udp frames greater than max_length.", pr_number:                                     12023, scopes: ["socket source"], type:                       "fix", breaking_change:         false, author: "Stephen Wakely", files_count:     3, insertions_count:    141, deletions_count:  15},
		{sha: "1d7467b473e1b5daf53635894b0701c2396c1a7c", date: "2022-04-28 01:50:50 UTC", description: "add highlight for new native codecs", pr_number:                                             12048, scopes: ["codecs"], type:                              "docs", breaking_change:        false, author: "Luke Steensen", files_count:      2036, insertions_count: 1202, deletions_count: 1067},
		{sha: "0b3ad363403dd0e246dc5bdccabb4ec45521e7f6", date: "2022-04-28 08:53:47 UTC", description: "Integrate `encoding::Encoder` with `azure_blob` sink", pr_number:                            12179, scopes: ["azure_blob sink", "codecs"], type:           "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      4, insertions_count:    78, deletions_count:   22},
		{sha: "3e377cc3f1f8c757f463fb14c78b4deb556bd594", date: "2022-04-28 08:54:31 UTC", description: "Integrate `encoding::Encoder` with `aws_s3` sink", pr_number:                                12136, scopes: ["aws_s3 sink", "codecs"], type:               "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      3, insertions_count:    38, deletions_count:   14},
		{sha: "f7484ade93e141c439d890c89a116d2e5c7cf65a", date: "2022-04-28 10:00:11 UTC", description: "Integrate `encoding::Encoder` with `aws_s3` sink", pr_number:                                12136, scopes: ["aws_s3 sink", "codecs"], type:               "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      0, insertions_count:    0, deletions_count:    0},
		{sha: "3eadc96742a33754a5859203b58249f6a806972a", date: "2022-04-28 03:48:45 UTC", description: "Rename Vector development image", pr_number:                                                 12439, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    3, deletions_count:    3},
		{sha: "d6ab40282d3f39406ce1cfad6b723b70679bf82f", date: "2022-04-28 07:13:26 UTC", description: "Apply a number of hadolint fixes to Dockerfiles", pr_number:                                 12417, scopes: [], type:                                      "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    12, insertions_count:   32, deletions_count:   30},
		{sha: "3d5cbf9684f2e8d9b8500e6a309c0d3c28e3124b", date: "2022-04-28 04:58:55 UTC", description: "Run checks directly on VM", pr_number:                                                       12437, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    2, deletions_count:    1},
		{sha: "e8a6755115d0e3e8833010a024803059a391b09c", date: "2022-04-28 10:19:55 UTC", description: "add support for sending aggregated summaries", pr_number:                                    12436, scopes: ["datadog_metrics sink"], type:                "enhancement", breaking_change: false, author: "Toby Lawrence", files_count:      22, insertions_count:   2679, deletions_count: 2182},
		{sha: "ef1223c6f260152ad07ca4ed42fa94843c331458", date: "2022-04-28 10:32:20 UTC", description: "fix integration test dockerfile by explicitly installing build-essential", pr_number:        12446, scopes: [], type:                                      "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:    10, deletions_count:   2},
		{sha: "f1f1be7c3e9862b38a2a411e3fe96bef61dfb83e", date: "2022-04-28 09:23:37 UTC", description: "Update Makefile to refer to new dev image", pr_number:                                       12441, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    1, deletions_count:    1},
		{sha: "879caf92f2786926c103a20d8a18d0cc1e906c6b", date: "2022-04-29 03:20:49 UTC", description: "correctly remove closure variables after closure ends", pr_number:                           12384, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         6, insertions_count:    77, deletions_count:   7},
		{sha: "26d2b58ec385edcce751a39075ded4c74d2b1153", date: "2022-04-28 23:49:36 UTC", description: "ensure all early buffered events are captured", pr_number:                                   12411, scopes: ["internal_logs source"], type:                "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      21, insertions_count:   213, deletions_count:  70},
		{sha: "cb60287573f601209743fa5fc24311f95747deac", date: "2022-04-28 21:55:37 UTC", description: "Shared shutdown signals don't need `Shared`", pr_number:                                     12449, scopes: ["sources"], type:                             "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      7, insertions_count:    20, deletions_count:   27},
		{sha: "427db95ff685301c660162001f51900ff0677f12", date: "2022-04-28 21:58:24 UTC", description: "Cache `size_of` in `LogEvent`", pr_number:                                                   12423, scopes: ["performance"], type:                         "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:      2, insertions_count:    125, deletions_count:  43},
		{sha: "fe756840ceabe991924347bb5d06a6311708b7ac", date: "2022-04-28 21:59:59 UTC", description: "Allow clippy lint for stderr output in VRL", pr_number:                                      12443, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    1, deletions_count:    0},
		{sha: "78af08a438465340621a5c7a268672824626c140", date: "2022-04-28 22:34:43 UTC", description: "bump tokio from 1.17.0 to 1.18.0", pr_number:                                                12454, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:    22, deletions_count:   15},
		{sha: "161511dbe0bfdc34ee0031163de40bf14ae68cc0", date: "2022-04-29 08:43:36 UTC", description: "add `map_keys` enumeration function", pr_number:                                             12385, scopes: ["vrl"], type:                                 "feat", breaking_change:        false, author: "Jean Mertz", files_count:         8, insertions_count:    143, deletions_count:  7},
		{sha: "465616cfa3e8872d5cc93b72fffae7646378b03f", date: "2022-04-29 01:16:26 UTC", description: "Use autoscaled soak runners", pr_number:                                                     12348, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 1, insertions_count:    89, deletions_count:   79},
		{sha: "f463aebd18cdd4addd651422f811dc71c8441a16", date: "2022-04-29 06:00:34 UTC", description: "bump docker/metadata-action from 3.7.0 to 3.8.0", pr_number:                                 12463, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:    4, deletions_count:    4},
		{sha: "87ce6905c1b9c3cc99cbfa0be627c8442b23fbfe", date: "2022-04-29 12:16:11 UTC", description: "add `map_values` enumeration function", pr_number:                                           12388, scopes: ["vrl"], type:                                 "feat", breaking_change:        false, author: "Jean Mertz", files_count:         10, insertions_count:   171, deletions_count:  13},
		{sha: "8a6f03b94545910b7841fd998d9e350d13f56778", date: "2022-04-29 06:50:19 UTC", description: "allow decoders to specify default stream framer", pr_number:                                 12407, scopes: ["codecs"], type:                              "enhancement", breaking_change: false, author: "Luke Steensen", files_count:      16, insertions_count:   96, deletions_count:   72},
		{sha: "45166b0d1d24d92e95123625518b2337053ddfbd", date: "2022-04-29 05:19:13 UTC", description: "bump git from 1.7.0 to 1.11.0 in /scripts", pr_number:                                       12471, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "d2a3ec97da5481fdcef489d2886a117c55a69c5f", date: "2022-04-29 05:36:24 UTC", description: "Require sinks to be healthy in soaks", pr_number:                                            12448, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 5, insertions_count:    1, deletions_count:    1},
		{sha: "251f548af9a0898541faed77059b3bc10d124a1d", date: "2022-04-29 06:54:55 UTC", description: "Run check-component-features in parallel", pr_number:                                        12465, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      7, insertions_count:    79, deletions_count:   7},
		{sha: "8f1ba3878ae935ed103498e154f220ea3264220a", date: "2022-04-29 06:14:59 UTC", description: "bump docker/setup-buildx-action from 1.6.0 to 1.7.0", pr_number:                             12464, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    6, deletions_count:    6},
		{sha: "77463407303c2577ebeb8d057920ed18a219dd37", date: "2022-04-29 06:15:14 UTC", description: "bump webbrowser from 0.7.0 to 0.7.1", pr_number:                                             12453, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "9502219c07cb68776afe0a7d1632dda237f3e19a", date: "2022-04-29 07:55:38 UTC", description: "[revert] run check-component-features in parallel", pr_number:                               12479, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      7, insertions_count:    7, deletions_count:    79},
		{sha: "832f0550af58d197c1da4534fe8629e1028ab32e", date: "2022-04-29 23:34:25 UTC", description: "Implement `LengthDelimitedEncoder` framer", pr_number:                                       12457, scopes: ["codecs"], type:                              "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      5, insertions_count:    128, deletions_count:  11},
		{sha: "5986ba64ca7564224547e2bd61bb5110ace81436", date: "2022-04-30 07:48:47 UTC", description: "add additional example to VRL iteration RFC", pr_number:                                     12455, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jean Mertz", files_count:         3, insertions_count:    173, deletions_count:  0},
		{sha: "61fbad56e9825dbc1cbf6ee783a22427e833a639", date: "2022-04-30 03:29:20 UTC", description: "Re-enable all soak tests but known-broken", pr_number:                                       12476, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 62, insertions_count:   10, deletions_count:   0},
		{sha: "1ee0a2ca65d44cd0acbbfb8d0cbe9b83c56d22c3", date: "2022-04-30 07:19:47 UTC", description: "run concurrently", pr_number:                                                                12494, scopes: ["route transform"], type:                     "perf", breaking_change:        false, author: "Luke Steensen", files_count:      1, insertions_count:    4, deletions_count:    0},
		{sha: "1d15bc27833126f99ea7fc48b5295ee48ed799a1", date: "2022-04-30 06:57:54 UTC", description: "bump http from 0.2.6 to 0.2.7", pr_number:                                                   12484, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:    4, deletions_count:    4},
		{sha: "cc74a8cc1b560d25a0379cca0ed3988a73e9a3ad", date: "2022-04-30 06:58:04 UTC", description: "bump tonic-build from 0.7.0 to 0.7.1", pr_number:                                            12485, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "d02d9f8cb34be0452c45849f81e27cca05385896", date: "2022-04-30 06:58:17 UTC", description: "bump nats from 0.19.0 to 0.19.1", pr_number:                                                 12486, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "ddbf4bbe63db017d291aa6348587b567ad553f17", date: "2022-05-03 00:15:30 UTC", description: "Removed unused skaffold and docker files", pr_number:                                        12502, scopes: [], type:                                      "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    6, insertions_count:    6, deletions_count:    218},
		{sha: "b421e024af6dc6bb91a85a5afd5483347707b1e6", date: "2022-05-02 21:18:06 UTC", description: "Update component spec for BytesSent re compression", pr_number:                              12501, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    1, deletions_count:    1},
		{sha: "546cdee23f36ba2a242c7133f2afa35ed7eff586", date: "2022-05-03 00:28:46 UTC", description: "basic implementation of `Configurable` trait and derive macro", pr_number:                   12353, scopes: ["config"], type:                              "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      20, insertions_count:   3349, deletions_count: 129},
		{sha: "db38b570a2271719e35533a601b0960abfa44c32", date: "2022-05-02 23:45:39 UTC", description: "Upgrade AWS SDKs to 0.11.0", pr_number:                                                      12513, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:    64, deletions_count:   64},
		{sha: "a89a8cee5679613dea5582d2f44fcda2a82c3425", date: "2022-05-03 04:02:48 UTC", description: "New GCP Pub/Sub source", pr_number:                                                          12057, scopes: ["sources"], type:                             "feat", breaking_change:        false, author: "Bruce Guenter", files_count:      33, insertions_count:   2634, deletions_count: 391},
		{sha: "85d2ed1a10b77e83e4ed65c589043889b8a8a17c", date: "2022-05-03 03:52:15 UTC", description: "bump thiserror from 1.0.30 to 1.0.31", pr_number:                                            12507, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    4, deletions_count:    4},
		{sha: "71a9159190fd6b0785261ccc5854adf62d898bc7", date: "2022-05-03 03:52:47 UTC", description: "bump ordered-float from 2.10.0 to 3.0.0", pr_number:                                         12516, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    11, insertions_count:   31, deletions_count:   22},
		{sha: "9ba7173b95a26b8c81bf09da21857d229a1c7f5f", date: "2022-05-03 03:53:19 UTC", description: "bump console-subscriber from 0.1.4 to 0.1.5", pr_number:                                     12519, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "5985a3b340679cff1cfbee9883ad52edee9a97b8", date: "2022-05-03 11:21:45 UTC", description: "bump darling from 0.14.0 to 0.14.1", pr_number:                                              12520, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    11, deletions_count:   11},
		{sha: "e2dc9453088f4f51ede0191e9ac374977729cf22", date: "2022-05-03 13:34:20 UTC", description: "bump semver from 1.0.7 to 1.0.9", pr_number:                                                 12517, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    5, deletions_count:    5},
		{sha: "b514f7944deefef1fb38b172a1b86a394abe9b37", date: "2022-05-03 15:21:28 UTC", description: "bump memchr from 2.4.1 to 2.5.0", pr_number:                                                 12533, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "2f259ae237d5e683f7c7a9b4a4c95c254d69292e", date: "2022-05-03 16:13:43 UTC", description: "bump serde_json from 1.0.79 to 1.0.80", pr_number:                                           12531, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:    10, deletions_count:   10},
		{sha: "46ab99e91964a3c1765158f537fa6e0c1589c2d7", date: "2022-05-03 16:31:14 UTC", description: "bump syn from 1.0.91 to 1.0.92", pr_number:                                                  12535, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    67, deletions_count:   67},
		{sha: "b174bb90b99034e8f85efa8cdc25f36ce57c38f0", date: "2022-05-03 18:44:05 UTC", description: "bump serde from 1.0.136 to 1.0.137", pr_number:                                              12509, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:    11, deletions_count:   11},
		{sha: "e222294361b92aa2495b4423ebb9115ce6b7e613", date: "2022-05-03 22:49:54 UTC", description: "Document vector config subcommand", pr_number:                                               12500, scopes: ["cli"], type:                                 "docs", breaking_change:        false, author: "Will", files_count:               1, insertions_count:    34, deletions_count:   12},
		{sha: "02aa94ae86cc6410e306ecac081a98e95c5c2c38", date: "2022-05-03 23:35:42 UTC", description: "replace list of features in docs with link to Cargo.toml", pr_number:                        12523, scopes: [], type:                                      "docs", breaking_change:        false, author: "Nathan Fox", files_count:         1, insertions_count:    3, deletions_count:    102},
		{sha: "6d657acbf95340f7182f9136c8a2918738ed89da", date: "2022-05-04 05:44:25 UTC", description: "do not print empty warning log in VRL-based transforms", pr_number:                          12544, scopes: ["vrl"], type:                                 "fix", breaking_change:         false, author: "Jean Mertz", files_count:         1, insertions_count:    4, deletions_count:    0},
		{sha: "20de03b747de72946600873d9b7b1ad265a633de", date: "2022-05-04 08:35:10 UTC", description: "Add 'websocket' sink", pr_number:                                                            9632, scopes: ["new sink"], type:                             "feat", breaking_change:        false, author: "Evgeny Nosov", files_count:       12, insertions_count:   878, deletions_count:  0},
		{sha: "e1dfbcd42a73c4e7e3f34f0db42f5288207c4938", date: "2022-05-04 01:26:26 UTC", description: "Create `Output::with_port`", pr_number:                                                      12525, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 7, insertions_count:    34, deletions_count:   25},
		{sha: "1529372da0fe368535582e570aa0d0f32933a003", date: "2022-05-04 06:14:18 UTC", description: "Small fixes on for deb and rpm targets", pr_number:                                          12563, scopes: [], type:                                      "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    2, insertions_count:    9, deletions_count:    8},
		{sha: "4f46f2369b7600c0ea278e0e1b810fab3285cc48", date: "2022-05-04 12:15:54 UTC", description: "warning on if-statement predicate that always/never succeeds", pr_number:                    12396, scopes: ["vrl"], type:                                 "feat", breaking_change:        false, author: "Jean Mertz", files_count:         10, insertions_count:   109, deletions_count:  26},
		{sha: "9a2bfcff12271abebf1fe0078119a61f3f84456d", date: "2022-05-04 03:19:34 UTC", description: "bump clap from 3.1.12 to 3.1.15", pr_number:                                                 12528, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    12, deletions_count:   12},
		{sha: "4206575c9fe122f329cac19d2fd9c689470b52a0", date: "2022-05-04 03:19:42 UTC", description: "bump serde_bytes from 0.11.5 to 0.11.6", pr_number:                                          12532, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "1c68613caddfe6156072c6d00accfbacf6881242", date: "2022-05-04 03:19:59 UTC", description: "bump log from 0.4.16 to 0.4.17", pr_number:                                                  12540, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "50ea9e78b891a5ff13e4a4ffa8b7229313fe3389", date: "2022-05-04 03:20:15 UTC", description: "bump snafu from 0.7.0 to 0.7.1", pr_number:                                                  12541, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    9, deletions_count:    9},
		{sha: "162afc650cdc2618c1e4f4a3fbe6b4b574da4384", date: "2022-05-04 03:20:29 UTC", description: "bump tokio-postgres from 0.7.5 to 0.7.6", pr_number:                                         12542, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    16, deletions_count:   47},
		{sha: "fb916c16bd06204a0929d772a88eac9856bab60c", date: "2022-05-04 03:23:09 UTC", description: "bump libc from 0.2.124 to 0.2.125", pr_number:                                               12530, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "ec1330daa2cc1873e5d503a4349c2036e3730ae0", date: "2022-05-04 12:57:49 UTC", description: "bump rkyv from 0.7.37 to 0.7.38", pr_number:                                                 12565, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    9, deletions_count:    9},
		{sha: "90aa9c572af1d66176e14d70b4a8fb6bb70471eb", date: "2022-05-04 06:16:24 UTC", description: "bump serde_yaml from 0.8.23 to 0.8.24", pr_number:                                           12568, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "d65f6cc19f5545d8faf1ba497f08dd93d20260f5", date: "2022-05-04 06:16:44 UTC", description: "bump openssl from 0.10.38 to 0.10.39", pr_number:                                            12539, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    17, deletions_count:   5},
		{sha: "815a1f33688438d60e60c4f0fa8bfb09769934b3", date: "2022-05-04 11:40:46 UTC", description: "Adding extra_namespace_label_selector", pr_number:                                           12438, scopes: ["kubernetes_logs source"], type:              "feat", breaking_change:        false, author: "Anastas Dancha", files_count:     4, insertions_count:    63, deletions_count:   8},
		{sha: "6dd422a7c5089f20bb9759a388958d3add6dff7d", date: "2022-05-04 17:23:22 UTC", description: "bump serde_json from 1.0.80 to 1.0.81", pr_number:                                           12567, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:    11, deletions_count:   11},
		{sha: "7e09e7b5d0403a64cf4034729d2043f8b9610e5f", date: "2022-05-05 01:49:35 UTC", description: "add iteration support to VM runtime", pr_number:                                             12549, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         15, insertions_count:   377, deletions_count:  271},
		{sha: "3fa43a9487ddbf6946e76e81a0a8855c3604d46e", date: "2022-05-04 22:40:11 UTC", description: "OP-282 Ensure config reporting resilience", pr_number:                                       12442, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Will", files_count:               3, insertions_count:    378, deletions_count:  67},
		{sha: "9658477bf36d7972a5da1e861730bf7701997911", date: "2022-05-05 07:46:55 UTC", description: "preserve event ordering", pr_number:                                                         12330, scopes: ["kubernetes_logs"], type:                     "enhancement", breaking_change: false, author: "Dmitry Ulyanov", files_count:     1, insertions_count:    3, deletions_count:    3},
		{sha: "fdb643bfb6c9488f0736c969fadeb20f4d218636", date: "2022-05-05 14:32:43 UTC", description: "add a max_depth parameter to the parse_json function", pr_number:                            12545, scopes: ["vrl"], type:                                 "feat", breaking_change:        false, author: "Maksim Nabokikh", files_count:    3, insertions_count:    197, deletions_count:  7},
		{sha: "03ca6355551488b11ba010f8d6daeffd1be6b026", date: "2022-05-05 06:36:09 UTC", description: "Bump k8s manifests to Helm chart 0.10.2 rendering", pr_number:                               12598, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      17, insertions_count:   21, deletions_count:   21},
		{sha: "45f70b1c8b3522afea60d31c48d4f1a2a7ec7749", date: "2022-05-05 07:46:56 UTC", description: "Make tonic load TLS system trust roots", pr_number:                                          12594, scopes: ["gcp_pubsub source"], type:                   "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      3, insertions_count:    4, deletions_count:    2},
		{sha: "c3e4932316ec2bf63e110e3982dbfe347522cf49", date: "2022-05-05 07:01:06 UTC", description: "bump num-traits from 0.2.14 to 0.2.15", pr_number:                                           12569, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:    4, deletions_count:    4},
		{sha: "20739087f4dd2ebf929b593ab2d527f634f10a6e", date: "2022-05-05 07:08:32 UTC", description: "bump inherent from 1.0.0 to 1.0.1", pr_number:                                               12575, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "18d5e27a959dc011e213748b1fa9a9a2e36bad1a", date: "2022-05-05 16:09:10 UTC", description: "bump tokio from 1.18.0 to 1.18.1", pr_number:                                                12570, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:    10, deletions_count:   10},
		{sha: "2fa5495be993d6756f72aedaf4d58867c4865a0b", date: "2022-05-05 21:37:55 UTC", description: "Allow stdout/stderr in vector config tests", pr_number:                                      12600, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    2, deletions_count:    0},
		{sha: "370a5a6c6deb57422d4612dbb9ab7c63bf766960", date: "2022-05-06 10:28:37 UTC", description: "update `Target::target_get` to return reference to `Value`", pr_number:                      12546, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         14, insertions_count:   409, deletions_count:  158},
		{sha: "ab1b5806ff4f710d55a7b0d9b768a19fcda4c50c", date: "2022-05-06 04:41:28 UTC", description: "bump docker/setup-qemu-action from 1.2.0 to 2.0.0", pr_number:                               12614, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    4, deletions_count:    4},
		{sha: "2db7e02e07d55d86714b6a3348d011fdadefb346", date: "2022-05-06 04:53:29 UTC", description: "bump docker/metadata-action from 3.8.0 to 4.0.1", pr_number:                                 12617, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    6, deletions_count:    6},
		{sha: "2e529df6d09b07c2f85762515f0d8fc7fe814ea1", date: "2022-05-06 04:53:51 UTC", description: "bump docker/login-action from 1.14.1 to 2.0.0", pr_number:                                   12616, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    4, deletions_count:    4},
		{sha: "e16519ef8fad7923501d70eb6541279ebd599213", date: "2022-05-06 10:11:59 UTC", description: "bump docker/setup-buildx-action from 1.7.0 to 2.0.0", pr_number:                             12615, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:    8, deletions_count:    8},
		{sha: "97e2842a15195e35ab63db9fbcf1953e4c002282", date: "2022-05-06 04:20:55 UTC", description: "bump bitmask-enum from 1.1.3 to 2.0.0", pr_number:                                           12603, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "40b03728550652202f4e1b55556df3c216bb0d90", date: "2022-05-06 04:21:18 UTC", description: "bump openssl from 0.10.39 to 0.10.40", pr_number:                                            12604, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "b887ce955aae0dcae6a90f005bdb26a1f4b7fc0f", date: "2022-05-06 04:21:29 UTC", description: "bump twox-hash from 1.6.2 to 1.6.3", pr_number:                                              12605, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "81cc105fbed702adfbfa6e52292dd1f0192fa544", date: "2022-05-06 07:50:57 UTC", description: "add batch size histogram metric", pr_number:                                                 11290, scopes: ["observability"], type:                       "enhancement", breaking_change: false, author: "Luke Steensen", files_count:      2, insertions_count:    8, deletions_count:    2},
		{sha: "fb05aa0c73eb3fe8b0a153bbedc0f7e6b4856a24", date: "2022-05-06 13:28:25 UTC", description: "Refactor handling of enterprise configuration for components", pr_number:                    12595, scopes: ["enterprise"], type:                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      12, insertions_count:   118, deletions_count:  173},
		{sha: "384b0bbbd3afe8d82df581064f612d419dbf0240", date: "2022-05-07 02:22:30 UTC", description: "correct extracting fields with the same name for 'parse_groks'", pr_number:                  12613, scopes: ["vrl"], type:                                 "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:      1, insertions_count:    8, deletions_count:    3},
		{sha: "e7e4f65e59e8542b0df60dd4d53cc8e930461ad6", date: "2022-05-07 02:33:46 UTC", description: "add `Target::target_get_mut` method", pr_number:                                             12576, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         4, insertions_count:    67, deletions_count:   5},
		{sha: "29baf349766c210509fd874a2ea24f19daff2775", date: "2022-05-06 23:44:10 UTC", description: "try using mold in CI to speed things up", pr_number:                                         12450, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      33, insertions_count:   924, deletions_count:  1099},
		{sha: "9aefaec827da1e29de614ca34ebc88a10e1cd59f", date: "2022-05-07 08:33:13 UTC", description: "Unwrap logged messages from quote marks", pr_number:                                         12609, scopes: ["vrl"], type:                                 "fix", breaking_change:         false, author: "Maksim Nabokikh", files_count:    3, insertions_count:    92, deletions_count:   18},
		{sha: "421b8dac6a815f40e25c2fbf108f884beffd794b", date: "2022-05-06 23:17:09 UTC", description: "bump clap from 3.1.15 to 3.1.16", pr_number:                                                 12628, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    10, deletions_count:   10},
		{sha: "c2fec84435d01cf37a8a9cf1dd9d391d63d61121", date: "2022-05-06 23:17:25 UTC", description: "bump prost from 0.10.1 to 0.10.3", pr_number:                                                12630, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    14, deletions_count:   14},
		{sha: "42254763bf735004b8eeb062b536c0f07f6d1e15", date: "2022-05-06 23:17:38 UTC", description: "bump lalrpop-util from 0.19.7 to 0.19.8", pr_number:                                         12631, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "6ad90643d58adc3b13f884c0ac77819c03b23f3b", date: "2022-05-06 23:17:47 UTC", description: "bump tonic-build from 0.7.1 to 0.7.2", pr_number:                                            12632, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "ef064093fafc96caa38a909d2bc7b68c45c14c5c", date: "2022-05-07 12:21:30 UTC", description: "Couple sink input types to `Encoder` input types", pr_number:                                12561, scopes: ["codecs"], type:                              "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      14, insertions_count:   82, deletions_count:   27},
		{sha: "baea4ca465d1801c49b4fa26d9d8f84b42cd5690", date: "2022-05-07 06:21:51 UTC", description: "Add option to disable enterprise logs reporting", pr_number:                                 12640, scopes: ["enterprise"], type:                          "chore", breaking_change:       false, author: "Will", files_count:               1, insertions_count:    84, deletions_count:   52},
		{sha: "1daba0d61088bd8f4df7426285152fcd445f5165", date: "2022-05-07 12:25:00 UTC", description: "Integrate `encoding::Encoder` with `gcp` sink", pr_number:                                   12488, scopes: ["gcp_cloud_storage sink", "codecs"], type:    "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      2, insertions_count:    48, deletions_count:   9},
		{sha: "c33930bf1c365c4d7727867fe991ee0f0c6a493c", date: "2022-05-07 12:26:04 UTC", description: "Integrate `encoding::Encoder` with `papertrail` sink", pr_number:                            12589, scopes: ["papertrail sink", "codecs"], type:           "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      2, insertions_count:    62, deletions_count:   25},
		{sha: "7e7b754ea70c3c68af10429c8e93dc51c0702754", date: "2022-05-07 12:26:21 UTC", description: "Integrate `encoding::Encoder` with `redis` sink", pr_number:                                 12596, scopes: ["redis sink", "codecs"], type:                "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      1, insertions_count:    58, deletions_count:   41},
		{sha: "01913cd2d8e2a5d430424d2f66a345338084290b", date: "2022-05-07 12:56:18 UTC", description: "Integrate `encoding::Encoder` with `nats` sink", pr_number:                                  12586, scopes: ["nats sink", "codecs"], type:                 "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      1, insertions_count:    56, deletions_count:   63},
		{sha: "d4d7ad7e44b915659299eb9f4d88d6499723e6d1", date: "2022-05-07 12:57:25 UTC", description: "Integrate `encoding::Encoder` with `influxdb` sink", pr_number:                              12583, scopes: ["influxdb sink", "codecs"], type:             "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      2, insertions_count:    90, deletions_count:   38},
		{sha: "0832f06d8f741b9587d195e73317ce113d184a52", date: "2022-05-07 12:59:03 UTC", description: "Integrate `encoding::Encoder` with `aws_sqs` sink", pr_number:                               12550, scopes: ["aws_sqs sink", "codecs"], type:              "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      3, insertions_count:    37, deletions_count:   31},
		{sha: "c31be39bcd61f7f32967a8f5e91e809e9afad506", date: "2022-05-07 12:59:44 UTC", description: "Integrate `encoding::Encoder` with `file` sink", pr_number:                                  12548, scopes: ["file sink", "codecs"], type:                 "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      2, insertions_count:    68, deletions_count:   36},
		{sha: "62ccca36ddaeb439f59b94ca4ab32f2d710a6330", date: "2022-05-07 06:47:01 UTC", description: "Check in uncommitted changes to Cargo.lock", pr_number:                                      12646, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "289deba316f2d8ff144fd7cf18afa7787b7b1738", date: "2022-05-07 07:01:44 UTC", description: "Handle connection errors by retrying", pr_number:                                            12622, scopes: ["gcp_pubsub source"], type:                   "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      1, insertions_count:    21, deletions_count:   18},
		{sha: "c06124a38de7b86b618bad5b5c7ad99d02b2e4b0", date: "2022-05-07 09:05:29 UTC", description: "Add dynamic tag configuration to enterprise section ", pr_number:                            12623, scopes: ["enterprise"], type:                          "enhancement", breaking_change: false, author: "Will", files_count:               2, insertions_count:    123, deletions_count:  3},
		{sha: "59c5c4425dfcc792dc1d587129d8d03251a9f296", date: "2022-05-07 06:40:45 UTC", description: "Fix silent merge conflict with encoding::adapter::Transformer", pr_number:                   12649, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    3, deletions_count:    6},
		{sha: "a4ff8b36e166363a2ffc0662de59d93e1f780c16", date: "2022-05-07 09:58:04 UTC", description: "Retry on errors, after a delay", pr_number:                                                  12641, scopes: ["gcp_pubsub source"], type:                   "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      3, insertions_count:    96, deletions_count:   14},
		{sha: "563529af36eb3208244c720d70233718aad47a85", date: "2022-05-07 21:52:01 UTC", description: "add additional VRL iteration example", pr_number:                                            12644, scopes: ["rfc"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         2, insertions_count:    31, deletions_count:   0},
		{sha: "4ce76a1ae53fd23dcd20f161228b5c5350e1b3d2", date: "2022-05-09 23:14:53 UTC", description: "add bytes sent metrics to Datadog sinks", pr_number:                                         12492, scopes: ["datadog service"], type:                     "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:     10, insertions_count:   178, deletions_count:  29},
		{sha: "4a98d0dbb1918192112ebf07bdec5fa804c9efe6", date: "2022-05-10 03:19:08 UTC", description: "allow compiling VRL without support for specific expressions", pr_number:                    12620, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         26, insertions_count:   444, deletions_count:  83},
		{sha: "7a8b148466d7f43aaf30db30132e711bf14af49e", date: "2022-05-10 02:16:10 UTC", description: "try a ReadyArrays", pr_number:                                                               12626, scopes: [], type:                                      "perf", breaking_change:        false, author: "Luke Steensen", files_count:      3, insertions_count:    120, deletions_count:  7},
		{sha: "a03b261050714a6b2db24b871a652310590fa577", date: "2022-05-10 11:17:58 UTC", description: "ensure all sources are checked for component spec compliance", pr_number:                    12572, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      59, insertions_count:   4005, deletions_count: 3581},
		{sha: "10dae1c201c22485e74b655d0546746a508ec58c", date: "2022-05-10 11:28:10 UTC", description: "use our custom runners for cross-linux + checks, and native action cancellation", pr_number: 12669, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      2, insertions_count:    7, deletions_count:    15},
		{sha: "3e0ca6bdf0f4f4bb3bc66ddfc14c6b961f83dbd2", date: "2022-05-10 10:55:08 UTC", description: "Check for and remove unused internal events", pr_number:                                     12671, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      13, insertions_count:   15, deletions_count:   322},
		{sha: "1816187e1e55ceaa9536996d47ef04ad2b86cce5", date: "2022-05-10 22:45:17 UTC", description: "handle spaces in filenames", pr_number:                                                      12664, scopes: ["aws_s3 source"], type:                       "fix", breaking_change:         false, author: "Stephen Wakely", files_count:     2, insertions_count:    54, deletions_count:   8},
		{sha: "b634d9395197289195a71169fc9f2e941edf508c", date: "2022-05-11 04:10:53 UTC", description: "remove `Value` clone in `Value::target_remove`", pr_number:                                  12659, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         2, insertions_count:    29, deletions_count:   15},
		{sha: "f26dc8c306874e51936c42f710bb53a782ca3b56", date: "2022-05-11 06:13:44 UTC", description: "improve performance of `block` expression", pr_number:                                       12679, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         1, insertions_count:    6, deletions_count:    4},
		{sha: "14b1d5765034f6d08ed5b98f708819895d302b75", date: "2022-05-10 23:12:47 UTC", description: "Improve token refresh handling", pr_number:                                                  12645, scopes: ["gcp service"], type:                         "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      6, insertions_count:    61, deletions_count:   57},
		{sha: "e1cfef00ec08bd46ad1e2a9a087ee5f54b789772", date: "2022-05-11 07:21:54 UTC", description: "improve vrl benchmark setup", pr_number:                                                     12680, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         3, insertions_count:    90, deletions_count:   19},
		{sha: "da21862760252a74cc47ba23aec404cb39ce2cf0", date: "2022-05-10 23:53:30 UTC", description: "bump lalrpop from 0.19.7 to 0.19.8", pr_number:                                              12657, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    6, deletions_count:    6},
		{sha: "c19c65e9da2e8f4c9ff8cd3a482d134730f9d88f", date: "2022-05-11 07:58:31 UTC", description: "improve performance of `op` expression", pr_number:                                          12681, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         1, insertions_count:    30, deletions_count:   20},
		{sha: "a8369fbf3a04aebfc6c19f74bc7e4c9f09990070", date: "2022-05-11 09:18:09 UTC", description: "improve performance of Runtime::resolve", pr_number:                                         12684, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         1, insertions_count:    17, deletions_count:   13},
		{sha: "7202c897f12c2f957ba1e7a0b82560dd8f6ef343", date: "2022-05-11 02:07:08 UTC", description: "bump tokio from 1.18.1 to 1.18.2", pr_number:                                                12656, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:    10, deletions_count:   10},
		{sha: "67b9bf3a3af72381c8dd30cd7e0e4f0547ed25e6", date: "2022-05-11 06:54:18 UTC", description: "bump proc-macro2 from 1.0.37 to 1.0.38", pr_number:                                          12689, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "c9ac80d18d3c080ad3a0f0e9536f4cfe3a23d840", date: "2022-05-11 06:54:48 UTC", description: "bump EmbarkStudios/cargo-deny-action from 1.2.15 to 1.2.17", pr_number:                      12688, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "a65636107dd8bbf288583b8ca2bce6a619070868", date: "2022-05-11 06:55:40 UTC", description: "bump prost-build from 0.10.1 to 0.10.3", pr_number:                                          12627, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    9, deletions_count:    9},
		{sha: "ca86b826471a2d839077a8ef806abac60a1f1d07", date: "2022-05-11 06:56:34 UTC", description: "bump docker/build-push-action from 2.10.0 to 3.0.0", pr_number:                              12639, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:    6, deletions_count:    6},
		{sha: "71704109c09d9911b5c3f6a96c45fe00d363eb61", date: "2022-05-11 12:53:42 UTC", description: "bump tonic from 0.7.1 to 0.7.2", pr_number:                                                  12629, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:    5, deletions_count:    5},
		{sha: "2f6e57afb26a4746f9c9988ad7a346d540d90d0e", date: "2022-05-11 15:31:48 UTC", description: "bump syn from 1.0.92 to 1.0.93", pr_number:                                                  12687, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    2, deletions_count:    2},
		{sha: "cd74d8f0a2f9c3a797d841a310fb8245a89c2a91", date: "2022-05-11 08:35:56 UTC", description: "Use `value::Value` directly in the codebase", pr_number:                                     12673, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 396, insertions_count:  1078, deletions_count: 737},
		{sha: "ef060b9dbc14635fda160bd50968cb09d03fbec3", date: "2022-05-11 10:46:33 UTC", description: "bump tokio-tungstenite from 0.15.0 to 0.17.1", pr_number:                                    12694, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:    38, deletions_count:   7},
		{sha: "89c501bbd7ecba7bdafd856bd9557a836795753e", date: "2022-05-11 10:46:50 UTC", description: "bump indoc from 1.0.4 to 1.0.6", pr_number:                                                  12693, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:    7, deletions_count:    16},
		{sha: "e584ef2c6539ec38a8316f4d49749162b21e73ab", date: "2022-05-11 10:47:13 UTC", description: "bump nats from 0.19.1 to 0.20.0", pr_number:                                                 12692, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:    3, deletions_count:    3},
		{sha: "cfad17173f2a64515de2070f5acb669f3887d9ea", date: "2022-05-11 22:41:14 UTC", description: "Extend sink input type to encoder input type", pr_number:                                    12678, scopes: ["codecs", "sinks"], type:                     "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      6, insertions_count:    6, deletions_count:    6},
		{sha: "eaa29c2b46e2902fd9003ef83cfaaead4a83536b", date: "2022-05-11 22:41:48 UTC", description: "Remove unnecessary `clone`s of `encoding`", pr_number:                                       12677, scopes: [], type:                                      "chore", breaking_change:       false, author: "Pablo Sichert", files_count:      7, insertions_count:    12, deletions_count:   17},
		{sha: "17adcc29294e7a716c384fb7cd4768cc7b88ea43", date: "2022-05-12 01:13:32 UTC", description: "change `Program` from `Vec<dyn Expression>` to `Block` expression", pr_number:               12690, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         4, insertions_count:    33, deletions_count:   55},
		{sha: "358382337a8660ac685feae479662be53c3f3d13", date: "2022-05-12 05:37:41 UTC", description: "improve performance of `Target` impl for `Event`", pr_number:                                12691, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         3, insertions_count:    50, deletions_count:   29},
		{sha: "f3fe30e920581f968b840d0a5f31c92b1616cbc8", date: "2022-05-11 23:04:09 UTC", description: "bump clap from 3.1.16 to 3.1.18", pr_number:                                                 12696, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:    12, deletions_count:   12},
		{sha: "1d371c39029f4e990d6faadc8f129fd33ddcd514", date: "2022-05-12 02:05:23 UTC", description: "Merge duplicate events", pr_number:                                                          12685, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      33, insertions_count:   166, deletions_count:  307},
		{sha: "31214586bfb981d2c266e10c741dddf381c73165", date: "2022-05-12 04:33:05 UTC", description: "ensure transforms are checked for component spec compliance", pr_number:                     12668, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      4, insertions_count:    309, deletions_count:  14},
		{sha: "980446d7efcbad039ac129d9740623382bed4728", date: "2022-05-12 15:12:04 UTC", description: "improve performance of VrlTarget::into_events", pr_number:                                   12698, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         4, insertions_count:    130, deletions_count:  50},
		{sha: "6dcaa32a44bf8f131787c916815468a8a3abee7f", date: "2022-05-13 06:02:37 UTC", description: "improve performance of `del` function", pr_number:                                           12699, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         1, insertions_count:    13, deletions_count:   10},
		{sha: "902655fda3e871daeb8ea3294c7b9d624eaf5c5d", date: "2022-05-13 06:02:52 UTC", description: "improve performance of `assignment` expression", pr_number:                                  12709, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         8, insertions_count:    59, deletions_count:   49},
		{sha: "ae94c8c2dcc54ca1fc8677a620f4166d71b782cf", date: "2022-05-13 00:05:45 UTC", description: "Simplify inner loop logic", pr_number:                                                       12708, scopes: ["kafka source"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:    170, deletions_count:  109},
		{sha: "0741d87e9bd7339bce7659431d02e35c35235eb5", date: "2022-05-13 03:00:42 UTC", description: "Cleanup old comment", pr_number:                                                             12667, scopes: ["core"], type:                                "chore", breaking_change:       false, author: "Nathan Fox", files_count:         2, insertions_count:    2, deletions_count:    8},
		{sha: "8228507bd8e225c4b47e30e498665ee6e52316af", date: "2022-05-14 01:21:21 UTC", description: "Continue polling http config after error", pr_number:                                        12580, scopes: ["config"], type:                              "fix", breaking_change:         false, author: "Jorge Bay-Gondra", files_count:   1, insertions_count:    1, deletions_count:    1},
		{sha: "86eb58f6c345285a3d085e2f7972d82035ffd4f0", date: "2022-05-14 00:37:21 UTC", description: "Remove map! macro", pr_number:                                                               12716, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 10, insertions_count:   260, deletions_count:  224},
		{sha: "58abdce840f05069221ef342a52ccf0b0baff14a", date: "2022-05-16 23:13:44 UTC", description: "Hide vector config subcommand", pr_number:                                                   12713, scopes: ["cli"], type:                                 "chore", breaking_change:       false, author: "Will", files_count:               4, insertions_count:    319, deletions_count:  66},
		{sha: "d800a9b4841b10a258bfe789809c871242e69869", date: "2022-05-16 22:34:31 UTC", description: "Convert the finalizer framework to output a stream", pr_number:                              12715, scopes: ["sources"], type:                             "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      9, insertions_count:    258, deletions_count:  163},
		{sha: "58dcd7fb22cc1e48d1555647b7537b4abc5c1d3d", date: "2022-05-17 01:51:23 UTC", description: "Combine pipeline stages", pr_number:                                                         12602, scopes: [], type:                                      "chore", breaking_change:       false, author: "Luke Steensen", files_count:      10, insertions_count:   201, deletions_count:  80},
		{sha: "e6ca31211560eb5517490f18425273948c243abc", date: "2022-05-17 07:39:56 UTC", description: "Fix integration test failure", pr_number:                                                    12736, scopes: ["kafka source"], type:                        "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      1, insertions_count:    3, deletions_count:    0},
		{sha: "c19d6edd291a4c2a7baed689aa7df2cd33d173be", date: "2022-05-17 07:10:25 UTC", description: "Correct concurrency options for test.yml", pr_number:                                        12740, scopes: ["ci"], type:                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "b280b523c55861369ceddd942e5b5ed47bc5e595", date: "2022-05-17 08:26:31 UTC", description: "Update uap-rs to 0.6.0 ", pr_number:                                                         12601, scopes: [], type:                                      "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 3, insertions_count:    16, deletions_count:   11},
		{sha: "42b37f8002a7136e7fa1e7122e25374f802e163b", date: "2022-05-17 09:47:26 UTC", description: "Use domain qualified names for all images", pr_number:                                       12739, scopes: ["tests"], type:                               "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      18, insertions_count:   34, deletions_count:   34},
		{sha: "b9610235e016e979a57a94785af7bdeeb46ecb25", date: "2022-05-17 09:54:05 UTC", description: "Receive all new finalizer entries at once", pr_number:                                       12737, scopes: ["sources"], type:                             "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:    14, deletions_count:   8},
		{sha: "f238e5ee50cb5b281165430b13a6e9b9ba358a81", date: "2022-05-17 14:11:33 UTC", description: "Wrap values containing quotes in quotes", pr_number:                                         12700, scopes: ["codecs"], type:                              "fix", breaking_change:         true, author:  "Jameel Al-Aziz", files_count:     3, insertions_count:    51, deletions_count:   1},
		{sha: "f269e8dee67391ce29adcf30ec3e4c204ebd1633", date: "2022-05-18 10:40:20 UTC", description: "Improve deserialization of `EncodingConfigAdapter` / fix overriding `framing`", pr_number:   12750, scopes: ["codecs"], type:                              "fix", breaking_change:         false, author: "Pablo Sichert", files_count:      19, insertions_count:   364, deletions_count:  303},
		{sha: "d3ab27e9b670be450d1f0682fe0551b2a68a37c6", date: "2022-05-18 04:26:21 UTC", description: "bump proc-macro2 from 1.0.38 to 1.0.39", pr_number:                                          12744, scopes: ["deps"], type:                                "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:    9, deletions_count:    3},
		{sha: "49dbe994a36d5bfc83e45aa9b14ef187c09455e0", date: "2022-05-18 13:29:23 UTC", description: "New function is_json", pr_number:                                                            12747, scopes: ["vrl"], type:                                 "feat", breaking_change:        false, author: "Maksim Nabokikh", files_count:    6, insertions_count:    173, deletions_count:  0},
		{sha: "97c35cec12689bcd63c85e49fe9ffe1858d99d55", date: "2022-05-18 03:35:03 UTC", description: "Correct documentation around structured data handling", pr_number:                           12430, scopes: ["syslog source"], type:                       "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      1, insertions_count:    16, deletions_count:   14},
		{sha: "27d9a3adb252813e91333dca8cd3caf01b6de90b", date: "2022-05-18 03:35:15 UTC", description: "Remove non-erratic soaks, declared erratic", pr_number:                                      12752, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:    2, deletions_count:    2},
		{sha: "aa540a21add8971556d4e2fc3dd852cfb31462a6", date: "2022-05-18 15:03:05 UTC", description: "Implement `TextSerializer` and use it as default for `Encoding::Text`", pr_number:           12754, scopes: ["codecs"], type:                              "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      18, insertions_count:   210, deletions_count:  46},
		{sha: "a3e34fa6e625f57a96fad34c8b72d870dc298aa3", date: "2022-05-18 21:56:24 UTC", description: "Regenerate manifests based on 0.10.3 of the helm chart", pr_number:                          12642, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      17, insertions_count:   21, deletions_count:   21},
		{sha: "4627f59b3a831d79540b0e0269b65e65e466e39e", date: "2022-05-18 22:31:48 UTC", description: "Fix handling of auth token", pr_number:                                                      12757, scopes: ["gcp_pubsub source"], type:                   "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      3, insertions_count:    63, deletions_count:   40},
		{sha: "41bf25ad43869448c28f8e8abaa840f5536f2314", date: "2022-05-19 08:53:22 UTC", description: "Add vector version and target triple info to enterprise logs", pr_number:                    12774, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Will", files_count:               2, insertions_count:    30, deletions_count:   1},
		{sha: "0302e2396885569835abd98bd3f056eac70470d6", date: "2022-05-19 23:12:08 UTC", description: "Handle connection resets with fast retry", pr_number:                                        12789, scopes: ["gcp_pubsub source"], type:                   "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      3, insertions_count:    58, deletions_count:   20},
		{sha: "52a7fc4252092a20f21c3f10496b4862cfc620b9", date: "2022-05-21 08:19:42 UTC", description: "VRL iteration support highlight article", pr_number:                                         12795, scopes: ["external docs"], type:                       "docs", breaking_change:        false, author: "Jean Mertz", files_count:         1, insertions_count:    132, deletions_count:  0},
		{sha: "9bdfcd3f21c58c13983d0278fdd29300fff4cee1", date: "2022-05-21 09:29:58 UTC", description: "add iteration docs", pr_number:                                                              12721, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:         4, insertions_count:    205, deletions_count:  1},
		{sha: "2f8e174f41f35eb14629f924327dba2029f9b63c", date: "2022-05-21 13:45:08 UTC", description: "correctly type-def external root path", pr_number:                                           12805, scopes: ["vrl"], type:                                 "fix", breaking_change:         false, author: "Jean Mertz", files_count:         2, insertions_count:    16, deletions_count:   13},
		{sha: "2549b15954fe3a4cfbb96cee260905e40d033626", date: "2022-05-21 09:11:25 UTC", description: "Emit debug info when fetching auth tokens", pr_number:                                       12814, scopes: ["gcp service"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:    20, deletions_count:   14},
		{sha: "4c750cc01631d8e16f96540f59c6cb779c5ebee3", date: "2022-05-25 03:57:38 UTC", description: "Revert warnings for static conditions", pr_number:                                           12842, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      10, insertions_count:   26, deletions_count:   109},
		{sha: "bb915dcc17e7b2ce0d3cf4a98a7ddaa3a9a0bd38", date: "2022-05-25 04:54:24 UTC", description: "Revert immutable target for conditions", pr_number:                                          12844, scopes: ["vrl"], type:                                 "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      3, insertions_count:    4, deletions_count:    95},
		{sha: "bd96e3d9b3ce29a537e2181aca1e8c955743db7b", date: "2022-05-25 21:26:37 UTC", description: "Document component_received_events_count", pr_number:                                        12848, scopes: ["observability"], type:                       "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      3, insertions_count:    38, deletions_count:   0},
		{sha: "9109df9180c8e09929e91f9b8d4f7f686e1b3475", date: "2022-05-26 00:42:10 UTC", description: "Add release highlight for VRL encrypt/decrypt functions", pr_number:                         12849, scopes: [], type:                                      "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      4, insertions_count:    105, deletions_count:  9},
		{sha: "0b23dfebd8ed4d2e66f06d37a100caa22a92e5ff", date: "2022-05-26 02:25:37 UTC", description: "Revisions to native encoding highlight", pr_number:                                          12864, scopes: [], type:                                      "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:    59, deletions_count:   9},
		{sha: "f5cb460e196545a1ff7c040e747028a2ab69692b", date: "2022-05-25 21:20:24 UTC", description: "Implement async enterprise configuration reporting", pr_number:                              12781, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Will", files_count:               8, insertions_count:    364, deletions_count:  216},
		{sha: "174b73b38041d48d758a9ccb2d3d14e543cec0ff", date: "2022-05-25 21:31:59 UTC", description: "Include component span fields in internal_logs log events", pr_number:                       12807, scopes: ["observability"], type:                       "enhancement", breaking_change: false, author: "Will", files_count:               3, insertions_count:    152, deletions_count:  28},
		{sha: "e712ff2b5b299b3e90feaab34379501ea367a92f", date: "2022-05-24 03:24:54 UTC", description: "Integrate `encoding::Encoder` with `console` sink", pr_number:                               12181, scopes: ["console sink", "codecs"], type:              "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:      6, insertions_count:    110, deletions_count:  64},
		{sha: "86f308dbe4a2dd77416904738d6d53f57ed49951", date: "2022-05-19 06:32:58 UTC", description: "make RequestBuilder provide (un)compressed payload sizes", pr_number:                        12768, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      21, insertions_count:   330, deletions_count:  122},
		{sha: "f766d0e67a0d9466995f2e8a2c66250efa63248b", date: "2022-05-26 23:43:30 UTC", description: "Adjust log fields and metric tags in enterprise reporting", pr_number:                       12852, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Will", files_count:               1, insertions_count:    32, deletions_count:   30},
		{sha: "09ee99f82ce1ca465895d9ea75861376a51491cf", date: "2022-05-27 00:57:42 UTC", description: "Use `write_all` for output", pr_number:                                                      12871, scopes: ["codecs"], type:                              "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      1, insertions_count:    13, deletions_count:   10},
		{sha: "e67ab34f3618ea30e8744243959f395dea36788f", date: "2022-05-27 01:30:17 UTC", description: "Fix all uses of `Write::write`", pr_number:                                                  12873, scopes: ["loki sink"], type:                           "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      3, insertions_count:    4, deletions_count:    4},
		{sha: "4aed184424564e73221f8460e384291fa14f9038", date: "2022-05-25 23:08:29 UTC", description: "ensure sinks are checked for component spec compliance", pr_number:                          12755, scopes: ["observability"], type:                       "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      118, insertions_count:  2047, deletions_count: 1417},
		{sha: "fe86cb46f697ae292644667093404b77c2d7c96b", date: "2022-05-26 03:43:31 UTC", description: "fix unconstrained generic parameter in `EventCount` impl", pr_number:                        12862, scopes: ["buffers"], type:                             "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      4, insertions_count:    27, deletions_count:   29},
	]
}
