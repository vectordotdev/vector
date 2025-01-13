package metadata

releases: "0.44.0": {
	date:     "2025-01-13"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.44.0!

		Be sure to check out the [upgrade guide](/highlights/2025-01-13-0-44-0-upgrade-guide) for
		breaking changes in this release.

		This release contains numerous enhancements and fixes.
		"""

	changelog: [
		{
			type: "feat"
			description: """
				VRL was updated to v0.21.0. This includes the following changes:

				#### Breaking Changes & Upgrade Guide

				- `to_unix_timestamp`, `to_float`, and `uuid_v7` can now return an error if the supplied timestamp is unrepresentable as a nanosecond timestamp. Previously the function calls would panic. (https://github.com/vectordotdev/vrl/pull/979)

				#### New Features

				- Added new `crc` function to calculate CRC (Cyclic Redundancy Check) checksum
				- Add `parse_cbor` function (https://github.com/vectordotdev/vrl/pull/1152)
				- Added new `zip` function to iterate over an array of arrays and produce a new
					arrays containing an item from each one. (https://github.com/vectordotdev/vrl/pull/1158)
				- Add new `decode_charset`, `encode_charset` functions to decode and encode strings between different charsets. (https://github.com/vectordotdev/vrl/pull/1162)
				- Added new `object_from_array` function to create an object from an array of
					value pairs such as what `zip` can produce. (https://github.com/vectordotdev/vrl/pull/1164)
				- Added support for multi-unit duration strings (e.g., `1h2s`, `2m3s`) in the `parse_duration` function. (https://github.com/vectordotdev/vrl/pull/1197)
				- Added new `parse_bytes` function to parse given bytes string such as `1MiB` or `1TB` either in binary or decimal base. (https://github.com/vectordotdev/vrl/pull/1198)
				- Add `main` log format for `parse_nginx_log`. (https://github.com/vectordotdev/vrl/pull/1202)
				- Added support for optional `timezone` argument in the `parse_timestamp` function. (https://github.com/vectordotdev/vrl/pull/1207)

				#### Fixes

				- Fix a panic in float subtraction that produces NaN values. (https://github.com/vectordotdev/vrl/pull/1186)
				"""
		},
		{
			type: "enhancement"
			description: """
				`aws_s3` source now logs when S3 objects are fetched. If ACKs are enabled, it also logs on delivery.
				"""
			contributors: ["fdamstra"]
		},
		{
			type: "enhancement"
			description: """
				The file sink now supports any input event type that the configured encoding supports. It previously only supported log events.
				"""
			contributors: ["nionata"]
		},
		{
			type: "enhancement"
			description: """
				The NATS sink can now parse comma separated urls.
				"""
			contributors: ["whatcouldbepizza"]
		},
		{
			type: "fix"
			description: """
				The `kubernetes_logs` source now sets a `user-agent` header when querying [k8s apiserver](https://kubernetes.io/docs/reference/command-line-tools-reference/kube-apiserver/).
				"""
			contributors: ["ganelo"]
		},
		{
			type: "fix"
			description: """
				Fix the HTTP config provider to correctly parse TOML provided by the given HTTP endpoint.
				"""
			contributors: ["PriceHiller"]
		},
		{
			type: "enhancement"
			description: """
				The `log_to_metric` transformer tag key are now template-able which enables tags expansion.
				"""
			contributors: ["titaneric"]
		},
		{
			type: "enhancement"
			description: """
				Add VRL function `parse_dnstap` that can parse dnstap data and produce output in the same format as `dnstap` source.
				"""
			contributors: ["esensar"]
		},
		{
			type: "enhancement"
			description: """
				Adds a `force_path_style` option to the `aws_s3` sink that allows users to configure virtual host style addressing. The value defaults to `true` to maintain existing behavior.
				"""
			contributors: ["sam6258"]
		},
		{
			type: "fix"
			description: """
				Allow the `skip_unknown_fields` setting to be optional, thereby allowing use of the defaults provided by the ClickHouse server. Setting it to `true` will permit skipping unknown fields and `false` will make ClickHouse strict on what fields it accepts.
				"""
			contributors: ["PriceHiller"]
		},
		{
			type: "fix"
			description: """
				Retry Kafka messages that error with a policy violation so messages are not lost.
				"""
			contributors: ["PriceHiller"]
		},
		{
			type: "fix"
			description: """
				Changes the fingerprint for file sources to use uncompressed file content
				as a source of truth when fingerprinting lines and checking
				`ignored_header_bytes`. Previously this was using the compressed bytes. For now, only gzip compression is supported.
				"""
			contributors: ["roykim98"]
		},
		{
			type: "fix"
			description: """
				The `filter` transform now generates a more accurate config when generated via `vector generate` by using a comparison rather than an assignment.
				"""
			contributors: ["abcdam"]
		},
		{
			type: "fix"
			description: """
				The `gcp_pubsub` source no longer has a 4MB message size limit.
				"""
			contributors: ["sbalmos"]
		},
		{
			type: "fix"
			description: """
				Fix `opentelemetry` sink input resolution. The sink is now using the underlying protocol to determine what inputs are accepted.
				"""
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: """
				The `socket` sink now supports `unix_datagram` as a valid `mode`. This feature is only available on Linux.
				"""
			contributors: ["jpovixwm"]
		},
		{
			type: "enhancement"
			description: """
				Add active and inactive metrics for anon and file memory to the cgroup collector. These additional metrics allow you to better understand the existing cgroup memory metrics.
				"""
			contributors: ["nionata"]
		},
		{
			type: "enhancement"
			description: """
				Add an option to Elasticsearch sink to set a fallback index if the provided template in the `bulk.index` field
				cannot be resolved
				"""
			contributors: ["ArunPiduguDD"]
		},
		{
			type: "enhancement"
			description: """
				The Alpine base image used for Vector `-alpine` and `-distroless-static` images was updated to `3.21`.
				"""
		},
		{
			type: "fix"
			description: """
				The `sample` transform now correctly uses the configured `sample_rate_key` instead of always using `"sample_rate"`.
				"""
			contributors: ["dekelpilli"]
		},
		{
			type: "enhancement"
			description: """
				A new `GLACIER_IR` option was added to `storage_class` for `aws_s3` sink.
				"""
			contributors: ["MikeHsu0618"]
		},
	]

	commits: [
		{sha: "f8af40e05ac49b6978c518e6f226a96d2f69ee73", date: "2024-12-03 06:14:17 UTC", description: "can now parse multiple nats urls", pr_number: 21823, scopes: ["nats sink"], type: "enhancement", breaking_change: false, author: "Dmitriy", files_count: 3, insertions_count: 21, deletions_count: 2},
		{sha: "77bbbb46ef814e5112f036f1d7c6cadbd84e9798", date: "2024-12-03 03:31:13 UTC", description: "Bump the patches group across 1 directory with 7 updates", pr_number: 21927, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 212, deletions_count: 189},
		{sha: "daa9f24cfec8617e04603b2701782d6705b865c2", date: "2024-12-03 04:02:32 UTC", description: "Bump bytes from 1.8.0 to 1.9.0", pr_number: 21912, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 94, deletions_count: 94},
		{sha: "dd8ed5dfa5e716c0ce8bff0b3f25398b5f5f09d0", date: "2024-12-02 20:29:47 UTC", description: "supports input based on encoding type", pr_number: 21726, scopes: ["file sink"], type: "feat", breaking_change: false, author: "Nicholas Ionata", files_count: 3, insertions_count: 174, deletions_count: 19},
		{sha: "2d785c374b3bde2d5cb2e1366662098dda985f2d", date: "2024-12-03 02:30:57 UTC", description: "Bump rustls-pemfile from 1.0.3 to 1.0.4", pr_number: 21886, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "03095171a60a25755d8b1efc1886940666ce6b7f", date: "2024-12-03 22:11:10 UTC", description: "Bump os_info from 3.8.2 to 3.9.0", pr_number: 21935, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4f14967d156ccbbe6f042f5c264389a33d3fd865", date: "2024-12-03 22:11:26 UTC", description: "Bump indexmap from 2.6.0 to 2.7.0", pr_number: 21934, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 26, deletions_count: 26},
		{sha: "aa48feaed2d8a2f1f705e181852b4bec4cb0ad95", date: "2024-12-03 23:56:56 UTC", description: "multiple workflows need 'contents: write' permissions", pr_number: 21940, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 4, deletions_count: 4},
		{sha: "129245a38a95af8f5ba073e0d1729ff4534cd15b", date: "2024-12-04 02:03:39 UTC", description: "introduce OSSF scorecard", pr_number: 21942, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 73, deletions_count: 0},
		{sha: "4de5a773f0a2c82318352876b597b18190549332", date: "2024-12-04 07:15:40 UTC", description: "Bump vrl from `a958c5d` to `c8af70c`", pr_number: 21936, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 57, deletions_count: 51},
		{sha: "a8371dd06b35f61a8afeb8b745287c2dfeef4f1a", date: "2024-12-04 03:11:55 UTC", description: "Add documentation for new `zip` function", pr_number: 21941, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 55, deletions_count: 0},
		{sha: "1ae5a5cf961df318ee50239e97aaa4d1cc7546db", date: "2024-12-04 17:26:18 UTC", description: "Bump the patches group with 4 updates", pr_number: 21944, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 18, deletions_count: 18},
		{sha: "4783ee269d2704fdcf8a6d7485682674c7913cd6", date: "2024-12-04 20:59:29 UTC", description: "tweaks to the minor release guide", pr_number: 21952, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 2},
		{sha: "023f5fef2775dc676122159d1733ec1f88849c4f", date: "2024-12-03 20:26:10 UTC", description: "Prepare v0.43.0", pr_number: 21906, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 38, insertions_count: 479, deletions_count: 108},
		{sha: "d5e9cbbf7efeeafd79cdde1f97c0aa80f04b2de6", date: "2024-12-04 22:21:58 UTC", description: "Regenerate Cargo.lock to fix bad conflict resolution", pr_number: 21957, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 13},
		{sha: "17b7f1af5703c590f12cfced48ab01829f7d6a30", date: "2024-12-05 01:22:35 UTC", description: "cargo vdev build manifests", pr_number: 21956, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "b400258c5a82f7428abd7c31fc421b9a1c4c5f70", date: "2024-12-05 01:22:44 UTC", description: "mention job manual trigger", pr_number: 21954, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 0},
		{sha: "d4a278e5aa624615efc076afe5b246953f3c0a8d", date: "2024-12-05 01:23:35 UTC", description: "change some  dependabot schedules to weekly", pr_number: 21930, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "53e4c666d1780e80ea66e12ef182e46834a22220", date: "2024-12-05 02:59:32 UTC", description: "dependabot allow all dep types", pr_number: 21958, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "32df3cdcb4b8ced2b786525feb00372701b1c698", date: "2024-12-05 03:05:28 UTC", description: "bump vector version to 0.44.0", pr_number: 21955, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 10, deletions_count: 11},
		{sha: "f9b7468782de38f1a24dcf49d4b28d9af794ba56", date: "2024-12-05 17:51:31 UTC", description: "replace the aqua local registry with the standard registry", pr_number: 21959, scopes: ["dev"], type: "chore", breaking_change: false, author: "Shunsuke Suzuki", files_count: 3, insertions_count: 7, deletions_count: 46},
		{sha: "e91fec648a4e3f034cd12576185320f3269405a2", date: "2024-12-05 03:14:51 UTC", description: "Bump xt0rted/pull-request-comment-branch from 2 to 3", pr_number: 21843, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "f929a9ab1cd229a8df342a6b9230c5cbee6e588d", date: "2024-12-05 07:03:56 UTC", description: "Bump hashbrown to 0.15.2", pr_number: 21962, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "75f35e2379e44614c811ee7ff5e6c0ab99ad2b89", date: "2024-12-05 18:12:34 UTC", description: "add example syntax for valid expressions.", pr_number: 21963, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Benson Fung", files_count: 1, insertions_count: 22, deletions_count: 0},
		{sha: "8bcc00f49de193d69d3291be50a189fed9356b37", date: "2024-12-06 02:14:48 UTC", description: "fix links in route/exclusive route", pr_number: 21964, scopes: ["exclusive_route transform", "route transform"], type: "docs", breaking_change: false, author: "Gareth Pelly", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "f4cda444cb363a19e017383d91c4f0ddc7d7050f", date: "2024-12-05 19:22:36 UTC", description: "Bump patched tokio-util to 0.7.13", pr_number: 21966, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "87c04ebc72fad70f3326f74632d283592d8920a1", date: "2024-12-06 20:42:02 UTC", description: "add notice for 'to_float'", pr_number: 21973, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "d3f160bccc588cc72f3b8d398fce15fce3894f67", date: "2024-12-07 01:25:20 UTC", description: "fix fallibility badge", pr_number: 21977, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 7, deletions_count: 2},
		{sha: "9d498fc6a62bc01378c3f67ed38e81d6e6d22d9c", date: "2024-12-07 03:30:00 UTC", description: "run regression suite weekly", pr_number: 21979, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "d4854ded038f2852c2676c89bf89bf075dd7f914", date: "2024-12-08 00:49:59 UTC", description: "Fix baseline SHA default for new weekly schedule", pr_number: 21980, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "a14aac7f8029b376baa286a1e2eac5fa295a00f0", date: "2024-12-10 02:01:33 UTC", description: "Bump regression replicas to 100", pr_number: 21986, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "7013ded278e5436ee371122060310c9a3dfdf457", date: "2024-12-09 21:14:26 UTC", description: "run nightly workflow on weekday commits (only)", pr_number: 21987, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "13a5508ed1d9bb74ba96e86ab71a00c62e9f53d4", date: "2024-12-10 13:17:10 UTC", description: "use configured sample_rate_key value", pr_number: 21971, scopes: ["sample transform"], type: "fix", breaking_change: false, author: "Dekel Pilli", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "e8a836f412cd41007afa176537784afdcd6d0cd3", date: "2024-12-10 03:45:24 UTC", description: "Render chores for release changelog", pr_number: 21990, scopes: ["website"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "d38961a6f785a2245625a50fc32aecef866b77c9", date: "2024-12-10 05:46:12 UTC", description: "correctly propagate http config provider Result to caller", pr_number: 21982, scopes: ["config"], type: "fix", breaking_change: false, author: "Price Hiller", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "6dbe5dac1d537a5484fb5f5ab228d41ba5b9f0c6", date: "2024-12-10 13:13:20 UTC", description: "update publicsuffix to use newer `idna`", pr_number: 21996, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 13},
		{sha: "9a71bc39cb7ddb6904743e90645f8491539ce1f6", date: "2024-12-10 20:22:15 UTC", description: "Bump nanoid from 3.3.6 to 3.3.8 in /website", pr_number: 22001, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "ae05a475beb91a3bcb3efc0406a531e4b738ccce", date: "2024-12-11 02:32:19 UTC", description: "Ignore RUSTSEC-2024-0421 temporarily", pr_number: 22000, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "2f53b1cb32df2d914b5029ce4dd5c7a7455d03a8", date: "2024-12-10 20:38:57 UTC", description: "Use newest SMP version", pr_number: 22004, scopes: ["ci"], type: "chore", breaking_change: false, author: "Scott Opell", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "2815ec08c3ecac38f768be7274ddb560c4423722", date: "2024-12-11 00:28:37 UTC", description: "remove obsolete note", pr_number: 22012, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "b6a59c72692da81c83195217be1b97f5e1e58009", date: "2024-12-11 22:46:59 UTC", description: "cherry-pick v0.43.1 patch commits", pr_number: 22011, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 47, deletions_count: 7},
		{sha: "0b801ebad6a4cd7c0ae23ba6e26ecfba73d48ed4", date: "2024-12-12 05:59:49 UTC", description: "Skip humio integration tests in CI", pr_number: 22016, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 12, deletions_count: 10},
		{sha: "69bfb1242033a30403ed1ac2541a9e4c7bce2de8", date: "2024-12-12 19:54:15 UTC", description: "Fix integration test file syntax", pr_number: 22025, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "fa392e1fb4e6219601269e4a6c8c083b47f3d9a7", date: "2024-12-12 13:55:10 UTC", description: "build manifests for vector 0.43.1 chart 0.38.1", pr_number: 22019, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 18, insertions_count: 22, deletions_count: 23},
		{sha: "18de1ea8a10bbe472bafca17c775b0fb8eaa0ed6", date: "2024-12-12 14:06:17 UTC", description: "bump hugo to latest version", pr_number: 22018, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 2, insertions_count: 39, deletions_count: 31},
		{sha: "70837f3b1f45c82f951e71ebf3480152ac569690", date: "2024-12-13 06:31:43 UTC", description: "fix greptimedb examples", pr_number: 21984, scopes: ["greptimedb_metrics sink"], type: "chore", breaking_change: false, author: "Ning Sun", files_count: 4, insertions_count: 11, deletions_count: 8},
		{sha: "3a89c8ed54269cbeb2a5ddc3dcc75e920721171b", date: "2024-12-13 02:07:13 UTC", description: "Bump rdkafka from 0.35.0 to 0.37.0", pr_number: 21929, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 9, deletions_count: 34},
		{sha: "08bb46fb3a3c7c4b9e8b73be7e4e0bd445213b6d", date: "2024-12-12 22:38:37 UTC", description: "remove some unused deps", pr_number: 22021, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 17, insertions_count: 3, deletions_count: 127},
		{sha: "81c215527d39837808a324075d6290afb3a5a4b2", date: "2024-12-12 23:13:53 UTC", description: "SMP Upgrade", pr_number: 22010, scopes: ["ci"], type: "chore", breaking_change: false, author: "Scott Opell", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f73fb10277b14d3e16884720a96d20c2bbd0550a", date: "2024-12-13 05:47:34 UTC", description: "Switch to new humio image", pr_number: 22015, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "f8f33b5afe87ea10d3395d5636594a9078e15cf8", date: "2024-12-13 13:03:56 UTC", description: "make `skip_unknown_fields` optional", pr_number: 22020, scopes: ["clickhouse sink"], type: "fix", breaking_change: true, author: "Price Hiller", files_count: 5, insertions_count: 43, deletions_count: 13},
		{sha: "7e93489fdb784c69074d4db17fd379554c62d77a", date: "2024-12-14 01:51:21 UTC", description: "Update to Rust 1.82.0", pr_number: 22030, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 9, deletions_count: 10},
		{sha: "d8dcf6c1ade08cec43a7d184a40db1561e28dad8", date: "2024-12-17 00:32:16 UTC", description: "Increases inner GH action timeout to match job-level timeout", pr_number: 22038, scopes: ["soak tests"], type: "chore", breaking_change: false, author: "Scott Opell", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "d4fc91f8482bc0ef4e16fef021e7dac9eca7bb81", date: "2024-12-17 01:18:59 UTC", description: "Fix SMP job timeouts", pr_number: 22043, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "475e93f182cc51e06ca74f8545a245203bcec1d0", date: "2024-12-17 05:23:43 UTC", description: "Pin ubuntu versions in GHA", pr_number: 22044, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 23, insertions_count: 30, deletions_count: 30},
		{sha: "a0dfa2bc40be16e0b80bcf2c76d9b6fb66c19380", date: "2024-12-18 00:59:28 UTC", description: "Bump docker/setup-buildx-action from 3.7.1 to 3.8.0", pr_number: 22039, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "d44c5d60655d8df7e70f0b68b8a12381c3fb25f7", date: "2024-12-18 18:01:51 UTC", description: "Regenerate Cargo.lock", pr_number: 22048, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 21, deletions_count: 0},
		{sha: "485c38b0ca77a87b214857d6c5ec6d256efacfe2", date: "2024-12-18 18:27:06 UTC", description: "Bump timeout for cli tests", pr_number: 22056, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6de31c3d70f7f6eb7c46f68eac34575b3132e304", date: "2024-12-18 23:38:02 UTC", description: "Add option to specify a default index if template cannot be resolved for Elasticsearch destination index", pr_number: 21953, scopes: ["elasticsearch sink"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 5, insertions_count: 41, deletions_count: 7},
		{sha: "434e742df2dbf9bb6a3e8ffdf2dc781a34096965", date: "2024-12-19 00:06:43 UTC", description: "Set user-agent for k8s apiserver requests.", pr_number: 21905, scopes: ["kubernetes_logs source"], type: "fix", breaking_change: false, author: "Orri Ganel", files_count: 7, insertions_count: 218, deletions_count: 120},
		{sha: "656136a922500cfbd367e800adf33708ce484126", date: "2024-12-18 22:13:24 UTC", description: "Bump cargo-deny timeout", pr_number: 22058, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1275f1ad67ea02cb0ba4ca476695268611d1f185", date: "2024-12-19 03:37:25 UTC", description: "retry messages that result in kafka policy violations", pr_number: 22041, scopes: ["kafka sink"], type: "fix", breaking_change: false, author: "Price Hiller", files_count: 2, insertions_count: 8, deletions_count: 2},
		{sha: "7c6d0c942831441c4aa84ecedfb19c627d3f426d", date: "2024-12-19 10:38:34 UTC", description: "support unix datagram mode", pr_number: 21762, scopes: ["socket sink"], type: "enhancement", breaking_change: false, author: "jpovixwm", files_count: 10, insertions_count: 426, deletions_count: 135},
		{sha: "029a2ffc33c66fe1d8b755ad309b695317b1f542", date: "2024-12-20 04:04:58 UTC", description: "add option to use virtual addressing", pr_number: 21999, scopes: ["aws_s3 sink"], type: "feat", breaking_change: false, author: "Scott Miller", files_count: 22, insertions_count: 112, deletions_count: 25},
		{sha: "922937fd89740e259f7bf02322d19538b5536d83", date: "2025-01-03 03:45:22 UTC", description: "Bump bufbuild/buf-setup-action from 1.47.2 to 1.48.0", pr_number: 22078, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f6295e758b95ba26fed7cfb43e2731066a3c2d63", date: "2025-01-02 20:25:35 UTC", description: "Update year for parse_klog test", pr_number: 22109, scopes: ["tests"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "792aeeae5c216288bc11b78185ecfb7184ac3ece", date: "2025-01-03 07:47:57 UTC", description: "add `parse_dnstap` function", pr_number: 21985, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 27, insertions_count: 672, deletions_count: 43},
		{sha: "f3549dbc00d285e06ee109233508a7fc3e02d099", date: "2025-01-02 23:58:21 UTC", description: "Revert add `parse_dnstap` function", pr_number: 22114, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 27, insertions_count: 43, deletions_count: 672},
		{sha: "25948370afa3ceae74404391c30d695a4fb17dd2", date: "2025-01-03 01:21:57 UTC", description: "add parse_klog notice", pr_number: 22111, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "d15dc0153fcdd256fa6b1231aaa0b2f10ff6b171", date: "2025-01-03 07:01:08 UTC", description: "Correct `filter` transform generation via `vector generate` to use a comparison", pr_number: 22079, scopes: ["config"], type: "fix", breaking_change: false, author: "Adam", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "ca084cc1473c08c5a91ab79fe6c6c4c6a3f4fab8", date: "2025-01-02 23:18:37 UTC", description: "Update Rust toolchain to 1.83.0", pr_number: 22068, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 152, insertions_count: 354, deletions_count: 346},
		{sha: "2ed4cd971d14c17c392143d44ac07488e116f7ed", date: "2025-01-03 03:57:22 UTC", description: "Revert addition of unix datagram mode", pr_number: 22113, scopes: ["socket sink"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 135, deletions_count: 426},
		{sha: "5cad353cca492db26d3b6eecf8a61a052fac6f99", date: "2025-01-03 01:26:05 UTC", description: "Fix example for linux auth log", pr_number: 22110, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "35845a19df01a33c62f7443125d436ab6404cfa2", date: "2025-01-03 21:52:18 UTC", description: "fix network_* metric type", pr_number: 22118, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "3b90e2695c6f878f78ef7d1699711d8bbca6ae40", date: "2025-01-03 23:09:25 UTC", description: "Remove Tonic default 4MB decode size limit", pr_number: 22091, scopes: ["gcp_pubsub source"], type: "fix", breaking_change: false, author: "Scott Balmos", files_count: 2, insertions_count: 6, deletions_count: 1},
		{sha: "f59fd9c2b129fc1732146ac7f5cf515843ea8665", date: "2025-01-06 23:03:07 UTC", description: "input() should delegate to internal config input()", pr_number: 22126, scopes: ["opentelemetry sink"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 6, deletions_count: 1},
		{sha: "343ab7c0d57af5448712a8dd1adcab3e91bdc08b", date: "2025-01-07 12:30:57 UTC", description: "add glacier instant retrieval storage class", pr_number: 22123, scopes: ["aws_s3 sink"], type: "enhancement", breaking_change: false, author: "mikehsu", files_count: 3, insertions_count: 9, deletions_count: 0},
		{sha: "d78f968e6f5046dfd3eff0ff2b5285c5bcab27d6", date: "2025-01-07 22:56:15 UTC", description: "macOS runner for Intel", pr_number: 22134, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 5},
		{sha: "08303a156cc08d05cc0b31ac45426940818bee63", date: "2025-01-08 04:56:32 UTC", description: "re-enable `parse_dnstap` without VRL playground support", pr_number: 22124, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Ensar Sarajčić", files_count: 25, insertions_count: 670, deletions_count: 43},
		{sha: "0af3e44511d238d95e25b2ec14b6396f12b2c64a", date: "2025-01-08 00:31:25 UTC", description: "bump VRL", pr_number: 22135, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 73, deletions_count: 37},
		{sha: "c593d4550a50c756a9ff5518a185cc8f83dc36fc", date: "2025-01-08 01:32:55 UTC", description: "add known issue for v0.43.1", pr_number: 22139, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 16, deletions_count: 0},
		{sha: "8668cdeaf696ea63af2262c4307bc85cf536683d", date: "2025-01-08 08:44:46 UTC", description: "support unix datagram mode", pr_number: 22120, scopes: ["socket sink"], type: "enhancement", breaking_change: false, author: "jpovixwm", files_count: 10, insertions_count: 434, deletions_count: 144},
		{sha: "0720345ef61398819174290fd6c4de6d0e1e9446", date: "2025-01-09 02:23:07 UTC", description: "publish ARM macOS builds", pr_number: 22140, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 56, deletions_count: 23},
		{sha: "99af5be92ff440d7bcd13993c6c320c724c82055", date: "2025-01-10 12:14:37 UTC", description: "render array of object type in component docs", pr_number: 22138, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 9, deletions_count: 1},
		{sha: "45cd651cc90e629d0835e0f8e8262213c60bb29b", date: "2025-01-09 21:58:44 UTC", description: "Update lading to 0.25.3, lower resource allocations in experiments", pr_number: 22147, scopes: ["ci"], type: "chore", breaking_change: false, author: "Brian L. Troutwine", files_count: 33, insertions_count: 27, deletions_count: 47},
		{sha: "68ee4f27d09084a4f9f28d167c4610a5e83067bd", date: "2025-01-09 22:41:44 UTC", description: "use uncompressed content for fingerprinting files (lines and ignored_header_bytes)", pr_number: 22050, scopes: ["file source"], type: "fix", breaking_change: true, author: "roykim98", files_count: 5, insertions_count: 272, deletions_count: 11},
		{sha: "1ef01aeeef592c21d32ba4d663e199f0608f615b", date: "2025-01-10 20:42:50 UTC", description: "Logs processed S3 objects", pr_number: 22083, scopes: ["aws_s3 source"], type: "enhancement", breaking_change: false, author: "Fred Damstra", files_count: 2, insertions_count: 49, deletions_count: 5},
		{sha: "5de96d4b7f85a883eedbff8a889a3f41b012abc2", date: "2025-01-11 12:36:16 UTC", description: "add docs for new `timezone` option in parse_timestamp function", pr_number: 22121, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 17, deletions_count: 1},
		{sha: "92a1034a13cdb094d29d8dcbd886a6e9ed18dd73", date: "2025-01-11 12:42:05 UTC", description: "add docs for new `parse_bytes` function", pr_number: 22089, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 2, insertions_count: 74, deletions_count: 0},
		{sha: "a976987b8cd7a5ad2a92cbd5d3f98c4572266180", date: "2025-01-10 23:49:33 UTC", description: "Revert \"dependabot allow all dep types\"", pr_number: 22160, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "3e3614ee14af2613222426ec0c1b5c36d7ee360b", date: "2025-01-10 23:53:08 UTC", description: "add cargo commands to PR template", pr_number: 22159, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "8d94da81f8461754d04ef4c4b78b7a0daf9162a1", date: "2025-01-11 14:06:26 UTC", description: "decode_charset.cue, encode_charset.cue documentations", pr_number: 21981, scopes: ["vrl"], type: "docs", breaking_change: false, author: "엄준일", files_count: 3, insertions_count: 105, deletions_count: 0},
		{sha: "f8fde0f3fef4f067280153c0d525f904166a006f", date: "2025-01-11 06:17:08 UTC", description: "update documentation for `parse_nginx_log` with the `main` log format", pr_number: 22119, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Aurélien Tamisier", files_count: 2, insertions_count: 25, deletions_count: 3},
		{sha: "f0fec05fa572c6d40b7a93d636414e103d35b659", date: "2025-01-10 21:27:54 UTC", description: "Add additional cgroup memory metrics", pr_number: 22153, scopes: ["host_metrics source"], type: "feat", breaking_change: false, author: "Nicholas Ionata", files_count: 3, insertions_count: 51, deletions_count: 0},
		{sha: "bd976f1d1d3a8f7e1bfe6eb6db9019b768457e76", date: "2025-01-11 01:28:10 UTC", description: "remove obsolete mentions to netlify", pr_number: 22162, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 29, deletions_count: 1545},
		{sha: "4605e5bb54d6d2fe47163c09f771670f96c68b3f", date: "2025-01-11 15:21:26 UTC", description: "add more `parse-duration` function doc", pr_number: 22088, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 7, deletions_count: 0},
		{sha: "890a60ade8b73bb7bc26c10ad0a6dad7707d7ead", date: "2025-01-11 01:24:46 UTC", description: "Add documentation for new `object_from_array` function", pr_number: 21969, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 54, deletions_count: 0},
		{sha: "a632a93b0e4384754772793e6f412d906ec65a7d", date: "2025-01-11 13:07:08 UTC", description: "add crc vrl function docs", pr_number: 21924, scopes: ["vrl"], type: "docs", breaking_change: false, author: "ivor11", files_count: 4, insertions_count: 200, deletions_count: 3},
		{sha: "a02d29623f8d78a23426706c1eb9c9d5505ab904", date: "2025-01-11 08:49:08 UTC", description: "Bump bufbuild/buf-setup-action from 1.48.0 to 1.49.0", pr_number: 22163, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "deb5846c860f6d4e33a0547a5296e07113e51e6f", date: "2025-01-11 08:49:18 UTC", description: "Bump docker/setup-qemu-action from 3.2.0 to 3.3.0", pr_number: 22164, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "801a229642a628e1d26c8830fd7a871c4666b470", date: "2025-01-11 08:49:28 UTC", description: "Bump docker/build-push-action from 6.10.0 to 6.11.0", pr_number: 22165, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3e213db56beac5f84fe3c92f8cb169026cb10647", date: "2025-01-11 09:26:33 UTC", description: "Bump tokio from 1.42.0 to 1.43.0", pr_number: 22178, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 22, deletions_count: 22},
		{sha: "eba81f4d35f63ec4ccbfc4136711ad5265370e37", date: "2025-01-11 09:26:55 UTC", description: "Bump itertools from 0.13.0 to 0.14.0", pr_number: 22176, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "cc8d4e48481f7c7793fea1458dfaa8086006a0be", date: "2025-01-11 09:27:26 UTC", description: "Bump serde_with from 3.11.0 to 3.12.0", pr_number: 22179, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 16, deletions_count: 16},
		{sha: "949b5413f2f276b8224d39335a51e2ef972e9a6c", date: "2025-01-11 09:28:32 UTC", description: "Bump colored from 2.1.0 to 3.0.0", pr_number: 22170, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 6},
		{sha: "3077efc5e2c1128f3527045aad64768ff05f3b25", date: "2025-01-11 09:33:48 UTC", description: "Bump rstest from 0.23.0 to 0.24.0", pr_number: 22180, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 7},
		{sha: "269e8627f9823fa3657bc74bcdf2b1882f7ea718", date: "2025-01-11 09:34:44 UTC", description: "Bump twox-hash from 2.0.1 to 2.1.0", pr_number: 22182, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ff8dfb7fc76a799caac703e426d1e5c24da581fa", date: "2025-01-14 09:58:59 UTC", description: "support tags expansion similar to `labels expansion` for loki sink", pr_number: 21939, scopes: ["log_to_metric transform"], type: "feat", breaking_change: false, author: "Huang Chen-Yi", files_count: 7, insertions_count: 294, deletions_count: 106},
		{sha: "6a50999a8a4f7583ffeb16a16e4a3742f59141ab", date: "2025-01-13 21:06:21 UTC", description: "Bump tempfile from 3.14.0 to 3.15.0", pr_number: 22175, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 5},
	]
}
