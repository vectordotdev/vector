package metadata

releases: "0.31.0": {
	date:     "2023-07-05"
	codename: ""

	description: """
		The Vector team is pleased to announce version 0.31.0!

		Be sure to check out the [upgrade guide](/highlights/2023-07-05-0-31-0-upgrade-guide) for
		breaking changes in this release.

		In addition to the usual smaller enhancements and bug fixes, this release includes an opt-in
		beta of a new log event data model that we think will make it easier to process logs by
		moving event metadata out of the log event itself. We are looking for feedback on this new
		feature before beginning towards making it the default and eventually removing the old log
		event data model.

		By way of example, an example event from the `datadog_agent` source currently looks like:

		```json
		{
			"ddsource": "vector",
			"ddtags": "env:prod",
			"hostname": "alpha",
			"foo": "foo field",
			"service": "cernan",
			"source_type": "datadog_agent",
			"bar": "bar field",
			"status": "warning",
			"timestamp": "1970-02-14T20:44:57.570Z"
		}
		```

		Will now look like:

		```json
		{
			"foo": "foo field",
			"bar": "bar field"
		}
		```

		(just the event itself)

		with additional buckets for source added metadata:


		```json
		{
			"ddsource": "vector",
			"ddtags": "env:prod",
			"hostname": "alpha",
			"service": "cernan",
			"status": "warning",
			"timestamp": "1970-02-14T20:44:57.570Z"
		}
		```

		accessible via `%<datadog_agent>.<field>`, and Vector added metadata:


		```json
		{
			"source_type": "datadog_agent",
			"ingest_timestamp": "1970-02-14T20:44:58.236Z"
		}
		```

		accessible via `%vector.<field>`.

		We think this new organization will be easier to reason about for users as well as avoid key
		conflicts between event fields and metadata.

		You can opt into this feature by setting `schema.log_namespace` as a global setting or the
		`log_namespace` option now available on each source itself. See [the blog
		post](/blog/log-namespacing) for an expanded explanation and details. Let us know what you think [on this issue](https://github.com/vectordotdev/vector/issues/17796).
		"""

	known_issues: []

	changelog: [
		{
			type: "fix"
			scopes: ["fluent source"]
			description: """
				The `fluent` source now correctly sends back message acknowledgements in msgpack
				rather than JSON. Previously fluentbit would fail to process them.
				"""
			contributors: ["ChezBunch"]
			pr_numbers: [17407]
		},
		{
			type: "enhancement"
			scopes: ["aws_s3 source"]
			description: """
				The `aws_s3` source now support bucket notifications in SQS that originated as SNS
				messages. It still does not support receiving SNS messages directly.
				"""
			contributors: ["sbalmos"]
			pr_numbers: [17352]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				A `from_unix_timestamp` function was added to VRL to decode timestamp values from
				unix timestamps. This deprecates the `to_timestamp` function, which will be removed
				in a future release.
				"""
			pr_numbers: [17793]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				The `parse_nginx_log` function now supports `ingress_upstreaminfo` as a format.
				"""
			pr_numbers: [17793]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				The `format_timestamp` function now supports an optional `timezone` argument to
				control the timezone of the encoded timestamp.
				"""
			pr_numbers: [17793]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				VRL now supports the `\\0` null byte escape sequence in strings.
				"""
			pr_numbers: [17793]
		},
		{
			type: "fix"
			scopes: ["statsd sink"]
			description: """
				The `statsd` sink now correctly encodes all counters as incremental, per the spec.
				"""
			pr_numbers: [16199]
		},
		{
			type: "chore"
			scopes: ["observability"]
			description: """
				Several deprecated internal metrics were removed:

				- `events_in_total`
				- `events_out_total`
				- `processed_bytes_total`
				- `processed_events_total`
				- `processing_errors_total`
				- `events_failed_total`
				- `events_discarded_total`

				See [the upgrade guide](/highlights/2023-07-05-0-31-0-upgrade-guide#deprecated-internal-metrics) for more details.
				"""
			breaking: true
			pr_numbers: [17516, 17542]
		},
		{
			type: "chore"
			scopes: ["observability"]
			description: """
				The `component_received_event_bytes_total` and `component_sent_event_bytes_total`
				internal metrics have been updated to use a new measure, "estimated JSON size", that
				is an estimate of the size of the event were it encoded as JSON rather than the
				"in-memory size" of the event, which is an implementation detail. See [the upgrade
				guide](/highlights/2023-07-05-0-31-0-upgrade-guide#event_json_size) for more
				details.
				"""
			breaking: true
			pr_numbers: [17516, 17542]
		},
		{
			type: "enhancement"
			scopes: ["shutdown"]
			description: """
				Vector's graceful shutdown time limit is now configurable (via
				`--graceful-shutdown-limit-secs`) and able to be disabled (via
				`--no-graceful-shutdown-limit`). See the [CLI
				docs](/docs/reference/cli/) for more.
				"""
			pr_numbers: [17479]
		},
		{
			type: "enhancement"
			scopes: ["sinks"]
			description: """
				Support for `zstd` compression was added to sinks support compression.
				"""
			contributors: ["akoshchiy"]
			pr_numbers: [17371]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_remote_write sink"]
			description: """
				The `prometheus_remote_write` sink now supports `zstd` and `gzip` compression in
				addition to `snappy` (the default).
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [17334]
		},
		{
			type: "enhancement"
			scopes: ["journald source"]
			description: """
				The `journald` source now supports a `journal_namespace` option to restrict the namespace of the units that the source consumes logs from.
				"""
			pr_numbers: [17648]
		},
		{
			type: "fix"
			scopes: ["buffers"]
			description: """
				A disk buffer deadlock that occurred on start-up after certain crash conditions was
				fixed.
				"""
			pr_numbers: [17657]
		},
		{
			type: "enhancement"
			scopes: ["codecs"]
			description: """
				The `gelf`, `native_json`, `syslog`, and `json` decoders (configurable as
				`decoding.codec` on sources) now have corresponding options for lossy UTF-8
				decoding via `decoding.<codec name>.lossy = true|false`. This can be used to
				accept invalid UTF-8 where invalid characters are replaced before decoded.
				"""
			pr_numbers: [17628, 17680]
		},
		{
			type: "fix"
			scopes: ["http_client source"]
			description: """
				The `http_client` no longer corrupts binary data by always trying to interpret as UTF-8 bytes. Instead options were added to encoders for lossy UTF-8 decoding (see above entry).
				"""
			pr_numbers: [17655]
		},
		{
			type: "enhancement"
			scopes: ["aws_kinesis_firehose sink", "aws_kinesis_streams sink"]
			description: """
				The `aws_kinesis_firehose` and `aws_kinesis_streams` sinks are now able to retry requests
				with partial failures by setting `request_retry_partial` to true. The default is
				`false` to avoid writing duplicate data if proper event idempotency is not in place.
				"""
			contributors: ["dengmingtong"]
			pr_numbers: [17535]
		},
		{
			type: "fix"
			scopes: ["http provider"]
			description: """
				The `Proxy-Authorization` header is now added to to HTTP requests from components
				that support HTTP proxies when authentication is used.
				"""
			contributors: ["syedriko"]
			pr_numbers: [17363]
		},
		{
			type: "fix"
			scopes: ["shutdown"]
			description: """
				Vector now exits non-zero if the graceful shutdown time limit expires before Vector
				finishes shutting down.
				"""
			pr_numbers: [17676]
		},
		{
			type: "fix"
			scopes: ["transforms", "sinks", "observability"]
			description: """
				The following components now log template render errors at the warning level rather
				than error and does not increment `component_errors_total`. This fixes a regression
				in v0.30.0 for the `loki` sink.

				- `loki` sink
				- `papertrail` sink
				- `splunk_hec_logs` sink
				- `splunk_hec_metrics` sink
				- `throttle` transform
				- `log_to_metric` transform
				"""
			pr_numbers: [17746]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				The `component_sent_event_bytes_total` and `component_sent_event_total` metrics can
				now optionally have a `service` and `source` tag added to them, driven from event
				data, from the added [`telemetry` global config
				options](/docs/reference/configuration/global-options/#telemetry). This can be used
				to break down processing volume by service and source.
				"""
			pr_numbers: [17549]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				The `internal_metrics` and `internal_logs` sources now shutdown last in order to
				capture as much telemetry as possible during Vector shutdown.
				"""
			pr_numbers: [17741]
		},
		{
			type: "fix"
			scopes: ["datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now incrementally encodes sketches. This avoids issues
				users have seen with sketch payloads exceeding the limits and being dropped.
				"""
			pr_numbers: [17764]
		},
		{
			type: "fix"
			scopes: ["datadog_agent source"]
			description: """
				The `datadog_agent` reporting of events and bytes received was fixed so it no longer
				double counted incoming events.
				"""
			pr_numbers: [17720]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				`log_schema` global configuration fields can now appear in a different file than
				defined sources.
				"""
			contributors: ["Hexta"]
			pr_numbers: [17759]
		},
		{
			type: "fix"
			scopes: ["file source"]
			description: """
				Vector now supports running greater than 512 sources. Previously it would lock up if
				more than 512 `file` sources were defined.
				"""
			contributors: ["honganan"]
			pr_numbers: [17717]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				Internal metrics for the Adaptive Concurrency Request module are now correctly
				tagged with component metadata like other sink metrics (`component_kind`,
				`component_id`, `component_type`).
				"""
			pr_numbers: [17765]
		},
	]

	commits: [
		{sha: "2ed8ec77d6effb6c373f56209aa52d9f6158f571", date: "2023-05-18 04:49:06 UTC", description: "bump reqwest from 0.11.17 to 0.11.18", pr_number:                                                17420, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   4, insertions_count:   22, deletions_count:   8},
		{sha: "e7fa8d373b74117c4d0d90902c3124e620c3c6c3", date: "2023-05-18 13:08:05 UTC", description: "bump rdkafka from 0.30.0 to 0.31.0", pr_number:                                                  17428, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "ae656c7124b9c148e7a678967f58edc2a32501e5", date: "2023-05-19 05:04:53 UTC", description: "bump proc-macro2 from 1.0.57 to 1.0.58", pr_number:                                              17426, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   65, deletions_count:   65},
		{sha: "c7d7cf8e36b9de6de7cd963e472d33b792c24413", date: "2023-05-19 14:51:58 UTC", description: "use get for page request", pr_number:                                                            17373, scopes: ["databend sink"], type:                  "fix", breaking_change:         false, author: "everpcpc", files_count:          1, insertions_count:   9, deletions_count:    14},
		{sha: "d1949921a81181e2eeb1780d7e081d767f758f5e", date: "2023-05-19 09:55:39 UTC", description: "fix ack message format", pr_number:                                                              17407, scopes: ["fluent source"], type:                  "fix", breaking_change:         false, author: "Benoît GARNIER", files_count:    1, insertions_count:   16, deletions_count:   10},
		{sha: "187f142ef5c28dec8e9b1ffbdfe0196acbe45804", date: "2023-05-19 02:00:47 UTC", description: "update fluentd link", pr_number:                                                                 17436, scopes: ["external docs"], type:                  "chore", breaking_change:       false, author: "neuronull", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "54d9c99492ec14924994a4857961aaafe3200f9b", date: "2023-05-20 08:46:28 UTC", description: "Add info about Vector Operator to Kubernetes instalation page", pr_number:                       17432, scopes: ["docs"], type:                           "chore", breaking_change:       false, author: "Vladimir", files_count:          1, insertions_count:   7, deletions_count:    1},
		{sha: "a8b7899bea771e6f2ca2e7c78c5a1c578f03d78f", date: "2023-05-20 00:00:07 UTC", description: "bump lapin from 2.1.1 to 2.1.2", pr_number:                                                      17439, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "ac0c7e82fc5877a58a60da872c40ad9b63143953", date: "2023-05-20 00:03:07 UTC", description: "bump security-framework from 2.9.0 to 2.9.1", pr_number:                                         17441, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "91ba052ba59d920761a02f7999c4b5d8b39d1766", date: "2023-05-20 08:27:29 UTC", description: "bump toml from 0.7.3 to 0.7.4", pr_number:                                                       17440, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   6, insertions_count:   21, deletions_count:   21},
		{sha: "b6394228d53508f22c6a65c69961baff19457c05", date: "2023-05-20 09:22:44 UTC", description: "bump lapin from 2.1.2 to 2.2.0", pr_number:                                                      17443, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "05bf262536031d199c06d980f47be317c97520ea", date: "2023-05-20 09:43:25 UTC", description: "bump clap_complete from 4.2.3 to 4.3.0", pr_number:                                              17447, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "618379a27583f6233a76c5b788616816b74bee03", date: "2023-05-20 10:36:37 UTC", description: "bump lapin from 2.2.0 to 2.2.1", pr_number:                                                      17448, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "060399a4bbef4280d1cea7c04304ed1308504ca0", date: "2023-05-22 23:37:55 UTC", description: "Move most CI checks to merge queue", pr_number:                                                  17340, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "neuronull", files_count:         21, insertions_count:  2308, deletions_count: 1101},
		{sha: "8e40b6850a57f874476f071d4ec98d699a99a65e", date: "2023-05-23 00:37:49 UTC", description: "temporarily disable flakey `aws_s3` integration test case `handles_errored_status` ", pr_number: 17455, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "neuronull", files_count:         1, insertions_count:   3, deletions_count:    0},
		{sha: "7554d9c8cc7b9b7134c7879dc941f8f55bc837e2", date: "2023-05-23 06:56:53 UTC", description: "bump bstr from 1.4.0 to 1.5.0", pr_number:                                                       17453, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   5, deletions_count:    5},
		{sha: "95cbba9116f12e1aa3665f89050132a28f9a0327", date: "2023-05-23 07:26:37 UTC", description: "bump base64 from 0.21.0 to 0.21.1", pr_number:                                                   17451, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   14, deletions_count:   14},
		{sha: "85703e792fe0ff70a466380823cf2d4b14b21603", date: "2023-05-23 01:07:21 UTC", description: "Bump PR limit for Dependabot to 100", pr_number:                                                 17459, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   1, deletions_count:    1},
		{sha: "299fd6ab53b1e818d09ae38f4321c20bdce4f30e", date: "2023-05-23 01:22:01 UTC", description: "Update fs_extra to 1.3.0", pr_number:                                                            17458, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   2, deletions_count:    2},
		{sha: "1f54415cb3fd4dc8f3f1b5989aa8d051cbe1faa5", date: "2023-05-23 01:47:25 UTC", description: "Bump lalrpop to 0.19.12", pr_number:                                                             17457, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   5, deletions_count:    5},
		{sha: "547783d17e8d2d3d351213a034e8d38fdcaa3047", date: "2023-05-23 02:11:46 UTC", description: "Clarify when component received and sent bytes events should be emitted", pr_number:             17464, scopes: ["docs"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   12, deletions_count:   9},
		{sha: "78bbfbc0205d97b401b5ba3084fe71e2bfdd7f33", date: "2023-05-23 03:49:14 UTC", description: "Bump version to 0.31.0", pr_number:                                                              17466, scopes: [], type:                                 "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     2, insertions_count:   2, deletions_count:    2},
		{sha: "36998428099da9b3ce4bcf0fd6f8787be1920363", date: "2023-05-23 05:43:33 UTC", description: "fix failure notify job conditional in publish workflow", pr_number:                              17468, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "neuronull", files_count:         1, insertions_count:   4, deletions_count:    4},
		{sha: "f54787190119255c1f97b2fe603ea5e65355b1cd", date: "2023-05-23 05:09:59 UTC", description: "Bump k8s manifests to 0.22.0", pr_number:                                                        17467, scopes: ["kubernetes"], type:                     "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     18, insertions_count:  22, deletions_count:   22},
		{sha: "897e45d5aa3d9ede6aa9115dae41a90b5a200ffa", date: "2023-05-23 23:06:22 UTC", description: "bump regex from 1.8.1 to 1.8.2", pr_number:                                                      17469, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   5, insertions_count:   9, deletions_count:    9},
		{sha: "9aaf864254bb05a92504533cd3d072341dbcb7e9", date: "2023-05-24 03:13:09 UTC", description: "bump data-encoding from 2.3.3 to 2.4.0", pr_number:                                              17452, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "bca45eb32bff27429a6beb3cf1d7b241d6de8c70", date: "2023-05-24 03:14:31 UTC", description: "bump myrotvorets/set-commit-status-action from 1.1.6 to 1.1.7", pr_number:                       17460, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   11, insertions_count:  27, deletions_count:   27},
		{sha: "c425006f299c7a5f91509f7bdb18963f4da0748f", date: "2023-05-24 03:15:58 UTC", description: "bump xt0rted/pull-request-comment-branch from 1 to 2", pr_number:                                17461, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   12, insertions_count:  18, deletions_count:   18},
		{sha: "9f6f6ecde0db3ffdd7b904647f490511433836b5", date: "2023-05-24 02:08:11 UTC", description: "minor fixes to workflows post merge queue enabling ", pr_number:                                 17462, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "neuronull", files_count:         4, insertions_count:   14, deletions_count:   12},
		{sha: "c1262cd162e04550b69913877d6b97037aceaea4", date: "2023-05-24 04:18:45 UTC", description: "Update metadata to match the editorial review for the schema.", pr_number:                       17475, scopes: ["aws_s3 sink"], type:                    "chore", breaking_change:       false, author: "Ari", files_count:               5, insertions_count:   9, deletions_count:    1},
		{sha: "9235fc249f4a0aa34d1119ed7dd334e23e5c3674", date: "2023-05-25 03:49:32 UTC", description: "bump proptest from 1.1.0 to 1.2.0", pr_number:                                                   17476, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   4, insertions_count:   8, deletions_count:    15},
		{sha: "ebf958b1355b4b729e7c99232bc40e2f7e809abf", date: "2023-05-25 03:57:35 UTC", description: "bump opendal from 0.34.0 to 0.35.0", pr_number:                                                  17471, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "9a44e6e8763c5d2bc91de1c24b14662d10d0b434", date: "2023-05-24 22:51:54 UTC", description: "Update the NOTICE file", pr_number:                                                              17430, scopes: [], type:                                 "chore", breaking_change:       false, author: "Bruce Guenter", files_count:     1, insertions_count:   3, deletions_count:    0},
		{sha: "58d7f3dfb0b57445db931604c6f72d93015da505", date: "2023-05-24 23:39:50 UTC", description: "temporarily disable comment_trigger workflow", pr_number:                                        17480, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "neuronull", files_count:         1, insertions_count:   5, deletions_count:    3},
		{sha: "541bb0087eb95b8d67c98547240c8104c5b2a69f", date: "2023-05-25 07:03:53 UTC", description: "Extend library functionality for secret scanning", pr_number:                                    17483, scopes: ["enterprise"], type:                     "chore", breaking_change:       false, author: "Will Wang", files_count:         3, insertions_count:   23, deletions_count:   18},
		{sha: "78fb4694c26d061314e8a01236a67633d8035d5c", date: "2023-05-25 05:04:04 UTC", description: "Fix architecture detection for ARMv7", pr_number:                                                17484, scopes: ["distribution"], type:                   "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:     1, insertions_count:   4, deletions_count:    3},
		{sha: "426d6602d22193940ac6e495fc5c175aa3bc8f90", date: "2023-05-25 22:17:36 UTC", description: "update `vrl` to `0.4.0`", pr_number:                                                             17378, scopes: [], type:                                 "chore", breaking_change:       false, author: "Nathan Fox", files_count:        44, insertions_count:  101, deletions_count:  269},
		{sha: "670bdea00ab7a13921aa3194667068b27f58e35a", date: "2023-05-26 04:26:55 UTC", description: "set source fields to mean service", pr_number:                                                   17470, scopes: ["observability"], type:                  "chore", breaking_change:       false, author: "Stephen Wakely", files_count:    6, insertions_count:   58, deletions_count:   15},
		{sha: "077a294d10412552e80c41429f23bd6a4f47724b", date: "2023-05-26 04:13:59 UTC", description: "Bump async-graphql from 5.0.8 to 5.0.9", pr_number:                                              17486, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   10, deletions_count:   10},
		{sha: "79f7dfb4d4633badf8ee89f0e940fa44f5bd59aa", date: "2023-05-26 04:14:38 UTC", description: "bump memmap2 from 0.6.1 to 0.6.2", pr_number:                                                    17482, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "84f0adac7a8e6306e12eaf13dc8c28f23e33f867", date: "2023-05-26 04:15:58 UTC", description: "bump criterion from 0.4.0 to 0.5.0", pr_number:                                                  17477, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   7, insertions_count:   13, deletions_count:   46},
		{sha: "7699f4ded19e520258adddd4c628a7a309c52c4e", date: "2023-05-26 01:33:59 UTC", description: "update comment_trigger note about concurrency groups", pr_number:                                17491, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "neuronull", files_count:         1, insertions_count:   5, deletions_count:    3},
		{sha: "ac81fc1318b229e2b9c6bbcd080af7438afde85a", date: "2023-05-26 08:18:38 UTC", description: "Bump async-graphql-warp from 5.0.8 to 5.0.9", pr_number:                                         17489, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "b28d915cb6a48da836bb4736c027f1ca5d623fe2", date: "2023-05-26 05:31:23 UTC", description: "remove custom async sleep impl", pr_number:                                                      17493, scopes: [], type:                                 "chore", breaking_change:       false, author: "Nathan Fox", files_count:        1, insertions_count:   3, deletions_count:    12},
		{sha: "2a76cac4d327eac537996d3409a64633c96f5ac8", date: "2023-05-26 06:00:07 UTC", description: "refactor `statsd` sink to stream-based style", pr_number:                                        16199, scopes: ["statsd sink"], type:                    "chore", breaking_change:       false, author: "Toby Lawrence", files_count:     31, insertions_count:  2346, deletions_count: 786},
		{sha: "5d90cff55c04701692dfe2b92416c3cf4ded5a4d", date: "2023-05-26 10:01:46 UTC", description: "bump regex from 1.8.2 to 1.8.3", pr_number:                                                      17494, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   5, insertions_count:   6, deletions_count:    6},
		{sha: "cc307460df2b45af6f33311d493c6bd7f9d44da5", date: "2023-05-26 23:02:43 UTC", description: "Bump quote from 1.0.27 to 1.0.28", pr_number:                                                    17496, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   67, deletions_count:   67},
		{sha: "f261781b5ce4389fb23017a2d4892c7f16753ad9", date: "2023-05-27 03:03:25 UTC", description: "Bump base64 from 0.21.1 to 0.21.2", pr_number:                                                   17488, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   14, deletions_count:   14},
		{sha: "2ad5b478f8948d0c3d92197f90100148cebda237", date: "2023-05-27 03:03:51 UTC", description: "bump aws-sigv4 from 0.55.1 to 0.55.3", pr_number:                                                17481, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   10, deletions_count:   10},
		{sha: "4ce3278ba5c2b92391818ff85c410a01f6b71cbf", date: "2023-05-27 04:47:28 UTC", description: "Bump proc-macro2 from 1.0.58 to 1.0.59", pr_number:                                              17495, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   66, deletions_count:   66},
		{sha: "a551f33da2b752229bd8139c72af80ce8b149638", date: "2023-05-27 11:12:45 UTC", description: "RFC for Data Volume Insights", pr_number:                                                        17322, scopes: [], type:                                 "chore", breaking_change:       false, author: "Stephen Wakely", files_count:    1, insertions_count:   240, deletions_count:  0},
		{sha: "98c54ad3a371ac710151367a953252f9eb293548", date: "2023-05-27 06:51:49 UTC", description: "remove deprecated internal metrics + massive cleanup to vector top and graphql API", pr_number:  17516, scopes: ["observability"], type:                  "chore", breaking_change:       false, author: "Toby Lawrence", files_count:     121, insertions_count: 1134, deletions_count: 2147},
		{sha: "bf372fd7cdef40704205e5fb5bf10bc50e002d94", date: "2023-05-30 02:03:43 UTC", description: "fix a few logic bugs and more strict comment parsing", pr_number:                                17502, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "neuronull", files_count:         6, insertions_count:   39, deletions_count:   61},
		{sha: "cc703da814928b41e0d9c0d7d211181f4aa5758a", date: "2023-05-30 06:10:19 UTC", description: "Bump tokio from 1.28.1 to 1.28.2", pr_number:                                                    17525, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   9, insertions_count:   12, deletions_count:   12},
		{sha: "2388c2f492a4952e48f1c1f8469045378ec60739", date: "2023-05-30 12:11:22 UTC", description: "Bump quanta from 0.11.0 to 0.11.1", pr_number:                                                   17524, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "da7bc951c450c1274fa37abb2d19b83dd3f965ab", date: "2023-05-30 12:12:17 UTC", description: "Bump criterion from 0.5.0 to 0.5.1", pr_number:                                                  17500, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   4, deletions_count:    4},
		{sha: "aa014528ca83bd3f1d17604d8c138ac2d0484074", date: "2023-05-31 00:17:29 UTC", description: "Drop VRL license exceptions", pr_number:                                                         17529, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "Bruce Guenter", files_count:     1, insertions_count:   0, deletions_count:    9},
		{sha: "078de661e7146a1924c0c31fed65b8b0ccbb7316", date: "2023-05-31 06:05:02 UTC", description: "Bump openssl from 0.10.52 to 0.10.53", pr_number:                                                17534, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   6, deletions_count:    6},
		{sha: "1565985746868265a1582a1b33b4eb56cc046c26", date: "2023-05-31 06:06:30 UTC", description: "Bump indicatif from 0.17.3 to 0.17.4", pr_number:                                                17532, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   6, deletions_count:    11},
		{sha: "8e113addc48328f3918e6abc7623284d93d4030b", date: "2023-05-31 06:07:26 UTC", description: "Bump once_cell from 1.17.1 to 1.17.2", pr_number:                                                17531, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   2, deletions_count:    2},
		{sha: "5a2fea10da7eaa04b7e51af84cdea87ab6e8326b", date: "2023-05-31 06:09:28 UTC", description: "Bump log from 0.4.17 to 0.4.18", pr_number:                                                      17526, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    6},
		{sha: "ecb707a633020bca8c805d5764b85302b74ca477", date: "2023-05-31 08:08:20 UTC", description: "Bump graphql_client from 0.12.0 to 0.13.0", pr_number:                                           17541, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   7, deletions_count:    7},
		{sha: "b0ed167d1ae22b8f0a7a762ad50750c912f0833b", date: "2023-05-31 05:04:00 UTC", description: "remove more deprecated internal metrics", pr_number:                                             17542, scopes: ["observability"], type:                  "chore", breaking_change:       false, author: "Toby Lawrence", files_count:     111, insertions_count: 216, deletions_count:  649},
		{sha: "3b87e00f3a62be93f55a89df676b47a8fad22201", date: "2023-05-31 05:02:15 UTC", description: "add missing logic to mark required checks failed", pr_number:                                    17543, scopes: ["ci"], type:                             "fix", breaking_change:         false, author: "neuronull", files_count:         2, insertions_count:   13, deletions_count:   2},
		{sha: "e2c025591c572efdd04728fac301b2e025596516", date: "2023-05-31 06:14:59 UTC", description: "post failed status to PR and isolate branch checkout on comment trigger", pr_number:             17544, scopes: ["ci"], type:                             "fix", breaking_change:         false, author: "neuronull", files_count:         1, insertions_count:   7, deletions_count:    3},
		{sha: "dbd7151aa4128638765e360f3f0f4e6582735041", date: "2023-05-31 12:57:35 UTC", description: "Bump opendal from 0.35.0 to 0.36.0", pr_number:                                                  17540, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "3b2a2be1b075344a92294c1248b09844f895ad72", date: "2023-06-01 05:18:38 UTC", description: "ensure `sent_event` and `received_event` metrics are estimated json size", pr_number:            17465, scopes: ["observability"], type:                  "chore", breaking_change:       false, author: "Stephen Wakely", files_count:    87, insertions_count:  807, deletions_count:  449},
		{sha: "247bb807cae195c5c987a43e3c4e6ab6b885a94b", date: "2023-05-31 23:49:54 UTC", description: "fix reference to supported aarch64 architecture", pr_number:                                     17553, scopes: ["external docs"], type:                  "chore", breaking_change:       false, author: "neuronull", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "0dfa09c4a9b7e753802a4fa0700557752e2fc945", date: "2023-06-01 01:25:38 UTC", description: "Bump chrono to 0.4.26", pr_number:                                                               17537, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "Bruce Guenter", files_count:     3, insertions_count:   12, deletions_count:   5},
		{sha: "349c7183067f0aa91b05914f34a68ee899fea88b", date: "2023-06-01 03:33:08 UTC", description: "Remove links to roadmap", pr_number:                                                             17554, scopes: [], type:                                 "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     3, insertions_count:   0, deletions_count:    9},
		{sha: "bcc5b6c5c883e16bd959b610890f67ffc0405860", date: "2023-06-01 09:23:24 UTC", description: "Bump csv from 1.2.1 to 1.2.2", pr_number:                                                        17555, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   2, deletions_count:    2},
		{sha: "7a4f1f77470fbc804299e2c1be867b193052d275", date: "2023-06-02 04:02:27 UTC", description: "correct emitted metrics", pr_number:                                                             17562, scopes: ["observability"], type:                  "fix", breaking_change:         false, author: "Stephen Wakely", files_count:    2, insertions_count:   8, deletions_count:    3},
		{sha: "23ed0e3adbffdd770a257635c3d6720a3bf072e7", date: "2023-06-02 05:49:12 UTC", description: "make shutdown duration configurable", pr_number:                                                 17479, scopes: ["configurable shutdown duration"], type: "feat", breaking_change:        false, author: "Dominic Burkart", files_count:   12, insertions_count:  130, deletions_count:  53},
		{sha: "f523f70d12053bd8d1d5ceee41c7c843780ded84", date: "2023-06-02 00:51:53 UTC", description: "Update field labels for commonly used sources and transforms ", pr_number:                       17517, scopes: ["config"], type:                         "chore", breaking_change:       false, author: "May Lee", files_count:           20, insertions_count:  41, deletions_count:   11},
		{sha: "ced219e70405c9ed9012444cc04efad8f91d3590", date: "2023-06-02 12:22:59 UTC", description: "zstd compression support", pr_number:                                                            17371, scopes: ["compression"], type:                    "enhancement", breaking_change: false, author: "Andrey Koshchiy", files_count:   29, insertions_count:  455, deletions_count:  121},
		{sha: "e1ddd0e99c0290a645a484c45cc42a391803c6c0", date: "2023-06-02 04:31:32 UTC", description: "Update field labels for sinks", pr_number:                                                       17560, scopes: ["config"], type:                         "chore", breaking_change:       false, author: "May Lee", files_count:           8, insertions_count:   15, deletions_count:   0},
		{sha: "8a741d55b8bfe361d6c5449cab4fd3728e1dae8d", date: "2023-06-02 02:42:54 UTC", description: "Bump aws-actions/configure-aws-credentials from 2.0.0 to 2.1.0", pr_number:                      17565, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   6, deletions_count:    6},
		{sha: "d7df52055152d9f85a6e48082d385e84c45f1501", date: "2023-06-03 02:15:58 UTC", description: "adapt int test to use breaking change of dep", pr_number:                                        17583, scopes: ["http_client source"], type:             "fix", breaking_change:         false, author: "neuronull", files_count:         2, insertions_count:   2, deletions_count:    2},
		{sha: "4af5e6d8886cfc326209f8d6aa65d27f86f6e579", date: "2023-06-03 03:07:14 UTC", description: "Bump openssl from 0.10.53 to 0.10.54", pr_number:                                                17573, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   4, deletions_count:    4},
		{sha: "8823561a8ad544b4acd29273b466b1a5bd606cc2", date: "2023-06-03 05:48:01 UTC", description: "Codify the use of abbreviate time units in config option names", pr_number:                      17582, scopes: [], type:                                 "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   2, deletions_count:    1},
		{sha: "134578db2165b4b522013d0e7d6ac974f9e4e744", date: "2023-06-03 05:48:10 UTC", description: "Codify flag naming including sentinel values", pr_number:                                        17569, scopes: [], type:                                 "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   16, deletions_count:   0},
		{sha: "6e45477ddc27147887346c8d09dd077225ea2ef3", date: "2023-06-03 06:06:18 UTC", description: "Update field labels for the rest of the sources and transforms fields", pr_number:               17564, scopes: ["config"], type:                         "chore", breaking_change:       false, author: "May Lee", files_count:           33, insertions_count:  46, deletions_count:   22},
		{sha: "1c1beb8123e1b0c82537ae3c2e26235bc6c0c43b", date: "2023-06-03 10:10:13 UTC", description: "Bump mock_instant from 0.3.0 to 0.3.1", pr_number:                                               17574, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   2, deletions_count:    2},
		{sha: "854980945e685485388bda2dd8f9cd9ad040029e", date: "2023-06-03 10:53:45 UTC", description: "Bump clap_complete from 4.3.0 to 4.3.1", pr_number:                                              17586, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "3395cfdb90b165653dda7e9014057aac1dba2d28", date: "2023-06-03 05:19:13 UTC", description: "bump pulsar from 5.1.1 to 6.0.0", pr_number:                                                     17587, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "neuronull", files_count:         4, insertions_count:   26, deletions_count:   14},
		{sha: "25e7699bb505e1856d04634ed6571eb22631b140", date: "2023-06-03 13:32:07 UTC", description: "use json size of unencoded event", pr_number:                                                    17572, scopes: ["loki sink"], type:                      "fix", breaking_change:         false, author: "Stephen Wakely", files_count:    2, insertions_count:   5, deletions_count:    17},
		{sha: "fa8a55385dd391aa2429c3f2e9821198c364c6a0", date: "2023-06-05 02:21:55 UTC", description: "int test yaml file detection", pr_number:                                                        17590, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "neuronull", files_count:         1, insertions_count:   38, deletions_count:   0},
		{sha: "a164952a145109d95c465645bf08b387a61e408a", date: "2023-06-06 03:10:16 UTC", description: "Bump indicatif from 0.17.4 to 0.17.5", pr_number:                                                17597, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "da939ca645e49cd02cbd739cddcdfe00dcb88a55", date: "2023-06-06 00:27:39 UTC", description: "add sink prelude", pr_number:                                                                    17595, scopes: [], type:                                 "chore", breaking_change:       false, author: "Stephen Wakely", files_count:    29, insertions_count:  97, deletions_count:   239},
		{sha: "6b34868e285a4608914405b7701ae1ee82deb536", date: "2023-06-06 01:11:04 UTC", description: " move blocked/waiting gardener issues to triage on comment", pr_number:                          17588, scopes: ["dev"], type:                            "enhancement", breaking_change: false, author: "neuronull", files_count:         1, insertions_count:   89, deletions_count:   0},
		{sha: "dc6bef2a2e6c47e145c776b4fd91042b112a0890", date: "2023-06-06 07:23:59 UTC", description: "Bump once_cell from 1.17.2 to 1.18.0", pr_number:                                                17596, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   6, insertions_count:   7, deletions_count:    7},
		{sha: "8e042590117989394f8bc246dc6d7de61d00123a", date: "2023-06-06 07:24:54 UTC", description: "Bump percent-encoding from 2.2.0 to 2.3.0", pr_number:                                           17602, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "7a55210ed814e0c47618905a299eba0d896a0646", date: "2023-06-06 07:50:36 UTC", description: "Bump cached from 0.43.0 to 0.44.0", pr_number:                                                   17599, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   5, deletions_count:    13},
		{sha: "657758db74496ec9adede09fc8f132bd8bed3bc3", date: "2023-06-06 08:54:46 UTC", description: "Bump regex from 1.8.3 to 1.8.4", pr_number:                                                      17601, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   5, insertions_count:   6, deletions_count:    6},
		{sha: "9395eba89ed10488914ac042aabba068356bb84b", date: "2023-06-06 20:59:56 UTC", description: "use correct secret for gardener board comment", pr_number:                                       17605, scopes: ["ci"], type:                             "fix", breaking_change:         false, author: "neuronull", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "baa04e59d9b234c4e71f8545a6ad8fdb2517f805", date: "2023-06-06 21:05:53 UTC", description: "checkout a greater depth in regression workflow", pr_number:                                     17604, scopes: ["ci"], type:                             "fix", breaking_change:         false, author: "neuronull", files_count:         1, insertions_count:   50, deletions_count:   1},
		{sha: "154e39382f4e80998814a693f9d6bb5c89ebebf7", date: "2023-06-07 03:10:22 UTC", description: "Bump hashbrown from 0.13.2 to 0.14.0", pr_number:                                                17609, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   12, deletions_count:   3},
		{sha: "d956092efdcc4ccea718365d9e9ef7bd537563a8", date: "2023-06-07 03:11:46 UTC", description: "Bump url from 2.3.1 to 2.4.0", pr_number:                                                        17608, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   4, insertions_count:   12, deletions_count:   12},
		{sha: "a9324892a289e94214707f1e09ea2931ae27d5e3", date: "2023-06-07 03:58:40 UTC", description: "Bump xml-rs from 0.8.4 to 0.8.14", pr_number:                                                    17607, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "b5bd85f87e39389a2ea3bb9a3d588fcbdfd0e29d", date: "2023-06-07 08:04:28 UTC", description: "Bump opendal from 0.36.0 to 0.37.0", pr_number:                                                  17614, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   4, deletions_count:    4},
		{sha: "bd880f55d2d8605733297acb4f96a8100a60dad4", date: "2023-06-07 08:22:12 UTC", description: "Bump getrandom from 0.2.9 to 0.2.10", pr_number:                                                 17613, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   14, deletions_count:   14},
		{sha: "b400acced6bd61d5927ab75bb82643b5927c0cbd", date: "2023-06-07 02:34:00 UTC", description: "fix copy-paste issue in component spec", pr_number:                                              17616, scopes: ["docs"], type:                           "fix", breaking_change:         false, author: "neuronull", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "c55c9ecbf904d9166c88af65a9a3f76f18289f58", date: "2023-06-07 19:45:36 UTC", description: "Bump tempfile from 3.5.0 to 3.6.0", pr_number:                                                   17617, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   4, insertions_count:   25, deletions_count:   23},
		{sha: "6c4856595410ee77d52d62ceb2cd808b1cdff04e", date: "2023-06-07 22:33:35 UTC", description: "Upgrade rust to 1.70.0", pr_number:                                                              17585, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "Bruce Guenter", files_count:     4, insertions_count:   6, deletions_count:    11},
		{sha: "460bbc7b9e532f93ac015ff871535c16135e4793", date: "2023-06-07 22:37:23 UTC", description: "Bump wiremock from 0.5.18 to 0.5.19", pr_number:                                                 17618, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "579108353e50546081b830d4e5788be7bb76a892", date: "2023-06-08 01:53:30 UTC", description: "change command to find baseline sha from issue comment trigger", pr_number:                      17622, scopes: ["ci"], type:                             "fix", breaking_change:         false, author: "neuronull", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "3005141f2097169a05af418e5f80765468645700", date: "2023-06-08 02:55:32 UTC", description: "Bump docker/setup-qemu-action from 2.1.0 to 2.2.0", pr_number:                                   17623, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   2, deletions_count:    2},
		{sha: "a54a12faae72ee64f4ba842746837a4787af5dc2", date: "2023-06-08 08:56:13 UTC", description: "Bump docker/metadata-action from 4.4.0 to 4.5.0", pr_number:                                     17624, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   1, deletions_count:    1},
		{sha: "15bc42a21bed188819da4d12e38d108f2e840202", date: "2023-06-08 08:56:43 UTC", description: "Bump docker/setup-buildx-action from 2.5.0 to 2.6.0", pr_number:                                 17625, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   4, deletions_count:    4},
		{sha: "10cfd0aec905c605248ad9d36abb312d4bfc1a5b", date: "2023-06-08 09:26:19 UTC", description: "Bump libc from 0.2.144 to 0.2.146", pr_number:                                                   17615, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "29315428b2c93ae0a5682ddb1fb25137b5eb3931", date: "2023-06-08 19:24:37 UTC", description: "Bump async-graphql from 5.0.9 to 5.0.10", pr_number:                                             17619, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   10, deletions_count:   10},
		{sha: "f1e1ae36ec4f244a03cbc7084cde64ea2d9631fa", date: "2023-06-08 22:23:07 UTC", description: "reg workflow alt approach to getting baseline sha", pr_number:                                   17645, scopes: ["ci"], type:                             "fix", breaking_change:         false, author: "neuronull", files_count:         1, insertions_count:   3, deletions_count:    25},
		{sha: "e35150e8b376db1f19b60b828233eb47393bb2dd", date: "2023-06-09 04:23:41 UTC", description: "Bump serde from 1.0.163 to 1.0.164", pr_number:                                                  17632, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   8, insertions_count:   11, deletions_count:   11},
		{sha: "593ea1bc89303f2f2344cca58d7c1aa5de939084", date: "2023-06-09 04:23:45 UTC", description: "Bump memmap2 from 0.6.2 to 0.7.0", pr_number:                                                    17641, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "b3885f693ebbdddd338b72bfd594e164d4fa361d", date: "2023-06-09 04:26:45 UTC", description: "Bump async-graphql-warp from 5.0.9 to 5.0.10", pr_number:                                        17642, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "f20eb2ff554c0163ea4955c9a5ad1ef0acd9f492", date: "2023-06-09 04:40:57 UTC", description: "Bump proc-macro2 from 1.0.59 to 1.0.60", pr_number:                                              17643, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   66, deletions_count:   66},
		{sha: "2638cca6cbf5103f71944383255b3e335d7f5790", date: "2023-06-08 23:34:18 UTC", description: "use correct ID for Triage in Gardener Board", pr_number:                                         17647, scopes: ["ci"], type:                             "fix", breaking_change:         false, author: "neuronull", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "380d7adb72a02e8da0af35fd3d80ecb1d8b0b541", date: "2023-06-09 09:16:28 UTC", description: "add more compression algorithms to Prometheus Remote Write", pr_number:                          17334, scopes: ["prometheus"], type:                     "feat", breaking_change:        false, author: "Alexander Zaitsev", files_count: 4, insertions_count:   77, deletions_count:   6},
		{sha: "a324a07ba1b62baac08d74b287595846b787b887", date: "2023-06-09 05:43:44 UTC", description: "add journal_namespace option", pr_number:                                                        17648, scopes: ["journald source"], type:                "feat", breaking_change:        false, author: "Doug Smith", files_count:        2, insertions_count:   71, deletions_count:   6},
		{sha: "0dc450fac14ac0236ca48466fd4fe42630d421ed", date: "2023-06-09 05:44:35 UTC", description: "mark VectorSink::from_event_sink as deprecated", pr_number:                                      17649, scopes: ["sinks"], type:                          "chore", breaking_change:       false, author: "Doug Smith", files_count:        20, insertions_count:  26, deletions_count:   0},
		{sha: "45a28f88a910c8492872773cc2e86045c8e2f4b6", date: "2023-06-10 05:28:33 UTC", description: "avoid importing vector-common in enrichment module", pr_number:                                  17653, scopes: ["enrichment"], type:                     "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:    4, insertions_count:   7, deletions_count:    6},
		{sha: "bf7d79623c0b575dd0bb6f851cc12c15cea5eb5f", date: "2023-06-10 03:12:55 UTC", description: "Add lossy option to JSON deserializer", pr_number:                                               17628, scopes: ["codecs"], type:                         "feat", breaking_change:        false, author: "Doug Smith", files_count:        28, insertions_count:  1131, deletions_count: 686},
		{sha: "cb9a3a548877b222afb14159393b8bc7bc3f8518", date: "2023-06-10 02:20:50 UTC", description: "Bump docker/build-push-action from 4.0.0 to 4.1.0", pr_number:                                   17656, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "e1b335748ef3b1345db9f5b9af11b5df2f24868a", date: "2023-06-14 01:02:27 UTC", description: "Bump log from 0.4.18 to 0.4.19", pr_number:                                                      17662, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "37a662a9c2e388dc1699f90288c5d856381d15d4", date: "2023-06-13 21:26:02 UTC", description: "deadlock when seeking after entire write fails to be flushed", pr_number:                        17657, scopes: ["buffers"], type:                        "fix", breaking_change:         false, author: "Toby Lawrence", files_count:     4, insertions_count:   224, deletions_count:  7},
		{sha: "19c4d4f72a4c08fdf51299bd7b3b906f8f8d08c1", date: "2023-06-14 03:40:45 UTC", description: "Bump wasm-bindgen from 0.2.86 to 0.2.87", pr_number:                                             17672, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   10, deletions_count:   10},
		{sha: "ab1169bd40ff7f1fa8cf1e77d24cd779112b2178", date: "2023-06-14 01:22:08 UTC", description: "Add apt retries to cross builds", pr_number:                                                     17683, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   2, deletions_count:    0},
		{sha: "2dfa8509bcdb4220d32e3d91f7fdd61c081db5ea", date: "2023-06-14 06:42:33 UTC", description: "add lossy option to `gelf`, `native_json`, and `syslog` deserializers", pr_number:               17680, scopes: ["codecs"], type:                         "feat", breaking_change:        false, author: "Doug Smith", files_count:        30, insertions_count:  1185, deletions_count: 165},
		{sha: "ac68a7b8d8238f4d64d5f3850e15dc9931e39349", date: "2023-06-15 01:17:32 UTC", description: "Bump rdkafka from 0.31.0 to 0.32.2", pr_number:                                                  17664, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   5, deletions_count:    5},
		{sha: "8d98bb8c4f4a4dd44e433caf8846aee4df1eec2b", date: "2023-06-15 01:21:03 UTC", description: "Bump pulsar from 6.0.0 to 6.0.1", pr_number:                                                     17673, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "714ccf8e77426b916ab88121c45a611106ebd6fe", date: "2023-06-15 00:21:21 UTC", description: "Bump crossbeam-utils from 0.8.15 to 0.8.16", pr_number:                                          17674, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   4, insertions_count:   5, deletions_count:    5},
		{sha: "c97d619d47b1171d592dcf55692b5caa01e97992", date: "2023-06-15 01:39:40 UTC", description: "Bump uuid from 1.3.3 to 1.3.4", pr_number:                                                       17682, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   2, deletions_count:    2},
		{sha: "80069871df7d0809411053435486c604b7b8c15d", date: "2023-06-15 01:40:20 UTC", description: "Bump docker/setup-buildx-action from 2.6.0 to 2.7.0", pr_number:                                 17685, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   4, deletions_count:    4},
		{sha: "71273dfc64206dd66290426fe7d65a68afb13d51", date: "2023-06-15 01:41:19 UTC", description: "Bump docker/metadata-action from 4.5.0 to 4.6.0", pr_number:                                     17686, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   1, deletions_count:    1},
		{sha: "bce5e65d9562983f0094f1b7359775cf17043285", date: "2023-06-15 01:41:49 UTC", description: "Bump docker/build-push-action from 4.1.0 to 4.1.1", pr_number:                                   17687, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "41ee39414ea3210c841659f1f41b3295ad8bfd23", date: "2023-06-14 22:01:17 UTC", description: "Drop use of `hashlink` crate", pr_number:                                                        17678, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "Bruce Guenter", files_count:     3, insertions_count:   5, deletions_count:    15},
		{sha: "59e2cbff7bce014209813369d2a33a25ac193bb3", date: "2023-06-15 00:20:47 UTC", description: "remove utf8 lossy conversion", pr_number:                                                        17655, scopes: ["http_client source"], type:             "fix", breaking_change:         false, author: "Doug Smith", files_count:        1, insertions_count:   1, deletions_count:    2},
		{sha: "ee480cd08a5451bc3f0b83a2b037ba131e38d4b9", date: "2023-06-15 01:00:28 UTC", description: "Dropped error field from StreamClosed Error", pr_number:                                         17693, scopes: [], type:                                 "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:   37, insertions_count:  74, deletions_count:   84},
		{sha: "9c4539436ecbbf48dc0dd454ea25230d539b2c9b", date: "2023-06-15 07:19:03 UTC", description: "consolidate enum types", pr_number:                                                              17688, scopes: ["codecs"], type:                         "chore", breaking_change:       false, author: "Doug Smith", files_count:        18, insertions_count:  144, deletions_count:  296},
		{sha: "2263756d0a39cb99d62a826ff0993f461ae80937", date: "2023-06-16 00:04:32 UTC", description: "Update to Alpine 3.18", pr_number:                                                               17695, scopes: ["deps", "releasing"], type:              "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     3, insertions_count:   5, deletions_count:    3},
		{sha: "079d895ebffeb62cf51cb11144b17fd481292510", date: "2023-06-16 05:55:48 UTC", description: "Add docker config to dependabot", pr_number:                                                     17696, scopes: [], type:                                 "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:   1, insertions_count:   10, deletions_count:   0},
		{sha: "960635387235ea270d748038a3a0ddd615813f29", date: "2023-06-16 04:00:58 UTC", description: "Make config schema output ordered", pr_number:                                                   17694, scopes: ["config"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:     6, insertions_count:   9, deletions_count:    17},
		{sha: "2ad964d43b9a47808104eced885cebf6541f4a72", date: "2023-06-16 23:48:28 UTC", description: "Additional notes on proposing new integrations", pr_number:                                      17658, scopes: [], type:                                 "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:   1, insertions_count:   28, deletions_count:   6},
		{sha: "bebac21cb699be64d1b009d3619d5af5c5be20ec", date: "2023-06-17 12:32:09 UTC", description: "implement full retry of partial failures in firehose/streams", pr_number:                        17535, scopes: ["kinesis sinks"], type:                  "feat", breaking_change:        false, author: "dengmingtong", files_count:      13, insertions_count:  103, deletions_count:  23},
		{sha: "c21f892e574579e323742da009f15a39c43555af", date: "2023-06-17 08:03:47 UTC", description: "validate s3 sink flushes", pr_number:                                                            17667, scopes: ["flush on shutdown"], type:              "chore", breaking_change:       false, author: "Dominic Burkart", files_count:   1, insertions_count:   74, deletions_count:   0},
		{sha: "d122d32b8c83133b753c9e31d19be6c6609fb9a5", date: "2023-06-20 03:43:56 UTC", description: "Bump sha2 from 0.10.6 to 0.10.7", pr_number:                                                     17698, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   24, deletions_count:   24},
		{sha: "cd6d1540bf74d13ad6bc9c90fc3fe2affb11e6dc", date: "2023-06-21 00:06:24 UTC", description: "Bump notify from 6.0.0 to 6.0.1", pr_number:                                                     17700, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "53e178570b5b87bc2124f4299865cbb00916fe20", date: "2023-06-21 01:55:02 UTC", description: "Bump gloo-utils from 0.1.6 to 0.1.7", pr_number:                                                 17707, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   2, deletions_count:    2},
		{sha: "9cd54043fab1e82722adaeeaee290d7084074439", date: "2023-06-21 01:48:48 UTC", description: "Convert top-level sinks enum to typetag", pr_number:                                             17710, scopes: ["config"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:     74, insertions_count:  270, deletions_count:  540},
		{sha: "6705bdde058b1a532eda9398c9610dff46bb783b", date: "2023-06-21 05:30:41 UTC", description: "Vector does not put the Proxy-Authorization header on the wire (#17353)", pr_number:             17363, scopes: ["auth"], type:                           "fix", breaking_change:         false, author: "Sergey Yedrikov", files_count:   2, insertions_count:   38, deletions_count:   18},
		{sha: "12bc4a7d116273cda322fccf41b4e3ea6c333be3", date: "2023-06-21 04:17:53 UTC", description: "Bump aws-actions/configure-aws-credentials from 2.1.0 to 2.2.0", pr_number:                      17697, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   6, deletions_count:    6},
		{sha: "dd2527dcea295f4f9f6eb617306a822892e08a59", date: "2023-06-22 07:33:19 UTC", description: "Bump openssl from 0.10.54 to 0.10.55", pr_number:                                                17716, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   6, deletions_count:    6},
		{sha: "e8e7e0448f51ed9646c484123fd4953442545c86", date: "2023-06-22 00:00:00 UTC", description: "Retry `make check-component-docs` check", pr_number:                                             17718, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   5, deletions_count:    1},
		{sha: "ddebde97bac79eaecb7feb286bfe5a25591e7d13", date: "2023-06-22 03:29:50 UTC", description: "Upgrade Ruby version to 3.1.4", pr_number:                                                       17722, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     5, insertions_count:   6, deletions_count:    12},
		{sha: "bc6925592f8d954212efb99f2f17bcac8a454169", date: "2023-06-22 06:41:01 UTC", description: "reduce billable time of Test Suite", pr_number:                                                  17714, scopes: ["ci"], type:                             "enhancement", breaking_change: false, author: "neuronull", files_count:         1, insertions_count:   36, deletions_count:   51},
		{sha: "25131efdbe855a8f4d2491bd68fb76c58f7f8ad4", date: "2023-06-22 23:54:09 UTC", description: "Bump serde_json from 1.0.96 to 1.0.97", pr_number:                                               17701, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   6, insertions_count:   7, deletions_count:    7},
		{sha: "e5e6b9635cf3fd13676d845f184ef3a04167ceef", date: "2023-06-22 23:54:27 UTC", description: "Bump tower-http from 0.4.0 to 0.4.1", pr_number:                                                 17711, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   38, deletions_count:   32},
		{sha: "c96e3be34c239e94a366f9ced8e0e8b69570a562", date: "2023-06-22 22:55:03 UTC", description: "Bump mongodb from 2.5.0 to 2.6.0", pr_number:                                                    17726, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "08099a8b567663416d907600e2f9c678482af272", date: "2023-06-23 01:28:52 UTC", description: "Have `tower_limit` use configured log level", pr_number:                                         17715, scopes: ["observability"], type:                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   1, deletions_count:    1},
		{sha: "a08443c890cc0e3223e4d17c71eb267f0305d50c", date: "2023-06-23 04:17:06 UTC", description: "Add @dsmith3197 to CODEOWNERS", pr_number:                                                       17729, scopes: ["dev"], type:                            "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     2, insertions_count:   19, deletions_count:   18},
		{sha: "9a899c5d7c40a271b17eafec2f840c1bfd082b04", date: "2023-06-23 04:39:49 UTC", description: "Add additional warning around APM stats for `peer.service`", pr_number:                          17733, scopes: ["datadog_traces sink"], type:            "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   12, deletions_count:   1},
		{sha: "326ad0861215f22c83f681e725abb88b33107e2e", date: "2023-06-23 23:24:38 UTC", description: "Bump infer from 0.13.0 to 0.14.0", pr_number:                                                    17737, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   4, deletions_count:    4},
		{sha: "cc52c0ea99e03f451c24c165b24430c045ff365d", date: "2023-06-24 04:00:39 UTC", description: "set exit flag to non-zero when shutdown times out", pr_number:                                   17676, scopes: ["error code when shutdown fails"], type: "feat", breaking_change:        false, author: "Dominic Burkart", files_count:   3, insertions_count:   59, deletions_count:   14},
		{sha: "ff6a1b4f06b1e32f3192f2bc391e8ab59f466993", date: "2023-06-23 22:13:14 UTC", description: "Remove upload of config schema", pr_number:                                                      17740, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   0, deletions_count:    6},
		{sha: "44be37843c0599abb64073fe737ce146e30b3aa5", date: "2023-06-24 02:57:59 UTC", description: "add metadata support to `set_semantic_meaning`", pr_number:                                      17730, scopes: ["schemas"], type:                        "feat", breaking_change:        false, author: "Nathan Fox", files_count:        2, insertions_count:   23, deletions_count:   18},
		{sha: "7a0dec13537211b4a7e460cdf57b079709649b5f", date: "2023-06-24 02:55:43 UTC", description: "Move CONTRIBUTING.md to top-level", pr_number:                                                   17744, scopes: ["docs"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     2, insertions_count:   256, deletions_count:  257},
		{sha: "7d10fc97f32c053f9336d1d69d530f39ef258268", date: "2023-06-24 04:54:20 UTC", description: "Clarify `bytes` framing for streams", pr_number:                                                 17745, scopes: ["docs"], type:                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   1, deletions_count:    1},
		{sha: "92a36e0119e0e1f50b8bfcdcaf1c536018b69d5f", date: "2023-06-24 05:59:45 UTC", description: "refactor logic for int test file path changes detection", pr_number:                             17725, scopes: ["ci"], type:                             "enhancement", breaking_change: false, author: "neuronull", files_count:         38, insertions_count:  413, deletions_count:  252},
		{sha: "4ebc3e1171cba4f00023f0ef860a6b66c98763a9", date: "2023-06-24 05:05:16 UTC", description: "Drop non-fatal template render errors to warnings", pr_number:                                   17746, scopes: ["loki sink", "observability"], type:     "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:     1, insertions_count:   21, deletions_count:   13},
		{sha: "c35ebd167b029eb0fb6c180301e8ff911f938f9f", date: "2023-06-24 06:27:25 UTC", description: "add domain label for vdev", pr_number:                                                           17748, scopes: ["administration"], type:                 "chore", breaking_change:       false, author: "neuronull", files_count:         1, insertions_count:   3, deletions_count:    0},
		{sha: "6e1878b1c151a19d7a99fd6c8c8a847cc69db3c8", date: "2023-06-27 00:31:06 UTC", description: "Bump itertools from 0.10.5 to 0.11.0", pr_number:                                                17736, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   3, insertions_count:   23, deletions_count:   14},
		{sha: "6a6b42bedbd27dec0c91e274698785cc73f805df", date: "2023-06-26 22:38:47 UTC", description: "Upgrade aws-smithy and aws-sdk crates", pr_number:                                               17731, scopes: [], type:                                 "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:   27, insertions_count:  487, deletions_count:  468},
		{sha: "dcf7f9ae538c821eb7b3baf494d3e8938083832c", date: "2023-06-27 04:03:39 UTC", description: "emit `component_sent` events by `source` and `service`", pr_number:                              17549, scopes: ["observability"], type:                  "chore", breaking_change:       false, author: "Stephen Wakely", files_count:    77, insertions_count:  1387, deletions_count: 501},
		{sha: "94e3f1542be0c4ba93f554803973c9e26e7dc566", date: "2023-06-27 06:53:51 UTC", description: "remove aggregator beta warning", pr_number:                                                      17750, scopes: [], type:                                 "docs", breaking_change:        false, author: "gadisn", files_count:            1, insertions_count:   0, deletions_count:    7},
		{sha: "63ba2a95d972bbba11cd9a1f913f2606bb2ba20b", date: "2023-06-26 23:53:17 UTC", description: "Bump proc-macro2 from 1.0.60 to 1.0.63", pr_number:                                              17757, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   1, insertions_count:   67, deletions_count:   67},
		{sha: "a53c7a2153960038b8e68e13d6beede09eb1a69a", date: "2023-06-26 23:48:37 UTC", description: "Add warning about Windows support", pr_number:                                                   17762, scopes: ["kubernetes_logs source"], type:         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   3, deletions_count:    1},
		{sha: "e164b36436b85a332b5a3b4c492caab6b53578d3", date: "2023-06-27 06:53:13 UTC", description: "Bump serde_yaml from 0.9.21 to 0.9.22", pr_number:                                               17756, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   4, insertions_count:   46, deletions_count:   29},
		{sha: "96e68f76efe2208a8899b3f8961125ba5424a9ba", date: "2023-07-01 06:25:20 UTC", description: "Bump lru from 0.10.0 to 0.10.1", pr_number:                                                      17810, scopes: ["deps"], type:                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:   2, insertions_count:   3, deletions_count:    3},
		{sha: "708b7f6088c14180945d80e2a8f13ed471ded77a", date: "2023-07-01 00:17:31 UTC", description: "Add schedule to component features workflow conditional check", pr_number:                       17816, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   1, deletions_count:    1},
		{sha: "fe730adee64c45bc9a0737838a8aaa2bd8ef61d8", date: "2023-07-01 01:38:51 UTC", description: "Bump up OSX runners for release builds", pr_number:                                              17823, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   1, deletions_count:    1},
		{sha: "47c3da1f21d3cc3d4af09d321ae3754972e0a150", date: "2023-07-01 06:35:02 UTC", description: "fix gardener issues comment workflow", pr_number:                                                17825, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "Doug Smith", files_count:        1, insertions_count:   28, deletions_count:   29},
		{sha: "77ac63c5bd87309b1ddd54e55b933072b40e34ea", date: "2023-07-01 07:00:11 UTC", description: "refactor to new style", pr_number:                                                               17723, scopes: ["clickhouse sink"], type:                "chore", breaking_change:       false, author: "Doug Smith", files_count:        9, insertions_count:   474, deletions_count:  299},
		{sha: "93ef6c3e9241601253b48e27ee817e73474a89c6", date: "2023-07-01 07:30:23 UTC", description: "add instructions for regenerating component docs and licenses", pr_number:                       17828, scopes: ["docs"], type:                           "chore", breaking_change:       false, author: "Doug Smith", files_count:        2, insertions_count:   19, deletions_count:   4},
		{sha: "4786743dcaa73e16781e8b43ce0a1ce0315a55d1", date: "2023-07-01 09:13:02 UTC", description: "`aws_ec2_metadata` transform when using log namespacing", pr_number:                             17819, scopes: ["aws_ec2_metadata transform"], type:     "fix", breaking_change:         false, author: "Nathan Fox", files_count:        1, insertions_count:   38, deletions_count:   0},
		{sha: "ee10b8cbae51b9c0bade8d8bd8273a8dbeb3bb58", date: "2023-07-01 06:41:15 UTC", description: "revert fix gardener issues comment workflow", pr_number:                                         17829, scopes: ["ci"], type:                             "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:     1, insertions_count:   29, deletions_count:   28},
	]
}
