package metadata

releases: "0.30.0": {
	date:     "2023-05-22"
	codename: ""

	description: """
		The Vector team is pleased to announce version 0.30.0!

		In addition to the usual smaller enhancements and bug fixes, this release also includes
		a refresh of the component statuses in the docs following the [stability
		guarantees](/docs/about/under-the-hood/guarantees/#stability-guarantees).

		Be sure to check out the [upgrade guide](/highlights/2023-05-22-0-30-0-upgrade-guide) for
		breaking changes in this release.
		"""

	known_issues: []

	changelog: [
		{
			type: "fix"
			scopes: ["disk buffers"]
			description: """
				Disk buffers now recover from partial writes that can occur during unclean shutdowns.
				"""
			pr_numbers: [17099]
		},
		{
			type: "enhancement"
			scopes: ["pulsar sink"]
			description: """
				The `pulsar` sink supports a few new features:

				- Dynamic topics using a topic template
				- Can receive both logs and metrics
				- Dynamic message `properties` can be set via `properties_key`

				This brings functionality in-line with that which is supported by the `kafka` sink.
				"""
			contributors: ["addisonj"]
			pr_numbers: [14345]
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source supports a new `use_apiserver_cache` option to have
				requests from Vector hit the Kubernetes API server cache rather than always hitting
				etcd. It can significantly reduce Kubernetes control plane memory pressure in
				exchange for a chance of receiving stale data.
				"""
			contributors: ["nabokihms"]
			pr_numbers: [17095]
		},
		{
			type: "enhancement"
			scopes: ["appsignal sink"]
			description: """
				The `appsignal` sink now allows configuration of TLS options via the `tls` config
				field. This brings it in-line with other sinks that support TLS.
				"""
			contributors: ["tombruijn"]
			pr_numbers: [17122]
		},
		{
			type: "fix"
			scopes: ["influxdb_logs sink"]
			description: """
				The `influxdb_logs` sink now correctly encodes logs when `tags` are present.
				"""
			contributors: ["juvenn"]
			pr_numbers: [17029]
		},
		{
			type: "fix"
			scopes: ["loki sink"]
			description: """
				The `loki` sink now warns when added `labels` collide via wildcard
				expansion.
				"""
			contributors: ["hargut"]
			pr_numbers: [17052]
		},
		{
			type: "chore"
			scopes: ["socket source"]
			description: """
				The deprecated `max_length` option of the `socket` source was removed. Please see
				the [upgrade
				guide](/highlights/2023-05-22-0-30-0-upgrade-guide#socket-source-max-length) for
				more details.
				"""
			breaking: true
			pr_numbers: [17162]
		},
		{
			type: "enhancement"
			scopes: ["amqp sink"]
			description: """
				The `amqp` sink now allows configuration of the `content_encoding` and
				`content_type` message properties via the new `properties` configuration option.
				"""
			contributors: ["arouene"]
			pr_numbers: [17174]
		},
		{
			type: "enhancement"
			scopes: ["docker_logs source"]
			description: """
				The `docker_logs` source now supports usage of the `tcp://` scheme for the `host`
				option. The connection is the same as-if the `http://` scheme was used.
				"""
			contributors: ["OrangeFlag"]
			pr_numbers: [17124]
		},
		{
			type: "enhancement"
			scopes: ["releasing"]
			description: """
				Vector's distroless libc docker images (tags ending in `-distroless-libc`) are now
				based on Debian 11 rather than Debian 10. This matches Vector's published Debian
				images (tags ending in `-debian`).
				"""
			contributors: ["SIPR-octo"]
			pr_numbers: [17160]
		},
		{
			type: "enhancement"
			scopes: ["aws_s3 source", "aws_s3 sink"]
			description: """
				The `aws_s3` source and `aws_s3` sink now have full support for codecs and can
				receive/send any event type allowing `aws_s3` to be used as a transport layer
				between Vector instances.
				"""
			pr_numbers: [17098]
		},
		{
			type: "fix"
			scopes: ["elasticsearch sink"]
			description: """
				The `elasticsearch` sink now uses the correct API to automatically determine the
				version of the downstream Elasticsearch instance (when `api_version = "auto"`).
				"""
			contributors: ["syedriko"]
			pr_numbers: [17227]
		},
		{
			type: "enhancement"
			scopes: ["tag_cardinality_limit transform", "observability"]
			description: """
				The `tag_cardinality_limit` now includes the `metric_name` field on logs it produces
				to more easily identify the metric that was limited.
				"""
			contributors: ["nomonamo"]
			pr_numbers: [17295]
		},
		{
			type: "fix"
			scopes: ["gcp_stackdriver_metrics sink"]
			description: """
				The `gcp_stackdriver_metrics` sink now correctly refreshes the authentication token before it expires.
				"""
			pr_numbers: [17297]
		},
		{
			type: "enhancement"
			scopes: ["http sink"]
			description: """
				HTTP-based sinks now log the underlying error if an unexpected error condition is
				hit. This makes debugging easier.
				"""
			pr_numbers: [17327]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				Vector's internal logs were updated to use "suppress" rather than "rate-limit" in
				the hopes that it makes it clearer that it is only Vector's log output that is being
				suppressed, rather than data processing being throttled.
				"""
			pr_numbers: [17394]
		},
		{
			type: "fix"
			scopes: ["kafka source"]
			description: """
				The `kafka` source now attempts to send any pending acknowledgements to the Kafka
				server before reading additional messages to process.
				"""
			pr_numbers: [17380]
		},
		{
			type: "enhancement"
			scopes: ["aws provider"]
			description: """
				AWS components now allow configuring `auth.region` without any of the other
				authentication options so that a different region can be given to the default
				authentication provider chain than the region that the component is otherwise
				connecting to.
				"""
			pr_numbers: [17414]
		},
	]

	commits: [
		{sha: "cbc17be42af382dc200d8f1516be29f231485026", date: "2023-04-07 21:45:24 UTC", description: "bump enumflags2 from 0.7.5 to 0.7.6", pr_number: 17079, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "c29c8171bdcea02f991ef9bdc3cbd3ea0b8adedb", date: "2023-04-07 21:46:16 UTC", description: "bump async-stream from 0.3.4 to 0.3.5", pr_number: 17076, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 8, deletions_count: 8},
		{sha: "eafba69a355c8b7ae099134392c6ebd7cab6dcce", date: "2023-04-08 04:15:40 UTC", description: "bump tonic from 0.8.3 to 0.9.1", pr_number: 17077, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 82, deletions_count: 23},
		{sha: "5d886550784e1fe49ba5d670f81161c5b8614abc", date: "2023-04-08 05:43:35 UTC", description: "Load compose files and inject network block", pr_number: 17025, scopes: ["vdev"], type: "enhancement", breaking_change: false, author: "Jonathan Padilla", files_count: 32, insertions_count: 80, deletions_count: 132},
		{sha: "9a56ed8226a764fa09dcfe9f4e8d968646555bf9", date: "2023-04-10 22:54:06 UTC", description: "bump openssl from 0.10.48 to 0.10.50", pr_number: 17087, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 7},
		{sha: "623b838b2193e019173ad5d223fb217bbf5d94bd", date: "2023-04-10 22:54:44 UTC", description: "bump chrono-tz from 0.8.1 to 0.8.2", pr_number: 17088, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "e0906105bc0c6ed297ed96ab8c545535c4fa83b2", date: "2023-04-10 22:55:29 UTC", description: "bump prettydiff from 0.6.2 to 0.6.4", pr_number: 17089, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 12, deletions_count: 2},
		{sha: "adbf4d54e5b11a562b1323d3dcbc2587c855b093", date: "2023-04-10 22:56:14 UTC", description: "bump serde_with from 2.3.1 to 2.3.2", pr_number: 17090, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 13, deletions_count: 13},
		{sha: "9cc2f1de1cce6c43e335ec1815363f510e111fbd", date: "2023-04-10 22:56:51 UTC", description: "bump uuid from 1.3.0 to 1.3.1", pr_number: 17091, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "64d560d7737e553190d473dbbb07ae001cf169b3", date: "2023-04-10 23:43:39 UTC", description: "correctly mark some sinks as stateful", pr_number: 17085, scopes: ["external docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 6, insertions_count: 10, deletions_count: 6},
		{sha: "51312aaa919cbe4e0d25dcfc202a6e9f618389a3", date: "2023-04-11 00:23:07 UTC", description: "bump wiremock from 0.5.17 to 0.5.18", pr_number: 17092, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "887d6d7971c86e17448054484e7956b8fd393be7", date: "2023-04-11 07:14:19 UTC", description: "Reset dependencies bumped by a61dea1", pr_number: 17100, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 256, deletions_count: 352},
		{sha: "1bdb24d04329aabb7212942b08f503e910ed60ff", date: "2023-04-12 00:59:35 UTC", description: "Transform outputs hash table of OutputId -> Definition", pr_number: 17059, scopes: ["topology"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 24, insertions_count: 283, deletions_count: 212},
		{sha: "42f298b3721098aca7754b1759cf6abd84a1e6fc", date: "2023-04-11 22:47:37 UTC", description: "bump num_enum from 0.5.11 to 0.6.0", pr_number: 17106, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 25, deletions_count: 4},
		{sha: "6f745234ed3c7d22cd446769fcac86549c105416", date: "2023-04-11 22:49:55 UTC", description: "bump proc-macro2 from 1.0.55 to 1.0.56", pr_number: 17103, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 65, deletions_count: 65},
		{sha: "d53240b53a789edec8bd6700953dccbe660c7a65", date: "2023-04-11 22:51:31 UTC", description: "bump getrandom from 0.2.8 to 0.2.9", pr_number: 17101, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "a791595b0cfcae36d0c46708a91d5e2813ed38eb", date: "2023-04-12 01:27:05 UTC", description: "correctly handle partial writes in reader seek during initialization", pr_number: 17099, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 3, insertions_count: 163, deletions_count: 12},
		{sha: "edaa6124bd7a47cbb551127168b764d496bf73c2", date: "2023-04-12 02:26:21 UTC", description: "tidy up some of the module level docs for `disk_v2`", pr_number: 17093, scopes: ["buffer"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 2, insertions_count: 96, deletions_count: 61},
		{sha: "fd13d64c7b911f7fa4cb901640dbe6b1042018cc", date: "2023-04-12 01:53:39 UTC", description: "Regenerate Kubernetes manifests for 0.21.2", pr_number: 17108, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "dd9608a40da7758ab06f1a298093130abfc72418", date: "2023-04-12 07:55:24 UTC", description: "bump libc from 0.2.140 to 0.2.141", pr_number: 17104, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "f3b5d42cd5d01acf86235e6edc17f5b0d3b837c4", date: "2023-04-12 02:07:09 UTC", description: "Disable `appsignal` integration test until CA issues are resolved", pr_number: 17109, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "48fc574e7177bfcc5acf2f9aac85474cb38faef8", date: "2023-04-12 04:17:23 UTC", description: "re-enable `appsignal` integration test", pr_number: 17111, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "2d72f82b22054a3a7c392061010f94eec15c66be", date: "2023-04-12 07:24:54 UTC", description: "improve config schema output with more precise `unevaluatedProperties` + schema ref flattening", pr_number: 17026, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 17, insertions_count: 1523, deletions_count: 103},
		{sha: "1e97a2fc5c5cbdee8b27aa34ca14dde67eac2166", date: "2023-04-12 05:27:09 UTC", description: "Refactor to use StreamSink", pr_number: 14345, scopes: ["pulsar sink"], type: "enhancement", breaking_change: false, author: "Addison Higham", files_count: 16, insertions_count: 1000, deletions_count: 601},
		{sha: "e7c481558373625e04d763ea34451f219f7656d9", date: "2023-04-12 06:27:49 UTC", description: "update unsupported ubuntu version runners", pr_number: 17113, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7a40c817151819ba72ed2e31d5860956f693fa8d", date: "2023-04-12 08:06:47 UTC", description: "use python v3.8 in ubuntu 20.04 runner", pr_number: 17116, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f56d1ef50d57a5057807b1d122032980bbc70d8d", date: "2023-04-13 03:12:21 UTC", description: "remove unnecessary dep install", pr_number: 17128, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "f90b3b305f23bcb9e4c03d7199a6ad3d4a27045b", date: "2023-04-13 03:51:48 UTC", description: "bump cached from 0.42.0 to 0.43.0", pr_number: 17118, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7b15d191b9b019dfdfea8dd743ff5fa07a19b82f", date: "2023-04-13 04:01:08 UTC", description: "add `appsignal` to codeowners", pr_number: 17127, scopes: ["administration"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "3834612cb052edcae99f22aecbf07fdad32f816c", date: "2023-04-13 06:35:34 UTC", description: "Bump Vector version to 0.30.0", pr_number: 17134, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "e7ea0a82132d7572aad66c6d0b1297777d1196c6", date: "2023-04-13 07:52:27 UTC", description: "Regenerate manifests for 0.22.0 chart", pr_number: 17135, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "8762563a3b19d5b65df3172a5f7bdcd670102eee", date: "2023-04-13 07:06:24 UTC", description: "bump opendal from 0.30.5 to 0.31.0", pr_number: 17119, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 20, deletions_count: 250},
		{sha: "dbb3f251ce952bcbe47e996d72a00972b12e1095", date: "2023-04-13 22:50:58 UTC", description: "bump aws-sigv4 from 0.55.0 to 0.55.1", pr_number: 17138, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 10},
		{sha: "db39d837e5083fe2788ea729dd20abf20234cc72", date: "2023-04-13 22:57:49 UTC", description: "bump socket2 from 0.4.7 to 0.5.2", pr_number: 17121, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 102, deletions_count: 26},
		{sha: "ba63e2148afeb3824fc413d63ed165c3c5068eee", date: "2023-04-14 01:57:04 UTC", description: "Add a quickfix to handle special capitalization cases", pr_number: 17141, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 19, deletions_count: 4},
		{sha: "d245927f570bca42082f9495844ca4eddef715f2", date: "2023-04-14 03:05:49 UTC", description: "Remove skaffold from project", pr_number: 17145, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 8, insertions_count: 0, deletions_count: 54},
		{sha: "e46fae798120f7d3ce762382dcf9cfd3b79e4a55", date: "2023-04-14 12:56:23 UTC", description: "use kube-apiserver cache for list requests", pr_number: 17095, scopes: ["kubernetes_logs"], type: "feat", breaking_change: false, author: "Maksim Nabokikh", files_count: 4, insertions_count: 80, deletions_count: 65},
		{sha: "198068cf55732a3bfe034697d9dc5c9abadb1b63", date: "2023-04-14 11:37:02 UTC", description: "Add TLS config option", pr_number: 17122, scopes: ["appsignal sink"], type: "fix", breaking_change: false, author: "Tom de Bruijn", files_count: 2, insertions_count: 90, deletions_count: 2},
		{sha: "5247972ed8ae85dc384571c2bcc473aa9cb8e661", date: "2023-04-14 05:50:54 UTC", description: "add unit test (ignored) for support for encoding special chars in `ProxyConfig`", pr_number: 17148, scopes: ["core"], type: "enhancement", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 24, deletions_count: 0},
		{sha: "aad811540ff2a544c8d1fd7410d2c029099845f0", date: "2023-04-15 00:04:58 UTC", description: "begin laying out primitives for programmatically querying schema", pr_number: 17130, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 26, insertions_count: 1303, deletions_count: 162},
		{sha: "c3aa14fd4d2b72a3863b8a8f6590c8ba65cc6c56", date: "2023-04-15 13:08:25 UTC", description: "encode influx line when no tags present", pr_number: 17029, scopes: ["influxdb_logs"], type: "fix", breaking_change: false, author: "Juvenn Woo", files_count: 2, insertions_count: 24, deletions_count: 14},
		{sha: "71d1bf6bae80b4d4518e9ea3f87d0d6ecd000984", date: "2023-04-15 01:10:01 UTC", description: "recurse through schema refs when determining eligibility for unevaluated properties", pr_number: 17150, scopes: ["config"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 4, insertions_count: 64, deletions_count: 24},
		{sha: "10fce656f624344facf662c7a54282dc46d63303", date: "2023-04-14 23:18:52 UTC", description: "true up cargo.lock", pr_number: 17149, scopes: ["deps"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "730c9386f66b6348c64a268ef37e752343d8fb9a", date: "2023-04-15 05:56:20 UTC", description: "Adjust doc comment locations", pr_number: 17154, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 31, deletions_count: 33},
		{sha: "f06692b27ac480eb258faab14adce1f7b500f030", date: "2023-04-15 12:41:32 UTC", description: "warn on label expansions and collisions", pr_number: 17052, scopes: ["loki sink"], type: "chore", breaking_change: false, author: "Harald Gutmann", files_count: 1, insertions_count: 151, deletions_count: 23},
		{sha: "65a8856ab08296bf6da22f7dbf3b9a6da64aff6a", date: "2023-04-15 07:04:27 UTC", description: "make doc style edits", pr_number: 17155, scopes: ["docs"], type: "chore", breaking_change: false, author: "May Lee", files_count: 21, insertions_count: 44, deletions_count: 44},
		{sha: "c1691313e34fc77af5c37abdefa1322ee20e3398", date: "2023-04-15 06:50:22 UTC", description: "update the `v0.28.0` upgrade guide with note about `datadog_logs` sink `hostname` key", pr_number: 17156, scopes: ["docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "854d71e48883b703b1eb67b538e7ac3b21037fae", date: "2023-04-19 03:19:16 UTC", description: "bump docker/metadata-action from 4.3.0 to 4.4.0", pr_number: 17170, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9ecfc8c8159d4093a28de270885e880628a90127", date: "2023-04-19 23:22:53 UTC", description: "Remove deprecated `max_length` setting from `tcp` and `unix` modes.", pr_number: 17162, scopes: ["socket source"], type: "chore", breaking_change: false, author: "neuronull", files_count: 8, insertions_count: 86, deletions_count: 97},
		{sha: "3c9255658c994a002b024db89c9cc32dd718de9c", date: "2023-04-20 01:25:50 UTC", description: "Remove trailing, unmatched quote", pr_number: 17163, scopes: ["docs"], type: "chore", breaking_change: false, author: "Mark Lodato", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3b38ba82c3727eac93c0d0a992f248b72435dac6", date: "2023-04-20 02:52:30 UTC", description: "emit human-friendly version of enum variant/property names in schema", pr_number: 17171, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 14, insertions_count: 454, deletions_count: 135},
		{sha: "68b54a9bc0ae07d916ec48e997a03f7681e54ccc", date: "2023-04-20 13:34:16 UTC", description: "pulsar-rs bump to v5.1.1", pr_number: 17159, scopes: ["pulsar"], type: "chore", breaking_change: false, author: "kannar", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1f0de6b5b90734b99b2c44ea500767f2c013e25c", date: "2023-04-21 05:11:50 UTC", description: "Regenerate manifests for 0.21.1 chart", pr_number: 17187, scopes: ["releasing"], type: "chore", breaking_change: false, author: "neuronull", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "a2882f384e24c13efc2dcf55885f609470e7e9e4", date: "2023-04-21 07:31:41 UTC", description: "Update h2", pr_number: 17189, scopes: ["deps"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "5dff0ed37a89e8cfc9db3ca499454dfe8198eadf", date: "2023-04-22 00:15:30 UTC", description: "remove the remove of source_ip", pr_number: 17184, scopes: ["syslog source"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "c10d30bd35494ea336d90d0abf9977349c38d154", date: "2023-04-25 01:27:59 UTC", description: "Support AMQ Properties (content-type) in AMQP sink", pr_number: 17174, scopes: ["amqp sink"], type: "enhancement", breaking_change: false, author: "Aurélien Rouëné", files_count: 5, insertions_count: 75, deletions_count: 4},
		{sha: "c304a8c9b554a18dc39eadcd4d06f81d0d3baed1", date: "2023-04-25 09:40:24 UTC", description: "Upgrade Debian to bullseye for distroless image", pr_number: 17160, scopes: ["deps"], type: "chore", breaking_change: false, author: "SIPR", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "32a935b4d74bd38ba96c717291430f03fa80f4c4", date: "2023-04-25 01:57:12 UTC", description: "ignore `.helix` dir", pr_number: 17203, scopes: ["dev"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "d396320162e068d82f8f7d4e47bc8984503750e7", date: "2023-04-25 06:23:44 UTC", description: "Upgrade cue to 0.5.0", pr_number: 17204, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "ef15696292c80b80932e20093e833792d9b2aa71", date: "2023-04-25 05:24:19 UTC", description: "Upgrade rust to 1.69.0", pr_number: 17194, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 87, insertions_count: 195, deletions_count: 213},
		{sha: "40c9afc584be350117ada03216cbdf43cbe8775d", date: "2023-04-26 04:48:03 UTC", description: "bump mock_instant from 0.2.1 to 0.3.0", pr_number: 17210, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c80c5eb22c1f238903d5c291d944a2b8db7b73b9", date: "2023-04-26 08:24:54 UTC", description: "bump enumflags2 from 0.7.6 to 0.7.7", pr_number: 17206, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "aa9cbd078ff7f8ac35dc5555533b7394764b86ca", date: "2023-04-26 22:44:12 UTC", description: "bump tonic from 0.9.1 to 0.9.2", pr_number: 17221, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "410aa3cab29b91b59abadadceccffe14e022f06e", date: "2023-04-26 22:47:50 UTC", description: "bump regex from 1.7.3 to 1.8.1", pr_number: 17222, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 27, deletions_count: 12},
		{sha: "40d543a6a4cfc70a870080df6e543257b4004198", date: "2023-04-27 00:34:10 UTC", description: "Add known issues fixed by 0.29.1", pr_number: 17218, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 9, deletions_count: 3},
		{sha: "752d4245c7f4cfbb4513183aeada24ce8a0e4891", date: "2023-04-27 02:10:45 UTC", description: "Remove unneeded `yaml` dependency from website", pr_number: 17215, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 2},
		{sha: "9031d0faa2811187874364e1b5a3305c9ed0c0da", date: "2023-04-28 01:58:17 UTC", description: "Re-add transform definitions", pr_number: 17152, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 87, insertions_count: 2714, deletions_count: 2088},
		{sha: "29c34c073c0dde0e5d9f69c94193ae547538da5d", date: "2023-04-28 05:41:31 UTC", description: "(syslog source): add source_ip to some syslog tests", pr_number: 17235, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 39, deletions_count: 5},
		{sha: "8067f84ae38ad613af0063179e19e7bbf5448ca4", date: "2023-04-27 23:38:37 UTC", description: "bump tokio from 1.27.0 to 1.28.0", pr_number: 17231, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 15, deletions_count: 15},
		{sha: "1e432089f4a3375b2a6dfefb1de3197af5f2313d", date: "2023-04-28 07:07:09 UTC", description: "Dont panic with non object field kinds", pr_number: 17140, scopes: ["schemas"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 29, insertions_count: 855, deletions_count: 119},
		{sha: "cfc387d8c4595bfd031cd28d88ac2483200cb53e", date: "2023-04-28 22:35:18 UTC", description: "bump dunce from 1.0.3 to 1.0.4", pr_number: 17244, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d286d16dcccca67ea2c1bd994f5440cfca303732", date: "2023-04-29 05:51:58 UTC", description: "bump clap_complete from 4.2.0 to 4.2.1", pr_number: 17229, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d648c86721a689f2e4add0da46c6c9b011e438d6", date: "2023-04-29 06:52:13 UTC", description: "Add full codec support to AWS S3 source/sink", pr_number: 17098, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 5, insertions_count: 308, deletions_count: 101},
		{sha: "4b80c714b68bb9acc2869c48b71848d11954c6aa", date: "2023-04-29 09:58:15 UTC", description: "Install the correct `mold` based on CPU architecture", pr_number: 17248, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "bc6f7fd5109242cc53d7f388ff264662b6a6c223", date: "2023-05-01 23:06:09 UTC", description: "bump uuid from 1.3.0 to 1.3.2", pr_number: 17256, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "41ac76ed03bfc7c08e2f8262eee66c7bae01d5af", date: "2023-05-01 23:07:28 UTC", description: "bump axum from 0.6.12 to 0.6.18", pr_number: 17257, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a88aba49a357e547a43a7d985a9ebd8d5c58f9a2", date: "2023-05-02 03:32:38 UTC", description: "bump prost-build from 0.11.8 to 0.11.9", pr_number: 17260, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "7570bb31e2f471e3ff8bc818c24e9bde3090818c", date: "2023-05-02 03:32:55 UTC", description: "bump serde_json from 1.0.95 to 1.0.96", pr_number: 17258, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "be69f5f361ce4621c01f522c7270c5f97b2b7069", date: "2023-05-02 22:51:36 UTC", description: "bump directories from 5.0.0 to 5.0.1", pr_number: 17271, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 15, deletions_count: 8},
		{sha: "036ad4ab17ddadfa1e24164ffbfa28b458e4c74e", date: "2023-05-02 22:52:34 UTC", description: "bump serde from 1.0.159 to 1.0.160", pr_number: 17270, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "1406c087db2f377eff65065c5f2fbcb295d4d138", date: "2023-05-02 22:54:19 UTC", description: "bump tracing-subscriber from 0.3.16 to 0.3.17", pr_number: 17268, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 9, deletions_count: 9},
		{sha: "f696e7bde782eac78d4692ad5d0de81a7e99c400", date: "2023-05-03 03:19:53 UTC", description: "bump num_enum from 0.6.0 to 0.6.1", pr_number: 17272, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "03e905e304d2253dfcd0019105337df23e72d80c", date: "2023-05-03 04:22:00 UTC", description: "add note to DEVELOPING.md re panics", pr_number: 17277, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 11, deletions_count: 0},
		{sha: "bc618a25e4c501857a0ac3747c4c7735a6192791", date: "2023-05-03 04:31:40 UTC", description: "bump libc from 0.2.141 to 0.2.142", pr_number: 17273, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9b6ef243cac4abc758e288133fb732b7b504f032", date: "2023-05-03 00:40:40 UTC", description: " Elasticsearch sink with api_version set to \"auto\" does not recognize the API version of ES6 as V6 (#17226)", pr_number: 17227, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Sergey Yedrikov", files_count: 1, insertions_count: 27, deletions_count: 18},
		{sha: "61c0d764af78826c8d01c5295924bf0a31c810e2", date: "2023-05-03 00:39:32 UTC", description: "remove editors from gitignore", pr_number: 17267, scopes: ["dev"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 0, deletions_count: 3},
		{sha: "4335b0a34a44af82bb63739e8e9b3ffc72ecf3f7", date: "2023-05-03 04:48:47 UTC", description: "Disable scheduled runs of Baseline Timings workflow", pr_number: 17281, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "e0a07c6dfe3ecadb8f88fcd343d302d5c142761d", date: "2023-05-03 11:45:36 UTC", description: "bump tonic-build from 0.8.4 to 0.9.2", pr_number: 17274, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 4},
		{sha: "3d419315987671c1d3867e357d921f266c549c71", date: "2023-05-03 23:46:34 UTC", description: "bump opendal from 0.31.0 to 0.33.2", pr_number: 17286, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d8c1f12f4a65129cad225632c9a43b13ac354a7a", date: "2023-05-03 23:46:59 UTC", description: "bump warp from 0.3.4 to 0.3.5", pr_number: 17288, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c4784fd6e62d6cec76ced412512d909df304d005", date: "2023-05-03 23:47:24 UTC", description: "bump assert_cmd from 2.0.10 to 2.0.11", pr_number: 17290, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 5},
		{sha: "6a5af3b862b0ffdcb509bd8a49641e41294b77b8", date: "2023-05-04 23:03:12 UTC", description: "bump anyhow from 1.0.70 to 1.0.71", pr_number: 17300, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "c8e0e5febbffece0a9a2fd7776767fd93a04e0db", date: "2023-05-04 23:04:24 UTC", description: "bump typetag from 0.2.7 to 0.2.8", pr_number: 17302, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "61e6154fd5f4712dae0b60661ff34ae586ce8ac4", date: "2023-05-04 23:05:08 UTC", description: "bump syslog from 6.0.1 to 6.1.0", pr_number: 17301, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0ecceb3ba95312ed2a22b7f4350547d875184be9", date: "2023-05-04 23:06:11 UTC", description: "bump openssl from 0.10.50 to 0.10.52", pr_number: 17299, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "43173403e7f01d169a9b10a53b0e462e77c9c0f0", date: "2023-05-05 06:28:17 UTC", description: "adds 'metric_name' field to internal logs for the tag_cardinality_limit component", pr_number: 17295, scopes: ["observability"], type: "feat", breaking_change: false, author: "Pablo Pérez Schröder", files_count: 2, insertions_count: 7, deletions_count: 0},
		{sha: "bf7904b4ff9dbe354c401b816f43123ba6d48335", date: "2023-05-05 00:33:31 UTC", description: "Call function to regenerate auth token", pr_number: 17297, scopes: ["gcp_stackdriver_metrics sink"], type: "fix", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "a1ec68d302757a7fae1082cc90c27ce8aad2c6ea", date: "2023-05-05 23:00:15 UTC", description: "bump prettydiff from 0.6.2 to 0.6.4", pr_number: 17315, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 12, deletions_count: 2},
		{sha: "79e97a2bc96f424335c62fe3519c8e1501f65bcf", date: "2023-05-05 23:01:14 UTC", description: "bump serde from 1.0.160 to 1.0.162", pr_number: 17317, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "09176ec3e98febbca0ee54985248c5ecd0fdb39d", date: "2023-05-05 23:01:47 UTC", description: "bump reqwest from 0.11.16 to 0.11.17", pr_number: 17316, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "ef1337024677d4c6ff25cf9cb571cbada39fbe55", date: "2023-05-05 23:02:32 UTC", description: "bump flate2 from 1.0.25 to 1.0.26", pr_number: 17320, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "e1f125a34c91b2344174298a1f508124a0a0b4dd", date: "2023-05-06 02:00:57 UTC", description: "Increase timeout for integration tests", pr_number: 17326, scopes: ["ci"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4911d3600a3fcce81f70fd8cb427b8389aca0bfb", date: "2023-05-06 04:05:32 UTC", description: "Upgrade `VRL` to `0.3.0`", pr_number: 17325, scopes: [], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 146, insertions_count: 514, deletions_count: 635},
		{sha: "a9c8dc88ce7c35b75ab3d1bf903aca0a6feaee53", date: "2023-05-06 03:59:16 UTC", description: "Document event type conditions", pr_number: 17311, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 17, deletions_count: 2},
		{sha: "6afe206bd595d7933c518342a1602fa15668c0c9", date: "2023-05-09 00:26:21 UTC", description: "bump libc from 0.2.142 to 0.2.143", pr_number: 17338, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "64f4f697ecaf8c67096d6ceb5a33e42042e57cdc", date: "2023-05-09 00:27:02 UTC", description: "bump mongodb from 2.4.0 to 2.5.0", pr_number: 17337, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 46, deletions_count: 5},
		{sha: "80c82470b309901d83de03529312fc3e733d8e3e", date: "2023-05-09 00:27:34 UTC", description: "bump tokio-stream from 0.1.12 to 0.1.14", pr_number: 17339, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "bf8376c3030e6d6df61ca245f2d8be87443bf075", date: "2023-05-08 23:58:26 UTC", description: "Log underlying error for unhandled HTTP errors", pr_number: 17327, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "9a723e33cc161b680140c4ef230fedf071e68031", date: "2023-05-09 04:56:27 UTC", description: "bump metrics, metrics-tracing-context, metrics-util", pr_number: 17336, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 39, deletions_count: 56},
		{sha: "99b8dc13bcff379062ac276119e650055e08d0fc", date: "2023-05-09 23:08:13 UTC", description: "bump libc from 0.2.143 to 0.2.144", pr_number: 17346, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f81ff1837adcf1cc4419bc936fe539e7dd882dbb", date: "2023-05-09 23:08:40 UTC", description: "bump quote from 1.0.26 to 1.0.27", pr_number: 17348, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 67, deletions_count: 67},
		{sha: "c43dcfdba4781b81f6418e96b286f37323c7fb26", date: "2023-05-09 23:42:33 UTC", description: "bump hyper from 0.14.25 to 0.14.26", pr_number: 17347, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5d3f619ef3295180657529ad5bd44d837cb123b5", date: "2023-05-10 00:49:27 UTC", description: "Increase timeout for integration tests to 30m", pr_number: 17350, scopes: ["ci"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "ae602da29daad0c1c0081cac0bc27440d28440ad", date: "2023-05-10 23:29:01 UTC", description: "bump opendal from 0.33.2 to 0.34.0", pr_number: 17354, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 4},
		{sha: "05a4f17c555c1d2bd25acd7f3173940d98224b53", date: "2023-05-10 23:29:27 UTC", description: "bump async-graphql from 5.0.7 to 5.0.8", pr_number: 17357, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 10},
		{sha: "ea24b4d1695e2484ad54f7e03edb6fcd1b8d0971", date: "2023-05-10 23:29:57 UTC", description: "bump wasm-bindgen from 0.2.84 to 0.2.85", pr_number: 17356, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 12, deletions_count: 12},
		{sha: "dae0c6ad6882bf0bdfa75bde439e3e0f9f4a9dea", date: "2023-05-10 23:31:44 UTC", description: "bump memmap2 from 0.5.10 to 0.6.0", pr_number: 17355, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "97b862c4db77a0192da3b505accf43dcba1c8d59", date: "2023-05-10 23:32:30 UTC", description: "bump console-subscriber from 0.1.8 to 0.1.9", pr_number: 17358, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 42},
		{sha: "565668ea6598992ba47a039e872a18b2ffd19844", date: "2023-05-10 23:32:56 UTC", description: "bump clap_complete from 4.2.1 to 4.2.2", pr_number: 17359, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9852c1770bd2dceecc9b30ffa72b1f95c0dfd719", date: "2023-05-11 23:45:35 UTC", description: "bump serde from 1.0.162 to 1.0.163", pr_number: 17366, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "693584eb5002fc0c00586afa1c058bb8cfd0d58e", date: "2023-05-11 23:57:25 UTC", description: "bump async-graphql-warp from 5.0.7 to 5.0.8", pr_number: 17367, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b9aac475025905943c80dd710f833e2e445c9093", date: "2023-05-12 00:07:31 UTC", description: "bump async-compression from 0.3.15 to 0.4.0", pr_number: 17365, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 18, deletions_count: 5},
		{sha: "ae6a51b52d2a0f93b3cf16638fd10a52e33294c9", date: "2023-05-12 05:35:29 UTC", description: "bump tokio from 1.28.0 to 1.28.1", pr_number: 17368, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "22cda94d3b8fa555533b51f3ee6de39932b04775", date: "2023-05-12 01:40:00 UTC", description: "Update component statuses 2023Q2", pr_number: 17362, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 42, insertions_count: 43, deletions_count: 44},
		{sha: "58ba7411967af541199042f76590e306e4c8c41f", date: "2023-05-12 03:04:12 UTC", description: "bump memmap2 from 0.6.0 to 0.6.1", pr_number: 17364, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7350e1a11805db510814d4fc357e84d0e8d2cf25", date: "2023-05-12 22:10:31 UTC", description: "Add 3rd party license file and CI checks", pr_number: 17344, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 687, deletions_count: 2},
		{sha: "d1e558800a570556372949fd332097c3e138a2e8", date: "2023-05-12 23:14:02 UTC", description: "Clarify `key_field` for `sample` and `throttle` transforms", pr_number: 17372, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 24, deletions_count: 18},
		{sha: "a2b890352bc42e9a9a30163e26a2f181f08c4a3b", date: "2023-05-13 00:33:31 UTC", description: "Fix up missing license", pr_number: 17379, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "c6839995e28fd17aefbe440f092046e660d2fd70", date: "2023-05-16 02:06:09 UTC", description: "Add source id to metadata", pr_number: 17369, scopes: ["topology"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 12, insertions_count: 258, deletions_count: 53},
		{sha: "6c57ca07aee4402582b7b7c9c37324f49c14bf65", date: "2023-05-16 00:52:37 UTC", description: "Regen docs for sample and throttle", pr_number: 17390, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "6b3db04f7f7ca700e7696d3430b989efc2a4b3b4", date: "2023-05-16 00:23:43 UTC", description: "Try to fix apt retries", pr_number: 17393, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "111cd07702befce55242c3940c59f05e374d52cf", date: "2023-05-16 08:02:52 UTC", description: "bump clap_complete from 4.2.2 to 4.2.3", pr_number: 17383, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1951535eefe7e0812952d3037b40216106350e95", date: "2023-05-16 02:12:49 UTC", description: "Update internal log rate limiting messages", pr_number: 17394, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 49, deletions_count: 47},
		{sha: "f3734e81cb6409e496e771c0f75f18101b5e9605", date: "2023-05-16 05:29:55 UTC", description: "Fix formatting in labels example", pr_number: 17396, scopes: ["loki sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "970318839d5722a3ab40e8276a0ee6982fa798b3", date: "2023-05-16 02:36:48 UTC", description: "bump rdkafka from 0.29.0 to 0.30.0", pr_number: 17387, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 7},
		{sha: "e8d3002d4bcb226ab79ed8b3212d1a123833c535", date: "2023-05-16 10:50:16 UTC", description: "bump pin-project from 1.0.12 to 1.1.0", pr_number: 17385, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 9, deletions_count: 9},
		{sha: "ac51b8a35d83e5c24ac0686eb57f4f4bb347773b", date: "2023-05-16 10:55:14 UTC", description: "bump socket2 from 0.5.2 to 0.5.3", pr_number: 17384, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "6088abdf6b956940fee4ee827eefb9dce3e84a43", date: "2023-05-16 12:11:30 UTC", description: "bump h2 from 0.3.18 to 0.3.19", pr_number: 17388, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "12871685d3f6261ee0d50171584426aba96264ee", date: "2023-05-16 21:23:09 UTC", description: "bump security-framework from 2.8.2 to 2.9.0", pr_number: 17386, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a6e1ae737e6ad17f9d3deecc6c887e41a1d86099", date: "2023-05-16 21:23:21 UTC", description: "bump proc-macro2 from 1.0.56 to 1.0.57", pr_number: 17400, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 65, deletions_count: 65},
		{sha: "3a3fe6337d940af3d2667c7775b2fa2e657648fc", date: "2023-05-16 21:24:26 UTC", description: "bump uuid from 1.3.2 to 1.3.3", pr_number: 17403, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "ae1dd6e4a67d046037154dab425e4fe6bfd11087", date: "2023-05-16 21:24:38 UTC", description: "bump tokio-tungstenite from 0.18.0 to 0.19.0", pr_number: 17404, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 38, deletions_count: 7},
		{sha: "05181765a5d2c7610adfcf6cd1e44610eb7ed79e", date: "2023-05-16 21:24:49 UTC", description: "bump wasm-bindgen from 0.2.85 to 0.2.86", pr_number: 17402, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 10, deletions_count: 10},
		{sha: "539f379911f735656eaff3aadd4f6aeeb4b681d5", date: "2023-05-17 02:24:29 UTC", description: "Add note about generating licenses to CONTRIBUTING.md", pr_number: 17410, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 13, deletions_count: 1},
		{sha: "5b5ad1682dc827e17610eb086d68f4f56e17138d", date: "2023-05-17 03:19:09 UTC", description: "bump inventory from 0.3.5 to 0.3.6", pr_number: 17401, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 3, deletions_count: 15},
		{sha: "dc6e54c18cc3eb7754d3865602b54ae46ec1f67a", date: "2023-05-17 03:19:50 UTC", description: "Add UX note about encoding of log_schema keys", pr_number: 17266, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 8, deletions_count: 0},
		{sha: "5c33f999f1e0814c4cc1857cef67415f7bba5cb7", date: "2023-05-17 03:36:29 UTC", description: "Remove ci-sweep tasks", pr_number: 17415, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 0, deletions_count: 15},
		{sha: "da36fb6f9df3724267b30d845e092d2f7628d359", date: "2023-05-17 03:49:02 UTC", description: "Fix event assertions for `aws_ec2_metadata` transform", pr_number: 17413, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 4},
		{sha: "5184d50f115426306a236402b9c76b0e6aa12fe6", date: "2023-05-17 03:49:57 UTC", description: "Add Enterprise link and update Support link", pr_number: 17408, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "b6c7e0ae43222cd173e3d3bae7a62c3dcc985639", date: "2023-05-17 06:34:37 UTC", description: "remove transform type coercion", pr_number: 17411, scopes: [], type: "chore", breaking_change: false, author: "Luke Steensen", files_count: 7, insertions_count: 37, deletions_count: 80},
		{sha: "01b3cd7698dd9a7bf5e2fce909d6e7ef1ffa1313", date: "2023-05-17 21:20:12 UTC", description: "bump hashlink from 0.8.1 to 0.8.2", pr_number: 17419, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "3320eda52e5144eb8c0214481705a97edc197e81", date: "2023-05-17 21:20:28 UTC", description: "bump nkeys from 0.2.0 to 0.3.0", pr_number: 17421, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 18, deletions_count: 3},
		{sha: "57f8bd4ea2cfdf305dab9875f49e3d5c348c2529", date: "2023-05-17 21:21:06 UTC", description: "bump mlua from 0.8.8 to 0.8.9", pr_number: 17423, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "58603b90ad595df96b6239c42c2dd9e4dce46475", date: "2023-05-17 21:23:23 UTC", description: "bump notify from 5.1.0 to 6.0.0", pr_number: 17422, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "c81ad30c3f6627a70586703e4e5e8db7625aeef7", date: "2023-05-17 23:45:29 UTC", description: "Let `region` be configured for default authentication", pr_number: 17414, scopes: ["aws provider"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 39, deletions_count: 1},
	]
}
