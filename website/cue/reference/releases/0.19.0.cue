package metadata

releases: "0.19.0": {
	date:     "2021-12-28"
	codename: ""

	whats_next: [
		{
			title: "Faster disk buffers"
			description: """
				We are in the process of replacing our current disk buffer
				implementation, which leverages LevelDB in a way that doesn't
				quite match common LevelDB use-cases, with a custom
				implementation specific to Vector's needs. The end result is
				faster disk buffers.
				"""
		},
		{
			title:       "Component metric standardization"
			description: """
				We are in the process of ensuring that all Vector components
				report a consistent set of metrics to make it easier to monitor
				the performance of Vector.  These metrics are outlined in this
				new [instrumentation
				specification](\(urls.specs_instrumentation)).
				"""
		},
	]

	known_issues: [
		"A regression was introduced that changed the name of the data directory for sinks using a disk buffer. This means, when upgrading from `v0.18.0`, if there was any data in existing disk buffers, it will not be sent. Vector's starting with clean or empty disk buffers are unaffected. Fixed in v0.19.1. See [#10430](https://github.com/vectordotdev/vector/issues/10430) for more details.",
		"Presence of `framing.character_delimited.delimiter` in configs causes Vector to fail to start with `invalid type: string`. Fixed in v0.19.1.",
		"When using `decoding.codec` on sources, invalid data will cause the source to cease processing. Fixed in v0.19.2.",
		"`encoding.only_fields` failed to deserialize correctly for sinks that used fixed encodings (i.e. those that don't have `encoding.codec`). Fixed in v0.19.2.",
		"Buffers using `when_full` of `block` were incorrectly counting `buffer_events_total` by including discarded events. Fixed in v0.19.2.",
		"Transforms neglect to tag logs and metrics with their component span tags (like `component_id`). Fixed in v0.19.2.",
	]

	description: """
		The Vector team is pleased to announce version 0.19.0!

		In addition to the below features, enhancements, and fixes, we've been
		hard at work improving Vector's performance and were able to move the
		needle 10-100% for most configurations in our [soak test
		framework](https://github.com/vectordotdev/vector/tree/master/soaks/tests)
		from the last release, `v0.18`.

		Be sure to check out the [upgrade
		guide](/highlights/2021-12-28-0-19-0-upgrade-guide) for breaking
		changes in this release.
		"""

	changelog: [
		{
			type: "enhancement"
			scopes: ["aws_s3 source", "delivery"]
			description: """
				Added end-to-end acknowledgement for the `aws_s3` source.
				"""
			pr_numbers: [10045]
		},
		{
			type: "feat"
			scopes: ["splunk_hec source", "splunk_hec sink"]
			description: """
				The `splunk_hec` source and sink have added support for the
				acknowledgement part of Splunk's HTTP Event Collector protocol.
				This improves delivery guarantees for data from Splunk clients
				and when sending events to Splunk. See [the highlight
				article](/highlights/2021-12-15-splunk-hec-improvements) for
				more details.
				"""
			pr_numbers: [10444, 10162, 10135, 10350]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				VRL now allows for writing multi-line string literals by ending the line with a backslash (\\\\).

				Example:

				```text
				.thing = "foo \\
				bar"

				assert(.thing == "foo bar")
				```
				"""
			pr_numbers: [10149]
		},
		{
			type: "enhancement"
			scopes: ["influxdb_logs sink"]
			description: """
				A couple of enhancements have been made to the `influxdb_logs`
				sink to modify how Vector encodes events.

				A `measurement` config field was added to allow overriding the
				measurement name (previously hardcoded to `vector`). Along with
				this the `namespace` option was made optional as `measurement`
				can be used to set the full name directly.

				For example:

				```toml
				[sinks.log_to_influxdb]
				type = "influxdb_logs"
				measurement = "vector-logs"
				endpoint = "http://localhost:9999"
				```

				Now outputs events like:

				```text
				vector-logs,metric_type=logs,host=example.com message="hello world",size=10 {timestamp}
				```

				A `metric_type` config field was added to allow customizing the
				metric type (previously hardcoded to `log`).

				For example:

				```toml
				[sinks.log_to_influxdb]
				type = "influxdb_logs"
				measurement_type = "foo"
				endpoint = "http://localhost:9999"
				```

				Now outputs events like:

				```text
				ns.vector,metric_type=foo,host=example.com message="hello world",size=10 {timestamp}
				```
				"""
			pr_numbers: [10082, 10217]
			contributors: ["juvenn"]
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source has been updated to read older pod
				logs first. This should result in better behavior with Vector
				releasing file handles for rotated pod files more quickly.
				"""
			pr_numbers: [10218]
		},
		{
			type: "fix"
			scopes: ["kafka source"]
			description: """
				The `headers_key` config option for the `kafka` source was
				restored. This was accidentally renamed to `headers_field` in
				v0.18.0. For compatibility with v0.18.0, `headers_field` will
				also be accepted.
				"""
			pr_numbers: [10222]
		},
		{
			type: "fix"
			scopes: ["loki sink"]
			description: """
				The `loki` sink now accepts any 200-level HTTP response from
				servers as success. This was added for compatibility with other
				Loki-compatible APIs like cLoki which didn't respond with the
				expected 204 response code.
				"""
			pr_numbers: [10224]
		},
		{
			type: "enhancement"
			scopes: ["config"]
			description: """
				Vector's support for environment variable expansion in
				configuration files now allows `.`s in the variable names as
				these commonly appear in environment variables set by Java
				properties files.
				"""
			pr_numbers: [10194]
		},
		{
			type: "feat"
			scopes: ["datadog_agent source"]
			description: """
				The `datadog_agent` source is now able to accept metrics from
				the Datadog Agent; however, some [changes are pending in the
				Datadog
				Agent](https://github.com/DataDog/datadog-agent/pull/9633) to be
				able to send metrics to Vector. We expect this to be released in
				version 6.33 / 7.33 of the Datadog Agent.
				"""
			pr_numbers: [9563]
		},
		{
			type: "feat"
			scopes: ["humio_logs sink"]
			description: """
				The `humio_logs` sink now transmits sub-millisecond timestamps
				to Humio.
				"""
			pr_numbers: [10216]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				VRL has added new functions for interacting with event metadata.:

				* `get_metadata_field("key")`
				* `set_metadata_field("key", "value")`
				* `remove_metadata_field("key")`

				Right now, the only event metadata that is accessible is Datadog
				API keys (`datadog_api_key`) or Splunk HEC channel tokens
				(`splunk_hec_token`) that are associated by the source, but we
				expect metadata use-cases to grow.

				This can be used with, for example, CSV enrichment tables to
				lookup the `datadog_api_key` to use with events based on other
				metadata.
				"""
			pr_numbers: [10198]
		},
		{
			type: "feat"
			scopes: ["splunk_hec source", "splunk_hec sink"]
			description: """
				The `splunk_hec` source and sink have added support for passing
				the channel token events were sent with from the source to the
				sink. This makes it easier to use Vector in-between a Splunk
				sender and receiver to transform the data.  See [the highlight
				article](/highlights/2021-12-15-splunk-hec-improvements) for
				more details.
				"""
			pr_numbers: [10261]
		},
		{
			type: "enhancement"
			scopes: ["statsd sink"]
			description: """
				The `statsd` sink now compresses histograms to result in smaller
				payloads without data loss.
				"""
			pr_numbers: [10279]
		},
		{
			type: "enhancement"
			scopes: ["topology", "performance"]
			description: """
				Vector's CPU utilization has improved by running eligible
				transforms on multiple cores when possible. Previously,
				a transform could be a significant bottleneck since only one
				copy of it was ran which would result Vector under-utilizing
				available CPU resources. See [the highlight
				article](/highlights/2021-11-18-implicit-namespacing) for
				more details.
				"""
			pr_numbers: [10265]
		},
		{
			type: "feat"
			scopes: ["elasticsearch sink"]
			description: """
				A new config option has been added to the `elasticsearch` sink
				to allow suppressing the `type` field from being sent by Vector.
				This field is deprecated in Elasticsearch v7 and will be removed
				in v8.
				"""
			pr_numbers: [10357]
		},
		{
			type: "fix"
			scopes: ["internal_logs source"]
			description: """
				The `host` and `pid` fields are correctly added to all internal
				logs now. Previously they were only added to start-up logs, but
				not logs while Vector was running.
				"""
			pr_numbers: [10425]
		},
		{
			type: "enhancement"
			scopes: ["performance"]
			description: """
				We have improved Vector's performance for most use-cases by
				re-introducing the jemalloc memory allocator as the allocator
				for *nix platforms. We continue to evaluate other allocators to
				see if they are a better fit for Vector's allocation patterns.
				"""
			pr_numbers: [10459]
		},
		{
			type: "fix"
			scopes: ["sinks"]
			description: """
				Fix a panic that could occur during when reloading Vector config
				that requires shutting down and recreating a sink.
				"""
			pr_numbers: [10490]
		},
		{
			type: "enhancement"
			scopes: ["blackhole sink"]
			description: """
				Fix metric emission for the `blackhole` sink when a rate limit
				lower than `1024` was used.
				"""
			pr_numbers: [10526]
		},
		{
			type: "feat"
			scopes: ["sources"]
			description: """
				A new `connection_limit` option has been added to TCP-based
				sources like `socket` and `syslog` to limit the number of
				allowed TCP connections. This can be useful to limit resource
				utilization by Vector.
				"""
			pr_numbers: [10491]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Vector, when using `--config-dir`, no longer tries to load
				unknown file extensions. This was a regression in `v0.18`.
				"""
			pr_numbers: [10564]
		},
		{
			type: "fix"
			scopes: ["codecs", "sources"]
			description: """
				The `max_length` option available on some `decoding` framers for
				sources previously caused Vector to stop decoding a given input
				stream (like a TCP connection) when a frame that was too big was
				encountered. It now correctly just discards that frame and
				continues.
				"""
			pr_numbers: [10568]
		},
		{
			type: "fix"
			scopes: ["codecs", "sources"]
			description: """
				Previously the `framing.character_delimited.delimiter` option
				available on some sources allowed for characters greater than
				a byte, but the implementation assumed the delimiter was only
				one byte. Vector now correctly errors if a delimiter that is
				greater than a byte is used. Only byte delimiters are allowed
				for efficiency in scanning.
				"""
			pr_numbers: [10570]
		},
	]

	commits: [
		{sha: "7296e12f10a97a2a18f42c02d96ebcb07669cb3d", date: "2021-11-18 09:04:27 UTC", description: "optimize cycle detection in the graph", pr_number:                                            10090, scopes: ["config"], type:                                      "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:        2, insertions_count:   84, deletions_count:   110},
		{sha: "664dac0ac13fb5bd8d69ac824f9c056f42857f57", date: "2021-11-18 15:52:34 UTC", description: "Add additional label so Vector service works in soak tests", pr_number:                       10094, scopes: [], type:                                              "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      1, insertions_count:   9, deletions_count:    5},
		{sha: "2bb47895c96e3175e2c6375c58d582e86d63acb5", date: "2021-11-19 01:53:40 UTC", description: "Add lading_image soak test variable", pr_number:                                              10097, scopes: [], type:                                              "chore", breaking_change:       false, author: "Will", files_count:                 35, insertions_count:  114, deletions_count:  6},
		{sha: "58124fd725fb3e6b9ec69384f6463f09c5edb11f", date: "2021-11-19 06:44:15 UTC", description: "Add end-to-end acknowledgement support", pr_number:                                           10045, scopes: ["aws_s3 source"], type:                               "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:        10, insertions_count:  182, deletions_count:  64},
		{sha: "914567b2f7ac6ccf25cbd67a9aefbd465425791f", date: "2021-11-19 08:11:06 UTC", description: "add `date` matcher function for `parse_groks` function", pr_number:                           9868, scopes: ["vrl"], type:                                          "enhancement", breaking_change: false, author: "Vladimir Zhuk", files_count:        8, insertions_count:   532, deletions_count:  3},
		{sha: "40fde8f69307b4165a2cd46a1de5fc93663ecf24", date: "2021-11-19 09:25:06 UTC", description: "fix some `parse_groks` issues", pr_number:                                                    10106, scopes: ["vrl"], type:                                         "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:        5, insertions_count:   101, deletions_count:  15},
		{sha: "71509714e9a529b200cf4806af0b44650ec8220f", date: "2021-11-19 10:29:10 UTC", description: "add `parse_groks` function with multiple grok patterns and aliases", pr_number:               9827, scopes: ["vrl"], type:                                          "feat", breaking_change:        false, author: "Vladimir Zhuk", files_count:        9, insertions_count:   520, deletions_count:  30},
		{sha: "38d76d3b7f0050ce5218f0c10c1a9e1bf32d246c", date: "2021-11-20 07:47:05 UTC", description: "Add filter on pipelines", pr_number:                                                          9984, scopes: ["pipelines"], type:                                    "feat", breaking_change:        false, author: "Jérémie Drouet", files_count:       11, insertions_count:  198, deletions_count:  25},
		{sha: "b8a5b7fb2860fb33cdf06bca16169e210d4d1c18", date: "2021-11-20 08:03:20 UTC", description: "Fix clippy lint in `BufferConfig::build`", pr_number:                                         10110, scopes: ["buffers"], type:                                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:   1, deletions_count:    0},
		{sha: "2b58db7bc2efbf1f9b73a26c2f746d97eb5932e8", date: "2021-11-20 09:52:53 UTC", description: "Clean unused images from soak machines", pr_number:                                           10105, scopes: [], type:                                              "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   1, insertions_count:   8, deletions_count:    2},
		{sha: "4b0f4dc687f37cc8e75d2b25d063888c4e98e55b", date: "2021-11-20 09:12:00 UTC", description: "Reduce unneeded string clones", pr_number:                                                    10109, scopes: ["influxdb_logs sink", "influxdb_metrics sink"], type: "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        4, insertions_count:   12, deletions_count:   12},
		{sha: "b2746709fccc371f20760c252dc3246778d3b520", date: "2021-11-23 08:46:44 UTC", description: "Fix more typos", pr_number:                                                                   10131, scopes: [], type:                                              "chore", breaking_change:       false, author: "Tshepang Lekhonkhobe", files_count: 15, insertions_count:  33, deletions_count:   33},
		{sha: "afaedf2a357ee96a99388446d48549ca5bc64fe7", date: "2021-11-23 03:21:20 UTC", description: "Differentiate between stream and invocation desyncs", pr_number:                              10085, scopes: ["kubernetes_logs source"], type:                      "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      1, insertions_count:   5, deletions_count:    5},
		{sha: "127f05b906bee566dc03ff256dbe07885d0eefab", date: "2021-11-23 06:09:08 UTC", description: "re-write the `Batcher` to remove the Partition requirement", pr_number:                       10096, scopes: ["core"], type:                                        "chore", breaking_change:       false, author: "Nathan Fox", files_count:           16, insertions_count:  975, deletions_count:  719},
		{sha: "3c2469cdf11d78214863ad3e0c457a78ef243590", date: "2021-11-24 02:02:58 UTC", description: "Upgrade lading", pr_number:                                                                   10141, scopes: [], type:                                              "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   54, insertions_count:  99, deletions_count:   200},
		{sha: "218dea161d714f3a349f304ca8026bfc03a3251a", date: "2021-11-24 04:40:47 UTC", description: "Refactor source acknowledgements config", pr_number:                                          10150, scopes: ["sources"], type:                                     "chore", breaking_change:       false, author: "Will", files_count:                 18, insertions_count:  127, deletions_count:  56},
		{sha: "7f8e41c026a4b24b81d21984766ffd423b1b38a8", date: "2021-11-24 02:13:21 UTC", description: "Have clippy catch debug statements", pr_number:                                               10125, scopes: ["dev"], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        31, insertions_count:  207, deletions_count:  44},
		{sha: "b4d2b8b5388d07d88b9f106a4599cc0df1a2d12f", date: "2021-11-24 08:57:01 UTC", description: "Implement indexer acknowledgements", pr_number:                                               10044, scopes: ["splunk_hec source"], type:                           "enhancement", breaking_change: false, author: "Will", files_count:                 6, insertions_count:   1207, deletions_count: 149},
		{sha: "23195227094bf5df40ce999d44256c0028f78a0c", date: "2021-11-24 10:43:41 UTC", description: "Correct splunk_hec source acknowledgements docs", pr_number:                                  10162, scopes: ["external docs"], type:                               "fix", breaking_change:         false, author: "Will", files_count:                 1, insertions_count:   36, deletions_count:   40},
		{sha: "5f9b930cbf5349f9d9c4d064c6f7c4d5ccd6928e", date: "2021-11-24 10:47:39 UTC", description: "Remove unnecessary debug print", pr_number:                                                   10161, scopes: ["dev"], type:                                         "fix", breaking_change:         false, author: "Will", files_count:                 1, insertions_count:   0, deletions_count:    1},
		{sha: "00777ee3aa06c96482715c85f18aaefcecc69465", date: "2021-11-24 16:09:21 UTC", description: "rewrite vector sink (v2) in the new style", pr_number:                                        10148, scopes: ["vector sink"], type:                                 "chore", breaking_change:       false, author: "Nathan Fox", files_count:           20, insertions_count:  1104, deletions_count: 807},
		{sha: "808c1e3106d7d138c6da84b93e72e94633d590a5", date: "2021-11-25 02:54:59 UTC", description: "Introduce a new http -> pipelines -> blackhole soak", pr_number:                              10142, scopes: [], type:                                              "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   13, insertions_count:  3722, deletions_count: 9},
		{sha: "17d86cc815785d63c3fcdc160c207b1fc9b03751", date: "2021-11-25 02:16:54 UTC", description: "Add community note to issue templates", pr_number:                                            10160, scopes: [], type:                                              "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        3, insertions_count:   27, deletions_count:   0},
		{sha: "b516b116c3ff9b744fad398b3c549a700feacfcf", date: "2021-11-25 11:42:53 UTC", description: "forbid nesting pipelines transform", pr_number:                                               10098, scopes: ["pipelines transform"], type:                         "feat", breaking_change:        false, author: "Jérémie Drouet", files_count:       4, insertions_count:   83, deletions_count:   3},
		{sha: "01fa760504d1b1a297b30cb8e64493def3e73eee", date: "2021-11-25 03:07:19 UTC", description: "bump algoliasearch-helper from 3.5.5 to 3.6.2 in /website", pr_number:                        10159, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:   3, deletions_count:    3},
		{sha: "ed8062f08dcbec986e4790e07e1ff236232c667a", date: "2021-11-26 04:34:02 UTC", description: "Allow multiline strings", pr_number:                                                          10149, scopes: ["vrl"], type:                                         "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:       3, insertions_count:   65, deletions_count:   22},
		{sha: "5e29e7756fb7f5f061386cc0b02059fe8bdd3a76", date: "2021-11-30 02:39:35 UTC", description: "bump docker/metadata-action from 3.6.0 to 3.6.1", pr_number:                                  10190, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "be5cfa1a2dcc6d44c6ea339e293543d2883c9be5", date: "2021-12-01 09:12:29 UTC", description: "Set up symmetric naming scheme (decoder/encoder, deserializer/serializer)", pr_number:        10087, scopes: ["codecs"], type:                                      "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        40, insertions_count:  575, deletions_count:  523},
		{sha: "1b1dc6cbad1fb823badfa0b39a6da7728a2436c1", date: "2021-12-01 01:44:27 UTC", description: "Improve soak aesthetics", pr_number:                                                          10184, scopes: [], type:                                              "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   44, insertions_count:  727, deletions_count:  312},
		{sha: "feedcbec051400c69cd22e1878983b4203fc79b7", date: "2021-12-01 10:57:42 UTC", description: "Rename TcpError -> StreamDecodingError", pr_number:                                           10114, scopes: ["codecs"], type:                                      "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        18, insertions_count:  38, deletions_count:   41},
		{sha: "d8b3cdd367a7716f7b888293a9479d7938e3ed00", date: "2021-12-01 02:05:43 UTC", description: "Remove use of `check_fields` from guides", pr_number:                                         10181, scopes: [], type:                                              "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:   6, deletions_count:    6},
		{sha: "181487bcf472534be71b33a1f39bfd37c0d7da10", date: "2021-12-01 20:44:59 UTC", description: "Add measurement option to replace namespace", pr_number:                                      10082, scopes: ["influxdb_logs sink"], type:                          "feat", breaking_change:        false, author: "Juvenn Woo", files_count:           2, insertions_count:   76, deletions_count:   40},
		{sha: "a56bddc7822a47d61f43999a245005ba4f1d3459", date: "2021-12-01 04:47:14 UTC", description: "Add note to v0.17.0 about `events_processed_total` going away", pr_number:                    10207, scopes: ["releasing"], type:                                   "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:   6, deletions_count:    1},
		{sha: "43770434b91b9ccf3eeaafad48c943f106c20c1b", date: "2021-12-01 06:53:43 UTC", description: "Rename `Failed` batch/event status to `Rejected`", pr_number:                                 10180, scopes: [], type:                                              "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        28, insertions_count:  62, deletions_count:   62},
		{sha: "99400f0b0ec7b06080aa6a5de6c0606069070582", date: "2021-12-01 05:43:48 UTC", description: "Remove unused alias for `events_processed_total`", pr_number:                                 10206, scopes: ["observability"], type:                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:   0, deletions_count:    9},
		{sha: "b4e2ae250d98cb24c8fe9fac8653249a183208ac", date: "2021-12-01 06:11:39 UTC", description: "Document `max_read_bytes`", pr_number:                                                        10210, scopes: ["kubernetes_logs source"], type:                      "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:   12, deletions_count:   1},
		{sha: "d7e0d313ed15b528e39d350315e2693b8c3381ce", date: "2021-12-01 09:36:16 UTC", description: "Add design goals to UX doc", pr_number:                                                       10183, scopes: ["internal docs"], type:                               "chore", breaking_change:       false, author: "Ben Johnson", files_count:          1, insertions_count:   58, deletions_count:   17},
		{sha: "e1dbd3a0ad5a97e3355e7c67e6a2c9bf02e2ce13", date: "2021-12-01 06:40:23 UTC", description: "Update component spec for aggregate pull-based sinks", pr_number:                             10211, scopes: [], type:                                              "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:   12, deletions_count:   4},
		{sha: "947bc20da8df5454faa5cad503236616b7543129", date: "2021-12-02 02:03:20 UTC", description: "automatic namespacing", pr_number:                                                            10179, scopes: ["pipelines transform"], type:                         "feat", breaking_change:        false, author: "Jérémie Drouet", files_count:       27, insertions_count:  494, deletions_count:  163},
		{sha: "f9d70e91351be94b10e6bc815e280a20d152e377", date: "2021-12-02 02:40:38 UTC", description: "Fix basic internal_logs example config", pr_number:                                           10219, scopes: [], type:                                              "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:      1, insertions_count:   1, deletions_count:    0},
		{sha: "72067bb8eee63ea058526dd7005332503306fbaa", date: "2021-12-02 00:28:39 UTC", description: "Read older files first", pr_number:                                                           10218, scopes: ["kubernetes_logs source"], type:                      "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        1, insertions_count:   3, deletions_count:    7},
		{sha: "2d867c4ad540361f515151a57d5623008e59ca5d", date: "2021-12-02 02:37:06 UTC", description: "Correct the name for the `headers_key` config option", pr_number:                             10222, scopes: ["kafka sink"], type:                                  "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        5, insertions_count:   18, deletions_count:   15},
		{sha: "ea572d512c9b03d131a25988eda90a8ed78913f1", date: "2021-12-02 11:57:25 UTC", description: "update component key", pr_number:                                                             10134, scopes: ["config"], type:                                      "feat", breaking_change:        false, author: "Jérémie Drouet", files_count:       12, insertions_count:  37, deletions_count:   73},
		{sha: "c9d1de762973451c2fde0592ee37f18ffdcd2596", date: "2021-12-02 04:16:36 UTC", description: "Revert read older files first (#10218)", pr_number:                                           10225, scopes: ["kubernetes_logs source"], type:                      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:   7, deletions_count:    3},
		{sha: "beb21445920259fe178bb454d950175bf0178f25", date: "2021-12-02 07:09:46 UTC", description: "Revert remove unused alias for `events_processed_total`", pr_number:                          10230, scopes: ["observability"], type:                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:   9, deletions_count:    0},
		{sha: "4cdbab18bb6d2e8f4ebaac79a353c8fc2bd41a40", date: "2021-12-02 09:28:06 UTC", description: "Add simple end-to-end acknowledgement full-system tests", pr_number:                          10204, scopes: ["tests"], type:                                       "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:   146, deletions_count:  0},
		{sha: "41505043aabbb2038d04d5a875ce3f6668be7b81", date: "2021-12-03 02:15:28 UTC", description: "Fix cue indentation for 0.18.1 release", pr_number:                                           10237, scopes: [], type:                                              "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      1, insertions_count:   22, deletions_count:   22},
		{sha: "4cb60b3d606b897d7382f23405dd1194a2aea6b6", date: "2021-12-03 14:26:15 UTC", description: "fix enrichment anchor in highlight post", pr_number:                                          10233, scopes: [], type:                                              "docs", breaking_change:        false, author: "Hoàng Đức Hiếu", files_count:       1, insertions_count:   1, deletions_count:    1},
		{sha: "db382c8d4213cb062cc681efe91b1e527d11cbe1", date: "2021-12-03 08:48:15 UTC", description: "lower `CARGO_BUILD_JOBS` for Windows runner", pr_number:                                      10235, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Pierre Rognant", files_count:       1, insertions_count:   1, deletions_count:    0},
		{sha: "ca93a53ab3916e26fd4497d03c8d79204ca26cb5", date: "2021-12-03 00:35:12 UTC", description: "Fix 0.17 note about processed_events_total metric", pr_number:                                10229, scopes: ["observability"], type:                               "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:   7, deletions_count:    5},
		{sha: "0dc1a06d73cec866c48835a8b77df0982d408571", date: "2021-12-03 05:52:28 UTC", description: "Fix title for 0.18.1 release page", pr_number:                                                10243, scopes: [], type:                                              "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "31883416edcc58a8bb3ffda3013e806d3012ff77", date: "2021-12-03 02:53:06 UTC", description: "Remove references to playground.vector.dev", pr_number:                                       10242, scopes: [], type:                                              "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        5, insertions_count:   21, deletions_count:   22},
		{sha: "05f8e4f2e192a9af72483317040a947f20998f4c", date: "2021-12-03 07:17:12 UTC", description: "Add multiple favicons", pr_number:                                                            10156, scopes: ["external docs"], type:                               "enhancement", breaking_change: false, author: "Luc Perkins", files_count:          6, insertions_count:   20, deletions_count:   3},
		{sha: "be22f084652f2d25f77fed08a2b0397a88ae7d4f", date: "2021-12-03 04:37:18 UTC", description: "Allow any 2xx code as success", pr_number:                                                    10224, scopes: ["loki sink"], type:                                   "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:   5, deletions_count:    4},
		{sha: "9a3881fad639bae889c87052fc5318586b17d9bb", date: "2021-12-03 06:03:51 UTC", description: "Read read older files first", pr_number:                                                      10226, scopes: ["kubernetes_logs source"], type:                      "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        1, insertions_count:   3, deletions_count:    7},
		{sha: "d216b53008c7431fb813c7c6e21344b80291c530", date: "2021-12-03 06:06:50 UTC", description: "Re-add processed_events_total", pr_number:                                                    10228, scopes: ["observability"], type:                               "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        3, insertions_count:   3, deletions_count:    14},
		{sha: "d085e7f00b190481a3bd731c26d5bcfab14e2bf1", date: "2021-12-03 07:57:40 UTC", description: "Expand allowed environment variable name characters", pr_number:                              10194, scopes: ["config"], type:                                      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:   18, deletions_count:   1},
		{sha: "81df6cbe868eedaa0a1e7f437d28d84278813f18", date: "2021-12-04 02:32:44 UTC", description: "metric support in `datadog_agent` source", pr_number:                                         9563, scopes: ["datadog_agent source"], type:                         "feat", breaking_change:        false, author: "Pierre Rognant", files_count:       9, insertions_count:   725, deletions_count:  88},
		{sha: "be22095b38da95ca7c5ab42766ad0c83833f387f", date: "2021-12-04 03:10:57 UTC", description: "add support for sub-milliseconds timestamps", pr_number:                                      10216, scopes: ["humio_logs sink"], type:                             "enhancement", breaking_change: false, author: "Pierre Rognant", files_count:       7, insertions_count:   50, deletions_count:   20},
		{sha: "2d66a10a43ad0b4f64765f705991ed25ff304fea", date: "2021-12-04 07:22:35 UTC", description: "set CARGO_BUILD_JOBS to half the number of core for Windows runner", pr_number:               10250, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Pierre Rognant", files_count:       1, insertions_count:   2, deletions_count:    1},
		{sha: "c83461efec115dd269653f0adc4a9be4bb26fc8f", date: "2021-12-04 07:32:54 UTC", description: "add missing \"site\" documentation", pr_number:                                               10251, scopes: ["datadog_metrics sinks"], type:                       "docs", breaking_change:        false, author: "Jean Mertz", files_count:           1, insertions_count:   1, deletions_count:    0},
		{sha: "b646614333d23285405dd09613561d997cc0928b", date: "2021-12-04 03:35:16 UTC", description: "Add acknowledgements support to `TcpSource`", pr_number:                                      10176, scopes: ["sources"], type:                                     "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:        14, insertions_count:  707, deletions_count:  428},
		{sha: "2aaafa0a69792beafd2b46c101da0ee67b054db6", date: "2021-12-04 08:43:47 UTC", description: "Implement indexer acknowledgements", pr_number:                                               10135, scopes: ["splunk_hec sink"], type:                             "enhancement", breaking_change: false, author: "Will", files_count:                 18, insertions_count:  1135, deletions_count: 155},
		{sha: "27507eba5eb8432d3b533e75091d8cb6887bdd10", date: "2021-12-04 07:08:19 UTC", description: "bump actions/cache from 2.1.6 to 2.1.7", pr_number:                                           10154, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:   9, deletions_count:    9},
		{sha: "f816882b4a28cd8cfd6f55f7c6bf63a6fbb8b226", date: "2021-12-04 15:10:55 UTC", description: "bump serde-toml-merge from 0.2.0 to 0.3.0", pr_number:                                        10260, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "d6ace26a214f61f33b5063f45333e9765befd2f3", date: "2021-12-04 16:20:03 UTC", description: "bump tokio-util from 0.6.8 to 0.6.9", pr_number:                                              9938, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   2, deletions_count:    3},
		{sha: "1c2f12d19b399a6165a3f002f629cc47311cdca1", date: "2021-12-04 17:20:40 UTC", description: "bump ndarray from 0.15.3 to 0.15.4", pr_number:                                               10263, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   5, deletions_count:    5},
		{sha: "2b59e41d0f151d9791b1ed10b6867d8118a1abdc", date: "2021-12-04 18:54:29 UTC", description: "bump mlua from 0.6.6 to 0.7.0", pr_number:                                                    10266, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   7, deletions_count:    6},
		{sha: "b310076e6de6df0539cc11b4599304152468b270", date: "2021-12-05 10:48:06 UTC", description: "Improve performance of `match_datadog_query`", pr_number:                                     10189, scopes: ["vrl"], type:                                         "enhancement", breaking_change: false, author: "Lee Benson", files_count:           10, insertions_count:  618, deletions_count:  356},
		{sha: "48adc823aeabbdbca09e979ec5d56700fc49d551", date: "2021-12-05 04:31:41 UTC", description: "bump typetag from 0.1.7 to 0.1.8", pr_number:                                                 10267, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   19, deletions_count:   9},
		{sha: "70da9f5a72a4ac4c886f3597c87fe6af526c6aa5", date: "2021-12-05 04:33:12 UTC", description: "bump weak-table from 0.3.0 to 0.3.2", pr_number:                                              10262, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "2d125eb0cc14a85b1572adb34c784793e95a7046", date: "2021-12-05 04:34:48 UTC", description: "bump tonic from 0.5.2 to 0.6.1", pr_number:                                                   9847, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:   155, deletions_count:  43},
		{sha: "49e3f83380d233588f819633585167444b4a499f", date: "2021-12-05 14:47:37 UTC", description: "bump peeking_take_while from 0.1.2 to 1.0.0", pr_number:                                      10272, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   9, deletions_count:    3},
		{sha: "834fd1d9c4b5632a63488afd0b54317b3b8df2fe", date: "2021-12-05 08:33:40 UTC", description: "bump strum_macros from 0.22.0 to 0.23.1", pr_number:                                          10264, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   5, deletions_count:    4},
		{sha: "ed63b90a9054867eb6398df8de6559cb70c19a30", date: "2021-12-05 17:59:20 UTC", description: "bump mongodb from 2.0.1 to 2.0.2", pr_number:                                                 10271, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "70d1c5ab3addffde2e065b26b44331f9b802363f", date: "2021-12-07 05:35:38 UTC", description: "optimize `parse_groks` performance", pr_number:                                               10202, scopes: ["vrl"], type:                                         "enhancement", breaking_change: false, author: "Vladimir Zhuk", files_count:        3, insertions_count:   156, deletions_count:  71},
		{sha: "77e9498c4384d4ff6bb5ccb5133a22a32eb11f53", date: "2021-12-06 23:18:58 UTC", description: "bump anyhow from 1.0.45 to 1.0.51", pr_number:                                                10277, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "569994bef397b591600e2c9ab6247730895b3154", date: "2021-12-06 23:19:18 UTC", description: "bump getset from 0.1.1 to 0.1.2", pr_number:                                                  10276, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   4, deletions_count:    4},
		{sha: "e818143939ea677170993dee2886bfbfb22627de", date: "2021-12-07 08:03:42 UTC", description: "added get and set event_metadata vrl functions", pr_number:                                   10198, scopes: ["vrl"], type:                                         "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:       19, insertions_count:  430, deletions_count:  57},
		{sha: "2b324e51d0743f27e9a02f8f6acd51b04c1c67fd", date: "2021-12-07 02:12:42 UTC", description: "Investigate impact of end-to-end acknowledgements", pr_number:                                10255, scopes: ["performance"], type:                                 "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        10, insertions_count:  268, deletions_count:  1},
		{sha: "19d2acd6fd6e2ce7e4618de22f785e39370cb3d5", date: "2021-12-07 03:45:21 UTC", description: "new disk buffer v2 implementation", pr_number:                                                10143, scopes: ["buffers"], type:                                     "chore", breaking_change:       false, author: "Toby Lawrence", files_count:        31, insertions_count:  5424, deletions_count: 618},
		{sha: "9e73e6dd009243b1c107e30a0ee38734bf4978a8", date: "2021-12-07 00:59:37 UTC", description: "Document VRL metric transform limitations", pr_number:                                        10241, scopes: ["external docs"], type:                               "enhancement", breaking_change: false, author: "Luc Perkins", files_count:          2, insertions_count:   28, deletions_count:   9},
		{sha: "c9e57fa3b11ce5214fcd6f7c78c9dd036e5dd4d1", date: "2021-12-07 01:00:13 UTC", description: "Multiple condition types", pr_number:                                                         10157, scopes: ["external docs"], type:                               "enhancement", breaking_change: false, author: "Luc Perkins", files_count:          7, insertions_count:   138, deletions_count:  35},
		{sha: "448aeb3b58c6a5f2b3e8fe6767a324099be62c60", date: "2021-12-07 01:11:34 UTC", description: "bump docker/metadata-action from 3.6.1 to 3.6.2", pr_number:                                  10295, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "b10ca188e4c49dc21135167906594683538ba7df", date: "2021-12-07 10:41:14 UTC", description: "bump EmbarkStudios/cargo-deny-action from 1.2.6 to 1.2.9", pr_number:                         10294, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:   2, deletions_count:    2},
		{sha: "696d70745df83e44700bfa0b05b58f35fc03a7cc", date: "2021-12-07 03:03:01 UTC", description: "bump memmap2 from 0.3.1 to 0.5.0", pr_number:                                                 10293, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   4, deletions_count:    4},
		{sha: "4055ec1247e9e2d3e2c0490715e931a53bc1e43b", date: "2021-12-07 09:58:31 UTC", description: "Added a guide article on how to use enrichment tables with two example use cases", pr_number: 10124, scopes: ["enriching"], type:                                   "docs", breaking_change:        false, author: "Barry Eom", files_count:            1, insertions_count:   217, deletions_count:  0},
		{sha: "5d0cddc76b556f324c138958c9df9a31036dbf1c", date: "2021-12-07 07:11:40 UTC", description: "Upgrade to Rust 1.57", pr_number:                                                             10246, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        39, insertions_count:  56, deletions_count:   89},
		{sha: "4719b476aa21902531dfd51854ef119c4270ca90", date: "2021-12-07 07:11:57 UTC", description: "Bump allowed dependabot PRs", pr_number:                                                      10305, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:   1, deletions_count:    0},
		{sha: "ee941dc6e47702cbb7192232abbf1c324abf333d", date: "2021-12-07 07:12:45 UTC", description: "bump syslog from 5.0.0 to 6.0.0", pr_number:                                                  10289, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   6, deletions_count:    6},
		{sha: "0ba572749b19de53e30107526bc7134cb92d4f3e", date: "2021-12-07 22:51:32 UTC", description: "Revert upgrade to Rust 1.57", pr_number:                                                      10315, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        39, insertions_count:  89, deletions_count:   56},
		{sha: "6e469a89e1aa93948172492e384babeb4491e83e", date: "2021-12-08 17:54:08 UTC", description: "Allow to exclude `metric_type` tag", pr_number:                                               10217, scopes: ["influxdb_logs sink"], type:                          "chore", breaking_change:       false, author: "Juvenn Woo", files_count:           7, insertions_count:   61, deletions_count:   30},
		{sha: "283689960a978efaa2d70ad761b506da6463752f", date: "2021-12-08 10:34:12 UTC", description: "bump aws-config from 0.0.22-alpha to 0.2.0", pr_number:                                       10311, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   45, deletions_count:   42},
		{sha: "c0db1927e262ac13e4648bb90e8a448bd4dfa405", date: "2021-12-08 03:50:45 UTC", description: "Avoid dinging PRs on erratic soaks", pr_number:                                               10323, scopes: [], type:                                              "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   3, insertions_count:   24, deletions_count:   11},
		{sha: "0f67fd6a7ece4a9db3d858935d9d111036f34ff4", date: "2021-12-08 07:10:17 UTC", description: "try and make disk_v2 tests more reliable", pr_number:                                         10325, scopes: ["buffers"], type:                                     "chore", breaking_change:       false, author: "Toby Lawrence", files_count:        3, insertions_count:   39, deletions_count:   30},
		{sha: "03271829434474c3a4eb301796be7473091c61f6", date: "2021-12-08 07:15:57 UTC", description: "Add Under the Hood section on End-to-End Acknowledgements", pr_number:                        10306, scopes: ["architecture"], type:                                "docs", breaking_change:        false, author: "Bruce Guenter", files_count:        2, insertions_count:   19, deletions_count:   0},
		{sha: "a77837ea4f26463b0535344834c31e8c24353c96", date: "2021-12-08 15:35:54 UTC", description: "bump bollard from 0.11.0 to 0.11.1", pr_number:                                               10309, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "caebaf3fe790a2e7093324437782f05b90d3b9e4", date: "2021-12-09 01:50:32 UTC", description: "add VRL VM RFC", pr_number:                                                                   10011, scopes: [], type:                                              "chore", breaking_change:       false, author: "Stephen Wakely", files_count:       1, insertions_count:   347, deletions_count:  0},
		{sha: "222e8d1c204c50c6f12740c5b3f9bcd28031b0c2", date: "2021-12-08 22:53:52 UTC", description: "bump itertools from 0.10.1 to 0.10.3", pr_number:                                             10338, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:   5, deletions_count:    5},
		{sha: "96be37ea94b7490a749d47991f6bcbfa2df92279", date: "2021-12-08 23:49:06 UTC", description: "Upgrade to Rust 1.57", pr_number:                                                             10316, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        39, insertions_count:  66, deletions_count:   91},
		{sha: "acefebda59ec4e7e7f9980ecf05f8e6b751d1ad3", date: "2021-12-09 08:32:31 UTC", description: "bump chrono-tz from 0.6.0 to 0.6.1", pr_number:                                               10343, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   8, deletions_count:    18},
		{sha: "ca4d28f1fdc3eb7cf70d515c4030470776480aa8", date: "2021-12-09 00:58:14 UTC", description: "Implement Splunk HEC token passthrough routing", pr_number:                                   10261, scopes: ["splunk_hec source"], type:                           "enhancement", breaking_change: false, author: "Will", files_count:                 31, insertions_count:  498, deletions_count:  110},
		{sha: "6710af176c09e542545d436c1f7d74e9c72afbf2", date: "2021-12-09 01:00:17 UTC", description: "bump libc from 0.2.108 to 0.2.109", pr_number:                                                10335, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "d20361b689eb1afa8ed08da346f945d412234ae7", date: "2021-12-09 01:00:31 UTC", description: "bump sha2 from 0.9.8 to 0.10.0", pr_number:                                                   10336, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   48, deletions_count:   8},
		{sha: "fa36a00923b713121e2da53575ab4ee823a880ed", date: "2021-12-09 01:00:50 UTC", description: "bump md-5 from 0.9.1 to 0.10.0", pr_number:                                                   10337, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   16, deletions_count:   7},
		{sha: "1513cbe9dcceaa25685a2fead1197f2d23e6f3e6", date: "2021-12-09 10:14:56 UTC", description: "bump encoding_rs from 0.8.29 to 0.8.30", pr_number:                                           10348, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "ca4f68e148140a8503eac8f5266ff7a2ba27dffc", date: "2021-12-09 11:08:48 UTC", description: "bump tonic from 0.6.1 to 0.6.2", pr_number:                                                   10352, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:   3, deletions_count:    3},
		{sha: "96128a02490cae1c95e3916a73b9891578d63d88", date: "2021-12-09 11:27:41 UTC", description: "bump sha-1 from 0.9.8 to 0.10.0", pr_number:                                                  10354, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   13, deletions_count:   2},
		{sha: "3ffbc3786864df713ebfc66c68f4fba1ee67c601", date: "2021-12-09 12:43:51 UTC", description: "bump argh from 0.1.6 to 0.1.7", pr_number:                                                    10359, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:   6, deletions_count:    6},
		{sha: "566e3c0973c58ae10ea35d55b90aad802d4957f5", date: "2021-12-09 13:02:13 UTC", description: "bump rkyv from 0.7.25 to 0.7.26", pr_number:                                                  10358, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   5, deletions_count:    5},
		{sha: "a23defec1698e8bb5209ba63cd9cd7db91a8aab6", date: "2021-12-09 08:09:45 UTC", description: "try and fix another flaky assertion in disk v2 tests", pr_number:                             10360, scopes: ["buffers"], type:                                     "fix", breaking_change:         false, author: "Toby Lawrence", files_count:        1, insertions_count:   3, deletions_count:    3},
		{sha: "071ec4d544ef861f4035c9dfc000db1a8ef3d428", date: "2021-12-09 05:23:38 UTC", description: "Fix k8s end-to-end test metric detection", pr_number:                                         10227, scopes: ["ci"], type:                                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        2, insertions_count:   31, deletions_count:   28},
		{sha: "7c31189d78423de204d8faf294ebbcb554371a6e", date: "2021-12-09 05:41:53 UTC", description: "Patch dependencies to remove dependency on `time` v0.1", pr_number:                           9678, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        3, insertions_count:   10, deletions_count:   20},
		{sha: "877d22090ff56b0e79e0649ab00c3771fb52c1de", date: "2021-12-09 05:59:24 UTC", description: "Clarify that `/etc/vector` just includes configuration", pr_number:                           10361, scopes: [], type:                                              "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:   3, deletions_count:    0},
		{sha: "8c913b216c7f7415638e8394384a7f7838d37062", date: "2021-12-09 08:39:34 UTC", description: "Shrink the visible soak test boilerplate explanation", pr_number:                             10331, scopes: [], type:                                              "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:   30, deletions_count:   24},
		{sha: "0404ca0803f7cc228db7e7a9e9dfb8e891f71816", date: "2021-12-09 12:06:20 UTC", description: "Support histogram deduplication in statsd sink", pr_number:                                   10279, scopes: ["statsd sink"], type:                                 "enhancement", breaking_change: false, author: "Chin-Ying Li", files_count:         2, insertions_count:   17, deletions_count:   4},
		{sha: "9e6ac88fc091bd33a3f3265a1b8285220eff273e", date: "2021-12-10 02:55:50 UTC", description: "Move `datadog_search` condition outside of VRL", pr_number:                                   10341, scopes: ["filtering"], type:                                   "enhancement", breaking_change: false, author: "Lee Benson", files_count:           15, insertions_count:  1130, deletions_count: 746},
		{sha: "3644b32bf4e1800d5d408395d08042e303f13348", date: "2021-12-10 04:23:31 UTC", description: "bump tonic-build from 0.6.0 to 0.6.2", pr_number:                                             10353, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:   3, deletions_count:    3},
		{sha: "45d7c613bf4132e5b1a6bb292e6b00a51ca52744", date: "2021-12-09 23:26:45 UTC", description: "bump serde from 1.0.130 to 1.0.131", pr_number:                                               10367, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      8, insertions_count:   11, deletions_count:   11},
		{sha: "086dd5d6f357003a4a100efa4e826b4d3617061b", date: "2021-12-10 01:37:16 UTC", description: "bump async-trait from 0.1.51 to 0.1.52", pr_number:                                           10373, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "adcdff301d0c148fb44e763279b7e8e79cae62ad", date: "2021-12-10 05:58:49 UTC", description: "run transforms concurrently", pr_number:                                                      10265, scopes: ["topology"], type:                                    "enhancement", breaking_change: false, author: "Luke Steensen", files_count:        26, insertions_count:  684, deletions_count:  372},
		{sha: "ca9c89dfd805e51957b72689a5e43bd6c4144dd1", date: "2021-12-10 07:04:25 UTC", description: "Fix whitespace in highlight guide for 0.19", pr_number:                                       10383, scopes: [], type:                                              "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:   2, deletions_count:    1},
		{sha: "b718a41305f36fa2c8a7bf1f92b4f68100884993", date: "2021-12-11 05:06:24 UTC", description: "Fix description of tls options", pr_number:                                                   10394, scopes: [], type:                                              "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "e6abae33b3cb3fee115f0664b83b0f0e4a3b8e8a", date: "2021-12-11 05:30:36 UTC", description: "Fix example for filter transform", pr_number:                                                 10395, scopes: [], type:                                              "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "e8ca9b55086749c6736f07ab1fa7cabd72261bc6", date: "2021-12-11 07:59:55 UTC", description: "Add suppress_type_name to allow for ESv8 compatibility", pr_number:                           10357, scopes: ["elasticsearch sink"], type:                          "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:      4, insertions_count:   137, deletions_count:  4},
		{sha: "7fedbd196d2187314617696641c2ce0c511700f0", date: "2021-12-11 07:24:03 UTC", description: "Fix `use` warnings in end-to-end acknowledgement tests", pr_number:                           10382, scopes: [], type:                                              "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:   10, deletions_count:   10},
		{sha: "12fb942add6263d4ececfee6266d4dbcc630a8eb", date: "2021-12-11 05:24:54 UTC", description: "Update splunk_hec sink instrumentation", pr_number:                                           10350, scopes: ["splunk_hec sink"], type:                             "chore", breaking_change:       false, author: "Will", files_count:                 8, insertions_count:   45, deletions_count:   37},
		{sha: "587baaac58ff97f078bea4f58d62a257fb5fe741", date: "2021-12-11 14:14:57 UTC", description: "bump hyper from 0.14.15 to 0.14.16", pr_number:                                               10387, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   5, deletions_count:    5},
		{sha: "0e00cb9b0fc58d53518f20b85bcba7a32172faec", date: "2021-12-12 04:23:08 UTC", description: "re-write aws cloudwatch logs sink in new style", pr_number:                                   10355, scopes: ["aws_cloudwatch_logs sink"], type:                    "chore", breaking_change:       false, author: "Nathan Fox", files_count:           11, insertions_count:  1341, deletions_count: 1275},
		{sha: "4a771da9b0eb9cc0e04fc838d828ba0bc499fcd2", date: "2021-12-14 02:38:44 UTC", description: "create a smoke test for the metrics sink", pr_number:                                         10376, scopes: ["datadog_metrics sink"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       3, insertions_count:   157, deletions_count:  0},
		{sha: "4371d2c4bfba8362709703607d5180109063141f", date: "2021-12-14 06:44:09 UTC", description: "add `keyvalue` filter to `parse_groks` function", pr_number:                                  10329, scopes: ["vrl"], type:                                         "enhancement", breaking_change: false, author: "Vladimir Zhuk", files_count:        7, insertions_count:   673, deletions_count:  2},
		{sha: "816d42c1e6f5718c531a12e2a71a11b6e6e8806b", date: "2021-12-14 01:08:12 UTC", description: "rewrite blackhole sink in the new style", pr_number:                                          10396, scopes: ["blackhole sink"], type:                              "chore", breaking_change:       false, author: "Nathan Fox", files_count:           3, insertions_count:   105, deletions_count:  89},
		{sha: "5bdfcb0c39b6b55b5c844b5f53de1613c7c22ae3", date: "2021-12-14 08:08:29 UTC", description: "add `array` filter to `parse_groks` function", pr_number:                                     10252, scopes: ["vrl"], type:                                         "enhancement", breaking_change: false, author: "Vladimir Zhuk", files_count:        8, insertions_count:   392, deletions_count:  94},
		{sha: "ae068a438f55e287aac919d192726e0ab3c6a718", date: "2021-12-14 03:17:32 UTC", description: "re-write the console sink in the new style", pr_number:                                       10377, scopes: ["console sink"], type:                                "chore", breaking_change:       false, author: "Nathan Fox", files_count:           7, insertions_count:   265, deletions_count:  266},
		{sha: "5021ca3feea76e59c28ce9eb999ce8112d801739", date: "2021-12-14 00:29:13 UTC", description: "bump dashmap from 4.0.2 to 5.0.0", pr_number:                                                 10406, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:   19, deletions_count:   8},
		{sha: "6198fc62be1fda645b9d6d15c0cfd71a9d07aa67", date: "2021-12-14 00:30:08 UTC", description: "bump libc from 0.2.109 to 0.2.111", pr_number:                                                10408, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "e24a88c944d98dd803d501a2ce794304700f888c", date: "2021-12-14 08:30:26 UTC", description: "Add HTTP -> Pipelines (no grok) -> Blackhole soak", pr_number:                                10415, scopes: ["tests"], type:                                       "chore", breaking_change:       false, author: "Lee Benson", files_count:           7, insertions_count:   687, deletions_count:  1},
		{sha: "f74805c3f1ededd730b2a98f111ec66873e74f52", date: "2021-12-14 00:33:56 UTC", description: "bump serde_yaml from 0.8.21 to 0.8.23", pr_number:                                            10417, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   4, deletions_count:    4},
		{sha: "50f1372171228e2dc57f40d84b7c17da469d04e3", date: "2021-12-14 05:36:47 UTC", description: "switch over to the new buffer topology builder + v1/v2 buffer type symmetry", pr_number:      10379, scopes: ["buffers"], type:                                     "enhancement", breaking_change: false, author: "Toby Lawrence", files_count:        83, insertions_count:  3623, deletions_count: 2663},
		{sha: "962b3fd79445951a914af20e7922e6eb6f0454d8", date: "2021-12-14 05:21:23 UTC", description: "Add deprecation policy", pr_number:                                                           10366, scopes: ["releasing"], type:                                   "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:   89, deletions_count:   0},
		{sha: "1fc2d18ee08b92a7a33aefb41a73aa52921b99e9", date: "2021-12-15 02:30:35 UTC", description: "Add `http_datadog_filter_blackhole` soak", pr_number:                                         10419, scopes: ["tests"], type:                                       "chore", breaking_change:       false, author: "Lee Benson", files_count:           6, insertions_count:   563, deletions_count:  0},
		{sha: "6a60510bb42f7ece1928441b14c2474bb6e6c031", date: "2021-12-15 10:58:25 UTC", description: "Bump `async-graphql` to 3.0.15", pr_number:                                                   10441, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "Lee Benson", files_count:           9, insertions_count:   2233, deletions_count: 2220},
		{sha: "c058081030bcc247e1e426cfcf7b4bdd799a1342", date: "2021-12-15 03:35:14 UTC", description: "Associate pid and hostname with all logs", pr_number:                                         10425, scopes: ["internal_logs source"], type:                        "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:   22, deletions_count:   24},
		{sha: "bdce89e83faebb4a1f64a2473c256685d2c09bc1", date: "2021-12-15 12:15:10 UTC", description: "bump once_cell from 1.8.0 to 1.9.0", pr_number:                                               10448, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:   5, deletions_count:    5},
		{sha: "4897bf3e59923ffb4490930ab43c0bdbd77fab79", date: "2021-12-15 07:05:03 UTC", description: "Avoid unnecessary cargo check/clippy rebuilds", pr_number:                                    10450, scopes: ["dev"], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:   1, deletions_count:    1},
		{sha: "5fe1767d3a0b847cfacc855b44164b03574d51a8", date: "2021-12-15 05:25:05 UTC", description: "Consider jemalloc again", pr_number:                                                          10443, scopes: ["performance"], type:                                 "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        5, insertions_count:   39, deletions_count:   1},
		{sha: "e61ff35c0b35ec08bb0e8067eede4892539bf3e7", date: "2021-12-15 13:39:32 UTC", description: "bump metrics-util from 0.10.1 to 0.10.2", pr_number:                                          10407, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:   28, deletions_count:   115},
		{sha: "2d5ceb24d59cdcb697ff4f6084f2c6cf9d306d1c", date: "2021-12-15 13:40:36 UTC", description: "bump mongodb from 2.0.2 to 2.1.0", pr_number:                                                 10452, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   6, deletions_count:    5},
		{sha: "37bf8eb0e6794f797cc3e34bb33c76644ff2092f", date: "2021-12-15 05:49:50 UTC", description: "Add soak tests related to Splunk HEC indexer acknowledgement ", pr_number:                    10365, scopes: [], type:                                              "chore", breaking_change:       false, author: "Will", files_count:                 16, insertions_count:  401, deletions_count:  0},
		{sha: "90489c28a2c6e3df5bf020ddf8c607282f5a4dec", date: "2021-12-15 06:38:17 UTC", description: "Reduce CI cargo jobs to nproc / 2", pr_number:                                                10453, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:   2, deletions_count:    0},
		{sha: "933858445373381f53ddf4c901498a0e019a123e", date: "2021-12-15 07:22:12 UTC", description: "Pin prometheus to 2.31.0", pr_number:                                                         10457, scopes: ["ci"], type:                                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:   1, deletions_count:    1},
		{sha: "34118c72e59d676dee37fee62920f9dfdb4dfbae", date: "2021-12-15 07:38:34 UTC", description: "Add note about health checks to component spec", pr_number:                                   10431, scopes: ["internal docs"], type:                               "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:   16, deletions_count:   0},
		{sha: "fb307d99b1462c7a63faf95d486acbd689afb164", date: "2021-12-15 08:26:54 UTC", description: "Revert jemalloc (#10443)", pr_number:                                                         10460, scopes: ["performance"], type:                                 "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        5, insertions_count:   1, deletions_count:    39},
		{sha: "ed70c1b0ae233730e3bac7621ab07be61e2b3d63", date: "2021-12-15 17:59:54 UTC", description: "bump libc from 0.2.111 to 0.2.112", pr_number:                                                10438, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "faa845d5ea532f25762d0612632f107c54eb50f2", date: "2021-12-15 18:04:15 UTC", description: "bump serde_json from 1.0.72 to 1.0.73", pr_number:                                            10437, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      8, insertions_count:   27, deletions_count:   21},
		{sha: "556a60e367070a9f1958e7eecf6cf95325282e22", date: "2021-12-15 11:18:08 UTC", description: "Switch jemalloc crate to tikv_jemallocator", pr_number:                                       10459, scopes: ["performance"], type:                                 "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        5, insertions_count:   42, deletions_count:   1},
		{sha: "cec99e1b73c1f264f81d249a2b6f5212eaeaa9e1", date: "2021-12-16 06:03:03 UTC", description: "APM stats support in Datadog source", pr_number:                                              9900, scopes: ["rfc"], type:                                          "chore", breaking_change:       false, author: "Pierre Rognant", files_count:       1, insertions_count:   313, deletions_count:  0},
		{sha: "0221b97cdc55f2ef7be62110aed4886e58729d8e", date: "2021-12-16 03:35:10 UTC", description: "Refactor soak repliacs", pr_number:                                                           10470, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:   11, deletions_count:   86},
		{sha: "405f4a464db1ce386ba66ce7582a0d4e47c1d5d9", date: "2021-12-16 07:02:24 UTC", description: "update buffer metrics correctly + disk_v2 warning", pr_number:                                10461, scopes: ["buffers"], type:                                     "fix", breaking_change:         false, author: "Toby Lawrence", files_count:        25, insertions_count:  158, deletions_count:  382},
		{sha: "691b6050ca616e61418c854f54f4bb2ae96aaf33", date: "2021-12-16 04:19:45 UTC", description: "Bump soak test Rust versions", pr_number:                                                     10472, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:   2, deletions_count:    2},
		{sha: "65629829f8deaebbd2c1fad31b6bcdfe8bcee595", date: "2021-12-16 06:28:52 UTC", description: "bump openssl from 0.10.36 to 0.10.38", pr_number:                                             10308, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   5, deletions_count:    5},
		{sha: "cc67dceec1021baa0913af4efff924999a49fb98", date: "2021-12-16 06:56:41 UTC", description: "More soak refactoring to leverage matrices", pr_number:                                       10474, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:   71, deletions_count:   120},
		{sha: "717dcacb3fc642a25df92d39ca8bd5e9ba47bc7c", date: "2021-12-17 08:12:54 UTC", description: "Datadog traces (from trace-agent) early support", pr_number:                                  9634, scopes: ["rfc"], type:                                          "chore", breaking_change:       false, author: "Pierre Rognant", files_count:       1, insertions_count:   220, deletions_count:  0},
		{sha: "190d453d8997af10e384af6a82139b834de0b5ae", date: "2021-12-16 23:36:25 UTC", description: "bump aws-config from 0.2.0 to 0.3.0", pr_number:                                              10476, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   40, deletions_count:   63},
		{sha: "9e57bf1a3de3b6e5b68bc18079c257cdde9fe46f", date: "2021-12-17 01:43:18 UTC", description: "bump tokio from 1.14.0 to 1.15.0", pr_number:                                                 10475, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      9, insertions_count:   13, deletions_count:   14},
		{sha: "7b79a74199998562c7f1e6abc2ea511d72dfadd7", date: "2021-12-17 01:43:38 UTC", description: "bump async-graphql-warp from 3.0.15 to 3.0.16", pr_number:                                    10486, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "ff8b6ad5b620375727fcb14c7b079739be9a8fa6", date: "2021-12-17 01:45:40 UTC", description: "bump async-graphql from 3.0.15 to 3.0.16", pr_number:                                         10487, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   4, deletions_count:    4},
		{sha: "351f48698dcf6611b5d1ae4a1e75ae653fd8ebb0", date: "2021-12-17 06:36:50 UTC", description: "improve disk v2 buffer tests + fix some bugs", pr_number:                                     10479, scopes: ["buffers"], type:                                     "fix", breaking_change:         false, author: "Toby Lawrence", files_count:        21, insertions_count:  1078, deletions_count: 393},
		{sha: "094158890c1863fe02c27e57f465fc4d48decbbf", date: "2021-12-17 12:58:20 UTC", description: "bump serde from 1.0.131 to 1.0.132", pr_number:                                               10492, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      8, insertions_count:   11, deletions_count:   11},
		{sha: "e6122ce88b202908f76be1f32e6c898b624d7825", date: "2021-12-17 07:23:27 UTC", description: "Add mirror soak tests with acknowledgements enabled", pr_number:                              10402, scopes: ["performance"], type:                                 "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        14, insertions_count:  3974, deletions_count: 0},
		{sha: "598d73e2904e7828c014652302cd534282c849fb", date: "2021-12-17 06:22:09 UTC", description: "Use inner service and correctly apply request settings", pr_number:                           10449, scopes: ["splunk_hec sink"], type:                             "chore", breaking_change:       false, author: "Will", files_count:                 7, insertions_count:   143, deletions_count:  122},
		{sha: "bd8f097e82f8e3f1c30a4ed28e86c3e6ec97fc00", date: "2021-12-17 07:59:58 UTC", description: "reformatting imports", pr_number:                                                             10496, scopes: [], type:                                              "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        801, insertions_count: 6210, deletions_count: 4628},
		{sha: "db5da0a49a3c8e15708e70f45f597bc3737ced8e", date: "2021-12-18 08:44:24 UTC", description: "ensure it works with real datadog endpoint", pr_number:                                       10414, scopes: ["datadog_metrics sink"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       5, insertions_count:   46, deletions_count:   12},
		{sha: "7c3ad5a25915cb8cf0babbc64f9337e0d60a2d1e", date: "2021-12-18 00:39:18 UTC", description: "bump metrics-exporter-prometheus from 0.6.1 to 0.7.0", pr_number:                             10499, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "6324298a0fdb1d223174b21447af5832299216e8", date: "2021-12-18 00:39:34 UTC", description: "bump tracing-fluent-assertions from 0.1.3 to 0.2.0", pr_number:                               10500, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "2678742ec1e2778607bb60ab869718194116445a", date: "2021-12-18 00:39:54 UTC", description: "bump async-graphql from 3.0.16 to 3.0.17", pr_number:                                         10502, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   4, deletions_count:    4},
		{sha: "eed95e148e2c7ea2284f360fc6b6f0fa82e7ee3f", date: "2021-12-18 00:41:23 UTC", description: "bump async-graphql-warp from 3.0.16 to 3.0.17", pr_number:                                    10504, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "77ad1e7f40261ffed49ac9a10897c14541aa11c5", date: "2021-12-18 09:14:28 UTC", description: "bump nix from 0.22.2 to 0.23.1", pr_number:                                                   10501, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   15, deletions_count:   2},
		{sha: "21e252598300e0777ca412f0ae72e36b3dd3b1fb", date: "2021-12-18 10:40:43 UTC", description: "bump metrics from 0.17.0 to 0.17.1", pr_number:                                               10505, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:   5, deletions_count:    6},
		{sha: "f1404bea186ba83c4426a32bbef3f633c17cf4d2", date: "2021-12-20 15:29:14 UTC", description: "update Fluent Bit status in readme", pr_number:                                               10510, scopes: [], type:                                              "chore", breaking_change:       false, author: "Zero King", files_count:            1, insertions_count:   1, deletions_count:    1},
		{sha: "1f2a7dc1b14715f3bbc9d379dfff61fec3237d1a", date: "2021-12-20 23:15:41 UTC", description: "bump futures-util from 0.3.18 to 0.3.19", pr_number:                                          10511, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   16, deletions_count:   16},
		{sha: "cd0ae8934d5ea272d4d88c65afd4a07677758806", date: "2021-12-20 23:17:01 UTC", description: "bump futures from 0.3.18 to 0.3.19", pr_number:                                               10512, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:   41, deletions_count:   41},
		{sha: "ec24858818f13ba918e17b5b04670cdaa1220282", date: "2021-12-20 23:17:25 UTC", description: "bump syslog from 6.0.0 to 6.0.1", pr_number:                                                  10513, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   4, deletions_count:    3},
		{sha: "53d22cd6dc66674d7d977ceb8bde5e8a45937458", date: "2021-12-20 23:17:40 UTC", description: "bump num_enum from 0.5.4 to 0.5.5", pr_number:                                                10514, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   5, deletions_count:    5},
		{sha: "284ebe9710a453d10d40cedb5c513a8f5731b24b", date: "2021-12-20 23:18:15 UTC", description: "bump rkyv from 0.7.26 to 0.7.28", pr_number:                                                  10515, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   7, deletions_count:    7},
		{sha: "92200492302d23f79d848c7986700ea38d3892fb", date: "2021-12-20 23:18:35 UTC", description: "bump lru from 0.7.0 to 0.7.1", pr_number:                                                     10516, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "c838701fef0a45ca20d19ed7cb6cc4e0d2f32b4c", date: "2021-12-21 01:17:03 UTC", description: "bump docker/login-action from 1.10.0 to 1.12.0", pr_number:                                   10520, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:   5, deletions_count:    5},
		{sha: "fa10123399290f6c90672699d61359c23eaa9773", date: "2021-12-21 02:47:35 UTC", description: "Use PR base for soak comparisons", pr_number:                                                 10519, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:   11, deletions_count:   11},
		{sha: "b60992b4f8aef6ed64219778fb59c0bf01cdb4a6", date: "2021-12-21 02:53:18 UTC", description: "Add cue schema for structured release changelogs", pr_number:                                 10140, scopes: [], type:                                              "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:   343, deletions_count:  1},
		{sha: "dd811082327baa31b8d06a9402e1a133beb13510", date: "2021-12-21 04:08:58 UTC", description: "bump tracing-subscriber from 0.2.25 to 0.3.1", pr_number:                                     9798, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      10, insertions_count:  36, deletions_count:   78},
		{sha: "9500436be32b93cad1fb9c146daffb30083038b4", date: "2021-12-21 13:27:39 UTC", description: "bump num_cpus from 1.13.0 to 1.13.1", pr_number:                                              10524, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "03416dacbb649b18d4ad74665bafc763ee8f9240", date: "2021-12-21 13:49:27 UTC", description: "bump fslock from 0.2.0 to 0.2.1", pr_number:                                                  10525, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "e4d391e3a3fc823328a57d86b7ec11cd4434df4a", date: "2021-12-21 14:22:46 UTC", description: "actually batch TCP source decoder outputs", pr_number:                                        10506, scopes: ["sources"], type:                                     "fix", breaking_change:         false, author: "Luke Steensen", files_count:        9, insertions_count:   258, deletions_count:  38},
		{sha: "905c4bedfc8f9cea992dd1e0c1a81350ecdeee4c", date: "2021-12-21 23:23:36 UTC", description: "bump reqwest from 0.11.7 to 0.11.8", pr_number:                                               10532, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:   15, deletions_count:   6},
		{sha: "85431ff77a919486b942f783490f7f2235ae32be", date: "2021-12-21 23:58:21 UTC", description: "Basic release changelog rendering", pr_number:                                                10527, scopes: [], type:                                              "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        3, insertions_count:   107, deletions_count:  171},
		{sha: "ce8572ba2bb5518b4114783ee015f18879d3adcd", date: "2021-12-22 03:07:26 UTC", description: "remove deprecated config", pr_number:                                                         10488, scopes: ["elasticsearch sink"], type:                          "chore", breaking_change:       false, author: "Nathan Fox", files_count:           12, insertions_count:  115, deletions_count:  75},
		{sha: "1cd6548b2dfd65dbad17ad7cfbfdd335713709cf", date: "2021-12-22 08:49:43 UTC", description: "Update k8s-e2e to use new helm chart", pr_number:                                             10521, scopes: [], type:                                              "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      8, insertions_count:   264, deletions_count:  400},
		{sha: "633a27c3c28bbc6cde3eeb34ff7c32651b42f12f", date: "2021-12-23 06:55:19 UTC", description: "add config option to limit source tcp connections", pr_number:                                10491, scopes: ["sources"], type:                                     "enhancement", breaking_change: false, author: "Nathan Fox", files_count:           16, insertions_count:  134, deletions_count:  24},
		{sha: "e14acc7f30aaf6ff361c1047ab8d7cff95ad4d08", date: "2021-12-22 16:57:58 UTC", description: "Skip files with unknown extension when `--config-dir` is used", pr_number:                    10564, scopes: ["config"], type:                                      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        13, insertions_count:  114, deletions_count:  87},
		{sha: "4c8abf34f6f4d185a97976efd9298148929973d2", date: "2021-12-22 18:48:14 UTC", description: "Fix framing handling of long frames", pr_number:                                              10568, scopes: ["codecs"], type:                                      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        2, insertions_count:   12, deletions_count:   9},
		{sha: "27b4d04ef6d64422b2ebca9a50a1d994606cd5cb", date: "2021-12-23 10:43:26 UTC", description: "added highlight article for improved concurrency ", pr_number:                                10469, scopes: ["performance"], type:                                 "docs", breaking_change:        false, author: "Barry Eom", files_count:            2, insertions_count:   40, deletions_count:   1},
		{sha: "5a18d06b0c158c4e00e5c5ebc4bb25e1c6a57f07", date: "2021-12-23 08:33:43 UTC", description: "Document that character delimited codec only accepts a byte", pr_number:                      10573, scopes: ["codecs"], type:                                      "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:   14, deletions_count:   1},
		{sha: "8ab99d1804e3f12161ddaec22afe9d79ecc1b31d", date: "2021-12-23 09:34:43 UTC", description: "Convert delimiter to byte", pr_number:                                                        10570, scopes: [], type:                                              "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   6, insertions_count:   18, deletions_count:   18},
		{sha: "d2b1b2a29107c06b6f4937a0e90cc8a0e96fa2f0", date: "2021-12-23 10:51:11 UTC", description: "Drop benches workflow and topology benches", pr_number:                                       10577, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        3, insertions_count:   0, deletions_count:    941},
		{sha: "1329df1321726e9d9a5692a61592c5d39b4a3077", date: "2021-12-23 15:34:15 UTC", description: "Document and test backpressure", pr_number:                                                   10575, scopes: ["external docs"], type:                               "docs", breaking_change:        false, author: "Nathan Fox", files_count:           8, insertions_count:   347, deletions_count:  4},
		{sha: "cf4bba7ef78de412f0ac1672a5a18b84be477696", date: "2021-12-23 13:24:57 UTC", description: "Test additional bench suites", pr_number:                                                     10576, scopes: ["ci"], type:                                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        2, insertions_count:   3, deletions_count:    2},
		{sha: "2400b48893043f9bcf00e0d3743e12cbcb0a8a2a", date: "2021-12-23 16:44:54 UTC", description: "added highlight article for Splunk HEC improvements", pr_number:                              10473, scopes: ["splunk service"], type:                              "docs", breaking_change:        false, author: "Barry Eom", files_count:            1, insertions_count:   74, deletions_count:   0},
		{sha: "a158e89b7e9f5c590dd4c0f92ea6fff4f15b7134", date: "2021-12-23 18:10:42 UTC", description: "Improve character delimited code performance somewhat", pr_number:                            10581, scopes: [], type:                                              "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   4, insertions_count:   62, deletions_count:   77},
		{sha: "c959047168b47e192937f8f1f2cc65641a57e4be", date: "2021-12-27 08:38:33 UTC", description: "add an integration test", pr_number:                                                          10440, scopes: ["datadog_agent source"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       10, insertions_count:  1556, deletions_count: 1422},
		{sha: "62917f785580be39ea1896ea7ca6e9f1d4035fb0", date: "2021-12-27 07:23:16 UTC", description: "bump anyhow from 1.0.51 to 1.0.52", pr_number:                                                10584, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "10694ee89b00b8bea8652b0eae9d421999d73aff", date: "2021-12-27 07:25:02 UTC", description: "bump tracing-subscriber from 0.3.3 to 0.3.4", pr_number:                                      10585, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      6, insertions_count:   8, deletions_count:    8},
		{sha: "cc54d1538565e96d571b0c16087f44985d09d2bf", date: "2021-12-27 07:27:11 UTC", description: "bump async-graphql from 3.0.17 to 3.0.18", pr_number:                                         10588, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   26, deletions_count:   10},
		{sha: "e3dd70ea30973266baeffc57c77084e63ed4bc83", date: "2021-12-27 08:28:09 UTC", description: "Enable std feature for tracing-subscriber", pr_number:                                        10592, scopes: ["ci"], type:                                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:   1, deletions_count:    1},
		{sha: "5e04f1c7429f8947bc00eca7350f54b57fffdae9", date: "2021-12-27 10:44:32 UTC", description: "bump rust_decimal from 1.18.0 to 1.19.0", pr_number:                                          10589, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:   9, deletions_count:    3},
		{sha: "4000853115422c55387ff10d08c74b90b03f1d6d", date: "2021-12-27 16:00:47 UTC", description: "Update Kubernetes and Helm docs to new chart", pr_number:                                     10498, scopes: [], type:                                              "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:      2, insertions_count:   17, deletions_count:   72},
		{sha: "6656e10c45f24d78ac060f4f9ba685fef1000b5f", date: "2021-12-27 21:18:25 UTC", description: "bump mlua from 0.7.0 to 0.7.1", pr_number:                                                    10586, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   4, deletions_count:    4},
		{sha: "d8657e8f950ae35531ee9fa6854b41798db7bf12", date: "2021-12-27 21:18:42 UTC", description: "bump pin-project from 1.0.8 to 1.0.9", pr_number:                                             10590, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:   88, deletions_count:   88},
		{sha: "4f690125c756cb766e616f9cb999ea08cdd3be9c", date: "2021-12-27 13:21:12 UTC", description: "Clean `target/` in CI", pr_number:                                                            10594, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        5, insertions_count:   28, deletions_count:   0},
		{sha: "f228891018f38b2e62f5df6297ab4907a059cffb", date: "2021-12-27 21:23:03 UTC", description: "bump twox-hash from 1.6.1 to 1.6.2", pr_number:                                               10587, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:   4, deletions_count:    4},
		{sha: "54cab5149ed551af660b19c207428bcf4e3b4e2c", date: "2021-12-28 09:54:53 UTC", description: "replace boilerplate by simple docker-compose file", pr_number:                                10466, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       6, insertions_count:   74, deletions_count:   61},
		{sha: "9638ec4f33fa4c6db2f4029527a098deefa03cc9", date: "2021-12-28 07:13:13 UTC", description: "bump async-graphql-warp from 3.0.17 to 3.0.18", pr_number:                                    10599, scopes: ["deps"], type:                                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:   3, deletions_count:    3},
		{sha: "2d6dd1c8a8e2b96ea95c11d3fe55ea5a1b8905c2", date: "2021-12-28 07:32:52 UTC", description: "Re-update k8s-e2e to use new helm chart", pr_number:                                          10567, scopes: ["ci"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        10, insertions_count:  269, deletions_count:  404},
		{sha: "77912dba3e41591a9140780fe4877518853d50a3", date: "2021-12-28 07:44:11 UTC", description: "Remove clean step from Windows nightly", pr_number:                                           10604, scopes: ["ci"], type:                                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:   0, deletions_count:    1},

	]
}
