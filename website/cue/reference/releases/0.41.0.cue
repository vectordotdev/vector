package metadata

releases: "0.41.0": {
	date:     "2024-09-09"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.41.0!

		In addition to the enhancements and fixes listed below, this release includes the following notable features:
		- A new `greptimedb_logs` sink was added for sending logs to
		  [GreptimeDB](https://greptime.com/)
		- A new `static_metrics` source was added for emitting preconfigured metrics on an interval.
		  This can be useful for heartbeat-style metrics or for emitting a metric based on an
		  environment variable.
		- Windows now supports automatic config reloading via `--watch-config` as exists on other
		  *nix platforms.

		There are no breaking changes or deprecations with this release and so no upgrade guide.
		"""

	known_issues: [
		"""
			The `vector` source cannot receive events encoded by the`vector` sink
			[#21252](https://github.com/vectordotdev/vector/issues/21252). This will be fixed in
			v0.41.1.
			""",
	]

	changelog: [
		{
			type: "feat"
			description: """
				VRL was updated to v0.18.0. This includes the following changes:

				### New Features

				- Added `unflatten` function to inverse the result of the `flatten` function. This function is useful when you want to convert a flattened object back to its original form.
				- The `parse_json` function now accepts an optional `lossy` parameter (which defaults to `true`).

				This new parameter allows to control whether the UTF-8 decoding should be lossy or not, replacing
				invalid UTF-8 sequences with the Unicode replacement character (U+FFFD) if set to `true` or raising an error
				if set to `false` and an invalid utf-8 sequence is found. (https://github.com/vectordotdev/vrl/pull/269)
				- Added casing functions `camelcase`, `kebabcase`, `screamingsnakecase`, `snakecase`, `pascalcase` (https://github.com/vectordotdev/vrl/pull/973)
				- Added `parse_influxdb` function to parse events encoded using the [InfluxDB line protocol](https://docs.influxdata.com/influxdb/cloud/reference/syntax/line-protocol/).

				### Enhancements

				- The `match_datadog_query` function now accepts `||` in place of `OR` and `&&` in
				place of `AND` in the query string, which is common Datadog syntax. (https://github.com/vectordotdev/vrl/pull/1001)

				### Fixes

				- `decode_base64` no longer requires canonical padding. (https://github.com/vectordotdev/vrl/pull/960)
				- The assumption of a Datadog Logs-based intake event structure has been removed
				from the `match_datadog_query` function. (https://github.com/vectordotdev/vrl/pull/1003)
				- For the `parse_influxdb` function the `timestamp` and `tags` fields of returned objects are now
				correctly marked as nullable.
				- The `loki` sink now has the ability to send structured metadata via the added
				`structured_metadata` option.
				"""
		},
		{
			type: "feat"
			description: """
				The `loki` sink now has the ability to send structured metadata via the added
				`structured_metadata` option.
				"""
			contributors: ["maxboone"]
		},
		{
			type: "fix"
			description: """
				Support was added for fetching secrets from AWS secrets manager when using SSO
				profiles in a `~/.aws/config` file.
				"""
			contributors: ["britton-from-notion", "ycrliu"]
		},
		{
			type: "feat"
			description: """
				A new `greptimedb_logs` sink has been added to forward logs to
				[Greptime](https://greptime.com/).

				As part of this addition, the existing `greptimedb` sink was renamed to
				`greptimedb_metrics`.
				"""
			contributors: ["GreptimeTeam"]
		},
		{
			type: "feat"
			description: """
				A new `static_metrics` source was added. This source periodically emits preconfigured values.
				"""
			contributors: ["esensar"]
		},
		{
			type: "fix"
			description: """
				The `socket` sink can now accept metric events when using codecs that support encoding metrics.
				"""
			contributors: ["nichtverstehen"]
		},
		{
			type: "enhancement"
			description: """
				The `geoip` enrichment table now has support for the GeoIP2-Anonymous-IP MaxMind database type.
				"""
			contributors: ["publicfacingusername"]
		},
		{
			type: "feat"
			description: """
				Support was added for configuring the endpoint of the `honeycomb` sink. This allows
				sending data to Honeycomb's EU endpoint in addition to the default US endpoint.
				"""
			contributors: ["raytung"]
		},
		{
			type: "fix"
			description: """
				An issue was fixed where the configuration watcher did not properly handle recursive
				directories. This fix ensures configuration will be reloaded when using the
				automatic namespacing feature of configuration loading.
				"""
			contributors: ["ifuryst"]
		},
		{
			type: "enhancement"
			description: """
				The `kafka` sink now retries sending events that failed to be sent for transient reasons. Previously
				it would reject these events.
				"""
			contributors: ["frankh"]
		},
		{
			type: "fix"
			description: """
				Log sources can now use metrics-only decoders such as the recently added `influxdb` decoder.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "fix"
			description: """
				The `influxdb` decoder now uses nanosecond-precision for timestamps instead of
				microsecond-precision, as stated in [InfluxDB's
				documentation](https://docs.influxdata.com/influxdb/v1/write_protocols/line_protocol_tutorial/#timestamp).
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "enhancement"
			description: """
				The `kafka` sink now supports OIDC authentication through the exposed `librdkafka_options`.
				"""
			contributors: ["zapdos26"]
		},
		{
			type: "fix"
			description: """
				The `kafka` sink no longer emits warnings due to applying rdkafka options to
				a consumer used for the health check. Now it uses the producer client for the health
				check.
				"""
			contributors: ["belltoy"]
		},
		{
			type: "fix"
			description: """
				The `amqp` source no longer panics when deserializing metrics (such as the
				`native_json` codec).
				"""
			contributors: ["kghost"]
		},
		{
			type: "fix"
			description: """
				The `datadog_search` condition, which may be used in filter component conditions,
				now properly handles direct numeric type equality checks for log attributes.
				"""
		},
		{
			type: "fix"
			description: """
				The `reduce` transform can now reduce fields that contain special characters.
				"""
		},
		{
			type: "fix"
			description: """
				Vector no longer panics during configuration loading if a secret is used for
				a configuration option that has additional validation (such as URIs).
				"""
		},
		{
			type: "fix"
			description: """
				The `file` source now properly handle exclude patterns with multiple slashes when
				matching files.
				"""
			contributors: ["suikammd"]
		},
		{
			type: "fix"
			description: """
				The `http_server` source now properly writes all the specified query parameters to
				`%http_server.query_parameters` when log namespacing is enabled.
				"""
			contributors: ["Zettroke"]
		},
		{
			type: "fix"
			description: """
				The `socket` source now respects the global `log_namespace` setting.
				"""
			contributors: ["Zettroke"]
		},
		{
			type: "enhancement"
			description: """
				Windows now supports the `--watch-config` command line parameter just like every
				other platform and will reload the configuration files on any change.
				"""
			contributors: ["darklajid"]
		},
	]

	commits: [
		{sha: "d728d5aa9f82712f0660e5fb313bda8f6bd19912", date: "2024-07-27 04:03:36 UTC", description: "Add global CODEOWNERS fallback", pr_number: 20947, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "cf8eecfa9db313da44fc09e1bf2b6fad0ca4d6f6", date: "2024-07-27 05:16:34 UTC", description: "Bump bstr from 1.9.1 to 1.10.0", pr_number: 20937, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "18d1ce8de8c0ac06c737621260e8f21894b25971", date: "2024-07-27 05:16:48 UTC", description: "Bump assert_cmd from 2.0.14 to 2.0.15", pr_number: 20936, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d85394f687742cce6fc29e83f0c39de6f7a6edc6", date: "2024-07-27 12:16:56 UTC", description: "Bump env_logger from 0.11.4 to 0.11.5", pr_number: 20935, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "1b584bda5fdf03b48212f6feaffbef0cf6a21734", date: "2024-07-27 12:17:06 UTC", description: "Bump toml from 0.8.15 to 0.8.16", pr_number: 20934, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 10},
		{sha: "96de0ac0be0da6228a38906cd8e3400e7b6910cc", date: "2024-07-27 12:17:29 UTC", description: "Bump bufbuild/buf-setup-action from 1.35.0 to 1.35.1", pr_number: 20929, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7f286c4798d9313b2602a4a94e8f0932c3025383", date: "2024-07-27 08:23:50 UTC", description: "Changes to support GeoIP Anonymous IP database", pr_number: 20946, scopes: ["enrichment_tables"], type: "enhancement", breaking_change: false, author: "Justin Wolfington", files_count: 7, insertions_count: 43, deletions_count: 11},
		{sha: "886f4e1f5978ef652c8dbca9981c73f566efd0f8", date: "2024-07-27 13:10:08 UTC", description: "Bump chrono from 0.4.37 to 0.4.38", pr_number: 20309, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "6e803a3aa24dc00ca232b152c275e41da1adc090", date: "2024-07-31 00:47:22 UTC", description: "Switch to PR Reviews for triggering CI", pr_number: 20892, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 12, insertions_count: 290, deletions_count: 366},
		{sha: "9121a7fe711ba673aa5aa791a6e11530f40dceb6", date: "2024-07-31 12:23:42 UTC", description: "Allow socket sink to accept metrics.", pr_number: 20930, scopes: ["socket sink"], type: "fix", breaking_change: false, author: "Kirill Nikolaev", files_count: 3, insertions_count: 70, deletions_count: 16},
		{sha: "ecce2ed10ed593f1e0d1d1bf17726013aeaf2e4e", date: "2024-07-31 13:45:41 UTC", description: "fix socket source ignoring global log_namespace when computing outputs", pr_number: 20966, scopes: ["socket source"], type: "fix", breaking_change: false, author: "Zettroke", files_count: 2, insertions_count: 9, deletions_count: 6},
		{sha: "584ed34d33627ec93a91e04ed5ddda91f5db8f19", date: "2024-07-31 11:25:50 UTC", description: "Bump docker/setup-buildx-action from 3.5.0 to 3.6.1", pr_number: 20960, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "8bf5d494f31daa1456a54d613c5dc9f5d5bd8924", date: "2024-07-31 11:26:57 UTC", description: "Bump serde_json from 1.0.120 to 1.0.121", pr_number: 20957, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "742c6121bd431006006d55606fbffd64933bb5ea", date: "2024-07-31 11:27:06 UTC", description: "Bump lapin from 2.4.0 to 2.5.0", pr_number: 20956, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f56a1e086121a445e56a2514e971fa7f8a176639", date: "2024-07-31 11:28:10 UTC", description: "Bump tokio from 1.39.1 to 1.39.2", pr_number: 20953, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 15, deletions_count: 15},
		{sha: "789848fdb9137a9d291f4254de62a1cb58518e0f", date: "2024-08-01 07:15:57 UTC", description: "add new `static_metrics` source", pr_number: 20889, scopes: ["source"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 6, insertions_count: 587, deletions_count: 0},
		{sha: "70e61fc344b9db2f191cfba9ad58ee08e79d30b4", date: "2024-08-01 09:36:47 UTC", description: "reuse `BytesReceived` event for internal metrics", pr_number: 20977, scopes: ["internal_metrics"], type: "enhancement", breaking_change: false, author: "Ensar Sarajčić", files_count: 4, insertions_count: 7, deletions_count: 29},
		{sha: "210ff0925d391213556f07bf6ce621967f0368ca", date: "2024-08-03 17:09:57 UTC", description: "Emphasize the $$ rather than $ in config file.", pr_number: 20991, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Leo", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f1a1e1c93c12857644c85f89a77bae3eab1f8a43", date: "2024-08-06 21:28:44 UTC", description: "Fix review comment trigger context", pr_number: 21010, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 11, insertions_count: 36, deletions_count: 36},
		{sha: "2fbb072155008b54cb064ab62d9ebc9783b30479", date: "2024-08-06 22:51:58 UTC", description: "Replace usages of `docker-compose` with `docker compose`", pr_number: 21009, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 5, deletions_count: 13},
		{sha: "8a77d7cea16db8cf527fe992cc1495da3ef7c952", date: "2024-08-07 15:59:24 UTC", description: "Fix the wrong log-namespace key.", pr_number: 21006, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Leo", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "aab836b502d3cc23594655d0d72224768122905a", date: "2024-08-07 11:34:09 UTC", description: "add missing assert in codec decoding tests", pr_number: 20998, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Jorge Hermo", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "abc21331e59bf0492c21466c0e50af15af5fd1d9", date: "2024-08-09 04:05:49 UTC", description: "Bump docker/build-push-action from 6.5.0 to 6.6.1", pr_number: 21030, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1111d0c49fb050d759d2a3bbd9ffc62ee67eef2a", date: "2024-08-09 04:05:59 UTC", description: "Bump serde from 1.0.204 to 1.0.205", pr_number: 21022, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "2a0c51fbcfb951d7ba4b4b82d01bf2c41b70b898", date: "2024-08-09 11:06:08 UTC", description: "Bump bufbuild/buf-setup-action from 1.35.1 to 1.36.0", pr_number: 21019, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "87e6636f404dc3d001a3fe90638fc2d4ad909224", date: "2024-08-09 11:06:19 UTC", description: "Bump tempfile from 3.10.1 to 3.12.0", pr_number: 21016, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 50, deletions_count: 33},
		{sha: "aa248cb268a66aef0ffd5917ce1d07d6918bc967", date: "2024-08-09 11:06:48 UTC", description: "Bump ndarray from 0.15.6 to 0.16.0", pr_number: 21002, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 27, deletions_count: 3},
		{sha: "16d2300bd8def42248e49e42643c8c1a604835c8", date: "2024-08-09 11:07:22 UTC", description: "Bump dunce from 1.0.4 to 1.0.5", pr_number: 21001, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "acd34aaa9400b367ac8dc1769688b433d045e374", date: "2024-08-09 11:10:20 UTC", description: "Bump typetag from 0.2.16 to 0.2.17", pr_number: 20954, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "c621470b0315ac238aec3a8e4a2692ee9e15ecba", date: "2024-08-09 11:10:26 UTC", description: "Bump nkeys from 0.4.2 to 0.4.3", pr_number: 20938, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "0ed8caa16f0f235c47365f127ae1219c3ffb3bbf", date: "2024-08-09 12:12:12 UTC", description: "Bump databend-client from 0.19.5 to 0.20.0", pr_number: 20981, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "270bdc5a715a25e3a7e687a4c50e698e2baac367", date: "2024-08-10 06:20:48 UTC", description: "Fix example syntax", pr_number: 20783, scopes: ["remap transform"], type: "docs", breaking_change: false, author: "Sondre Lillebø Gundersen", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "93e423feeea60cfbabe9af692d4afab221eac788", date: "2024-08-09 21:22:04 UTC", description: "Avoid parsing configuration files without interpolating secrets", pr_number: 20985, scopes: ["config"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 17, insertions_count: 91, deletions_count: 90},
		{sha: "559e069caffaf7826d8e2feea4647b9c944128be", date: "2024-08-10 13:15:00 UTC", description: "Make config watcher recursive.", pr_number: 20996, scopes: ["config"], type: "fix", breaking_change: false, author: "Leo", files_count: 2, insertions_count: 23, deletions_count: 1},
		{sha: "e99ed52449bae79e095d1857d7a06e18a5446fc9", date: "2024-08-10 09:38:14 UTC", description: "Bump assert_cmd from 2.0.15 to 2.0.16", pr_number: 21034, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "34b502d2279d571255482d104edef26cab5758b6", date: "2024-08-10 09:38:39 UTC", description: "Bump the clap group across 1 directory with 2 updates", pr_number: 21032, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "1beda059ce8b55cc37fdc6d2370ba4949d20e205", date: "2024-08-10 09:40:07 UTC", description: "Bump rstest from 0.21.0 to 0.22.0", pr_number: 21004, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "9ad96a358ce65fe2ace7d91a6ae3d05ea247d5a0", date: "2024-08-10 09:40:14 UTC", description: "Bump regex from 1.10.5 to 1.10.6", pr_number: 21003, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "0ae182c8221c82a316a51dfb455f68fb7f7fd3c0", date: "2024-08-10 09:40:18 UTC", description: "Bump flate2 from 1.0.30 to 1.0.31", pr_number: 21000, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e78af3b7767ba010cdcec988ea7e3c7da3ebed32", date: "2024-08-10 09:40:24 UTC", description: "Bump bytes from 1.6.1 to 1.7.1", pr_number: 20987, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 89, deletions_count: 89},
		{sha: "65c7287d1886b8cfdd13bd1b736b5c2c8d2ff8ae", date: "2024-08-10 09:41:06 UTC", description: "Bump lru from 0.12.3 to 0.12.4", pr_number: 20975, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "264024798b0deca2bdc9dca07fc760c708ace940", date: "2024-08-10 09:41:10 UTC", description: "Bump num_enum from 0.7.2 to 0.7.3", pr_number: 20963, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "a358576f968e4b363f52a54aed0e43d6b0b744f4", date: "2024-08-10 09:41:14 UTC", description: "Bump ordered-float from 4.2.1 to 4.2.2", pr_number: 20962, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "7be4be622fb1e4308266d90aa1c244be636ffb8a", date: "2024-08-10 18:03:34 UTC", description: "Update ChangLog doc", pr_number: 21029, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Leo", files_count: 1, insertions_count: 4, deletions_count: 3},
		{sha: "b9b98ff20ba61d5f20f5a90ed3c2e99830298655", date: "2024-08-13 09:37:44 UTC", description: "use nanosecond-precision timestamps in `influxdb` decoder", pr_number: 21042, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jorge Hermo", files_count: 2, insertions_count: 11, deletions_count: 7},
		{sha: "ee0168a65725da544508e890ff2c6ab014f2957a", date: "2024-08-13 09:53:54 UTC", description: "allow usage of metrics-only decoders in log sources", pr_number: 21040, scopes: ["config"], type: "fix", breaking_change: false, author: "Jorge Hermo", files_count: 39, insertions_count: 107, deletions_count: 59},
		{sha: "9c6275d0eda069d34a45a8a372b96d89c4f3c35e", date: "2024-08-13 07:58:38 UTC", description: "Bump serde from 1.0.205 to 1.0.206", pr_number: 21047, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "601fdf98e71fa1556926f2b8bbbc4ddfd8936080", date: "2024-08-13 07:59:09 UTC", description: "Bump databend-client from 0.20.0 to 0.20.1", pr_number: 21049, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "94fd34fefe21a0522f33e149504a96eea66b5874", date: "2024-08-13 07:59:19 UTC", description: "Bump typetag from 0.2.17 to 0.2.18", pr_number: 21050, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "b859bf4ee86c8cd55d08efa43889339308e90f55", date: "2024-08-13 07:59:29 UTC", description: "Bump syn from 2.0.72 to 2.0.74", pr_number: 21051, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
		{sha: "e595dccd98d31af403b69e5268d5421b5d541740", date: "2024-08-13 08:00:03 UTC", description: "Bump clap_complete from 4.5.13 to 4.5.14 in the clap group", pr_number: 21046, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 11},
		{sha: "645804a72438197c8e8ccd02ad29e4024f9a59ce", date: "2024-08-13 08:00:07 UTC", description: "Bump aws-smithy-runtime-api from 1.7.1 to 1.7.2 in the aws group", pr_number: 21033, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b24e9efa12cf8550d96bf37a86c2d5ae69abf805", date: "2024-08-13 08:00:29 UTC", description: "Bump async-compression from 0.4.11 to 0.4.12", pr_number: 20897, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 14, deletions_count: 14},
		{sha: "38fdd46f434e6688665bb436a2d02d195ac0d280", date: "2024-08-13 07:11:06 UTC", description: "surface send error", pr_number: 21056, scopes: ["tap"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "dc0b4087095b4968cca0201e233919de8cff9918", date: "2024-08-13 12:27:14 UTC", description: "update service to set Errored status on events", pr_number: 21036, scopes: ["kafka sink"], type: "enhancement", breaking_change: false, author: "Frank Hamand", files_count: 2, insertions_count: 32, deletions_count: 3},
		{sha: "fb9e6d26e743ae73cf43aeff5a53f375aca9989d", date: "2024-08-14 03:18:36 UTC", description: "Update fork of tokio-util to 0.7.11 (latest)", pr_number: 21066, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 4},
		{sha: "7f206cdc507d775154f7d6c1ae96a9a6fd0a5650", date: "2024-08-14 03:32:06 UTC", description: "allows fetching secrets from AWS secrets manager with sso profiles", pr_number: 21038, scopes: ["config"], type: "fix", breaking_change: false, author: "britton", files_count: 4, insertions_count: 55, deletions_count: 1},
		{sha: "61b1b187169739870d94b6e709a901398f865aac", date: "2024-08-14 22:34:02 UTC", description: "use correct series payload origin labels", pr_number: 21068, scopes: ["metrics"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 9, deletions_count: 9},
		{sha: "e601b9b636df9a8e1214f4c8b02559d31d979fc0", date: "2024-08-15 04:17:48 UTC", description: "Refactor secrets loading to avoid use of futures::executor::block_on", pr_number: 21073, scopes: ["config"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 7, insertions_count: 44, deletions_count: 44},
		{sha: "ac4e1944fc29bf7d3ab25af029b8fdfdba0dc910", date: "2024-08-15 22:14:28 UTC", description: "Regenerate Cargo.lock", pr_number: 21083, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "af57155653520b6248a07164858a30897ed868a9", date: "2024-08-17 04:29:57 UTC", description: "Bump libc from 0.2.155 to 0.2.156", pr_number: 21093, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ae3696f8ec4677970b2e0e023a56a8a28df44c27", date: "2024-08-17 04:30:11 UTC", description: "Bump serde_json from 1.0.121 to 1.0.125", pr_number: 21092, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "41901f626baa3575b884b3cfa48ebb58c2601369", date: "2024-08-17 11:30:21 UTC", description: "Bump the aws group across 1 directory with 2 updates", pr_number: 21091, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "4884c295e5ea6478529e9b4d149356f28a79bbc2", date: "2024-08-17 11:33:11 UTC", description: "Bump docker/build-push-action from 6.6.1 to 6.7.0", pr_number: 21064, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c28e947b7781cde74a54658e9ae079020ecc2e30", date: "2024-08-17 11:33:23 UTC", description: "Bump wasm-bindgen from 0.2.92 to 0.2.93", pr_number: 21060, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 11, deletions_count: 10},
		{sha: "84666a4a41302e665651db68c7d2ce866b96d78c", date: "2024-08-17 11:33:35 UTC", description: "Bump toml from 0.8.16 to 0.8.19", pr_number: 20982, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 11},
		{sha: "f371bc2ce5a806a4dc9fa8ba573594dabb2a60ea", date: "2024-08-17 05:02:22 UTC", description: "Group dependabot tower updates", pr_number: 21096, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "35e82bd9d9983097e5af75a98300b74b4fc433e9", date: "2024-08-20 12:23:14 UTC", description: "add greptime log sink", pr_number: 20812, scopes: ["new sink"], type: "feat", breaking_change: false, author: "localhost", files_count: 29, insertions_count: 2236, deletions_count: 411},
		{sha: "eb2d786617ad8afdf799323fe3f0ae7cddd25936", date: "2024-08-20 05:59:40 UTC", description: "redesign vrl playground and make responsive", pr_number: 21078, scopes: ["playground"], type: "feat", breaking_change: false, author: "britton", files_count: 4, insertions_count: 386, deletions_count: 286},
		{sha: "03b030eb1cb2af277c49ceed0f8e809b0bbf1543", date: "2024-08-20 07:05:22 UTC", description: "Regenerate certificates used for nats integration tests", pr_number: 21113, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 130, deletions_count: 124},
		{sha: "73f5b97cf77d8b32a7b44a6dff59b8ab1db95bfd", date: "2024-08-20 07:06:06 UTC", description: "Bump libc from 0.2.156 to 0.2.157", pr_number: 21110, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1549cf04e3b560c89885332b6e6fbfd87386eca4", date: "2024-08-20 14:06:28 UTC", description: "Bump syn from 2.0.74 to 2.0.75", pr_number: 21108, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 50, deletions_count: 50},
		{sha: "4909c522edc90a782580a0d302ec437a507380ba", date: "2024-08-20 14:07:59 UTC", description: "Bump the clap group across 1 directory with 2 updates", pr_number: 21101, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "970b2225c45b72d4a5dacb918eca533f0fffbe70", date: "2024-08-20 14:08:09 UTC", description: "Bump bufbuild/buf-setup-action from 1.36.0 to 1.37.0", pr_number: 21100, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c86bdcc40ce7fadd92b2eb59db6c4f65788b315f", date: "2024-08-20 14:08:33 UTC", description: "Bump ndarray from 0.16.0 to 0.16.1", pr_number: 21080, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "e95f098f3e10af6b40038d100c1601252ef156c3", date: "2024-08-21 13:12:41 UTC", description: "exclude pattern with multi slashes can not match some files", pr_number: 21082, scopes: ["file sources"], type: "fix", breaking_change: false, author: "Suika", files_count: 2, insertions_count: 55, deletions_count: 1},
		{sha: "0f396ac784999deb7ae18bd844447bf8ee4b7a53", date: "2024-08-21 09:33:35 UTC", description: "Bump libc from 0.2.157 to 0.2.158", pr_number: 21118, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "286200632ed2acf451d662dda84e290e30416530", date: "2024-08-21 09:33:46 UTC", description: "Bump rkyv from 0.7.44 to 0.7.45", pr_number: 21117, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "b262eec6c5038e6a7c4cb817fa5dd6296c2970bd", date: "2024-08-21 09:33:57 UTC", description: "Bump clap_complete from 4.5.18 to 4.5.19 in the clap group", pr_number: 21116, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "bf9375816b4a0e6c694caf49df44432dd8313c5f", date: "2024-08-21 09:34:08 UTC", description: "Bump tokio from 1.39.2 to 1.39.3", pr_number: 21109, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 15, deletions_count: 15},
		{sha: "76a525b17368a3b656ff7d3c76ef8feb28fd4908", date: "2024-08-21 03:19:57 UTC", description: "Correctly render character delimiter as a char in the docs", pr_number: 21124, scopes: ["docs", "codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 30, insertions_count: 34, deletions_count: 28},
		{sha: "97558d3d59aecb6bc71c82a18953c3537233c934", date: "2024-08-21 06:48:06 UTC", description: "Allow OIDC usage for Kafka", pr_number: 21103, scopes: ["kafka sink"], type: "enhancement", breaking_change: false, author: "zapdos26", files_count: 5, insertions_count: 23, deletions_count: 2},
		{sha: "7774c5f8cd110b2a4bca2b4ced8b11e598053d6a", date: "2024-08-22 05:19:41 UTC", description: "Bump clap_complete from 4.5.19 to 4.5.20 in the clap group", pr_number: 21125, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "143e01714c2e1011fa00f12b014ecd37c3dd4050", date: "2024-08-22 05:19:51 UTC", description: "Bump h2 from 0.4.5 to 0.4.6", pr_number: 21119, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "82fd2a55a68a97eb3a9043ee9273bca6faf61d81", date: "2024-08-22 13:04:40 UTC", description: "Bump flate2 from 1.0.31 to 1.0.32", pr_number: 21126, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 21, deletions_count: 5},
		{sha: "66b55fe74b1ca231368dfe5aeea29a6cc41bc9dd", date: "2024-08-23 15:20:51 UTC", description: "Use rdkafka::client::Client instead of Consumer", pr_number: 21129, scopes: ["kafka sink"], type: "fix", breaking_change: false, author: "Zhongqiu Zhao", files_count: 4, insertions_count: 75, deletions_count: 87},
		{sha: "08a1a4cf5c9388dae3bd909c18ab0d859a8b7a37", date: "2024-08-23 01:09:29 UTC", description: "Bump regression workflow timeout to 60 minutes", pr_number: 21136, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b5c137bc5e0acb773ab453789be0494db51e6344", date: "2024-08-23 08:17:12 UTC", description: "Bump bufbuild/buf-setup-action from 1.37.0 to 1.38.0", pr_number: 21135, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "fe2cc26a217364d5dd3f8c00289ce45af2446f24", date: "2024-08-23 08:17:22 UTC", description: "Bump clap_complete from 4.5.20 to 4.5.22 in the clap group", pr_number: 21130, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "46fccce76c8d579611d5bb4c148d64f27919e550", date: "2024-08-26 21:20:33 UTC", description: "Swap out mockwatchlogs for localstack", pr_number: 21114, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 30, deletions_count: 33},
		{sha: "968b5df014a027dd1a4cb366e3db383c1c501c6c", date: "2024-08-26 23:22:23 UTC", description: "Document that `to_int` coerces `null`s", pr_number: 21154, scopes: ["docs", "vrl stdlib"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "d76330b8efed0ad9278f48f0430ba644c04dbd73", date: "2024-08-27 23:12:10 UTC", description: "endpoint is now configurable", pr_number: 21147, scopes: ["honeycomb sink"], type: "feat", breaking_change: false, author: "Ray", files_count: 3, insertions_count: 25, deletions_count: 4},
		{sha: "f87fc24870dff3566c8b2db998bd18ff6fc17db8", date: "2024-08-27 13:19:23 UTC", description: "Bump serde from 1.0.206 to 1.0.209", pr_number: 21149, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "f6d8f72ba5f65984d74f7c462e19b9cf20e3019a", date: "2024-08-27 13:20:06 UTC", description: "Bump quote from 1.0.36 to 1.0.37", pr_number: 21139, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 71, deletions_count: 71},
		{sha: "c3c0ec0cf34f4d5defa19458a76e4e69678ee2a2", date: "2024-08-27 13:20:16 UTC", description: "Bump clap_complete from 4.5.22 to 4.5.23 in the clap group", pr_number: 21138, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "cade0d2e87d9b7c6da13dcc3934b84299af8232d", date: "2024-08-28 13:19:46 UTC", description: "fix crash when handling metrics", pr_number: 21141, scopes: ["amqp source"], type: "fix", breaking_change: false, author: "Zang MingJie", files_count: 2, insertions_count: 12, deletions_count: 9},
		{sha: "e3d0ebfca572f7d9b70f1944db593d5c735ef47e", date: "2024-08-28 08:46:14 UTC", description: "Bump micromatch from 4.0.4 to 4.0.8 in /website", pr_number: 21156, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 7, deletions_count: 7},
		{sha: "e90cecef2598ae267ed6f645356da6f8a494a827", date: "2024-08-28 04:30:32 UTC", description: "Bump the tonic group with 2 updates", pr_number: 19837, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 19, deletions_count: 64},
		{sha: "c3cd2325e180907c1c9e1e8547547e0e9b240df8", date: "2024-08-29 12:41:33 UTC", description: "Remove the watcher's coupling to SIGHUP / prepare for automatic Windows config reload", pr_number: 20989, scopes: ["config"], type: "enhancement", breaking_change: false, author: "Benjamin Podszun", files_count: 3, insertions_count: 45, deletions_count: 50},
		{sha: "012a18df708a299369002f56f1b7510c9c5c4c9f", date: "2024-08-28 22:30:22 UTC", description: "removes conflicting overflow settings creating multiple scrollbars", pr_number: 21168, scopes: ["playground"], type: "fix", breaking_change: false, author: "britton", files_count: 2, insertions_count: 3, deletions_count: 15},
		{sha: "5060aa5c223643d5198825896f62109353eb3e2f", date: "2024-08-29 02:24:11 UTC", description: "Bump VRL to the latest ref", pr_number: 21171, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 189, deletions_count: 173},
		{sha: "1cc17611c0a71f4d6afbfe72b4da835ae91874cd", date: "2024-08-29 08:28:09 UTC", description: "Configure `datadog_search` condition directly", pr_number: 21174, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 25, deletions_count: 12},
		{sha: "c2dc1aa81f55588c44b6b6f0bcb0bbcb34d0fe06", date: "2024-08-29 22:42:13 UTC", description: "Hande numeric equality in `datadog_search` condition ", pr_number: 21179, scopes: ["transforms"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 155, deletions_count: 8},
		{sha: "360780dcaa1d79e115048864917bf30512012b9b", date: "2024-08-30 00:32:54 UTC", description: "Add conversion helpers for `DatadogSearchConfig`", pr_number: 21181, scopes: ["transforms"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 16, deletions_count: 9},
		{sha: "c132f35336aab3922e8595bfc910d103a074ac61", date: "2024-08-29 23:55:56 UTC", description: "Fix review workflow trigger", pr_number: 21182, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 11, deletions_count: 13},
		{sha: "3bf93ef1f19a5a5afcc1679a002e8b0f46cdf67e", date: "2024-08-30 02:44:04 UTC", description: "Fix fetching PR number for regression workflow", pr_number: 21183, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "78c470e490f1c0d544e02093c832858bc724d307", date: "2024-08-30 04:07:25 UTC", description: "add codec test data dir to regression workflow ignore filter", pr_number: 21185, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "ebc53379fb443291e6ca56c6ec72f9262110d73c", date: "2024-08-30 05:05:51 UTC", description: "Fix finding of merge-base for regression workflow", pr_number: 21186, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "06ad674ff4fbf2df82c50b787c45316c5b5c8d50", date: "2024-08-30 08:53:10 UTC", description: "Add internal event id to event metadata", pr_number: 21074, scopes: ["events"], type: "feat", breaking_change: false, author: "ArunPiduguDD", files_count: 4099, insertions_count: 2574, deletions_count: 1047},
		{sha: "d174d55fadaaee8e8a55223f43a6000e2814382d", date: "2024-09-04 12:32:57 UTC", description: "add support for structured metadata", pr_number: 20576, scopes: ["loki sink"], type: "feat", breaking_change: false, author: "Max Boone", files_count: 12, insertions_count: 917, deletions_count: 253},
		{sha: "f2155f173ef33996cec0d6a05fab7776a2ab543d", date: "2024-09-04 09:20:32 UTC", description: "surround invalid path segments with quotes", pr_number: 21201, scopes: ["reduce transform"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 48, deletions_count: 107},
		{sha: "0cb5cf41d689d675d47568a004eb9813d7f0ac22", date: "2024-09-04 21:02:08 UTC", description: "Bump openssl-src from 300.3.1+3.3.1 to 300.3.2+3.3.2", pr_number: 21205, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "bac72a8821a17a0688a75bbbb556b122fb4d6b79", date: "2024-09-04 21:53:50 UTC", description: "Bump serde_json from 1.0.125 to 1.0.127", pr_number: 21150, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ff2e505e2bcb46c6561d8e1d95c30a7b6932d70c", date: "2024-09-05 04:53:55 UTC", description: "Bump flate2 from 1.0.32 to 1.0.33", pr_number: 21151, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1d896daa18c2e87c6f92a147f5bb74956a64dc05", date: "2024-09-05 04:54:05 UTC", description: "Bump clap_complete from 4.5.23 to 4.5.24 in the clap group", pr_number: 21170, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "832c02eb124c8e003d08de609177e9fde806f0fd", date: "2024-09-05 04:54:15 UTC", description: "Bump bufbuild/buf-setup-action from 1.38.0 to 1.39.0", pr_number: 21172, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0cd16eb7e52ac73166af771ea746ef1fe599aa08", date: "2024-09-05 04:54:29 UTC", description: "Bump ndarray-stats from 0.5.1 to 0.6.0", pr_number: 21177, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 20},
		{sha: "6128d76e4b341c4ea8866cdbbfe89fa7b6c5acda", date: "2024-09-05 04:54:46 UTC", description: "Bump the aws group across 1 directory with 4 updates", pr_number: 21187, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 11},
		{sha: "17fba1a980e233997654a38b8f9936e9fde0bba1", date: "2024-09-05 04:55:02 UTC", description: "Bump tokio from 1.39.3 to 1.40.0", pr_number: 21189, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 15, deletions_count: 15},
		{sha: "42ca811ac6e06b5536d0ddd0a548e74d906b515a", date: "2024-09-05 04:55:11 UTC", description: "Bump indexmap from 2.4.0 to 2.5.0", pr_number: 21190, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 26, deletions_count: 26},
		{sha: "944217c137ff14724850d73fcf08b0e3838d4b0e", date: "2024-09-05 04:55:44 UTC", description: "Bump async-trait from 0.1.81 to 0.1.82", pr_number: 21197, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "beceef4a964a77acc3064c064eb27d265261c17d", date: "2024-09-04 21:56:27 UTC", description: "Regenerate k8s manifsts for chart 0.35.0", pr_number: 21199, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "695bb476d6116ba65d57a77aa0ac655af76f063a", date: "2024-09-05 15:11:00 UTC", description: "Refactor internal logic for vector tap into lib", pr_number: 21200, scopes: ["tap"], type: "chore", breaking_change: false, author: "ArunPiduguDD", files_count: 34, insertions_count: 225, deletions_count: 193},
		{sha: "cf8f94b7d4b2d126650210f8bab62a6dba242c9a", date: "2024-09-06 00:53:23 UTC", description: "Bump VRL to 0.18.0", pr_number: 21214, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "75609314dd61b6923550411f60635f443a854f98", date: "2024-09-06 06:17:06 UTC", description: "add casing functions (https://github.com/vectordotdev/vrl/…", pr_number: 21021, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Evan Cameron", files_count: 6, insertions_count: 218, deletions_count: 0},
		{sha: "b546f12aa5c20559f48ceb5fa13d1c1002902618", date: "2024-09-06 04:24:27 UTC", description: "usage of `a deprecated Node.js version`", pr_number: 21210, scopes: ["ci"], type: "fix", breaking_change: false, author: "Hamir Mahal", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7656ce071ea691ad0cc556ce1a161ef2f8c2f2b3", date: "2024-09-06 13:47:25 UTC", description: "add documentation for the new `lossy` option of the `parse_json` vrl function", pr_number: 21076, scopes: [], type: "docs", breaking_change: false, author: "Jorge Hermo", files_count: 1, insertions_count: 11, deletions_count: 0},
		{sha: "c23653ff1650c15892147feb4ae4986903c2d77b", date: "2024-09-06 13:50:58 UTC", description: "Add `unflatten` vrl function documentation", pr_number: 21142, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jorge Hermo", files_count: 3, insertions_count: 113, deletions_count: 1},
		{sha: "fbcda67644757f4d63ce829c435af2031b2911a8", date: "2024-09-06 14:27:12 UTC", description: "add docs for the `parse_influxdb` vrl function", pr_number: 21105, scopes: [], type: "docs", breaking_change: false, author: "Jorge Hermo", files_count: 1, insertions_count: 109, deletions_count: 0},
		{sha: "9693f5253c2ce306de3cfa40d0c52439df98fc71", date: "2024-09-06 09:02:57 UTC", description: "Update version of cue to latest (0.10.0)", pr_number: 21217, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "00ef42baf0a22fbbff2d31b514e0012501356bc8", date: "2024-09-07 01:57:42 UTC", description: "Bump heim from `a66c440` to `4925b53`", pr_number: 21220, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 8, deletions_count: 8},
	]
}
