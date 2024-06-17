package metadata

releases: "0.36.0": {
	date:     "2024-02-13"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.36.0!

		There are no breaking changes in this release.

		In addition to the usual enhancements and bug fixes, this release also includes

		- A new `prometheus_pushgateway` source to receive Prometheus data
		- A new `vrl` decoder that can be used to decode data in sources using a VRL program

		A reminder that the `repositories.timber.io` package repositories will be decommissioned on
		February 28th, 2024. Please see the [release
		highlight](/highlights/2023-11-07-new-linux-repos) for details about this change and
		instructions on how to migrate.
		"""

	known_issues: [
		"""
			AWS components don't support use of `credentials_process` in AWS configs. Fixed in v0.36.1.
			""",
		"""
			AWS components don't support auto-detection of region . Fixed in v0.36.1.
			""",
		"""
			AWS components don't support use of `assume_role`. Fixed in v0.36.1.
			""",
		"""
			The `kafka` sink occasionally panics during rebalance events. Fixed in v0.36.1.
			""",
	]

	changelog: [
		{
			type: "feat"
			description: """
				Vector can now emulate a [Prometheus Pushgateway](https://github.com/prometheus/pushgateway) through the new `prometheus_pushgateway` source. Counters and histograms can optionally be aggregated across pushes to support use-cases like cron jobs.

				There are some caveats, which are listed [in the implementation](https://github.com/vectordotdev/vector/tree/v0.36/src/sources/prometheus/pushgateway.rs#L8-L12).

				"""
			contributors: ["Sinjo"]
		},
		{
			type: "feat"
			description: """
				The `clickhouse` sink now supports `format`. This can be used to specify the data [format](https://clickhouse.com/docs/en/interfaces/formats) provided to `INSERT`s. The default is `JSONEachRow`.

				"""
			contributors: ["gabriel376"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue where the `aws_s3` sink adds a trailing period to the s3 key when the `filename_extension` is empty.
				"""
		},
		{
			type: "fix"
			description: """
				Removed warnings for unused outputs in `datadog_agent` source when the corresponding output is disabled in the source config.
				"""
		},
		{
			type: "enhancement"
			description: """
				Unit tests can now populate event metadata with the `% = ...` syntax.

				"""
			contributors: ["GreyTeardrop"]
		},
		{
			type: "fix"
			description: """
				When terminating idle HTTP connections using the configured `max_connection_age`, only send
				`Connection: Close` for HTTP/0.9, HTTP/1.0, and HTTP/1.1 requests. This header is not supported on
				HTTP/2 and HTTP/3 requests. This may be supported on these HTTP versions in the future.
				"""
		},
		{
			type: "feat"
			description: """
				Added a configuration option for the `aws_s3` source that prevents deletion of messages which failed to be delivered to a sink.

				"""
			contributors: ["tanushri-sundar"]
		},
		{
			type: "fix"
			description: """
				The following metrics now correctly have the `component_kind`, `component_type`, and `component_id` tags:
				    - `component_errors_total`
				    - `component_discarded_events_total`

				For the following sinks:
				    - `splunk_hec`
				    - `clickhouse`
				    - `loki`
				    - `redis`
				    - `azure_blob`
				    - `azure_monitor_logs`
				    - `webhdfs`
				    - `appsignal`
				    - `amqp`
				    - `aws_kinesis`
				    - `statsd`
				    - `honeycomb`
				    - `gcp_stackdriver_metrics`
				    - `gcs_chronicle_unstructured`
				    - `gcp_stackdriver_logs`
				    - `gcp_pubsub`
				    - `gcp_cloud_storage`
				    - `nats`
				    - `http`
				    - `kafka`
				    - `new_relic`
				    - `datadog_metrics`
				    - `datadog_traces`
				    - `datadog_events`
				    - `databend`
				    - `prometheus_remote_write`
				    - `pulsar`
				    - `aws_s3`
				    - `aws_sqs`
				    - `aws_sns`
				    - `elasticsearch`
				"""
		},
		{
			type: "enhancement"
			description: """
				Added support for parsing HTTPS (type 65) and SVCB (type 64) resource records from DNS messages
				"""
			contributors: ["esensar"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue where the `journald` source was not correctly emitting metadata when `log_namespace =
				True`.

				"""
			contributors: ["dalegaard"]
		},
		{
			type: "feat"
			description: """
				Implemented VRL decoder. This enables users to set `decoding.codec = "vrl"` in their
				source configurations and use VRL programs to decode logs.
				"""
		},
		{
			type: "enhancement"
			description: """
				The base for Vector's Alpine Docker images was updated from 3.18 to 3.19.
				"""
		},
		{
			type: "fix"
			description: """
				Fixed an issue where the `datadog_logs` sink could produce a request larger than the allowed API
				limit.
				"""
		},
		{
			type: "enhancement"
			description: """
				Gracefully accept `@` characters in labels when decoding GELF.
				"""
			contributors: ["MartinEmrich"]
		},
		{
			type: "enhancement"
			description: """
				Added a boolean `graphql` field to the api configuration to allow disabling the graphql endpoint.

				Note that the `playground` endpoint will now only be enabled if the `graphql` endpoint is also enabled.
				"""
		},
		{
			type: "enhancement"
			description: """
				New Option `--skip-healthchecks` for `vector validate` validates config
				including VRL, but skips health checks for sinks.

				Useful to validate configuration before deploying it remotely.
				"""
			contributors: ["MartinEmrich"]
		},
	]

	commits: [
		{sha: "d115e269dbbb06fe25977df74b10d5cd0fa04628", date: "2024-01-05 09:20:10 UTC", description: "Automated changelog generation", pr_number: 19429, scopes: ["releasing"], type: "chore", breaking_change: false, author: "neuronull", files_count: 9, insertions_count: 345, deletions_count: 7},
		{sha: "3525d062dd2387bdda8babc5a98f5a9997a0362a", date: "2024-01-06 08:35:59 UTC", description: "Fix link to RFC 3339", pr_number: 19509, scopes: ["docs"], type: "chore", breaking_change: false, author: "Benedikt Heine", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "81d22b30e20ba9a250b4d9a5d56aa4216fcd7ece", date: "2024-01-06 01:15:56 UTC", description: "fix changelog workflow extern contribs", pr_number: 19524, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "aa6fd40ae9fda3279cbfd4f4ec3bdbb7debde691", date: "2024-01-06 05:39:43 UTC", description: "improve retry behavior code quality", pr_number: 19450, scopes: ["datadog_metrics sink", "datadog_logs sink"], type: "enhancement", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 37, deletions_count: 39},
		{sha: "c2cc94a262ecf39798009d29751d59cc97baa0c5", date: "2024-01-09 09:16:21 UTC", description: "Update AWS crates", pr_number: 19312, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 54, insertions_count: 827, deletions_count: 1043},
		{sha: "9c832fd2f8677ddceb15e2e3a8e5a504b1b1cea3", date: "2024-01-09 02:18:50 UTC", description: "exclude dependabot from changelog job steps", pr_number: 19545, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "a3f033766dab2d41f00b68f19aa97eecb5f42728", date: "2024-01-09 05:42:06 UTC", description: "Bump serde from 1.0.194 to 1.0.195", pr_number: 19533, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "b2cc78869c7890ab00e586ab8b34f7ec5828da4a", date: "2024-01-09 09:57:09 UTC", description: "Bump Rust to 1.75.0", pr_number: 19518, scopes: ["deps"], type: "chore", breaking_change: false, author: "neuronull", files_count: 52, insertions_count: 245, deletions_count: 253},
		{sha: "3b120ff0c17ccedf07f423090f8c009bf7164410", date: "2024-01-10 06:12:54 UTC", description: "Add Prometheus Pushgateway source", pr_number: 18143, scopes: ["new source"], type: "feat", breaking_change: false, author: "Chris Sinjakli", files_count: 13, insertions_count: 1112, deletions_count: 46},
		{sha: "f914cf602e78685804efaf473a056bb87f612110", date: "2024-01-10 01:49:11 UTC", description: "Fix the check for external contributor author GH usernames", pr_number: 19568, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2b25a99a7347f40043434d1337a6b960338357c0", date: "2024-01-10 10:46:08 UTC", description: "Fix aws feature error", pr_number: 19567, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 9, deletions_count: 6},
		{sha: "05b07ab196b3891ca203dd64200fa5b064b7abb1", date: "2024-01-10 10:55:52 UTC", description: "only export RemoteWriteConfig for remote-write feature", pr_number: 19569, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "d6bd2696d138e3499deea7db9a9ac9432a96e687", date: "2024-01-10 09:26:13 UTC", description: "Add pure/impure badge for VRL functions", pr_number: 19571, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 22, deletions_count: 0},
		{sha: "1eda83b64c83e067c3577b9e63cc4bb28d064518", date: "2024-01-10 07:16:02 UTC", description: "Bump anyhow from 1.0.76 to 1.0.79", pr_number: 19500, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "f38796d3a8e341a9fc5fe5499a489af33c19a3b7", date: "2024-01-10 15:16:06 UTC", description: "Bump async-trait from 0.1.75 to 0.1.77", pr_number: 19498, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e08b187b5502b97cbbbd337c043e59227c2de291", date: "2024-01-10 15:17:13 UTC", description: "Bump serde_bytes from 0.11.12 to 0.11.14", pr_number: 19495, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f5bed3fd72f1239a41a82ac89b6ebb303318f5f9", date: "2024-01-10 15:20:12 UTC", description: "Bump semver from 1.0.20 to 1.0.21", pr_number: 19505, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a7a41661a4339c07034fb38c05ffdea4f5d3c4fc", date: "2024-01-10 15:20:53 UTC", description: "Bump serde_yaml from 0.9.29 to 0.9.30", pr_number: 19514, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "1d979cc6791f32b024459f5e76c503bf6947db76", date: "2024-01-10 15:22:07 UTC", description: "Bump syn from 2.0.46 to 2.0.48", pr_number: 19532, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 113, deletions_count: 113},
		{sha: "eec7eb5a9abfdc6f63cc1b8f4ed2c8364492622d", date: "2024-01-10 15:22:36 UTC", description: "Bump num_enum from 0.7.1 to 0.7.2", pr_number: 19536, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "47fcf91f8935df19d93b08a8420c79f67bdcfb68", date: "2024-01-10 15:23:12 UTC", description: "Bump opendal from 0.44.0 to 0.44.1", pr_number: 19538, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "61b2a3f365876b4a23115d38b7817eff450afa58", date: "2024-01-10 15:23:26 UTC", description: "Bump the clap group with 1 update", pr_number: 19552, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 20, deletions_count: 20},
		{sha: "c57435d9b142a34674e4260a9ef6ce7b044c6a4e", date: "2024-01-10 15:24:19 UTC", description: "Bump base64 from 0.21.5 to 0.21.6", pr_number: 19557, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 27, deletions_count: 27},
		{sha: "1daa0d38728665d1fd716be848544d2e2cf6579e", date: "2024-01-10 15:24:30 UTC", description: "Bump cargo_toml from 0.17.2 to 0.18.0", pr_number: 19558, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2be297649ba4e16d9b85802f2e0f69c71e2e310f", date: "2024-01-10 15:25:01 UTC", description: "Bump crossbeam-utils from 0.8.18 to 0.8.19", pr_number: 19560, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 8},
		{sha: "14ae52ed542514368495aa641e873a851c4bb2f4", date: "2024-01-10 07:45:59 UTC", description: "Group together crossbeam updates", pr_number: 19572, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "84de179739a45ba02878c1df0aee5cdee3b8082f", date: "2024-01-10 19:35:22 UTC", description: "Bump thiserror from 1.0.51 to 1.0.56", pr_number: 19510, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "79f0fd335e6ae92b3d3dab11e04b721536b6f0e8", date: "2024-01-10 22:27:42 UTC", description: "Bump libc from 0.2.151 to 0.2.152", pr_number: 19534, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8f504b35985b9cc1e29f1505b8fd42abd138851e", date: "2024-01-11 01:50:13 UTC", description: "Bump the aws group with 2 updates", pr_number: 19556, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 13},
		{sha: "586fb31a1678ca220cdeef7f37b091de41b6ce95", date: "2024-01-11 13:08:43 UTC", description: "update ingestion api for greptimedb sink", pr_number: 19410, scopes: ["greptimedb sink"], type: "feat", breaking_change: false, author: "Ning Sun", files_count: 6, insertions_count: 163, deletions_count: 150},
		{sha: "86b16e04a2f98701f13e7c814baf5cf837d0a82c", date: "2024-01-10 22:56:47 UTC", description: "Bump the crossbeam group with 1 update", pr_number: 19576, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 5},
		{sha: "e3f285c32e857b1b1a8de4504e9bdfebdf0e77ec", date: "2024-01-10 22:56:57 UTC", description: "Bump getrandom from 0.2.11 to 0.2.12", pr_number: 19575, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 15, deletions_count: 15},
		{sha: "8881cc4a811d2253699f025f2d20fa496e38fe32", date: "2024-01-11 06:57:11 UTC", description: "Bump maxminddb from 0.23.0 to 0.24.0", pr_number: 19574, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "27e49e7ee645da5f1bf33b49dc616a3c8592bc72", date: "2024-01-11 06:57:22 UTC", description: "Bump mlua from 0.9.2 to 0.9.3", pr_number: 19573, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "13a930afcfbe2f11a8eef9634a2229e3e8672b1f", date: "2024-01-11 07:06:06 UTC", description: "Bump serde_json from 1.0.109 to 1.0.111", pr_number: 19520, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "b8c268cf7e8853b41b50285f8959f87a99939f01", date: "2024-01-11 07:06:15 UTC", description: "Bump docker/metadata-action from 5.4.0 to 5.5.0", pr_number: 19526, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5fb8efcef24f231589e63e16b420f7f42dda7813", date: "2024-01-10 23:43:22 UTC", description: "Ensure PR runs of regression and k8s e2e tests don't cancel each other", pr_number: 19578, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 6, deletions_count: 4},
		{sha: "a412c3c013c2de24e6a1502ed1cfe19f4b511f81", date: "2024-01-10 23:44:38 UTC", description: "Bump manifests to v0.30.0 of the chart", pr_number: 19554, scopes: ["kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "d282d260ae1f950f25516498f80ee55512192866", date: "2024-01-10 23:49:42 UTC", description: "Fix proofreading mistake in v0.35.0 upgrade guide", pr_number: 19551, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "dd76ca8815679d1e791b3b16400639fd815168fd", date: "2024-01-11 02:26:18 UTC", description: "Bump graphql crates to 7.0.0", pr_number: 19579, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 107, deletions_count: 14},
		{sha: "9f7c92d8d4b605f14f9d65ee9f9e34dcedf297d8", date: "2024-01-11 04:32:13 UTC", description: "abort serialization and split batch when payload is too large", pr_number: 19189, scopes: ["datadog_logs sink"], type: "fix", breaking_change: false, author: "Luke Steensen", files_count: 3, insertions_count: 205, deletions_count: 137},
		{sha: "df0eafce599b8c58053c0f2d68b479507824fc0b", date: "2024-01-11 08:40:53 UTC", description: "Skip serializing default proxy config fields", pr_number: 19580, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 6, deletions_count: 7},
		{sha: "bbff1b2e325df0ce706b244e73126580acd1f846", date: "2024-01-11 14:40:53 UTC", description: "Bump cached from 0.46.1 to 0.47.0", pr_number: 19503, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "11f50370254b85d6ca79d8874b32a55458fa2b7c", date: "2024-01-11 14:41:08 UTC", description: "Bump h2 from 0.4.0 to 0.4.1", pr_number: 19559, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "f4ad8bf0978b4524305dbfddf77609cdedf8e92a", date: "2024-01-11 08:11:31 UTC", description: "enable running all int tests comment", pr_number: 19581, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 116, deletions_count: 43},
		{sha: "2e756a16dc4aaf2faca2a293cc4f99ea3ef59617", date: "2024-01-11 15:24:06 UTC", description: "Bump the aws group with 4 updates", pr_number: 19582, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 23, deletions_count: 24},
		{sha: "2448a72770444e4c203d7d937e1ccede22c23aed", date: "2024-01-11 22:42:24 UTC", description: "Bump the aws group with 1 update", pr_number: 19586, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "9dd9907b356996d9bbb395fd4aea2a207c930914", date: "2024-01-12 03:19:50 UTC", description: "Shorten name of `skip_serializing_if_default`", pr_number: 19591, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 70, insertions_count: 107, deletions_count: 210},
		{sha: "b1502ec185a517f2c95078f5a70acae7baaf1c30", date: "2024-01-12 11:22:19 UTC", description: "Allow @ as valid GELF field character in decoder", pr_number: 19544, scopes: ["codec"], type: "enhancement", breaking_change: false, author: "Martin Emrich", files_count: 3, insertions_count: 12, deletions_count: 4},
		{sha: "1e1f2ecdf96ec104234756efb5a47167a85bc25e", date: "2024-01-12 22:33:03 UTC", description: "Bump the aws group with 1 update", pr_number: 19605, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "59699f6cf7e4f96d2d7b3d633eb8082d85110695", date: "2024-01-12 22:33:13 UTC", description: "Bump the clap group with 1 update", pr_number: 19606, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 22, deletions_count: 22},
		{sha: "6fde1861fe8961b1c100c951e0752b48673fac12", date: "2024-01-13 06:33:26 UTC", description: "Bump mlua from 0.9.3 to 0.9.4", pr_number: 19607, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "37125b9af3c8dfaa6924a8f5e59cc2a37f58923a", date: "2024-01-13 06:33:48 UTC", description: "Bump confy from 0.5.1 to 0.6.0", pr_number: 19608, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 42},
		{sha: "af6169c99e2bf236b958d775bd8af868c9dac094", date: "2024-01-13 06:34:02 UTC", description: "Bump assert_cmd from 2.0.12 to 2.0.13", pr_number: 19610, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c2f32593776f1e9304dc20ae2adbfb3efb8a8eb8", date: "2024-01-13 06:34:13 UTC", description: "Bump base64 from 0.21.6 to 0.21.7", pr_number: 19611, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 26, deletions_count: 26},
		{sha: "1705dfd5d85b08be96a594dfbf9081ed78497ee1", date: "2024-01-13 01:43:47 UTC", description: "Fix handling of the default value for `ProxyConfig::enabled`", pr_number: 19604, scopes: ["config"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 41, deletions_count: 1},
		{sha: "e1d570d99621f5b9c58423bdc1e5e8cee8ca9c0f", date: "2024-01-13 02:37:33 UTC", description: "Bump Vector to v0.36.0", pr_number: 19550, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "f262324595883633a21ead16c5fc165a576c9f17", date: "2024-01-13 06:20:23 UTC", description: "improve source data_dir docs", pr_number: 19596, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 32, deletions_count: 12},
		{sha: "131ab453d4611699e6f6989546c4b5d289e8768a", date: "2024-01-13 06:14:49 UTC", description: "improve documentation of `RetryLogic` trait functions", pr_number: 19617, scopes: ["sinks"], type: "docs", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 8, deletions_count: 0},
		{sha: "20b4fc72bcb8f605e044e05ae3df0e26aa637875", date: "2024-01-13 09:38:22 UTC", description: "remove trailing dot from s3 filename extension", pr_number: 19616, scopes: ["aws_s3 sink"], type: "fix", breaking_change: false, author: "Sebastian Tia", files_count: 2, insertions_count: 24, deletions_count: 1},
		{sha: "38d8801d4096f1f9e12ffd01fe8014b92682297d", date: "2024-01-16 16:04:05 UTC", description: "pub prometheus sink configs", pr_number: 19540, scopes: [], type: "chore", breaking_change: false, author: "Suika", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "521512dcb07d4222630999e301f82ddd5fd16218", date: "2024-01-16 15:23:35 UTC", description: "Bump the aws group with 2 updates", pr_number: 19619, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "cebe6284595badef5112807fd1f7e9a5f0e7d3ce", date: "2024-01-16 15:24:33 UTC", description: "Bump wasm-bindgen from 0.2.89 to 0.2.90", pr_number: 19620, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 10, deletions_count: 10},
		{sha: "628d207bf4769ebd0bbf2b98ddbbf162ebd5be14", date: "2024-01-16 23:32:48 UTC", description: "fix filter out PRs for gardener issue comment workflow", pr_number: 19618, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4c098417baef4c0d2d7af09beaad3dfa1483ad3f", date: "2024-01-16 23:38:59 UTC", description: "update GELF codec", pr_number: 19602, scopes: ["docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 45, insertions_count: 586, deletions_count: 0},
		{sha: "26f2468f66bc22a0d66b3a382be17a46bc4bb1a9", date: "2024-01-17 00:10:55 UTC", description: "Bump smallvec from 1.11.2 to 1.12.0", pr_number: 19623, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b540936fc0ac132d257e168dae78e228c3cce324", date: "2024-01-17 07:15:23 UTC", description: "Bump the clap group with 2 updates", pr_number: 19626, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 23, deletions_count: 23},
		{sha: "045b38448482a4d090b3ac0fbafa10fbf2ba0030", date: "2024-01-17 04:13:28 UTC", description: "Clarify that this source receives data from Splunk clients", pr_number: 19615, scopes: ["splunk_hec source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 9, deletions_count: 1},
		{sha: "b2c9f27d4360cbdb211d9f7230ae90e6becfee8d", date: "2024-01-17 05:45:51 UTC", description: "fix and simplify concurrency groups", pr_number: 19630, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 10, deletions_count: 9},
		{sha: "c30a45f362550c1b2989a1ca43f60bb7267ccfa0", date: "2024-01-18 05:09:28 UTC", description: "Bump the graphql group with 1 update", pr_number: 19583, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 99},
		{sha: "4e877e53d112310ddee4d97417550ed0e20316d4", date: "2024-01-18 11:39:13 UTC", description: "Bump the clap group with 2 updates", pr_number: 19634, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 24, deletions_count: 23},
		{sha: "58a37b24dad42fc8aa0bd4737786a6aae780a3c5", date: "2024-01-18 08:55:08 UTC", description: "acquire exclusive lock to global data dir", pr_number: 19595, scopes: ["config"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 63, deletions_count: 5},
		{sha: "2adf6726906b54e4ef30524b635830a860590310", date: "2024-01-19 07:47:47 UTC", description: "add write perms to the default data_dir", pr_number: 19659, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 6, deletions_count: 2},
		{sha: "cc9203b610868d5de8daff7ac1051dce9038dfe8", date: "2024-01-19 07:28:33 UTC", description: "Document Vector's MSRV policy", pr_number: 19646, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 10, deletions_count: 0},
		{sha: "52c12c3fa0355dd53edfd01ffd979f5be40f09f6", date: "2024-01-19 15:44:00 UTC", description: "Bump async-compression from 0.4.5 to 0.4.6", pr_number: 19652, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "32748273fbbf3a65851f6e4f65ddaae385000cdd", date: "2024-01-20 00:58:42 UTC", description: "Bump the aws group with 2 updates", pr_number: 19660, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 15, deletions_count: 15},
		{sha: "846075c4bbe2fb982c7d289a5011ec96d4f9b0cc", date: "2024-01-20 00:59:11 UTC", description: "Bump uuid from 1.6.1 to 1.7.0", pr_number: 19661, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "50a86ef4fb59b9f9ac5e3179d6e8892019d552ee", date: "2024-01-20 08:37:36 UTC", description: "Remove warning for unused outputs when output is disabled", pr_number: 19629, scopes: ["datadog_agent source"], type: "fix", breaking_change: false, author: "Sebastian Tia", files_count: 4, insertions_count: 122, deletions_count: 18},
		{sha: "9b024b9564b24524ce9a305b3c00080779f63250", date: "2024-01-20 13:40:52 UTC", description: "Bump actions/cache from 3 to 4", pr_number: 19642, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 7, deletions_count: 7},
		{sha: "c1199512c73bfd58e76daf1297cf29f7eff6aa5a", date: "2024-01-20 06:54:21 UTC", description: "Update h2", pr_number: 19648, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 9, deletions_count: 9},
		{sha: "55317dcda1a26c533242eb3a9bd24a61dd5958e3", date: "2024-01-23 06:52:31 UTC", description: "Bump openssl from 0.10.62 to 0.10.63", pr_number: 19672, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "9c581836c9a4ba1993022be918a034d50f89794e", date: "2024-01-23 06:52:42 UTC", description: "Bump cached from 0.47.0 to 0.48.0", pr_number: 19673, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "c12c8e1bef9fb2f9a9a31892d7911b8637f581e7", date: "2024-01-23 06:53:02 UTC", description: "Bump regex from 1.10.2 to 1.10.3", pr_number: 19674, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "ba9b4bd7c4af1eed4cc6b7e64686a2e666a306d6", date: "2024-01-23 06:54:03 UTC", description: "Bump smallvec from 1.12.0 to 1.13.1", pr_number: 19677, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "a6fb31b2bfd3fedcf53d858d5d7f99942649ea21", date: "2024-01-23 08:03:48 UTC", description: "Fix docs for `ignore_older_secs`", pr_number: 19682, scopes: [], type: "docs", breaking_change: false, author: "silverwind", files_count: 5, insertions_count: 8, deletions_count: 8},
		{sha: "eeab67d7b86166dfeac345144aaa36d72f746253", date: "2024-01-23 07:23:12 UTC", description: "Bump the graphql group with 2 updates", pr_number: 19670, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 21, deletions_count: 14},
		{sha: "f41ca86876ae9c6fb98c8edd363691cfff963daf", date: "2024-01-23 07:27:21 UTC", description: "Bump opendal from 0.44.1 to 0.44.2", pr_number: 19676, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 20},
		{sha: "5bb492608d935c38a1ae6e748592f0ae9812413c", date: "2024-01-23 03:57:16 UTC", description: "Add `graphql` field to toggle graphql endpoint", pr_number: 19645, scopes: ["config api"], type: "enhancement", breaking_change: false, author: "Sebastian Tia", files_count: 4, insertions_count: 53, deletions_count: 12},
		{sha: "25b1b8c7d891bbc7bbe8addbde0342c820b5424f", date: "2024-01-23 22:46:48 UTC", description: "Bump the clap group with 1 update", pr_number: 19687, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b56f1c3a341df729a217256fa3fefa9772583c96", date: "2024-01-24 09:30:04 UTC", description: "Bump the aws group with 1 update", pr_number: 19688, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b51085b1a8d0c3e7c957bf9ad1d2a8db6a661dce", date: "2024-01-24 09:37:59 UTC", description: "Bump serde_with from 3.4.0 to 3.5.0", pr_number: 19675, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "2f8fbd135e1c7a683d70be0c09a8dbc43e6f5d0d", date: "2024-01-24 09:38:30 UTC", description: "Bump proc-macro2 from 1.0.76 to 1.0.78", pr_number: 19671, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 68, deletions_count: 68},
		{sha: "53f97c1c61ca176ba20852d0cfc1e45e44cf2235", date: "2024-01-24 09:39:18 UTC", description: "Bump env_logger from 0.10.1 to 0.10.2", pr_number: 19651, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "5d7ceaa8c963bd23e6c0b066fa36c0581103575f", date: "2024-01-24 07:03:16 UTC", description: "Run the changelog check on the merge queue to pass required checks", pr_number: 19696, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 24, deletions_count: 2},
		{sha: "7cf2f009dbd9be4177dfbce7950cd82d57f93448", date: "2024-01-24 10:57:26 UTC", description: "emit graphql field of api config", pr_number: 19692, scopes: ["config api"], type: "fix", breaking_change: false, author: "Sebastian Tia", files_count: 3, insertions_count: 9, deletions_count: 3},
		{sha: "88c10a9e0142a5aca06972ceba2e24983df631b6", date: "2024-01-25 01:46:33 UTC", description: "Bump the aws group with 2 updates", pr_number: 19697, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 6},
		{sha: "650d478fc28f79d1f075f43971cd2b54ca848652", date: "2024-01-25 01:20:06 UTC", description: "Drop dependency on `cached` crate", pr_number: 19693, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 10, deletions_count: 81},
		{sha: "cacba25ea31a394663169b253dba747f6f8a89f6", date: "2024-01-26 11:25:26 UTC", description: "Bump peter-evans/create-or-update-comment from 3 to 4", pr_number: 19710, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b72217cf40d3216625cf274fe79b669f823a1c8a", date: "2024-01-26 11:25:56 UTC", description: "Bump dorny/paths-filter from 2 to 3", pr_number: 19708, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8a82a3b1347c25efbb06b9ad300fd9d7a779b202", date: "2024-01-26 06:26:29 UTC", description: "remove cfg test attribute", pr_number: 19684, scopes: ["aws region"], type: "fix", breaking_change: false, author: "Sebastian Tia", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "a4aff31d54a3c820f50aa94acef66e0938f3c77e", date: "2024-01-26 11:54:16 UTC", description: "Bump bufbuild/buf-setup-action from 1.28.1 to 1.29.0", pr_number: 19709, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2b8334397212f749ad5ef4961d22a630568f7dd6", date: "2024-01-27 06:54:12 UTC", description: "Bump pin-project from 1.1.3 to 1.1.4", pr_number: 19718, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "dd50a46b92f33dfbf81ef150a7be892c896ab401", date: "2024-01-27 06:54:21 UTC", description: "Bump memmap2 from 0.9.3 to 0.9.4", pr_number: 19719, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "09668836bb8331e894d5c48e0376041fb92e385d", date: "2024-01-27 08:21:41 UTC", description: "Bump mlua from 0.9.4 to 0.9.5", pr_number: 19717, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 10, deletions_count: 10},
		{sha: "4195071d984a4d2107a2f5888bca82db0bab4b5c", date: "2024-01-27 05:12:26 UTC", description: "propagate tracing span context in stream sink request building", pr_number: 19712, scopes: ["observability"], type: "fix", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 44, deletions_count: 0},
		{sha: "b141f2ea0550410989a98bef80e5863a373dca4c", date: "2024-01-27 05:49:38 UTC", description: "Bump chrono from 0.4.31 to 0.4.33", pr_number: 19723, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 13, insertions_count: 87, deletions_count: 17},
		{sha: "650a738e63f3ff7d80ff872760fc8497b257e709", date: "2024-01-30 01:40:33 UTC", description: "update basic sink tutorial doc", pr_number: 19722, scopes: ["docs"], type: "chore", breaking_change: false, author: "Sebastian Tia", files_count: 1, insertions_count: 26, deletions_count: 13},
		{sha: "51c6466c7d848b49e9a66293ddfb8211c1f6acb5", date: "2024-01-30 10:20:20 UTC", description: "Bump lru from 0.12.1 to 0.12.2", pr_number: 19731, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7c27b2e5eb82150660a6066318aef2926be84ee1", date: "2024-01-30 16:21:04 UTC", description: "Bump inventory from 0.3.14 to 0.3.15", pr_number: 19732, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fc0958863b674fbca4550c274e6f0c7711264593", date: "2024-01-30 16:23:07 UTC", description: "Bump cargo_toml from 0.18.0 to 0.19.0", pr_number: 19733, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5f233f23700fb22a031168078cdcbaee79242775", date: "2024-01-30 16:25:25 UTC", description: "Bump the aws group with 2 updates", pr_number: 19720, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 17, deletions_count: 17},
		{sha: "7056f5fe02af3d11a0ac813c9043788d96ed233c", date: "2024-01-30 16:50:10 UTC", description: "Bump serde_json from 1.0.111 to 1.0.112", pr_number: 19730, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 13, insertions_count: 16, deletions_count: 15},
		{sha: "bf2d7329c0fd41f478f974b282923f00d89cf027", date: "2024-01-31 00:37:24 UTC", description: "Bump cargo_toml from 0.19.0 to 0.19.1", pr_number: 19744, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9571b4ec304f80a530ab312755cb93e9197ae1ba", date: "2024-01-31 06:38:35 UTC", description: "Bump itertools from 0.12.0 to 0.12.1", pr_number: 19745, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 10},
		{sha: "ec9b2c7df7eba02dc1c3c0252c05a0a6499d5371", date: "2024-01-31 07:05:47 UTC", description: "Bump serde from 1.0.195 to 1.0.196", pr_number: 19734, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 17, insertions_count: 24, deletions_count: 30},
		{sha: "f085b72615c7e98760aef1192b72f697d127e358", date: "2024-01-31 07:06:29 UTC", description: "Bump the aws group with 2 updates", pr_number: 19742, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "83be4258bf998e6a2741c0ddf44a5b2ff29cbc67", date: "2024-01-31 07:06:42 UTC", description: "Bump darling from 0.20.3 to 0.20.4", pr_number: 19743, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "60f5fe091dfb73139945c931a4fab2164d59cc92", date: "2024-01-31 04:59:43 UTC", description: "suggest make generate-component-docs", pr_number: 19740, scopes: ["docs"], type: "chore", breaking_change: false, author: "gabriel376", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "c1a39e4067362d6e699573c4be4a92cef044766f", date: "2024-01-31 13:44:37 UTC", description: "Enable population of event metadata by a VRL unit test source", pr_number: 19729, scopes: ["unit tests"], type: "enhancement", breaking_change: false, author: "Mykola Rybak", files_count: 3, insertions_count: 39, deletions_count: 2},
		{sha: "ba2b3508ef5e6995d3dbd47d70977aa1763e8a34", date: "2024-02-01 00:57:35 UTC", description: "Bump openssl-src from 300.2.1+3.2.0 to 300.2.2+3.2.1", pr_number: 19750, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "13ac2dfb981160e4f6d1541c8537e47d6ac761e9", date: "2024-02-01 06:58:21 UTC", description: "Bump darling from 0.20.4 to 0.20.5", pr_number: 19751, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "43a9a366c4dee15f0294a0cd22c2dc5b8b2daae8", date: "2024-02-01 05:38:24 UTC", description: "Add end-to-end tests with the Datadog Agent", pr_number: 18538, scopes: ["tests"], type: "chore", breaking_change: false, author: "neuronull", files_count: 56, insertions_count: 2071, deletions_count: 306},
		{sha: "d7c615c6837429d8e36cd02df8da2e7485656df2", date: "2024-02-01 07:11:01 UTC", description: "Add configurable support for `http::Uri`", pr_number: 19758, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 29, deletions_count: 0},
		{sha: "0f3faba5ee3fae2531ce4bb9b739a1a54d860f69", date: "2024-02-01 08:38:24 UTC", description: "Add `delete_failed_message` configuration option", pr_number: 19748, scopes: ["s3 source"], type: "feat", breaking_change: false, author: "tanushri-sundar", files_count: 5, insertions_count: 79, deletions_count: 4},
		{sha: "abb292a8c6179eb5650cc2a88f18897aa71509cf", date: "2024-02-01 17:45:21 UTC", description: "Bump nick-fields/retry from 2 to 3", pr_number: 19756, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 74, deletions_count: 74},
		{sha: "3da1a0206583500abad617147d76b3faf602a09b", date: "2024-02-02 00:52:15 UTC", description: "Bump libc from 0.2.152 to 0.2.153", pr_number: 19763, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "00a94801025a215a78ce684422b0a986727ccc50", date: "2024-02-02 07:34:49 UTC", description: "Bump docker/metadata-action from 5.5.0 to 5.5.1", pr_number: 19755, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "bd9fbd682b673e01f712a79af326eb307883cfad", date: "2024-02-02 02:56:28 UTC", description: "Bump reqwest from 0.11.23 to 0.11.24", pr_number: 19762, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 6},
		{sha: "65acf06934c733bf3608387b2264b071cca27f3d", date: "2024-02-02 04:51:59 UTC", description: "Bump toml from 0.8.8 to 0.8.9", pr_number: 19761, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 22, deletions_count: 21},
		{sha: "7cd151a822f0073f9df4bf01d7aec11500f5efe1", date: "2024-02-02 04:10:38 UTC", description: "Update labels used by dependabot", pr_number: 19760, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "29a91a44ac762f2b02938d144503849a570ec747", date: "2024-02-02 10:39:14 UTC", description: "Revert \"Add configurable support for `http::Uri`\"", pr_number: 19770, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 0, deletions_count: 29},
		{sha: "ac80d1ed07983d203671b7c2c625715fbc06a234", date: "2024-02-03 02:28:18 UTC", description: "expose DatadogSearch", pr_number: 19778, scopes: ["deps"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a215d59f1fcef34913e4316c36ca09ebea3bf7a0", date: "2024-02-03 04:20:06 UTC", description: "Pass the extra context to sources and transforms too", pr_number: 19779, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 19, deletions_count: 2},
		{sha: "0a2dc2bafa6e56218797a0c238118ed58fd94113", date: "2024-02-03 04:20:17 UTC", description: "Implement an easier creator for multi-valued `ExtraContext`", pr_number: 19777, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 15, deletions_count: 13},
		{sha: "28a4cb4ca348287fb336f248988dd39ee9a74907", date: "2024-02-06 02:23:03 UTC", description: "add sink validator", pr_number: 17980, scopes: ["component validation"], type: "feat", breaking_change: false, author: "neuronull", files_count: 9, insertions_count: 375, deletions_count: 347},
		{sha: "17b29628c742a2841a19b19f70c5465935089b68", date: "2024-02-06 11:01:08 UTC", description: "Bump rkyv from 0.7.43 to 0.7.44", pr_number: 19789, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "0dce77620fbc240f6e880c6f49f7ef7f8bb5e3df", date: "2024-02-06 11:01:22 UTC", description: "Bump tokio from 1.35.1 to 1.36.0", pr_number: 19790, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 13, deletions_count: 13},
		{sha: "774693772f6543166892c8497b3e9ab699045435", date: "2024-02-06 11:01:30 UTC", description: "Bump opendal from 0.44.2 to 0.45.0", pr_number: 19788, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a247c515f768ef2293821e802ec3c7793cd5a1d5", date: "2024-02-06 11:03:25 UTC", description: "Bump dorny/paths-filter from 2 to 3", pr_number: 19768, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "43b96baa64a8cd6eefec1679f3b34ad753121d62", date: "2024-02-06 11:35:59 UTC", description: "Bump serde_with from 3.5.0 to 3.6.0", pr_number: 19800, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "509a858e74d43a431589b21928c405ac461f6551", date: "2024-02-06 15:28:32 UTC", description: "Bump vrl from 0.9.1 to 0.10.0", pr_number: 19705, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 53, deletions_count: 29},
		{sha: "c5ee82faf01b543ad4db746abe5d4a305844a406", date: "2024-02-06 23:46:31 UTC", description: "Bump Alpine base image from 3.18 to 3.19", pr_number: 19804, scopes: ["releasing"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 1},
		{sha: "541e3086abcb4d95b77c273f6de19d9dc326c156", date: "2024-02-07 07:48:50 UTC", description: "Bump the clap group with 1 update", pr_number: 19786, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "68272040067f5cf167d925234bdfc15b6bd60f6f", date: "2024-02-07 07:48:58 UTC", description: "Bump ratatui from 0.25.0 to 0.26.0", pr_number: 19787, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 57, deletions_count: 10},
		{sha: "b3e0af7f268c2ef4c26299195a0aec0263df0b61", date: "2024-02-07 07:49:16 UTC", description: "Bump tempfile from 3.9.0 to 3.10.0", pr_number: 19807, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 16, deletions_count: 17},
		{sha: "f38ed158f939c6acf78cd039349d897f7127f0d1", date: "2024-02-07 07:49:26 UTC", description: "Bump toml from 0.8.9 to 0.8.10", pr_number: 19808, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "fd76dbf0fff80f89e1b7bdbfb57cf864709e9dfa", date: "2024-02-07 01:23:23 UTC", description: "re-organize to expose sampling logic", pr_number: 19806, scopes: ["sample transform"], type: "chore", breaking_change: false, author: "neuronull", files_count: 6, insertions_count: 183, deletions_count: 140},
		{sha: "c4fe1342ce8b80ef822203f01ef0093751195a3d", date: "2024-02-07 12:34:42 UTC", description: "make containing module pub", pr_number: 19816, scopes: ["sample transform"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9d4e89ee6304918be9a91e32a2edf89189bfe4c4", date: "2024-02-08 08:04:20 UTC", description: "fix example for high quality error messages", pr_number: 19821, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 21, deletions_count: 9},
		{sha: "92b83cd2bea0c075134ea33bb2b204d333e4f27e", date: "2024-02-08 01:43:14 UTC", description: "clippy lint on feature flag case", pr_number: 19822, scopes: ["sample transform"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "cf1aec66cd5fd9c4d01efce646de167a079b195e", date: "2024-02-08 04:22:21 UTC", description: "Add support for proptest to lookup types", pr_number: 19769, scopes: ["tests"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 16, insertions_count: 104, deletions_count: 55},
		{sha: "0046ee9b394274bc184efd2a07e76639cebe12fb", date: "2024-02-08 09:01:08 UTC", description: "expose component test utils", pr_number: 19826, scopes: [], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 1},
		{sha: "56486bafe6ce41a7c92a11ccd0e2cf6e8f7ef838", date: "2024-02-08 07:11:17 UTC", description: "Bump VRL to 0.11.0", pr_number: 19827, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 11, deletions_count: 9},
		{sha: "fa2c1941b3cf98316a94575a5faa9f0a025e8a9c", date: "2024-02-09 09:02:19 UTC", description: "Bump aws-actions/configure-aws-credentials from 4.0.1 to 4.0.2", pr_number: 19823, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "1c09d09cd4b9f86fd5e0a79d97fc6eb4b215cfa2", date: "2024-02-09 09:02:22 UTC", description: "Bump the prost group with 1 update", pr_number: 19830, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "c4593b743078762597c95c9a31430dfc2b845b37", date: "2024-02-09 09:02:24 UTC", description: "Bump num-traits from 0.2.17 to 0.2.18", pr_number: 19831, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "c797bc69b51574778b804f9bbdeb449af4f9af19", date: "2024-02-09 02:30:52 UTC", description: "improve example for `rate` setting", pr_number: 19834, scopes: ["sample transform"], type: "chore", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 7, deletions_count: 4},
		{sha: "c2917c1e22a9642d0e0072654c40be0c385c6b9b", date: "2024-02-09 02:40:10 UTC", description: "Documentation for redact redactor option", pr_number: 19749, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Thayne McCombs", files_count: 1, insertions_count: 61, deletions_count: 0},
		{sha: "0d57ad9548dbfc97f7e6d32d81c6e179e19a465e", date: "2024-02-09 11:03:21 UTC", description: "Bump serde_yaml from 0.9.30 to 0.9.31", pr_number: 19832, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 10, deletions_count: 10},
		{sha: "a5d9a2777f97d23ea880a2a9f819878d6c69cfa5", date: "2024-02-09 12:04:16 UTC", description: "add support for parsing HTTPS and SVCB records", pr_number: 19819, scopes: ["dnsmsg_parser"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 5, insertions_count: 109, deletions_count: 8},
		{sha: "9e297f6c4faa503d195f29648aa5e35c7343acdd", date: "2024-02-09 12:06:37 UTC", description: "Bump serde-toml-merge from 0.3.3 to 0.3.4", pr_number: 19771, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ff246b621b8c6d5c052621d4a4e86c6942a20f13", date: "2024-02-09 12:25:15 UTC", description: "Bump wasm-bindgen from 0.2.90 to 0.2.91", pr_number: 19817, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 10, deletions_count: 10},
		{sha: "9a20a12be927d29e929b62e4313193d91b86f543", date: "2024-02-09 07:41:38 UTC", description: "implement VRL decoder", pr_number: 19825, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 25, insertions_count: 992, deletions_count: 4},
		{sha: "4115c65587918e0f8a8ab31b1444e5c79e12e5ec", date: "2024-02-09 14:49:39 UTC", description: "add documentation for `parse_etld` function", pr_number: 19795, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 3, insertions_count: 68, deletions_count: 0},
		{sha: "de24167165a026c4df387459058efe341631668e", date: "2024-02-09 16:07:35 UTC", description: "fix inconsistency in docker configuration example", pr_number: 19797, scopes: ["setup"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0e6cf3e439e484f3e4e29d8a90b9250ebb274e95", date: "2024-02-09 16:41:05 UTC", description: "correctly emit metadata to log namespace", pr_number: 19812, scopes: ["journald source"], type: "fix", breaking_change: false, author: "dalegaard", files_count: 3, insertions_count: 38, deletions_count: 11},
		{sha: "18252206790c0c97863d110d0ec2cdd3bb15d24d", date: "2024-02-09 16:45:36 UTC", description: "New --skip-healthchecks option for vector validate", pr_number: 19691, scopes: ["administration config"], type: "enhancement", breaking_change: false, author: "Martin Emrich", files_count: 4, insertions_count: 28, deletions_count: 2},
		{sha: "7c3f91b3de204adcc154b9b0bcad1f5a85741ee3", date: "2024-02-09 07:53:00 UTC", description: "Look at merge base when looking for added changelog files", pr_number: 19835, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c172d504ea26f06a5be15c71dbfa6b135d732dc1", date: "2024-02-09 07:59:11 UTC", description: "Ensure changelog fragment author doesn't start with @", pr_number: 19836, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 6, deletions_count: 3},
		{sha: "ab9bf4ed2aa9e00223c973e5c899b1ef8aedade0", date: "2024-02-09 09:12:43 UTC", description: "Add VRL function get_vector_timezone", pr_number: 19727, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "klondikedragon", files_count: 3, insertions_count: 54, deletions_count: 0},
		{sha: "ed5578e89c1b0237e826ce0968713d67a99febef", date: "2024-02-10 07:34:22 UTC", description: "Bump heim from `76fa765` to `a66c440`", pr_number: 19840, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "86fe001b474cdd7cf74a63bd2f36b2fc81cf9f9f", date: "2024-02-10 07:34:35 UTC", description: "Bump serde_with from 3.6.0 to 3.6.1", pr_number: 19841, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 15, deletions_count: 14},
		{sha: "0851fca24799b9cd61df4eb7c7ab1838ae668236", date: "2024-02-10 10:49:40 UTC", description: "add documentation for punycode encoding functions", pr_number: 19794, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 6, insertions_count: 121, deletions_count: 0},
		{sha: "405f3ef22c3e25e196a4d9f76a8dfbb17f2e8c5c", date: "2024-02-10 10:50:09 UTC", description: "make `parse_etld` fallible in docs", pr_number: 19842, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "4ab4c4a3c846f3d295feb890e923e9116a0b0441", date: "2024-02-10 03:37:04 UTC", description: "checkout full depth for changelog workflow", pr_number: 19844, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "beb76a81e8761da4eb2e0873607ba327baa81ea9", date: "2024-02-10 04:35:45 UTC", description: "Add documentation for replace_with", pr_number: 19638, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Thayne McCombs", files_count: 1, insertions_count: 85, deletions_count: 0},
		{sha: "382ab32476d5204979e2170de90adcd6087edb64", date: "2024-02-10 11:36:37 UTC", description: "Bump the aws group with 5 updates", pr_number: 19838, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 15, deletions_count: 15},
		{sha: "76ab88dfcb51014986bed948f499cd51c5582bf4", date: "2024-02-10 04:31:06 UTC", description: "Reduce test timeout to 2 minutes", pr_number: 19845, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "52049f81459d064abaf92e302414160e1ab39512", date: "2024-02-10 12:26:45 UTC", description: "add format", pr_number: 19739, scopes: ["clickhouse sink"], type: "feat", breaking_change: false, author: "gabriel376", files_count: 6, insertions_count: 99, deletions_count: 4},
		{sha: "51ee1044a1a60528c52b87e3f1f4cbd0290308fe", date: "2024-02-13 00:53:06 UTC", description: "Bump indexmap from 2.2.2 to 2.2.3", pr_number: 19855, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 23, deletions_count: 23},
		{sha: "493fb74d9530e8dc536e61b0e94ba327f8aac8cb", date: "2024-02-13 00:54:03 UTC", description: "Bump mongodb from 2.8.0 to 2.8.1", pr_number: 19856, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9a610b009f7809458f50b9dd7ecab5aa15347282", date: "2024-02-13 06:54:56 UTC", description: "Bump thiserror from 1.0.56 to 1.0.57", pr_number: 19854, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "6bac428780de7d79cd750be9cfc36c4060a00019", date: "2024-02-13 07:45:12 UTC", description: "Bump chrono from 0.4.33 to 0.4.34", pr_number: 19851, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "9e7e658fa53c25d7d78d4fff00cdb3bb06f6af19", date: "2024-02-13 07:47:56 UTC", description: "Bump indicatif from 0.17.7 to 0.17.8", pr_number: 19850, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
	]
}
