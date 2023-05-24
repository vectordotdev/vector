package metadata

releases: "0.29.0": {
	date: "2023-04-11"

	description: """
		The Vector team is pleased to announce version 0.29.0!

		In addition to the usual smaller enhancements and bug fixes, this release also includes:

		- A new `appsignal` sink to send logs and metrics to [AppSignal](https://www.appsignal.com/)
		- A new `databend` sink to send logs to [Databend](https://databend.rs/)
		- A new `webhdfs` sink to send log data to HDFS via the
		  [WebHDFS](https://hadoop.apache.org/docs/r1.0.4/webhdfs.html) interface.
		- A new `csv` encoding option for sinks to encode events as CSV

		Be sure to check out the [upgrade guide](/highlights/2023-04-11-0-29-0-upgrade-guide) for
		breaking changes in this release.

		Special thanks to [Xuanwo](https://github.com/Xuanwo) for collaborating with us to introduce
		the [OpenDAL library](https://github.com/apache/incubator-opendal) into Vector, starting
		with the `webhdfs` sink, to prove out the ability to use it to more easily support
		additional Vector sinks in the future.

		Additionally, for Vector developers, two new tutorials were added for developing Vector
		sinks: [docs](https://github.com/vectordotdev/vector/tree/master/docs/tutorials/sinks). We
		hope this makes it easier to add Vector sinks in the future.
		"""

	known_issues: [
		"""
			Certain configurations containing a chain of at least two `remap` transforms no
			longer result in a panic at startup. Fixed in 0.29.1.
			""",
	]

	changelog: [
		{
			type: "feat"
			scopes: ["databend sink"]
			description: """
				A new `databend` sink was added to forward logs to [Databend](https://databend.rs/).
				"""
			contributors: ["everpcpc"]
			pr_numbers: [15898, 16829]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Vector components can now bind to the same port on different IP addresses including
				different IP address families. Previously Vector would report that the addresses
				conflicted.
				"""
			contributors: ["ruhi-stripe"]
			pr_numbers: [14187]
		},
		{
			type: "fix"
			scopes: ["amqp source", "amqp sink"]
			description: """
				The `amqp` source and sink now support TLS connections.
				"""
			pr_numbers: [16604]
		},
		{
			type: "enhancement"
			scopes: ["http_server source"]
			description: """
				The `http_server` source now supports receiving requests using `zstd` compression.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16587]
		},
		{
			type: "enhancement"
			scopes: ["socket source"]
			description: """
				The `socket` source now has a `max_connection_duration_secs` option that will
				terminate connections after the specified number of seconds. This is useful to force
				clients to reconnect when connecting through a load balancer.
				"""
			contributors: ["joscha-alisch"]
			pr_numbers: [16489]
		},
		{
			type: "enhancement"
			scopes: ["webhdfs sink"]
			description: """
				A new `webhdfs` sink was added to send log data to HDFS via the
				[WebHDFS](https://hadoop.apache.org/docs/r1.0.4/webhdfs.html) interface.

				This sink is also the first to use the new
				[OpenDAL](https://github.com/apache/incubator-opendal) library which may enable us
				to more quickly add support for other sinks in the future as that library expands
				storage support.
				"""
			contributors: ["Xuanwo"]
			pr_numbers: [16557]
		},
		{
			type: "fix"
			scopes: ["aws provider"]
			description: """
				Support for AWS credential files was re-added for all AWS components. This was
				dropped during the migration of the SDK we were using from `rusoto` to AWS's
				official Rust SDK.
				"""
			contributors: ["protochron"]
			pr_numbers: [16633]
		},
		{
			type: "enhancement"
			scopes: ["loki sink"]
			description: """
				The `loki` sink now supports sending a field containing a map as labels with
				configuration like:

				```yaml
				labels:
					"*": "{{ labels }}"
				```

				to send the key/value map stored at `labels` as the set of labels to Loki.
				"""
			contributors: ["hargut"]
			pr_numbers: [16591]
		},
		{
			type: "enhancement"
			scopes: ["aws provider"]
			description: """
				AWS components can now also assume-role whenever static credentials are provided in the
				configuration file through the `auth.assume_role` option.
				"""
			contributors: ["sbalmos"]
			pr_numbers: [16715]
		},
		{
			type: "feat"
			scopes: ["codecs", "sinks"]
			description: """
				A new `csv` encoding option was added for sinks. Example configuration:

				```yaml
				encoding:
					codec: csv
					csv:
						fields:
						- field1
						- field2
				```

				This will encode the event as CSV using the `field1` and `field2` fields, in order.

				Note that CSV headers are not added. This requires batch header support that will be
				added in the future.
				"""
			contributors: ["everpcpc"]
			pr_numbers: [16603, 16828]
		},
		{
			type: "enhancement"
			scopes: ["datadog provider"]
			description: """
				The deprecated `api_key` option was removed from Datadog sinks. It had been replaced
				by `default_api_key`.
				"""
			breaking: true
			pr_numbers: [16750]
		},
		{
			type: "fix"
			scopes: ["aws_s3 source"]
			description: """
				The `aws_s3` source no longer deadlocks under low `client_concurrency` when
				`acknowledgements` are enabled.
				"""
			contributors: ["mdeltito"]
			pr_numbers: [16742]
		},
		{
			type: "chore"
			scopes: ["socket source"]
			description: """
				The `socket` source `max_length` option was deprecated in-lieu of the options
				available on `framing`.

				See [the upgrade
				guide](/highlights/2023-04-11-0-29-0-upgrade-guide#socket-source-max-length) for
				more details.
				"""
			pr_numbers: [16752]
		},
		{
			type: "feat"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source now adds `container_image_id` as metadata to log
				events.
				"""
			contributors: ["nabokihms"]
			pr_numbers: [16769]
		},
		{
			type: "fix"
			scopes: ["statsd sink"]
			description: """
				The `statsd` sink now supports encoding aggregate histograms. Not supported, yet, is
				encoding sketches as histograms.
				"""
			contributors: ["pedro-stanaka"]
			pr_numbers: [14762]
		},
		{
			type: "fix"
			scopes: ["codecs"]
			description: """
				Encoding of timestamps by the `gelf` codec was fixed to correctly encode as an
				integer rather than an an RFC3339 string.
				"""
			contributors: ["scMarkus"]
			pr_numbers: [16749]
		},
		{
			type: "fix"
			scopes: ["releasing"]
			description: """
				The distributed SystemD unit file now has `Restart` set to `Always` to match
				expected user behavior when Vector exits.
				"""
			contributors: ["KannarFr"]
			pr_numbers: [16822]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				Vector now logs signals that it receives, and handles, at the INFO level. Unhandled
				signals are ignored.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16835]
		},
		{
			type: "enhancement"
			scopes: ["vrl: stdlib"]
			description: """
				An `hmac` function was added to VRL to calculate
				[HMACs](https://en.wikipedia.org/wiki/HMAC).
				"""
			contributors: ["sbalmos"]
			pr_numbers: [15371]
		},
		{
			type: "fix"
			scopes: ["core"]
			description: """
				Comparisons of float fields in Vector was fixed to completely compare the value.
				Previously it was only comparing the integer portion. This mostly came up in VRL
				when doing field comparisons like `.foo == .bar` where `.foo` and `.bar` were float
				values.
				"""
			pr_numbers: [16926]
		},
		{
			type: "fix"
			scopes: ["prometheus_scrape source"]
			description: """
				The `prometheus_scrape` source now handles larger counts for summary metrics (up to
				uint64 instead of uint32).
				"""
			contributors: ["uralmetal"]
			pr_numbers: [16946, 16508]
		},
		{
			type: "feat"
			scopes: ["vrl: stdlib"]
			description: """
				A few functions to generate random values were added to VRL:

				- `random_bool`: generate a random boolean
				- `random_int`: generate a random integer
				- `random_float`: generate a random float
				"""
			pr_numbers: [16768]
		},
		{
			type: "feat"
			scopes: ["aws_ecs_metrics source"]
			description: """
				The `aws_ecs_metrics` source now collects `precpu_online_cpus` stats.
				"""
			contributors: ["asingh072318"]
			pr_numbers: [16768]
		},
		{
			type: "feat"
			scopes: ["file source"]
			description: """
				The `file` source now handles acknowledgements when Vector is shutting down and
				sinks are flushing events. Previously the `file` source would ignore all
				acknowledgements once Vector starts to shut down.
				"""
			contributors: ["jches"]
			pr_numbers: [16928]
		},
		{
			type: "fix"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source now backs-off when retrying failed requests to the
				Kubernetes API.
				events.
				"""
			contributors: ["nabokihms"]
			pr_numbers: [17009]
		},
		{
			type: "feat"
			scopes: ["appsignal sink"]
			description: """
				A new `appsignal` sink was added to forward logs and metrics to
				[AppSignal](https://www.appsignal.com/).
				"""
			contributors: ["tombruijn"]
			pr_numbers: [16650]
		},
	]

	commits: [
		{sha: "d011d7ff85e3acad4df62d37b82f60b4b39fea21", date: "2023-02-24 14:53:31 UTC", description: "Initial `databend` sink", pr_number:                                                               15898, scopes: ["new sink"], type:                    "feat", breaking_change:        false, author: "everpcpc", files_count:           21, insertions_count:  1548, deletions_count: 1},
		{sha: "6278d32d52a991a03c442f93efff038956d6dc64", date: "2023-02-24 02:09:22 UTC", description: "Update CODEOWNERS for databend sink", pr_number:                                                   16567, scopes: [], type:                              "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   1, deletions_count:    0},
		{sha: "c960de7c47231b9430efbb28076b36480a4e297c", date: "2023-02-24 03:41:37 UTC", description: "Use log namespacing and semantic meaning", pr_number:                                              16564, scopes: ["datadog_archives sink"], type:       "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:    1, insertions_count:   34, deletions_count:   23},
		{sha: "f4962f9d1f06684880524ac8558b7d406cec5bb2", date: "2023-02-24 08:43:03 UTC", description: "Added tutorials on writing a sink", pr_number:                                                     16070, scopes: [], type:                              "docs", breaking_change:        false, author: "Stephen Wakely", files_count:     2, insertions_count:   926, deletions_count:  0},
		{sha: "a9ba2cbb6df83c38d5335afb83c1fe7a61ee8f2b", date: "2023-02-24 05:19:52 UTC", description: "add json schema 2019-09 support + reorganize schema module", pr_number:                            16569, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      25, insertions_count:  81, deletions_count:   46},
		{sha: "ba8c096976e70eb978088af273c1692e8c6e4626", date: "2023-02-24 05:31:29 UTC", description: "Set up `ConfigurableRef` for schema functions", pr_number:                                         16568, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      15, insertions_count:  211, deletions_count:  166},
		{sha: "ab459399a7ca58c088dfbd30dd6c08f5799c929e", date: "2023-02-24 14:17:51 UTC", description: "Fix flush not occuring when events arrive in high rate", pr_number:                                16146, scopes: ["reduce transform"], type:            "fix", breaking_change:         false, author: "Tomer Shalev", files_count:       1, insertions_count:   51, deletions_count:   3},
		{sha: "367cbcc898aa0f1a14bf8cca74e7ccf954f98d83", date: "2023-02-25 01:53:38 UTC", description: "bump clap_complete from 4.1.2 to 4.1.3", pr_number:                                                16577, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "9395ae0c6432f1ca152c03cf192ce5980da21c10", date: "2023-02-25 01:55:09 UTC", description: "bump num_enum from 0.5.10 to 0.5.11", pr_number:                                                   16576, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "bd01163b704ce50a9199db75827793a034d242e7", date: "2023-02-25 03:31:13 UTC", description: "bump prost from 0.11.6 to 0.11.7", pr_number:                                                      16575, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   9, deletions_count:    9},
		{sha: "0fb8d7dd8ec61e53eba4687a487bd544aedb922f", date: "2023-02-25 08:44:11 UTC", description: "bump rustyline from 10.1.1 to 11.0.0", pr_number:                                                  16542, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   9, deletions_count:    32},
		{sha: "55bc9e8913470ddd751274816b7af12a014ef09d", date: "2023-02-25 05:10:36 UTC", description: "extend SUPPORT.md to include guidelines on how to ask a support question about Vector", pr_number: 16501, scopes: ["docs"], type:                        "chore", breaking_change:       false, author: "neuronull", files_count:          1, insertions_count:   101, deletions_count:  2},
		{sha: "63e50684a42acdec9dde197d34ae4468b5b055fb", date: "2023-02-25 04:20:35 UTC", description: "bump aws-actions/configure-aws-credentials from 1.pre.node16 to 1.7.0", pr_number:                 16584, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "0ef9ce0f22ddca0c56a68101b8df00e17b5ab4e7", date: "2023-02-28 00:32:27 UTC", description: "Update k8s manifests to 0.19.2 of helm chart", pr_number:                                          16555, scopes: ["releasing"], type:                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      18, insertions_count:  25, deletions_count:   22},
		{sha: "1442ca3e73c89466642c37e10b1a28ff0c22515b", date: "2023-02-28 00:32:45 UTC", description: "Fix AWS CloudWatch forwarding example", pr_number:                                                 16100, scopes: ["aws_kinesis_firehose source"], type: "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:      1, insertions_count:   4, deletions_count:    4},
		{sha: "b809df1c1621b8f8c48d062c64077e3f27506ec1", date: "2023-02-28 00:41:38 UTC", description: "Convert top-level transforms enum to typetag", pr_number:                                          16572, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      33, insertions_count:  253, deletions_count:  208},
		{sha: "2f5e6b141327654a587c20d9f18ec2011d45dbc7", date: "2023-02-28 09:57:16 UTC", description: "bump crossterm from 0.26.0 to 0.26.1", pr_number:                                                  16594, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "9866e1f2df96cd51ad16c1167c3a73fa1ea4a1f3", date: "2023-02-28 10:51:28 UTC", description: "bump aws-smithy-http-tower from 0.54.3 to 0.54.4", pr_number:                                      16595, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   21, deletions_count:   23},
		{sha: "a7ef78401b4180fe47ab84d03e078ae38f1d8dc8", date: "2023-03-01 01:40:04 UTC", description: "bump syn from 1.0.108 to 1.0.109", pr_number:                                                      16599, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   72, deletions_count:   72},
		{sha: "47055872ad361ed8ca8619e5aa1f61f0ecb85a34", date: "2023-03-01 01:47:12 UTC", description: "bump dyn-clone from 1.0.10 to 1.0.11", pr_number:                                                  16622, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   7, deletions_count:    7},
		{sha: "c2c48236c6804b1b5aa55f29dd8dda32ed44a873", date: "2023-03-01 01:47:42 UTC", description: "bump h2 from 0.3.15 to 0.3.16", pr_number:                                                         16623, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "0eefc063e96db159529d48f57351ed8ba8fbbf10", date: "2023-03-01 01:48:37 UTC", description: "bump clap from 4.1.6 to 4.1.7", pr_number:                                                         16624, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    7, insertions_count:   18, deletions_count:   18},
		{sha: "4d095004c662cd3ff5ce07e1346397827dd5515b", date: "2023-03-01 02:47:14 UTC", description: "bump aws-smithy-async from 0.54.3 to 0.54.4", pr_number:                                           16596, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "7e10b6263e3c3a0fe25e49cedc604cba2904d93a", date: "2023-03-01 03:11:16 UTC", description: "bump tempfile from 3.3.0 to 3.4.0", pr_number:                                                     16602, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   6, deletions_count:    16},
		{sha: "a2e158eaf5c8d0d2ed24bd921ffb17eef0c60b51", date: "2023-03-01 03:16:29 UTC", description: "bump prost from 0.11.7 to 0.11.8", pr_number:                                                      16593, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   9, deletions_count:    9},
		{sha: "b7aa6cb56bca731f9ccf1b37f87b6267283d7b13", date: "2023-03-01 03:17:39 UTC", description: "bump tower-http from 0.3.5 to 0.4.0", pr_number:                                                   16600, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   25, deletions_count:   7},
		{sha: "f82272a4901987f9bf24814443f3c170505996a5", date: "2023-03-01 03:39:51 UTC", description: "add openssl feature flag to lapin crate", pr_number:                                               16604, scopes: ["amqp source", "amqp sink"], type:    "fix", breaking_change:         false, author: "Stephen Wakely", files_count:     2, insertions_count:   3, deletions_count:    1},
		{sha: "c201422e1f843f5f5c87ca782e6b08d578bc3c73", date: "2023-03-01 04:24:46 UTC", description: "bump bytesize from 1.1.0 to 1.2.0", pr_number:                                                     16597, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "6eb819cbb2e646da905927b03b96b488935e4dad", date: "2023-03-01 01:46:56 UTC", description: "Fix `FieldsIter` when using a non-object value", pr_number:                                        16612, scopes: ["core"], type:                        "fix", breaking_change:         false, author: "Nathan Fox", files_count:         2, insertions_count:   17, deletions_count:   5},
		{sha: "45a65078ae4e352776dcea5659ed3b386a8afbf6", date: "2023-03-01 02:30:10 UTC", description: "Some more tweaks to minor-release template", pr_number:                                            16621, scopes: ["releasing"], type:                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   7, deletions_count:    11},
		{sha: "81c36b8ffe160f2fc292d92dfd066f57c206f94e", date: "2023-03-01 05:15:21 UTC", description: "Bump Vector version to 0.29.0", pr_number:                                                         16611, scopes: ["releasing"], type:                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:   2, deletions_count:    2},
		{sha: "980da01e4e6008a40a4192b042f19986df7c8b2c", date: "2023-03-01 05:15:51 UTC", description: "Regenerate manifests for 0.20.0 of Helm chart", pr_number:                                         16635, scopes: ["releasing"], type:                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      18, insertions_count:  22, deletions_count:   22},
		{sha: "73efb7a56358a5371e7b3353d079fc0bde3c673b", date: "2023-03-01 09:19:37 UTC", description: "Make `Condition::check` public", pr_number:                                                        16639, scopes: ["core"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "c305b05a600e442dd54ae174d16066c255e2fa58", date: "2023-03-01 19:29:14 UTC", description: "Regularize `vdev` integration test arguments to `cargo`", pr_number:                               16570, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   36, insertions_count:  128, deletions_count:  135},
		{sha: "846e48cf5299b93b80d6d14e5d967165844e4f7f", date: "2023-03-02 03:22:11 UTC", description: "bump crossbeam-utils from 0.8.14 to 0.8.15", pr_number:                                            16641, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   5, deletions_count:    5},
		{sha: "4e987b3a6b7de0698fd63e96f9ce116c21efa20d", date: "2023-03-02 03:22:45 UTC", description: "bump aws-smithy-client from 0.54.3 to 0.54.4", pr_number:                                          16642, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "f09bc9b9191937a44e818067dad6d70f449fe88f", date: "2023-03-02 03:23:13 UTC", description: "bump clap_complete from 4.1.3 to 4.1.4", pr_number:                                                16643, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "9994f274595a67acdb2f8e443f58ecad7e7ddd69", date: "2023-03-02 03:23:51 UTC", description: "bump prost-types from 0.11.6 to 0.11.8", pr_number:                                                16644, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   5, deletions_count:    6},
		{sha: "6659e239bc9c329b4f8db0f4abe7e692f4bac568", date: "2023-03-02 03:54:59 UTC", description: "bump actions/add-to-project from 0.4.0 to 0.4.1", pr_number:                                       16610, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "c4b68b671f1d8e30d95deb4e24c7d9d61eeadf3c", date: "2023-03-02 05:29:57 UTC", description: "fix cue fmt command", pr_number:                                                                   16646, scopes: ["internal docs"], type:               "docs", breaking_change:        false, author: "Tom de Bruijn", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "048523e8bc9fd7873d206ba5d446d72475aee0ff", date: "2023-03-02 05:20:30 UTC", description: "Revert \"Regularize `vdev` integration test arguments to …", pr_number:                            16648, scopes: [], type:                              "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     36, insertions_count:  135, deletions_count:  128},
		{sha: "5e0a3ec39f40e4e74e25dbc3f301e191d5429785", date: "2023-03-02 05:24:32 UTC", description: "bump clap from 4.1.7 to 4.1.8", pr_number:                                                         16640, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    7, insertions_count:   18, deletions_count:   18},
		{sha: "8f6768f95a8b7fe48e463b527d197a829a704269", date: "2023-03-02 06:27:40 UTC", description: "add zstd support", pr_number:                                                                      16587, scopes: ["http_server"], type:                 "feat", breaking_change:        false, author: "Alexander Zaitsev", files_count:  3, insertions_count:   5, deletions_count:    1},
		{sha: "227e7cc8fc897914a97311a43cd94feff7ad1618", date: "2023-03-02 01:15:49 UTC", description: "VRL separation part 1 (lookup)", pr_number:                                                        16636, scopes: ["vrl"], type:                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:         66, insertions_count:  497, deletions_count:  41},
		{sha: "47c22a5b0ccf1d30c708997ed43c855a8b8174ad", date: "2023-03-02 05:03:27 UTC", description: "VRL separation part 2 (value)", pr_number:                                                         16638, scopes: ["vrl"], type:                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:         71, insertions_count:  23, deletions_count:   23},
		{sha: "b1b089e42744820102a95f11211a311cfe99b885", date: "2023-03-02 12:16:24 UTC", description: "update log namespacing", pr_number:                                                                16475, scopes: ["azure_monitor_logs sink"], type:     "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     3, insertions_count:   91, deletions_count:   16},
		{sha: "2e0de1f829adf6e54a83dd238f0050495b60339d", date: "2023-03-02 16:00:15 UTC", description: "Regularize vdev integration test arguments to cargo", pr_number:                                   16652, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   36, insertions_count:  140, deletions_count:  135},
		{sha: "c02ace3bc89dccf274f9c7ea4216d2a60781921f", date: "2023-03-03 01:52:17 UTC", description: "bump prost-build from 0.11.6 to 0.11.8", pr_number:                                                16656, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:   6, deletions_count:    6},
		{sha: "be7282d2c040754903afb76d999284ae9502f9ac", date: "2023-03-03 01:53:11 UTC", description: "bump infer from 0.12.0 to 0.13.0", pr_number:                                                      16657, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "b7f2ac44e530ad6832579423bfe941f83bd5d930", date: "2023-03-03 01:54:27 UTC", description: "bump once_cell from 1.17.0 to 1.17.1", pr_number:                                                  16658, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "a3098c55340997ccd027b3644eb50d6bd6dee45a", date: "2023-03-03 02:05:48 UTC", description: "bump mongodb from 2.3.1 to 2.4.0", pr_number:                                                      16659, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   23, deletions_count:   39},
		{sha: "ef623242bef832de4dbee62974e6266690a6386f", date: "2023-03-03 02:07:36 UTC", description: "bump tokio from 1.25.0 to 1.26.0", pr_number:                                                      16660, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:   56, deletions_count:   32},
		{sha: "d0ed464578b71e12564702e8a4c8abc006e23b03", date: "2023-03-03 05:12:40 UTC", description: "remove connection string log", pr_number:                                                          16655, scopes: ["amqp"], type:                        "fix", breaking_change:         false, author: "Alexander Zaitsev", files_count:  1, insertions_count:   0, deletions_count:    1},
		{sha: "dabc4507d33889005db88b88e2a15033be3a0ce0", date: "2023-03-03 03:29:10 UTC", description: "replace headings with bold to fix table of contents rendering", pr_number:                         16668, scopes: [], type:                              "docs", breaking_change:        false, author: "Blake Mealey", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "d8cfdfc696508506d9ae65578309608560b2cb20", date: "2023-03-03 07:23:21 UTC", description: "Add check-spelling workflow", pr_number:                                                           16654, scopes: ["ci"], type:                          "feat", breaking_change:        true, author:  "Josh Soref", files_count:         37, insertions_count:  3074, deletions_count: 42},
		{sha: "8747a69525385481bbec6635a3b75cc8b10c5f74", date: "2023-03-03 06:30:53 UTC", description: "Re-export `LogNamespace`", pr_number:                                                              16669, scopes: [], type:                              "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   2, deletions_count:    1},
		{sha: "9e9eb8e5af79ef72235c1bf97a1598340fbf650d", date: "2023-03-03 09:35:12 UTC", description: "Support `TEST_LOG` and add `--no-capture` flag to cargo nextest", pr_number:                       16670, scopes: ["vdev"], type:                        "feat", breaking_change:        false, author: "Jonathan Padilla", files_count:   1, insertions_count:   7, deletions_count:    0},
		{sha: "b731b7bb71c164e9cb4d6fa05c4dd4d58496f178", date: "2023-03-03 10:58:51 UTC", description: "Declare some Regression Detector experiments erratic", pr_number:                                  16671, scopes: ["ci"], type:                          "fix", breaking_change:         false, author: "Brian L. Troutwine", files_count: 2, insertions_count:   3, deletions_count:    1},
		{sha: "2555abc2c76f65b1ef7ebf257fcccbda66b45797", date: "2023-03-04 02:36:26 UTC", description: "bump syn from 1.0.107 to 1.0.109", pr_number:                                                      16676, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   71, deletions_count:   71},
		{sha: "824ac0be16ae338487d6d492d0fe4e4a90a06756", date: "2023-03-04 02:39:13 UTC", description: "bump aws-smithy-http-tower from 0.54.1 to 0.54.4", pr_number:                                      16677, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   36, deletions_count:   14},
		{sha: "815324e46a50ee08705fb02a91a53cfee3e30fc2", date: "2023-03-04 00:57:14 UTC", description: "Update `smp` to 0.7.2 from 0.7.1", pr_number:                                                      16672, scopes: ["ci"], type:                          "fix", breaking_change:         false, author: "Brian L. Troutwine", files_count: 1, insertions_count:   1, deletions_count:    1},
		{sha: "bdc67e6672389d0481ac4632c12fe715f6a2674b", date: "2023-03-04 11:16:04 UTC", description: "add optional `max_connection_duration_secs` configuration setting", pr_number:                     16489, scopes: ["socket source"], type:               "feat", breaking_change:        false, author: "Joscha Alisch", files_count:      8, insertions_count:   90, deletions_count:   0},
		{sha: "cad16435a98f910f83b24789e0133d845187cdc6", date: "2023-03-04 08:24:57 UTC", description: "Add patch release issue template", pr_number:                                                      16686, scopes: ["releasing"], type:                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   49, deletions_count:   0},
		{sha: "8ae41c4ce08fa32a4ff68409af87d39f39c46b61", date: "2023-03-07 03:27:30 UTC", description: "bump mlua from 0.8.7 to 0.8.8", pr_number:                                                         16693, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   6, deletions_count:    6},
		{sha: "641e4c0675b7a4a0d15e8137475b3a8566741286", date: "2023-03-07 05:19:12 UTC", description: "Regenerate k8s manifests for 0.20.1 chart", pr_number:                                             16697, scopes: ["releasing"], type:                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      18, insertions_count:  22, deletions_count:   22},
		{sha: "2789a41fc48f78dea2da2ec77935ac922b76492c", date: "2023-03-07 05:35:48 UTC", description: "bump async-trait from 0.1.64 to 0.1.66", pr_number:                                                16690, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "6a499d9821aa0bef7c2b14e00b871bacb82da683", date: "2023-03-07 06:00:15 UTC", description: "Fix 0.28.0 cue", pr_number:                                                                        16700, scopes: ["internal docs", "releasing"], type:  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   0, deletions_count:    4},
		{sha: "430517756a48726f25faa3882f3daacd50da7685", date: "2023-03-07 20:04:16 UTC", description: "Add webhdfs sink suppport", pr_number:                                                             16557, scopes: ["sinks"], type:                       "feat", breaking_change:        false, author: "Xuanwo", files_count:             16, insertions_count:  1403, deletions_count: 12},
		{sha: "90691c5d949db0ae7a19bd7d695ba668f21200e6", date: "2023-03-07 07:33:41 UTC", description: "bump typetag from 0.2.5 to 0.2.6", pr_number:                                                      16689, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   6, deletions_count:    6},
		{sha: "ae9c56f88541557361847e862b0d520eeca46105", date: "2023-03-07 07:34:54 UTC", description: "bump paste from 1.0.11 to 1.0.12", pr_number:                                                      16688, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   5, deletions_count:    5},
		{sha: "246509c5b85d07f1f3d0a76e68f83fe9922f908c", date: "2023-03-07 08:04:23 UTC", description: "Use info logging for smp job submission", pr_number:                                               16699, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "24943f2d9c2afa8d0d608f65b996dd70c2953dbd", date: "2023-03-08 01:12:01 UTC", description: "Tweaks to patch release template", pr_number:                                                      16710, scopes: ["releasing"], type:                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   4, deletions_count:    5},
		{sha: "d2a20f3bdab45c5b1ca258034cb7ee1b5d41d656", date: "2023-03-08 07:31:01 UTC", description: "bump lru from 0.9.0 to 0.10.0", pr_number:                                                         16707, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "1834ed07730844e87d364b82025a565a7819c90c", date: "2023-03-08 07:04:52 UTC", description: "Make the `bollard` dep optional", pr_number:                                                       16711, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   3, deletions_count:    3},
		{sha: "99aca6330ea6fc0a176bbf6282e09e3eb7ec6396", date: "2023-03-09 00:51:20 UTC", description: "bump ryu from 1.0.12 to 1.0.13", pr_number:                                                        16722, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "ca6cb3239a0194de08e9ea3ee8de23c57b5d4c23", date: "2023-03-09 00:56:54 UTC", description: "bump csv from 1.2.0 to 1.2.1", pr_number:                                                          16706, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "ee6b2e97fe239b118c01428f0b4ed14184fa27bf", date: "2023-03-09 01:01:49 UTC", description: "bump inventory from 0.3.3 to 0.3.4", pr_number:                                                    16705, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "9fb1e512c92501a37cc46367ceef06e526783a93", date: "2023-03-09 01:06:15 UTC", description: "bump serde_yaml from 0.9.17 to 0.9.19", pr_number:                                                 16692, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   9, deletions_count:    9},
		{sha: "b97306096d34f5e9f84566dfdc48d092f94c3fb7", date: "2023-03-09 01:27:22 UTC", description: "Adding ux-team as CODEOWNERS for docs/", pr_number:                                                16726, scopes: ["dev"], type:                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   1, deletions_count:    0},
		{sha: "83774298b62a3c6a191878d5de6afb933fc6066d", date: "2023-03-09 06:29:56 UTC", description: "Add notes on renaming components to Deprecation policy", pr_number:                                16725, scopes: [], type:                              "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     1, insertions_count:   4, deletions_count:    0},
		{sha: "152e7296be525bc17deda74fd7330c7c5a17c548", date: "2023-03-09 08:25:35 UTC", description: "bump serde-wasm-bindgen from 0.4.5 to 0.5.0", pr_number:                                           16721, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "1b27f46d8f9a9f242c9e4ee7b477bc219093802c", date: "2023-03-09 10:11:32 UTC", description: "Implement support for AWS credentials files", pr_number:                                           16633, scopes: ["aws"], type:                         "fix", breaking_change:         false, author: "Dan Norris", files_count:         11, insertions_count:  72, deletions_count:   16},
		{sha: "2f6b6d2a0f703525cd7d97e29f3b009c11abd50f", date: "2023-03-09 16:22:15 UTC", description: "allow dynamic labels without prefix", pr_number:                                                   16591, scopes: ["loki sink"], type:                   "feat", breaking_change:        false, author: "Harald Gutmann", files_count:     4, insertions_count:   95, deletions_count:   19},
		{sha: "577a90b7df97045ec6a2e73fd00264f86e1c0414", date: "2023-03-09 23:54:36 UTC", description: "bump serde from 1.0.152 to 1.0.154", pr_number:                                                    16735, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:   13, deletions_count:   13},
		{sha: "39f3560ced95745113dbc33c19d267f356319e45", date: "2023-03-09 23:58:52 UTC", description: "bump windows-service from 0.5.0 to 0.6.0", pr_number:                                              16736, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   14, deletions_count:   72},
		{sha: "82f13b615af898f5977dd14bb4ed6ca81c597d3c", date: "2023-03-10 00:46:34 UTC", description: "bump pest from 2.5.5 to 2.5.6", pr_number:                                                         16733, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "cf764991c1229a116dfc7b2661145d2484e7b8d3", date: "2023-03-10 05:49:19 UTC", description: "Validate only types when schema validation is true", pr_number:                                    16727, scopes: ["schemas"], type:                     "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:     3, insertions_count:   50, deletions_count:   28},
		{sha: "90db246de876b2de817253749dab130f1ef3ca84", date: "2023-03-10 06:41:01 UTC", description: "rename logdna to mezmo", pr_number:                                                                16488, scopes: ["logdna sink"], type:                 "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     16, insertions_count:  422, deletions_count:  60},
		{sha: "97f247238621128bf2475d480409363b67d2aedb", date: "2023-03-10 02:10:58 UTC", description: "bump indoc from 2.0.0 to 2.0.1", pr_number:                                                        16691, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   7, deletions_count:    7},
		{sha: "773526819f039b916dbda41aa05b70f30da82efc", date: "2023-03-10 02:23:01 UTC", description: "bump thiserror from 1.0.38 to 1.0.39", pr_number:                                                  16734, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   4, deletions_count:    4},
		{sha: "b3796788a029d3b7726405caf4117388ed5cc253", date: "2023-03-10 03:45:46 UTC", description: "bump serde_json from 1.0.93 to 1.0.94", pr_number:                                                 16694, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:   10, deletions_count:   10},
		{sha: "25b64cfab1623cdaa3434b79dd185306d82ad1fd", date: "2023-03-10 09:02:18 UTC", description: "Only validate definitions with Vector LogNamespace", pr_number:                                    16739, scopes: ["schemas"], type:                     "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:     2, insertions_count:   52, deletions_count:   34},
		{sha: "31908d82f05a34d139a42c2cdbfa7f215566dcfd", date: "2023-03-10 04:24:36 UTC", description: "bump opendal from 0.27.2 to 0.29.1", pr_number:                                                    16704, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   21, deletions_count:   11},
		{sha: "3e79d63c66febb64ff285b2eb4b0ab87db5ca3e0", date: "2023-03-10 05:10:24 UTC", description: "Rewrite scripts in vdev", pr_number:                                                               16661, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   15, insertions_count:  167, deletions_count:  104},
		{sha: "0eb721fbe4e24096b602d3afd2a72a22ef222aec", date: "2023-03-10 07:03:32 UTC", description: "bump aws-actions/configure-aws-credentials from 1.7.0 to 2.0.0", pr_number:                        16714, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   6, deletions_count:    6},
		{sha: "29fe2da51e04d11adbfd76fc72e381a6eb44524e", date: "2023-03-11 01:11:11 UTC", description: "bump pest_derive from 2.5.5 to 2.5.6", pr_number:                                                  16759, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   7, deletions_count:    7},
		{sha: "a96abfc4388900c700f445c3d4c95e744cb334ef", date: "2023-03-11 02:27:47 UTC", description: "Remove `Default` impl from owned paths", pr_number:                                                16728, scopes: ["vrl"], type:                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:         2, insertions_count:   2, deletions_count:    2},
		{sha: "2d476eab8b67b991c6987d7f1c04ae8db4a3e413", date: "2023-03-11 00:37:04 UTC", description: "restore POST as the correct default method", pr_number:                                            16746, scopes: ["http sink"], type:                   "fix", breaking_change:         false, author: "neuronull", files_count:          2, insertions_count:   10, deletions_count:   25},
		{sha: "e8ddc9c5fad750f3810b068bd54278e21dc1be8d", date: "2023-03-11 03:49:24 UTC", description: "Fix assume-role with access key, file-based auth profiles", pr_number:                             16715, scopes: ["aws service auth"], type:            "fix", breaking_change:         false, author: "Scott Balmos", files_count:       1, insertions_count:   63, deletions_count:   5},
		{sha: "827f8d8d2160836293739cfa7c538a31bbc2178b", date: "2023-03-11 17:25:38 UTC", description: "Add csv encoding for sinks", pr_number:                                                            16603, scopes: ["codec"], type:                       "feat", breaking_change:        false, author: "everpcpc", files_count:           33, insertions_count:  790, deletions_count:  10},
		{sha: "471e6c5b813a63b77b2f75b36fb98a8b019cea6b", date: "2023-03-11 06:00:27 UTC", description: "bump docker/setup-buildx-action from 2.4.1 to 2.5.0", pr_number:                                   16763, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "5b49afde5e556db837c2dfae538271555ae4aa4e", date: "2023-03-11 07:36:20 UTC", description: "bump serde_with from 2.2.0 to 2.3.0", pr_number:                                                   16757, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   13, deletions_count:   13},
		{sha: "23770c8a65c7f2ae612f793b6ec0fe4a81a20b2f", date: "2023-03-11 08:42:06 UTC", description: "Upgrade Vector to Rust 1.68 (clippy fixes only)", pr_number:                                       16745, scopes: ["core"], type:                        "chore", breaking_change:       false, author: "Nathan Fox", files_count:         66, insertions_count:  156, deletions_count:  338},
		{sha: "a01ed488aac0c5209e46fad7f5c1f09acccc87a9", date: "2023-03-11 08:49:47 UTC", description: "bump libc from 0.2.139 to 0.2.140", pr_number:                                                     16758, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "ae242ca2eb4989cf97870cbd8b1b696f23b8dcab", date: "2023-03-13 23:27:03 UTC", description: "bump rust_decimal from 1.28.1 to 1.29.0", pr_number:                                               16780, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "ce95090ca8ca47cd666a14b750b80189031e43e9", date: "2023-03-13 23:28:08 UTC", description: "bump quote from 1.0.23 to 1.0.25", pr_number:                                                      16778, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   137, deletions_count:  137},
		{sha: "662aed7835fe2a3bc05660ab2ea5451ddf97574e", date: "2023-03-13 23:28:39 UTC", description: "bump hyper from 0.14.24 to 0.14.25", pr_number:                                                    16777, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "5f0c72cb7d05bef674c38e2fa318b7dca3900c0d", date: "2023-03-13 23:33:39 UTC", description: "bump futures-util from 0.3.26 to 0.3.27", pr_number:                                               16776, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   16, deletions_count:   16},
		{sha: "2752cc3029d646abf473364753b7b5c41b02ea3e", date: "2023-03-14 00:13:12 UTC", description: "Use log namespacing and semantic meaning", pr_number:                                              16548, scopes: ["influxdb_logs sink"], type:          "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:    4, insertions_count:   227, deletions_count:  19},
		{sha: "c6e81d68a44872cfe673b395afe2a66a328553b1", date: "2023-03-14 07:52:20 UTC", description: "Fix panic in parsing `BulkConfig.index` by making `BulkConfig` non-optional.", pr_number:          16723, scopes: ["elasticsearch"], type:               "fix", breaking_change:         false, author: "Alexander Zaitsev", files_count:  6, insertions_count:   82, deletions_count:   72},
		{sha: "501cc69515d975a1c3c89e2dcd0dcbd4c4817123", date: "2023-03-13 23:01:03 UTC", description: "extract default_api_key into shared config struct", pr_number:                                     16750, scopes: ["datadog sinks"], type:               "chore", breaking_change:       true, author:  "neuronull", files_count:          8, insertions_count:   52, deletions_count:   85},
		{sha: "261bdba020c1fab5a24d118e90fb640b596fafea", date: "2023-03-14 01:42:15 UTC", description: "deprecate the `max_length` setting for `tcp` and `unix` modes", pr_number:                         16752, scopes: ["socket source"], type:               "chore", breaking_change:       false, author: "neuronull", files_count:          4, insertions_count:   37, deletions_count:   39},
		{sha: "d7d29a1881cb040d00ff6ab2dd4e28eebcf5696c", date: "2023-03-14 05:24:04 UTC", description: "bump serde from 1.0.154 to 1.0.155", pr_number:                                                    16773, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:   13, deletions_count:   13},
		{sha: "a71a190beba131fe4effdec0d942f6ab45cc2949", date: "2023-03-14 09:33:17 UTC", description: "bump futures from 0.3.26 to 0.3.27", pr_number:                                                    16774, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:   39, deletions_count:   39},
		{sha: "a89b90d66833b31b9f57a817977c40fd9bae4f5e", date: "2023-03-14 06:06:43 UTC", description: "Rewrite release-push.sh to vdev", pr_number:                                                       16724, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   6, insertions_count:   95, deletions_count:   50},
		{sha: "6edd9ace452596d414ea7b17601fec0998504da6", date: "2023-03-14 11:14:36 UTC", description: "Add container imageID field", pr_number:                                                           16769, scopes: ["kubernetes_logs"], type:             "feat", breaking_change:        false, author: "Maksim Nabokikh", files_count:    3, insertions_count:   59, deletions_count:   1},
		{sha: "386c29d4f49075dd245a1eba575e2596e2ac0b04", date: "2023-03-15 10:52:33 UTC", description: "Sinks webhdfs is not built", pr_number:                                                            16790, scopes: ["sinks"], type:                       "fix", breaking_change:         false, author: "Xuanwo", files_count:             7, insertions_count:   120, deletions_count:  37},
		{sha: "8a7b4e7178bb99ab219bf9f9ca51bee4b1123c31", date: "2023-03-14 23:24:12 UTC", description: "bump arbitrary from 1.2.3 to 1.3.0", pr_number:                                                    16789, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   4, deletions_count:    4},
		{sha: "09a9f1d0388fdbfe87221e9cb14eba02813a00f4", date: "2023-03-14 23:25:00 UTC", description: "bump serde_with from 2.3.0 to 2.3.1", pr_number:                                                   16788, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   13, deletions_count:   13},
		{sha: "5026db6a2ff0d27207d03b8b21ad9c4ad14b14bb", date: "2023-03-14 23:25:45 UTC", description: "bump listenfd from 1.0.0 to 1.0.1", pr_number:                                                     16787, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "75466b45aababa01fd728dce90bd906f8970b19b", date: "2023-03-14 23:26:29 UTC", description: "bump toml from 0.7.2 to 0.7.3", pr_number:                                                         16785, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   27, deletions_count:   27},
		{sha: "857c40670b6cc4a5ae80f0bada3f1efbfe0c38bd", date: "2023-03-14 23:27:10 UTC", description: "bump quote from 1.0.25 to 1.0.26", pr_number:                                                      16786, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   70, deletions_count:   70},
		{sha: "970e54269f42bf23bddef526c75ddf86a51630e1", date: "2023-03-15 03:14:56 UTC", description: "bump semver from 1.0.16 to 1.0.17", pr_number:                                                     16784, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "46a81f42fcf216c7365aeded27531e39ba5d32d2", date: "2023-03-15 08:34:56 UTC", description: "gelf encoder timestamp handling", pr_number:                                                       16749, scopes: ["gelf encoder"], type:                "fix", breaking_change:         false, author: "scMarkus", files_count:           1, insertions_count:   54, deletions_count:   0},
		{sha: "cb937fbf25a3ec88289c422c891affb880224040", date: "2023-03-15 04:38:07 UTC", description: "Override default description for timestamp", pr_number:                                            16799, scopes: ["journald source"], type:             "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:    1, insertions_count:   3, deletions_count:    1},
		{sha: "08c1a29ed0ad099e54675bec331cce159ea00601", date: "2023-03-15 22:54:12 UTC", description: "Name gardener workflows", pr_number:                                                               16803, scopes: ["ci"], type:                          "feat", breaking_change:        false, author: "Josh Soref", files_count:         2, insertions_count:   2, deletions_count:    0},
		{sha: "e8b855a3dbe3b4219bd3a178746c4b6cb1294ca3", date: "2023-03-15 23:03:40 UTC", description: "bump serde from 1.0.155 to 1.0.156", pr_number:                                                    16804, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:   13, deletions_count:   13},
		{sha: "afae677beb2ffc0d7f13fd0ff01af01cf2ae66bb", date: "2023-03-16 01:02:49 UTC", description: "Add schema generation visitor support", pr_number:                                                 16802, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   16, deletions_count:   4},
		{sha: "be9e2c43f2cab50ee26e5b1ce3859e852cde4211", date: "2023-03-16 11:17:38 UTC", description: "Rewrite compile-vrl-wasm.sh to vdev", pr_number:                                                   16751, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   4, insertions_count:   26, deletions_count:   33},
		{sha: "c15616f30d0d0fed45f15a7a027c7b5e63a13d1b", date: "2023-03-16 23:09:02 UTC", description: "bump openssl from 0.10.45 to 0.10.46", pr_number:                                                  16805, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   33, deletions_count:   23},
		{sha: "5a8b149ab0cbaec2fff6bcbd72601d4c6861e4a9", date: "2023-03-16 23:13:35 UTC", description: "bump assert_cmd from 2.0.8 to 2.0.9", pr_number:                                                   16815, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   15, deletions_count:   7},
		{sha: "cc1e0ef886154cd987735bc5bd92d6262a2abbeb", date: "2023-03-16 21:55:56 UTC", description: "Convert `Application::run` to an async fn", pr_number:                                             16811, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      3, insertions_count:   152, deletions_count:  146},
		{sha: "b25f857b4bd1d4b9996c6b64006f2a237a317d17", date: "2023-03-16 23:27:57 UTC", description: "Break up `Application::prepare_from_opts` a bit", pr_number:                                       16812, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      2, insertions_count:   79, deletions_count:   62},
		{sha: "e93b7ea322e36f139cc87341b209b383a0f5b77c", date: "2023-03-17 03:52:54 UTC", description: "Add link to rust crate docs in README", pr_number:                                                 16821, scopes: ["docs"], type:                        "chore", breaking_change:       false, author: "Nathan Fox", files_count:         1, insertions_count:   7, deletions_count:    6},
		{sha: "300a588ca581673a528778cf06780afc58a89a1e", date: "2023-03-17 04:26:40 UTC", description: "Add docs to the `lookup` crate", pr_number:                                                        16717, scopes: ["docs"], type:                        "chore", breaking_change:       false, author: "Nathan Fox", files_count:         1, insertions_count:   85, deletions_count:   2},
		{sha: "d60175984863b56c0eaa762422886513d02b8c0d", date: "2023-03-17 01:54:44 UTC", description: "add initial support for source metrics", pr_number:                                                16720, scopes: ["component validation tests"], type:  "feat", breaking_change:        false, author: "David Huie", files_count:         17, insertions_count:  759, deletions_count:  138},
		{sha: "c59d71cd76efa18807f772da4520208b4b5898f4", date: "2023-03-17 06:11:20 UTC", description: "Rewrite ci-generate-publish-metadata", pr_number:                                                  16765, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   7, insertions_count:   61, deletions_count:   34},
		{sha: "5cd741ec600b4bddd2513a1f929759642a483a80", date: "2023-03-17 06:13:00 UTC", description: "Break up application config setup more", pr_number:                                                16820, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      2, insertions_count:   166, deletions_count:  133},
		{sha: "b7c4d4c5add69d0adf6ffb8d4319df1984de70a3", date: "2023-03-17 23:41:27 UTC", description: "provide a description to the compression field", pr_number:                                        16819, scopes: ["http sink"], type:                   "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:     1, insertions_count:   2, deletions_count:    1},
		{sha: "7a96b2b9ff8787adebcefcb6335dbb8ff02b06a2", date: "2023-03-18 05:21:49 UTC", description: "Rename source_id -> source_ip", pr_number:                                                         16836, scopes: ["syslog"], type:                      "fix", breaking_change:         false, author: "Alexander Zaitsev", files_count:  1, insertions_count:   3, deletions_count:    3},
		{sha: "09fbada8b1f80209811c1471a5555d4164791e55", date: "2023-03-18 16:04:01 UTC", description: "format timestamp with utc in csv", pr_number:                                                      16828, scopes: ["codec"], type:                       "fix", breaking_change:         false, author: "everpcpc", files_count:           1, insertions_count:   5, deletions_count:    2},
		{sha: "a5f731543718142c5f259e3ee6daa2e5010f1e3d", date: "2023-03-18 11:01:07 UTC", description: "fix rustdoc errors", pr_number:                                                                    16855, scopes: ["docs"], type:                        "fix", breaking_change:         false, author: "Stephen Wakely", files_count:     25, insertions_count:  48, deletions_count:   45},
		{sha: "9cbb4911b9d74cd7d939b35cafe79c4d00a33c0a", date: "2023-03-21 03:13:33 UTC", description: "add `source_ip` to `syslog` schema", pr_number:                                                    16837, scopes: ["syslog"], type:                      "fix", breaking_change:         false, author: "Stephen Wakely", files_count:     2, insertions_count:   27, deletions_count:   2},
		{sha: "1e848f656508e6cec5dec9b90a117d4fbbc04148", date: "2023-03-21 04:17:18 UTC", description: "systemd service restart always", pr_number:                                                        16822, scopes: ["service"], type:                     "enhancement", breaking_change: false, author: "kannar", files_count:             1, insertions_count:   5, deletions_count:    2},
		{sha: "2cbcee9040739b2fc3344eb82ddf9c7e73495745", date: "2023-03-21 00:45:01 UTC", description: "bump opendal from 0.30.2 to 0.30.3", pr_number:                                                    16866, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   16, deletions_count:   6},
		{sha: "b1422c0be18d5f3f0f0dbe844b5897d77299d6b3", date: "2023-03-21 00:45:46 UTC", description: "bump bstr from 1.3.0 to 1.4.0", pr_number:                                                         16865, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   5, deletions_count:    5},
		{sha: "9088585dcbd6b8466353ab268b1704ea0ceb8862", date: "2023-03-21 00:46:53 UTC", description: "bump openssl from 0.10.46 to 0.10.47", pr_number:                                                  16863, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   6, deletions_count:    6},
		{sha: "ba9a211ae82cfec847243e585f6953cca191d253", date: "2023-03-21 05:41:08 UTC", description: "disallow unevaluated properties in generated schema", pr_number:                                   16856, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      6, insertions_count:   282, deletions_count:  62},
		{sha: "a8505c28597cbe591ce6cc4f805463d5c482027d", date: "2023-03-21 04:02:49 UTC", description: "Break up application config setup", pr_number:                                                     16826, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      4, insertions_count:   109, deletions_count:  72},
		{sha: "ab678631a7bfc64db5dccf09de805c83b7ee587b", date: "2023-03-21 06:09:05 UTC", description: "bump serde from 1.0.156 to 1.0.158", pr_number:                                                    16869, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:   25, deletions_count:   14},
		{sha: "210c0f23a7daa5bc9529da7dd734163c6ab273a6", date: "2023-03-22 03:39:32 UTC", description: "fix panic under low resources", pr_number:                                                         16858, scopes: ["panic"], type:                       "fix", breaking_change:         false, author: "Alexander Zaitsev", files_count:  1, insertions_count:   4, deletions_count:    1},
		{sha: "d8fdeb05554c3f66394744044cb2ee8a95b735a4", date: "2023-03-22 05:59:23 UTC", description: "log received signals", pr_number:                                                                  16835, scopes: ["signals"], type:                     "feat", breaking_change:        false, author: "Alexander Zaitsev", files_count:  1, insertions_count:   16, deletions_count:   4},
		{sha: "b5323870fe14d135667cab4253206da4ee84cd19", date: "2023-03-21 22:39:00 UTC", description: "Move the `build` subcommand to `build vector`", pr_number:                                         16877, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      2, insertions_count:   5, deletions_count:    1},
		{sha: "88cc085ba9680124db4788af9f2184c190e9354e", date: "2023-03-21 23:08:40 UTC", description: "Simplify parameter to `ApplicationConfig::from_opts`", pr_number:                                  16876, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   9, deletions_count:    6},
		{sha: "71aaa640c1db986d2ee61cf9e39435ac286f7e93", date: "2023-03-22 04:39:17 UTC", description: "Specify journald source requirements", pr_number:                                                  16892, scopes: [], type:                              "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:    1, insertions_count:   6, deletions_count:    1},
		{sha: "9aa44d2af69b0e90b55fda8726db1672f89140de", date: "2023-03-22 04:21:03 UTC", description: "Split up `Application::run` into logical phases", pr_number:                                       16873, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      5, insertions_count:   198, deletions_count:  118},
		{sha: "6c44ed07d0999d69b0b1958b6c8034aada75c54f", date: "2023-03-22 05:31:19 UTC", description: "dpkg install docs use the correct package name", pr_number:                                        16896, scopes: ["docs", "releasing"], type:           "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      2, insertions_count:   4, deletions_count:    4},
		{sha: "318b2337c2dff1997665d61e47b79df9548a642d", date: "2023-03-22 03:44:37 UTC", description: "bump webbrowser from 0.8.7 to 0.8.8", pr_number:                                                   16830, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   6, deletions_count:    4},
		{sha: "26de09cedbb7574e2bca62d1f506649131e26ec9", date: "2023-03-22 03:44:48 UTC", description: "bump clap_complete from 4.1.4 to 4.1.5", pr_number:                                                16832, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "ca94543d12591490d3adc6b285e473d5c37db237", date: "2023-03-22 03:45:00 UTC", description: "bump assert_cmd from 2.0.9 to 2.0.10", pr_number:                                                  16833, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "d74d64c7fe791a8099621bcf24589ecf19a03ee9", date: "2023-03-22 03:45:21 UTC", description: "bump clap from 4.1.8 to 4.1.11", pr_number:                                                        16867, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    7, insertions_count:   52, deletions_count:   46},
		{sha: "6a387e2efda82569698eb5d7c21ef759b1d89275", date: "2023-03-22 03:45:38 UTC", description: "bump os_info from 3.6.0 to 3.7.0", pr_number:                                                      16878, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "92d857c7d6f38609c07aff43830923512969f2b3", date: "2023-03-22 03:45:56 UTC", description: "bump ordered-float from 3.4.0 to 3.6.0", pr_number:                                                16879, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    7, insertions_count:   19, deletions_count:   19},
		{sha: "73ce050a22ed059f00b3ea7eb8fd75c1cf845b1b", date: "2023-03-22 03:46:14 UTC", description: "bump typetag from 0.2.6 to 0.2.7", pr_number:                                                      16881, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   7, deletions_count:    7},
		{sha: "07520b944137855ac3bb0c3fc92a350eaf6ee78c", date: "2023-03-22 03:46:50 UTC", description: "bump async-recursion from 1.0.2 to 1.0.4", pr_number:                                              16880, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "e5a96a050212e2dbea2089f5cfd406a9a789c9fe", date: "2023-03-22 05:57:56 UTC", description: "Make the `test-behavior` test build a little faster", pr_number:                                   16897, scopes: ["tests"], type:                       "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   3, deletions_count:    3},
		{sha: "a362fb81b844db591025e2ecef22e92ae5b0fe17", date: "2023-03-22 11:51:31 UTC", description: "[revert] Split up `Application::run` into logical phases", pr_number:                              16900, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      5, insertions_count:   118, deletions_count:  198},
		{sha: "dbb08a2ae8db3916e912ff65a5fd5b892b02f63c", date: "2023-03-22 19:52:17 UTC", description: "bump anyhow from 1.0.69 to 1.0.70", pr_number:                                                     16902, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "a328715a27f48980f5d941e2e7ca35a7ec3495a2", date: "2023-03-22 19:52:31 UTC", description: "bump regex from 1.7.1 to 1.7.2", pr_number:                                                        16903, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:   8, deletions_count:    8},
		{sha: "1a923316295c06ecb7b995939769cf4b20e5745a", date: "2023-03-22 19:52:50 UTC", description: "bump directories from 4.0.1 to 5.0.0", pr_number:                                                  16905, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   25, deletions_count:   5},
		{sha: "a448163190d8ea13a77574504de785e27eb398a6", date: "2023-03-22 19:53:11 UTC", description: "bump thiserror from 1.0.39 to 1.0.40", pr_number:                                                  16906, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   5, deletions_count:    5},
		{sha: "e7c8a9d228d20fce13803d52b22ac0d0540a0abf", date: "2023-03-22 19:53:36 UTC", description: "bump async-trait from 0.1.66 to 0.1.67", pr_number:                                                16908, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "4dbba5e5b00f9c070f208ece78534372b62390a5", date: "2023-03-22 19:53:53 UTC", description: "bump reqwest from 0.11.14 to 0.11.15", pr_number:                                                  16909, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "8270807a8c1fcf670cb77d47ee81804a2c7bd44e", date: "2023-03-23 11:47:49 UTC", description: "looser prost version requirement", pr_number:                                                      16862, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "Ning Sun", files_count:           4, insertions_count:   11, deletions_count:   11},
		{sha: "92d6dde8f1d09baa509a73e8214cfae46a93f815", date: "2023-03-23 00:35:13 UTC", description: "Migrate `LogSchema` `timestamp` to new lookup code", pr_number:                                    16839, scopes: ["core"], type:                        "chore", breaking_change:       false, author: "Nathan Fox", files_count:         60, insertions_count:  828, deletions_count:  412},
		{sha: "e85ae9cc5141152494459e9fd0396b4fc186bd06", date: "2023-03-23 01:07:19 UTC", description: "Unify the http component descriptions", pr_number:                                                 16911, scopes: [], type:                              "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:    3, insertions_count:   6, deletions_count:    6},
		{sha: "532e9da0a449cd2a3352179938f22557c4e21933", date: "2023-03-23 02:04:10 UTC", description: "VRL separation part 3 (vector-common)", pr_number:                                                 16684, scopes: ["vrl"], type:                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:         74, insertions_count:  334, deletions_count:  309},
		{sha: "9ef8323abc1b95dabe301d210820a9e692195723", date: "2023-03-23 12:04:29 UTC", description: "Rename `generate` subcommands to `build`", pr_number:                                              16917, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      8, insertions_count:   27, deletions_count:   31},
		{sha: "28195600e487a1524c1212e873e4e7105fda40f3", date: "2023-03-23 12:05:44 UTC", description: "Break out API startup from `Application::run`", pr_number:                                         16915, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   41, deletions_count:   43},
		{sha: "580bce73b822250e889d6e7b4651c888cef40638", date: "2023-03-23 12:26:58 UTC", description: "Add subcommand to report on crate versions", pr_number:                                            16898, scopes: ["vdev"], type:                        "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:      8, insertions_count:   86, deletions_count:   15},
		{sha: "9f4ab64ad47ba07ad7e1a133a5ae2504e4c05267", date: "2023-03-23 21:16:35 UTC", description: "bump proc-macro2 from 1.0.52 to 1.0.53", pr_number:                                                16922, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   68, deletions_count:   68},
		{sha: "0097e001ab0b419a8de777537a604c76d699e427", date: "2023-03-24 01:11:53 UTC", description: "VRL separation part 4 (cli)", pr_number:                                                           16918, scopes: ["vrl"], type:                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:         19, insertions_count:  454, deletions_count:  40},
		{sha: "4293f527e191abe4263d70f15542c4035b21605e", date: "2023-03-23 23:29:28 UTC", description: "Pass an explicit runtime into functions that require it to be running", pr_number:                 16921, scopes: ["core"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      2, insertions_count:   39, deletions_count:   37},
		{sha: "befedfea9a8ca0504d1d625ae1434eee3dc74d34", date: "2023-03-24 01:56:23 UTC", description: "Update kafka components to have mirrored descriptions", pr_number:                                 16894, scopes: [], type:                              "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "fd45828af156926be387fcfc20cac65fce225aab", date: "2023-03-24 02:54:52 UTC", description: "Add `hmac` function to calculate HMACs (#10333)", pr_number:                                       15371, scopes: ["vrl"], type:                         "feat", breaking_change:        false, author: "Scott Balmos", files_count:       6, insertions_count:   262, deletions_count:  0},
		{sha: "46f11f22617ec7ac6daf80f715ecc524427ce6a9", date: "2023-03-24 02:54:22 UTC", description: "Break up app startup from running", pr_number:                                                     16927, scopes: ["startup"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      4, insertions_count:   63, deletions_count:   28},
		{sha: "6ccb825a579eb4b21a8a5c9f52b5d83e3fc1ff1e", date: "2023-03-24 04:19:25 UTC", description: "Break out shutdown from main running loop", pr_number:                                             16932, scopes: ["core"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      2, insertions_count:   68, deletions_count:   21},
		{sha: "44d11f27c432883ddc9da3ce9c8b98bb26dc70c4", date: "2023-03-24 19:43:11 UTC", description: "bump openssl from 0.10.47 to 0.10.48", pr_number:                                                  16941, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   6, deletions_count:    6},
		{sha: "78bcd5ea4107cab77eb732fa8d22288b80d3b6c3", date: "2023-03-24 22:46:02 UTC", description: "VRL separation part 5 (tests)", pr_number:                                                         16931, scopes: ["vrl"], type:                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:         15, insertions_count:  1093, deletions_count: 482},
		{sha: "3c03350cf6b9c4a2eeffb39be46e0120e2680baa", date: "2023-03-24 23:09:08 UTC", description: "Re-triage done issues with new comment", pr_number:                                                15720, scopes: ["dev"], type:                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   66, deletions_count:   0},
		{sha: "0c07ce25555868f83b7effecb552650d36be6ba1", date: "2023-03-24 23:28:27 UTC", description: "Remove awaiting-author label when PR is updated", pr_number:                                       16838, scopes: [], type:                              "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   14, deletions_count:   0},
		{sha: "71bfadd2f42b0e45e0afb740a7b558abba052683", date: "2023-03-24 23:38:54 UTC", description: "Fix Value::Float comparisons", pr_number:                                                          16926, scopes: ["core"], type:                        "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      2, insertions_count:   4, deletions_count:    138},
		{sha: "22bdc78c62f4db2086463680de4796186364c2a1", date: "2023-03-24 23:45:39 UTC", description: "Log tui errors encountered during `vector top`", pr_number:                                        16857, scopes: ["cli"], type:                         "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:      1, insertions_count:   2, deletions_count:    2},
		{sha: "b9412db27f64bac78faa7918691bb36644537c52", date: "2023-03-24 23:51:58 UTC", description: "Ignore spelling in gardner workflow file", pr_number:                                              16947, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   1, deletions_count:    0},
		{sha: "c77c65c8a31b139164db95e5a2ee130dc3fd1eaf", date: "2023-03-24 23:31:16 UTC", description: "Simplify calling convention for `init_log_schema`", pr_number:                                     16938, scopes: ["core"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      4, insertions_count:   16, deletions_count:   43},
		{sha: "dc7214817df979ca4e8a895447bce5368e0af1af", date: "2023-03-24 23:31:34 UTC", description: "Clean up helper environment variable after schema gen", pr_number:                                 16937, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   6, deletions_count:    4},
		{sha: "549cf7300122639cffc7d31a916f32cf835c769f", date: "2023-03-25 00:41:28 UTC", description: "Increase max value count of summary", pr_number:                                                   16946, scopes: ["prometheus_scrape source"], type:    "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      2, insertions_count:   5, deletions_count:    69},
		{sha: "5cb09e75e318445c89b9f0e3afb9448859c130c8", date: "2023-03-25 01:57:46 UTC", description: "(revert) Re-triage done issues with new comment", pr_number:                                       16950, scopes: ["dev"], type:                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   0, deletions_count:    66},
		{sha: "81e0fcf637c805e1150f70b2751ff17945f705a1", date: "2023-03-25 01:46:05 UTC", description: "bump actions/checkout from 2 to 3", pr_number:                                                     16952, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "0a43987701fc08eee9440e822b045a632b248016", date: "2023-03-25 04:55:30 UTC", description: "Ignore gardener issue comment workflow again", pr_number:                                          16948, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "ad3b913803fa3cbd9e471187aab933d0a17be764", date: "2023-03-25 06:24:22 UTC", description: "Add additional random functions", pr_number:                                                       16768, scopes: ["vrl stdlib"], type:                  "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:      10, insertions_count:  458, deletions_count:  0},
		{sha: "45b9d8941f1a9067745e859775ab6171dd9748b3", date: "2023-03-25 07:15:29 UTC", description: "Remove experimental reload API", pr_number:                                                        16955, scopes: ["administration"], type:              "chore", breaking_change:       true, author:  "Bruce Guenter", files_count:      10, insertions_count:  97, deletions_count:   385},
		{sha: "928c96e8f6501777316dd39a967ea334e5e3460d", date: "2023-03-25 12:53:58 UTC", description: "VRL separation part 6 (final cleanup)", pr_number:                                                 16953, scopes: ["vrl"], type:                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:         66, insertions_count:  49, deletions_count:   31},
		{sha: "c98151b7a9ce8bb8b2b234ebe822c83aa3a6f015", date: "2023-03-27 23:32:28 UTC", description: "Ignore RUSTSEC-2023-0029", pr_number:                                                              16969, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:   9, deletions_count:    0},
		{sha: "8274d9aca254f7d3651ad49f42fbdbf7c3050a2a", date: "2023-03-28 08:18:39 UTC", description: "use date from 1999 days ago for auto_extracted timestamp integration test", pr_number:             16970, scopes: ["splunk_hec sink"], type:             "fix", breaking_change:         false, author: "Stephen Wakely", files_count:     1, insertions_count:   13, deletions_count:   8},
		{sha: "8f288df7fd33da546f4d16da9d0d5bac634c4f76", date: "2023-03-28 02:21:11 UTC", description: "bump quanta from 0.10.1 to 0.11.0", pr_number:                                                     16961, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   28, deletions_count:   3},
		{sha: "023355a63e19752dcf6fc069ea3686a7d3ec9a2a", date: "2023-03-28 02:21:24 UTC", description: "bump proc-macro2 from 1.0.53 to 1.0.54", pr_number:                                                16962, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   68, deletions_count:   68},
		{sha: "8a4b319488092e39136b4a3a31d344a2d1e10c9b", date: "2023-03-28 02:21:36 UTC", description: "bump rust_decimal from 1.29.0 to 1.29.1", pr_number:                                               16963, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "6cdf4e56ca005d9766d56a1a2b2131622fd36089", date: "2023-03-28 05:43:23 UTC", description: "add support for generating ambiguous enum schemas via `anyOf` instead of `oneOf`", pr_number:      16913, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Toby Lawrence", files_count:      9, insertions_count:   311, deletions_count:  121},
		{sha: "0a8d3efda69fa918d475f901825e854278c05c1c", date: "2023-03-28 11:05:03 UTC", description: "bump async-graphql from 5.0.6 to 5.0.7", pr_number:                                                16958, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:   12, deletions_count:   12},
		{sha: "8afce37608b95b18dd97be6aca8fd6a46d026eaa", date: "2023-03-28 08:42:24 UTC", description: "remove VRL", pr_number:                                                                            16971, scopes: ["vrl"], type:                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:         692, insertions_count: 46, deletions_count:   88603},
		{sha: "96a059aa7ad4a5ebe6c41b9e037da56bfda7fe1f", date: "2023-03-29 01:58:03 UTC", description: "editorial review of transforms and sources", pr_number:                                            16934, scopes: ["docs"], type:                        "chore", breaking_change:       false, author: "May Lee", files_count:            151, insertions_count: 1057, deletions_count: 1079},
		{sha: "1cc5f10e88bdf1e1e7b9825d36eac2522c5f678d", date: "2023-03-28 23:00:22 UTC", description: "bump regex from 1.7.2 to 1.7.3", pr_number:                                                        16959, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:   6, deletions_count:    6},
		{sha: "238d4bde53b5e729dab4c564214d1e74f880de9e", date: "2023-03-28 23:01:12 UTC", description: "bump lalrpop-util from 0.19.8 to 0.19.9", pr_number:                                               16960, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "dde546349418293a3dc24bb3138c26f560cb4dfa", date: "2023-03-28 23:02:56 UTC", description: "bump async-trait from 0.1.67 to 0.1.68", pr_number:                                                16975, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   10, deletions_count:   10},
		{sha: "dceb42aa56b3a958d89ea97f50f23b81c4e44c52", date: "2023-03-28 23:03:12 UTC", description: "bump async-graphql-warp from 5.0.6 to 5.0.7", pr_number:                                           16976, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    35},
		{sha: "112358e69ed937d0751a4e70af1cc57a5f1cf4f0", date: "2023-03-29 06:34:59 UTC", description: "bump opendal from 0.30.3 to 0.30.4", pr_number:                                                    16974, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "6542778af87ec8324ff1e75b9e68cb3251d1931c", date: "2023-03-29 07:19:55 UTC", description: "Add Github Workflow to publish custom builds from a branch.", pr_number:                           16281, scopes: ["ci"], type:                          "enhancement", breaking_change: false, author: "neuronull", files_count:          19, insertions_count:  176, deletions_count:  59},
		{sha: "cfc36fe27ad21efbf5cd4aa62b72e01124e216f3", date: "2023-03-29 20:01:48 UTC", description: "bump indexmap from 1.9.2 to 1.9.3", pr_number:                                                     16988, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   7, deletions_count:    7},
		{sha: "066f027fc88596f82cef22f2e6d0a466a2273dc6", date: "2023-03-29 20:02:02 UTC", description: "bump inventory from 0.3.4 to 0.3.5", pr_number:                                                    16989, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   6, deletions_count:    6},
		{sha: "2448996bec3c9de9de32de1aab8a789d795a3c89", date: "2023-03-29 20:02:15 UTC", description: "bump serde from 1.0.158 to 1.0.159", pr_number:                                                    16990, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:   11, deletions_count:   11},
		{sha: "ac6463bf140f359529f026018ef0057c9bb34bfc", date: "2023-03-29 20:02:36 UTC", description: "bump clap from 4.1.11 to 4.1.14", pr_number:                                                       16991, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    7, insertions_count:   65, deletions_count:   66},
		{sha: "11cb5f7463eb86d8ee064784c1741853f58484fe", date: "2023-03-29 20:02:56 UTC", description: "bump reqwest from 0.11.15 to 0.11.16", pr_number:                                                  16992, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   4, deletions_count:    4},
		{sha: "d9a3661c74e4b87680e6f5e97611c0a297748cbe", date: "2023-03-29 23:55:48 UTC", description: "(revert) Add Github Workflow to publish custom builds from a branch.", pr_number:                  16996, scopes: ["ci"], type:                          "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:      19, insertions_count:  59, deletions_count:   176},
		{sha: "11fb1aaf33125338a680596d74691e6ef71e3017", date: "2023-03-30 06:15:47 UTC", description: "bump serde_json from 1.0.94 to 1.0.95", pr_number:                                                 16977, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:   7, deletions_count:    7},
		{sha: "0c22536c7d6eac57b9dd88d3e2a9b314a703a93d", date: "2023-03-30 04:44:22 UTC", description: "aws_ecs_metrics modified to push precpu_stats", pr_number:                                         16985, scopes: ["sources"], type:                     "feat", breaking_change:        false, author: "Ankit Singh", files_count:        1, insertions_count:   176, deletions_count:  6},
		{sha: "6fafdbd9ed070467d8dd68960562f499a735c6d1", date: "2023-03-30 22:50:27 UTC", description: "Exit early if AXIOM_TOKEN isn't set in integration tests", pr_number:                              16997, scopes: ["axiom sink"], type:                  "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   1, deletions_count:    0},
		{sha: "2b290f775c21373dd2d6b802fe84f0a16837c554", date: "2023-03-30 20:29:48 UTC", description: "bump clap_complete from 4.1.5 to 4.2.0", pr_number:                                                17002, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "466ab931de631db2fc4b594d6b6c970b3be37d17", date: "2023-03-30 20:30:09 UTC", description: "bump tempfile from 3.4.0 to 3.5.0", pr_number:                                                     17003, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   52, deletions_count:   12},
		{sha: "401974ebd11b46518dc58a544e11b5f917626d45", date: "2023-03-31 01:15:34 UTC", description: "Update stars and contributor counts on the website", pr_number:                                    17007, scopes: [], type:                              "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "06f15e7ccf067e33b654c5db2187e8bae31091cd", date: "2023-03-31 05:53:35 UTC", description: "make FinalizerSet optionally handle shutdown signals", pr_number:                                  16928, scopes: ["file source"], type:                 "fix", breaking_change:         false, author: "j chesley", files_count:          10, insertions_count:  78, deletions_count:   15},
		{sha: "84773f8535cf4cfaa284ff7a9ef1a4b8998f6b0e", date: "2023-03-31 06:43:32 UTC", description: "avoid resetting `unevaluatedProperties` value on subsequent visits", pr_number:                    17008, scopes: ["config"], type:                      "fix", breaking_change:         false, author: "Toby Lawrence", files_count:      4, insertions_count:   390, deletions_count:  25},
		{sha: "018637ad93ebb74bcec597e89d33127ac83202d8", date: "2023-03-31 13:57:28 UTC", description: "Revamp capture_output() to propagate errors", pr_number:                                           16981, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   4, insertions_count:   51, deletions_count:   23},
		{sha: "ff7f3492ce62f2e858578a8fa08b82d930586b27", date: "2023-03-31 20:06:49 UTC", description: "bump opendal from 0.30.4 to 0.30.5", pr_number:                                                    17016, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   2, deletions_count:    2},
		{sha: "106a06d7b0c7718eda6c3d902584ff513c12af00", date: "2023-03-31 20:07:28 UTC", description: "bump aws-sigv4 from 0.53.0 to 0.55.0", pr_number:                                                  17019, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   18, deletions_count:   42},
		{sha: "b7abb081ca67a2640b3cda49bded720dc8002813", date: "2023-04-01 07:27:07 UTC", description: "Add backoff for watchers", pr_number:                                                              17009, scopes: ["kubernetes_logs"], type:             "feat", breaking_change:        false, author: "Maksim Nabokikh", files_count:    1, insertions_count:   7, deletions_count:    4},
		{sha: "df7663a2b453d2883671fb8956f80deb5a896f18", date: "2023-04-01 13:52:52 UTC", description: "support csv encoding & compression none", pr_number:                                               16829, scopes: ["databend sink"], type:               "feat", breaking_change:        false, author: "everpcpc", files_count:           10, insertions_count:  391, deletions_count:  108},
		{sha: "dd3ca65d8fe27af010eeb3ea196e721cb4ebe6ea", date: "2023-04-01 01:53:54 UTC", description: "Store failure output for junit artifact", pr_number:                                               17023, scopes: [], type:                              "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   1, deletions_count:    0},
		{sha: "00c0316b0ca47ec77720912569f69708221f4bfe", date: "2023-04-01 02:02:54 UTC", description: "Expose a couple of schema helper functions", pr_number:                                            17010, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   13, deletions_count:   2},
		{sha: "e19f4fca320d817fd58ce2bdf429ec3384a3eacb", date: "2023-04-04 04:28:53 UTC", description: "Update transforms to handle multiple definitions", pr_number:                                      16793, scopes: ["topology"], type:                    "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:     87, insertions_count:  2071, deletions_count: 1564},
		{sha: "39c235e6299d716b5f5a2e7174786dee290f2885", date: "2023-04-03 23:36:59 UTC", description: "remove un-utilized env flag for limiting concurrency", pr_number:                                  16972, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "neuronull", files_count:          3, insertions_count:   0, deletions_count:    3},
		{sha: "9dbfc4fcc055feaca973a763b54086f08540bf58", date: "2023-04-04 00:55:11 UTC", description: "bump clap-verbosity-flag from 2.0.0 to 2.0.1", pr_number:                                          17036, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "beec486a4bfcbc6ade4c2eb519deb00c58862099", date: "2023-04-04 00:56:28 UTC", description: "bump proc-macro2 from 1.0.54 to 1.0.55", pr_number:                                                17035, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   65, deletions_count:   65},
		{sha: "11588da3bf52ae40529ef7d09c7eeaee3bf1933e", date: "2023-04-04 01:38:22 UTC", description: "Re-add custom builds workflow. Fixes integration test build error.", pr_number:                    17024, scopes: ["ci"], type:                          "fix", breaking_change:         false, author: "neuronull", files_count:          21, insertions_count:  177, deletions_count:  60},
		{sha: "0c170bc429db17f825ec8e837c9a0e0d1bae8554", date: "2023-04-04 08:03:53 UTC", description: "bump futures-util from 0.3.27 to 0.3.28", pr_number:                                               17018, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:   17, deletions_count:   17},
		{sha: "d4128ef70c732315d2ce0c0cdb714b57b0cec095", date: "2023-04-04 02:43:25 UTC", description: "Reintroduce the topology controller", pr_number:                                                   17027, scopes: ["topology"], type:                    "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      3, insertions_count:   197, deletions_count:  104},
		{sha: "f843ff6bfd2cf0ef85fcd66f8c9763919f495aeb", date: "2023-04-04 03:21:00 UTC", description: "bump actions/add-to-project from 0.4.1 to 0.5.0", pr_number:                                       17039, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "21fa2403a6eafb14e3b0f0f0fb45b855ba9e8945", date: "2023-04-04 07:48:59 UTC", description: "Rewrite release-homebrew into vdev", pr_number:                                                    16772, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   6, insertions_count:   126, deletions_count:  44},
		{sha: "56208cc9bded7fd5a12c827f5198f6c03e7f2a32", date: "2023-04-04 07:49:32 UTC", description: "Make network non optional in Compose struct", pr_number:                                           17038, scopes: ["vdev"], type:                        "chore", breaking_change:       false, author: "Jonathan Padilla", files_count:   1, insertions_count:   5, deletions_count:    6},
		{sha: "f2479de665fb0e605c58decb56a28a352a0183dd", date: "2023-04-04 21:40:23 UTC", description: "Simplify log schema init", pr_number:                                                              17012, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      4, insertions_count:   18, deletions_count:   18},
		{sha: "af0228eb7b129e5b3819feea17395f51fe18988f", date: "2023-04-05 00:10:54 UTC", description: "fix ComponentValidation's from_transform", pr_number:                                              17045, scopes: [], type:                              "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "c38f33d8516345a2dd011557895ecc96bfff40ba", date: "2023-04-05 13:46:39 UTC", description: "update chrono to 0.4.24 and resolve warnings", pr_number:                                          17022, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "Ning Sun", files_count:           31, insertions_count:  241, deletions_count:  162},
		{sha: "c854fc6e171c282213f3b06b9c61449e2b68ccb5", date: "2023-04-05 02:47:25 UTC", description: "Convert top-level sources enum to typetag", pr_number:                                             17044, scopes: ["config"], type:                      "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    59, insertions_count:  196, deletions_count:  430},
		{sha: "7e79fcad2a56317dc0e23e5890e29a9b4888d572", date: "2023-04-05 02:15:17 UTC", description: "downgrade 16 core to 8 core runners", pr_number:                                                   16999, scopes: ["ci"], type:                          "enhancement", breaking_change: false, author: "neuronull", files_count:          5, insertions_count:   19, deletions_count:   19},
		{sha: "fc028c9b7150d5bcade47c77e739d765f1d7f4b7", date: "2023-04-05 05:25:22 UTC", description: "Add Vector VRL web playground", pr_number:                                                         17042, scopes: [], type:                              "chore", breaking_change:       false, author: "Nathan Fox", files_count:         33, insertions_count:  1805, deletions_count: 139},
		{sha: "5074d82fb736e33b975372958bb3b3d13f68aff2", date: "2023-04-05 06:07:58 UTC", description: "Remove dh as a codeowner.", pr_number:                                                             17053, scopes: ["administration"], type:              "chore", breaking_change:       false, author: "neuronull", files_count:          1, insertions_count:   18, deletions_count:   18},
		{sha: "db2572590205228e2c9b8b2b26340f4aec31f62f", date: "2023-04-06 05:17:24 UTC", description: "Add AppSignal sink", pr_number:                                                                    16650, scopes: ["new sink"], type:                    "feat", breaking_change:        false, author: "Tom de Bruijn", files_count:      14, insertions_count:  772, deletions_count:  1},
		{sha: "c4c16dd8b4ddc513fff2dd11933457eb58d27f01", date: "2023-04-05 21:29:07 UTC", description: "bump futures from 0.3.27 to 0.3.28", pr_number:                                                    17054, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:   39, deletions_count:   39},
		{sha: "00e0d9041c8494cfa947a685e11c87398843762d", date: "2023-04-05 22:35:12 UTC", description: "add the env var for `appsignal` int test workflow from the secrets ", pr_number:                   17058, scopes: ["ci"], type:                          "fix", breaking_change:         false, author: "neuronull", files_count:          1, insertions_count:   1, deletions_count:    0},
		{sha: "6e6f1eb590146ce57e699ff6fc7922314abc892a", date: "2023-04-06 07:21:18 UTC", description: "split `build_pieces` into smaller functions", pr_number:                                           17037, scopes: ["topology"], type:                    "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     1, insertions_count:   518, deletions_count:  467},
		{sha: "88d6aec7992f8ae243628f891bd2ba845ecf9c40", date: "2023-04-06 01:18:17 UTC", description: "Add `Send` bound on `IntoBuffer` trait", pr_number:                                                17062, scopes: ["buffers"], type:                     "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:   1, deletions_count:    1},
		{sha: "3b496606c145ef9cb15dd6bf36d77667009ba284", date: "2023-04-06 03:39:54 UTC", description: "bump peter-evans/create-or-update-comment from 2 to 3", pr_number:                                 17064, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "b25a0fb005698437c18ce7d7c3969d4d17b47a06", date: "2023-04-06 06:36:23 UTC", description: "editorial review of sinks descriptions", pr_number:                                                17041, scopes: ["docs"], type:                        "chore", breaking_change:       false, author: "May Lee", files_count:            99, insertions_count:  712, deletions_count:  705},
		{sha: "c310b37fb20eb312643732f096a2f5b35e93e1de", date: "2023-04-06 23:34:45 UTC", description: "add error detail to SUPPORT.md", pr_number:                                                        17072, scopes: ["docs"], type:                        "chore", breaking_change:       false, author: "neuronull", files_count:          1, insertions_count:   3, deletions_count:    0},
		{sha: "9cdee35cde962585077f41fdf41361f02be08639", date: "2023-04-07 06:08:13 UTC", description: "bump warp from 0.3.3 to 0.3.4", pr_number:                                                         17055, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   8, deletions_count:    39},
		{sha: "3c6545ce25f5fb7f6e49272a47a554ad087960ce", date: "2023-04-07 00:29:04 UTC", description: "bump redis from 0.22.3 to 0.23.0", pr_number:                                                      17068, scopes: ["deps"], type:                        "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   3, deletions_count:    3},
		{sha: "f5f248108a33a7a2864ef55220f834f078e3bba9", date: "2023-04-07 07:55:00 UTC", description: "bump actions/github-script from 6.4.0 to 6.4.1", pr_number:                                        17073, scopes: ["ci"], type:                          "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:   4, deletions_count:    4},
		{sha: "c15da39a10372cc47577912700ed733fa7b61f36", date: "2023-04-08 05:26:51 UTC", description: "Revert flushing on interval change to `expire_metrics_ms`", pr_number:                             17084, scopes: ["reduce transform"], type:            "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:   3, deletions_count:    51},
	]
}
