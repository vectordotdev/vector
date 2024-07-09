package metadata

releases: "0.39.0": {
	date:     "2024-06-17"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.39.0!

		Be sure to check out the [upgrade guide](/highlights/2024-06-17-0-39-0-upgrade-guide) for
		breaking changes in this release.

		This release just contains a mix of small enhancements and bug fixes. See the changelog
		below.
		"""

	changelog: [
		{
			type: "enhancement"
			description: """
				VRL was updated to v0.16.0. This includes the following changes:

				Breaking Changes & Upgrade Guide:

				- The deprecated coalesce paths (i.e. `(field1|field2)`) feature is now removed. (https://github.com/vectordotdev/vrl/pull/836)

				New Features:

				- Added `psl` argument to the `parse_etld` function. It enables customizing used public suffix list. If none is provided the default (https://publicsuffix.org/list/public_suffix_list.dat) is used, which is that was used before this change.

				Enhancements:

				- Add traceability_id field support to parse_aws_alb_log (https://github.com/vectordotdev/vrl/pull/862)
				"""
		},
		{
			type: "enhancement"
			description: """
				The `kafka` sink has a new `healthcheck_topic` configuration option to configure the topic used for healthchecks. Previously, `topic` was used to perform healthchecks, but a templated value would cause healthchecks to fail.
				"""
			contributors: ["yalinglee"]
		},
		{
			type: "enhancement"
			description: """
				The `namespace` field for the `gcp_chronicle` sink is now a templatable field.
				"""
			contributors: ["chocpanda"]
		},
		{
			type: "enhancement"
			description: """
				The `prometheus_exporter` sink now supports compressing responses based on the `Accept-Encoding header sent with client requests.
				"""
			contributors: ["edhjer"]
		},
		{
			type: "enhancement"
			description: """
				A `pretty` option to the `json` codec to output events as prettified JSON.
				"""
			contributors: ["lsampras"]
		},
		{
			type: "enhancement"
			description: """
				The `http_server` source can now optionally annotate events with the remote IP by setting the `host_key` option.
				"""
			contributors: ["NishantJoshi00"]
		},
		{
			type: "enhancement"
			description: """
				Vector's start-up time was greatly improved when loading configurations including many `remap` transforms.
				"""
			contributors: ["Zettroke"]
		},
		{
			type: "fix"
			description: """
				The `endpoint` in the Datadog sinks is rewritten the same as the Datadog Agent
				does to produce the API validation endpoint to avoid a HTTP 404 Not Found error
				when running the health check.
				"""
		},
		{
			type: "fix"
			description: """
				The `kafka` source main loop has been biased to handle acknowledgements before new
				messages to avoid a memory leak under high load.
				"""
		},
		{
			type: "feat"
			description: """
				The `kafka` source will now consume less memory by not over allocating buffers while reading.
				"""
			contributors: ["biuboombiuboom"]
		},
		{
			type: "chore"
			description: """
				The loki sink's default `out_of_order_action` option has been changed from `deny` to
				`accept` as this has been the default behaviour for Loki since 2.4.0 released three
				years ago.
				"""
			contributors: ["jpds"]
		},
		{
			type: "feat"
			description: """
				The `opentelemetry` source now has experimental support for ingesting traces.
				"""
			contributors: ["caibirdme"]
		},
		{
			type: "chore"
			description: """
				The deprecated `enterprise` feature and global configuration option has been removed. See the upgrade guide for details.
				"""
		},
		{
			type: "enhancement"
			description: """
				The Alpine base image used for Vector `-alpine` and `-distroless-static` images was updated to 3.20.
				"""
		},
		{
			type: "feat"
			description: """
				A new configuration option `end_every_period_ms` is available on reduce transforms
				If supplied, every time this interval elapses for a given grouping, the reduced value
				for that grouping is flushed. Checked every `flush_period_ms`.
				"""
			contributors: ["charlesconnell"]
		},
	]

	commits: [
		{sha: "0da155d49f7fb01a4499c7e52f8357779c723888", date: "2024-05-08 21:13:38 UTC", description: "Fix link in release notes", pr_number: 20454, scopes: ["docs"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "e1d1e851e71bd8c20f9c53a8340f0cdc3d0e7c12", date: "2024-05-08 23:36:13 UTC", description: "Regenerate manifests for chart v0.33.0", pr_number: 20453, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "3a115c517fe91f4c70eb8211b6dfdd1899adf07c", date: "2024-05-10 02:16:44 UTC", description: "Switch to Confluent docker images since wurstmeister ones disappeared", pr_number: 20465, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 7, insertions_count: 33, deletions_count: 24},
		{sha: "783ed1fe1a4cf0de415fde71706b1e315a58d215", date: "2024-05-10 01:30:50 UTC", description: "Drop `cached`", pr_number: 20455, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 0, deletions_count: 35},
		{sha: "8301101968a3b3f4cf6c42afb25b2f6d49ded93e", date: "2024-05-10 02:22:29 UTC", description: "Reorder message consume loop to avoid memory growth", pr_number: 20467, scopes: ["kafka source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 29, deletions_count: 20},
		{sha: "df26f56a09607f90aa25dba16aade2e3a5a656e9", date: "2024-05-10 05:09:53 UTC", description: "Bump serde from 1.0.200 to 1.0.201", pr_number: 20459, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "e02118a35901fb27c59176608e0c15a0070825aa", date: "2024-05-10 05:10:02 UTC", description: "Bump pulsar from 6.1.0 to 6.2.0", pr_number: 20458, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f7561c0812b8d07eea4dbf1b9c3364a07ab1f9fe", date: "2024-05-10 09:10:12 UTC", description: "Bump the aws group with 3 updates", pr_number: 20456, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 10},
		{sha: "b5191f695ea1309b27cd0abd6bd1adf9524f1860", date: "2024-05-10 09:10:18 UTC", description: "Bump databend-client from 0.17.1 to 0.17.2", pr_number: 20450, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9cad9484ca84b9330ad2ad3eb6eab368fd70ff0b", date: "2024-05-10 09:10:26 UTC", description: "Bump semver from 1.0.22 to 1.0.23", pr_number: 20449, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "99446fa5f44520b89bc487b85cd3a2f2561737cd", date: "2024-05-10 09:10:31 UTC", description: "Bump paste from 1.0.14 to 1.0.15", pr_number: 20448, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "5015eaaf5d9be5c6324cfd2a1af006134b70e9ad", date: "2024-05-10 09:10:44 UTC", description: "Bump ryu from 1.0.17 to 1.0.18", pr_number: 20445, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "cdda013f35fa530abd02c469386da8cb8925b0f7", date: "2024-05-10 09:19:51 UTC", description: "Bump thiserror from 1.0.59 to 1.0.60", pr_number: 20441, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "f5f16cc9a7c0a9c1ba8895b6e054d96a6b1504c4", date: "2024-05-10 09:21:18 UTC", description: "Bump proc-macro2 from 1.0.81 to 1.0.82", pr_number: 20447, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 69, deletions_count: 69},
		{sha: "6d8ca9c24b70a436348f63d3cb9116b9ac595846", date: "2024-05-10 09:21:19 UTC", description: "Bump anyhow from 1.0.82 to 1.0.83", pr_number: 20446, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "eb9f4232159ee44038c1fa99439963fba28df5bd", date: "2024-05-10 20:49:40 UTC", description: "add CompressionLayer support", pr_number: 20065, scopes: ["prometheus_exporter sink"], type: "enhancement", breaking_change: false, author: "Jeremy", files_count: 6, insertions_count: 113, deletions_count: 23},
		{sha: "021f6458b2414846f23efeb480308d7781cc0487", date: "2024-05-11 01:01:52 UTC", description: "Bump aws-types from 1.2.0 to 1.2.1 in the aws group", pr_number: 20472, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3770249bfb31663fc3d3bfe2c52646055361e346", date: "2024-05-11 05:02:03 UTC", description: "Bump the graphql group with 2 updates", pr_number: 20473, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 59, deletions_count: 39},
		{sha: "9d94280f55fb208e4279f5e341f3838e30649784", date: "2024-05-11 05:02:16 UTC", description: "Bump async-compression from 0.4.9 to 0.4.10", pr_number: 20474, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "19def61b63aae1dafd9d9f655e8213c3d85fca0d", date: "2024-05-11 05:02:34 UTC", description: "Bump getrandom from 0.2.14 to 0.2.15", pr_number: 20442, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 15, deletions_count: 15},
		{sha: "13aaea12c25db21037a85c2b79095d4a2f0b11b2", date: "2024-05-11 06:25:38 UTC", description: "Bump prettydiff from 0.6.4 to 0.7.0", pr_number: 20460, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 33, deletions_count: 115},
		{sha: "02de73907ab5a205aae0bd04ebcf092b45e3eaf3", date: "2024-05-11 08:49:29 UTC", description: "Changed OutOfOrderAction default to accept", pr_number: 20469, scopes: ["loki sink"], type: "chore", breaking_change: false, author: "Jonathan Davies", files_count: 3, insertions_count: 13, deletions_count: 8},
		{sha: "f2be33ef1604883a56702b3e97fef60eb8a83183", date: "2024-05-11 04:57:52 UTC", description: "Bump num-traits from 0.2.18 to 0.2.19", pr_number: 20433, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "911e63d4ee35c1de5082b6c2df6bc4ac0678ff31", date: "2024-05-11 09:25:48 UTC", description: "Bump syn from 2.0.60 to 2.0.61", pr_number: 20444, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 50, deletions_count: 50},
		{sha: "50ef76b66633ff06c1b81f826c7f8fcec53ebe52", date: "2024-05-22 06:09:15 UTC", description: "Fix nextest / rustup dll issue", pr_number: 20544, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "98a795eaf12132a4dd6cd506781cf3127b9ad6f9", date: "2024-05-22 07:45:48 UTC", description: "add arbitrary trait", pr_number: 20537, scopes: ["reduce"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 9, deletions_count: 5},
		{sha: "d1c2aecd47ae90d14a133af03914160366a6e66c", date: "2024-05-22 09:38:12 UTC", description: "Bump serde-toml-merge from 0.3.6 to 0.3.7", pr_number: 20501, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7c745d2babc32d52848631b53f77a7a68255d65a", date: "2024-05-22 09:38:17 UTC", description: "Bump mlua from 0.9.7 to 0.9.8", pr_number: 20503, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "cddc1808d1a49e2766770fc06b25d2be6ddc0234", date: "2024-05-22 13:38:20 UTC", description: "Bump serde_derive_internals from 0.29.0 to 0.29.1", pr_number: 20504, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "d5c23fe1a0fd09a7e82921bf5065ae2fba83bd67", date: "2024-05-22 13:38:23 UTC", description: "Bump itertools from 0.12.1 to 0.13.0", pr_number: 20508, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 16, deletions_count: 7},
		{sha: "eacf1e8ec126cde432d6537b005566e79b9a8cc4", date: "2024-05-22 13:38:28 UTC", description: "Bump crossbeam-utils from 0.8.19 to 0.8.20 in the crossbeam group", pr_number: 20518, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "c880b960eb960ebfcfe8ecb1c943cb3b35f0b5c6", date: "2024-05-22 13:38:35 UTC", description: "Bump thiserror from 1.0.60 to 1.0.61", pr_number: 20521, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "879150ed348d9071b000497c0e9e84e357cbe47f", date: "2024-05-22 13:38:40 UTC", description: "Bump bufbuild/buf-setup-action from 1.31.0 to 1.32.0", pr_number: 20516, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2a52d2212a91cd001361f918ce19924285ff4dbc", date: "2024-05-22 13:38:43 UTC", description: "Bump pulsar from 6.2.0 to 6.3.0", pr_number: 20524, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 9, deletions_count: 5},
		{sha: "bd15a6b130cd6833f03ad288ad32e3f2e61fe4e8", date: "2024-05-22 13:38:45 UTC", description: "Bump syn from 2.0.61 to 2.0.65", pr_number: 20526, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 48, deletions_count: 48},
		{sha: "58047ad2c710a20f98a5634f73b5de1a3eea40a6", date: "2024-05-22 13:38:48 UTC", description: "Bump h2 from 0.4.4 to 0.4.5", pr_number: 20527, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "3031e9b307b021f27c3930b5e89dd0043479483f", date: "2024-05-22 13:39:24 UTC", description: "Bump crc32fast from 1.4.0 to 1.4.2", pr_number: 20539, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ff5af874069ee0be90fca949bd51640369507a1a", date: "2024-05-22 13:55:21 UTC", description: "Bump serde from 1.0.201 to 1.0.202", pr_number: 20502, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "27393b5c2a8ce4b71072db41df3210a72031735c", date: "2024-05-23 01:20:30 UTC", description: "expose reduce logic", pr_number: 20543, scopes: ["reduce"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 995, deletions_count: 972},
		{sha: "a725d740398226946b4476fa14c490a83d2733b2", date: "2024-05-23 07:16:55 UTC", description: "Bump toml from 0.8.12 to 0.8.13", pr_number: 20500, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 12},
		{sha: "b03a8fa93aad614da4f69249dfaf8359dbb96ce4", date: "2024-05-23 07:17:57 UTC", description: "Bump anyhow from 1.0.83 to 1.0.86", pr_number: 20520, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "8645cd9eec4eb59f65dced7c8e9f37b3dcc03a5d", date: "2024-05-23 07:18:04 UTC", description: "Bump libc from 0.2.154 to 0.2.155", pr_number: 20523, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "16e3550c0a13feb7e92117bb71afc40ec46c7c0c", date: "2024-05-23 07:19:47 UTC", description: "Bump databend-client from 0.17.2 to 0.18.1", pr_number: 20540, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 79, deletions_count: 17},
		{sha: "e418dd551e876f68279fa06d7feff6a59921869f", date: "2024-05-23 07:19:58 UTC", description: "Bump the aws group with 3 updates", pr_number: 20545, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 10},
		{sha: "d62fcdd64cb50b969144db6cfd2c63c354021485", date: "2024-05-23 08:01:04 UTC", description: "Bump the prost group with 3 updates", pr_number: 20519, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 21, deletions_count: 20},
		{sha: "16ef571da08e78a170c588ec6981c6f648a9eec4", date: "2024-05-23 08:28:06 UTC", description: "Bump ratatui from 0.26.2 to 0.26.3", pr_number: 20538, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 15, deletions_count: 4},
		{sha: "082d93575ad83ff2c824d3597d6cf2c64c3af57e", date: "2024-05-23 04:28:18 UTC", description: "Fix link to remap event data model", pr_number: 20536, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4cc1ecfb9aee23500611eaf0c02bb78d62fc1a34", date: "2024-05-23 04:28:31 UTC", description: "Remove unreported metrics from the docs", pr_number: 20530, scopes: ["kubernetes_logs source"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 0, deletions_count: 68},
		{sha: "f5abce9b144604ae9d49251cba39a9f8ea717497", date: "2024-05-24 12:52:27 UTC", description: "use `msg.payload_len()` to initialize `FramedRead`", pr_number: 20529, scopes: ["kafka source"], type: "feat", breaking_change: false, author: "you", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "8b4a3ba57851eb1d051961e504b9e7bd9b7e0f8a", date: "2024-05-23 23:59:49 UTC", description: "Drop support for missing example configs", pr_number: 20550, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 13, deletions_count: 22},
		{sha: "14d8f31bf288fa12bf464278d6917b0d651181c2", date: "2024-05-24 03:53:39 UTC", description: "correct feature gate", pr_number: 20554, scopes: ["reduce"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "d777dc55039e58ea9aeedd8781ba75320d0ee044", date: "2024-05-24 05:17:39 UTC", description: "Bump serde_json from 1.0.116 to 1.0.117", pr_number: 20461, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "12160b10e033e193e6a80e7ac1e1ef7821554a26", date: "2024-05-24 09:18:16 UTC", description: "Bump bufbuild/buf-setup-action from 1.32.0 to 1.32.1", pr_number: 20549, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "fb1af6aa0daec5e2ccf429f15d84d2eb3c7978ed", date: "2024-05-24 09:18:27 UTC", description: "Bump the aws group with 2 updates", pr_number: 20551, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "b5ec6ae8ddbdd37718375d33da42353bd344ee78", date: "2024-05-24 09:18:36 UTC", description: "Bump the prost group with 2 updates", pr_number: 20552, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 18, deletions_count: 18},
		{sha: "72d13735460c1c4bb34ace4f53aa2fc0f7e950db", date: "2024-05-24 10:00:48 UTC", description: "Bump bitmask-enum from 2.2.3 to 2.2.4", pr_number: 20528, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 22, insertions_count: 92, deletions_count: 74},
		{sha: "5abaa3209b5a294e4181e06976c72d5dc700e9f1", date: "2024-05-24 14:09:46 UTC", description: "allow enabling google chronicle separately from…", pr_number: 20557, scopes: ["gcp_chronicle"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 7, deletions_count: 3},
		{sha: "378f3b09b9f1adb8a03a772134e8a56f61fb99cd", date: "2024-05-25 00:10:13 UTC", description: "Move third-party proto files into their own module", pr_number: 20556, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 26, insertions_count: 34, deletions_count: 19},
		{sha: "45be7ad78fad7edbde5b9fe359f3cd674a85b643", date: "2024-05-25 02:02:23 UTC", description: "Replace kafka integration test filter", pr_number: 20534, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2e884c518838e7893c5baaf7d097dbfb8614e28e", date: "2024-05-25 02:45:23 UTC", description: "Update buf config to v2", pr_number: 20558, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 18, deletions_count: 19},
		{sha: "1418455e7119f0ff37a825f0f0ce910b7c3ef6c9", date: "2024-05-25 04:13:28 UTC", description: "Bump syn from 2.0.65 to 2.0.66", pr_number: 20559, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 116, deletions_count: 116},
		{sha: "1b57acd6bb806a09556a8d5cd3a64709c5f44354", date: "2024-05-25 04:13:32 UTC", description: "Bump openssl-src from 300.2.3+3.2.1 to 300.3.0+3.3.0", pr_number: 20546, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "5bd264b57a0b87bf7ef1f34950468db7a5418c7d", date: "2024-05-29 05:04:58 UTC", description: "Bump cargo-nextest", pr_number: 20572, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 5, deletions_count: 8},
		{sha: "7206fa454bb570783f95ae3fb4820adc18a39773", date: "2024-05-29 06:06:43 UTC", description: "Bump databend-client from 0.18.1 to 0.18.2", pr_number: 20569, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "082e80ffaa166529e057ceed9b5993f8beed207f", date: "2024-05-29 10:06:54 UTC", description: "Bump proc-macro2 from 1.0.83 to 1.0.84", pr_number: 20565, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 68, deletions_count: 68},
		{sha: "632ecd7553ce5a5bcfed3f8620e9986ad9056590", date: "2024-05-29 10:07:07 UTC", description: "Bump parking_lot from 0.12.2 to 0.12.3", pr_number: 20564, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "006844e661995fb0fce6e21f7e2b0ffae131bff5", date: "2024-05-29 13:01:49 UTC", description: "Fix parsing-csv-logs-with-lua.md example for postgres > 13", pr_number: 20513, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Alexandre Assouad", files_count: 1, insertions_count: 12, deletions_count: 5},
		{sha: "234b126f733472df87caa4cec23be6e4396c05de", date: "2024-05-29 12:06:21 UTC", description: "Improve NixOS documentation", pr_number: 20497, scopes: ["platforms"], type: "docs", breaking_change: false, author: "Jonathan Davies", files_count: 1, insertions_count: 101, deletions_count: 26},
		{sha: "4a4fc2e9162ece483365959f5222fc5a38d1dad9", date: "2024-05-31 01:12:01 UTC", description: "add arbitrary trait to ElasticsearchApiVersion", pr_number: 20580, scopes: ["elasticsearch"], type: "chore", breaking_change: false, author: "Tess Neau", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "3da355b0bde93ed2afa643c53e32b84ac387fd4f", date: "2024-06-04 06:57:37 UTC", description: "Bump proc-macro2 from 1.0.84 to 1.0.85", pr_number: 20599, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 68, deletions_count: 68},
		{sha: "d1d122e2aa8a29c1d505ca71c9e382eb3ad06691", date: "2024-06-05 12:28:52 UTC", description: "support trace ingestion", pr_number: 19728, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Deen", files_count: 13, insertions_count: 475, deletions_count: 44},
		{sha: "8652efe35f4f384b16cb1b7779fbac8f037cf043", date: "2024-06-05 03:14:50 UTC", description: "Update versions of OSes used for testing the RPM package", pr_number: 20611, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 4},
		{sha: "322c7dfa13fa44280d8d199d96d6cf4c92b73750", date: "2024-06-06 05:36:42 UTC", description: "Make configuration fields public", pr_number: 20614, scopes: ["enrichment_tables"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 138, deletions_count: 108},
		{sha: "8a73968ad0bb728e88c50b60d78d4b11c797213d", date: "2024-06-06 06:51:21 UTC", description: "Bump openssl-src from 300.3.0+3.3.0 to 300.3.1+3.3.1", pr_number: 20612, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "55d48d5ed1668789ec413296e6dd0016489c22b5", date: "2024-06-06 06:51:30 UTC", description: "Bump toml from 0.8.13 to 0.8.14", pr_number: 20608, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "72ba4d5dd544487e2df6bf38f78a427f3d301bd2", date: "2024-06-06 13:51:39 UTC", description: "Bump serde-toml-merge from 0.3.7 to 0.3.8", pr_number: 20607, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7d48cd20a900fe228f6537540e3af650ffd4a4e7", date: "2024-06-06 13:51:48 UTC", description: "Bump the aws group with 2 updates", pr_number: 20606, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "a32a02295d211ac3e57bb597ad33d8d8dc379280", date: "2024-06-06 13:52:46 UTC", description: "Bump rstest from 0.19.0 to 0.21.0", pr_number: 20596, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 6},
		{sha: "d34e6197ffb0589996b32213a1e536547b7d054d", date: "2024-06-06 13:53:08 UTC", description: "Bump tokio from 1.37.0 to 1.38.0", pr_number: 20588, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 15, deletions_count: 15},
		{sha: "62e1cb30d059d389fdabe502ee74b528aad777b1", date: "2024-06-06 13:53:19 UTC", description: "Bump async-compression from 0.4.10 to 0.4.11", pr_number: 20586, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8eff9bcdf10bcda7403a9b05b88ffb5159c9ae53", date: "2024-06-06 13:53:30 UTC", description: "Bump databend-client from 0.18.2 to 0.18.3", pr_number: 20585, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1caa1122d1b77289034d11f90bd9a51c43b50085", date: "2024-06-06 14:38:40 UTC", description: "Bump mock_instant from 0.4.0 to 0.5.1", pr_number: 20598, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "187f1199ecf05d4123f35ec881dd0b85d4f543b1", date: "2024-06-06 21:19:31 UTC", description: "Remove references to HTTP Content-Length from component spec", pr_number: 20615, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 4},
		{sha: "49867fd5c7c3307a0134e605f69d1cf87b137dc5", date: "2024-06-07 10:13:44 UTC", description: "log source IP as `host` key in the `http` source", pr_number: 19667, scopes: ["http"], type: "enhancement", breaking_change: false, author: "Nishant Joshi", files_count: 6, insertions_count: 123, deletions_count: 2},
		{sha: "9575d653d75cda3ba81c0e9d001d9057c80a3775", date: "2024-06-07 04:52:09 UTC", description: "Bump aws-types from 1.3.0 to 1.3.1 in the aws group", pr_number: 20616, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2451cc0eaa2e070426185553474ac670204f9187", date: "2024-06-07 04:52:17 UTC", description: "Bump infer from 0.15.0 to 0.16.0", pr_number: 20597, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "60673c714dcc31e0f44fec00fcfa1a09fe437b8c", date: "2024-06-11 07:31:10 UTC", description: "Bump regex from 1.10.4 to 1.10.5", pr_number: 20629, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "a01c198f8dae6930ddc55d0471e753203f56ecbd", date: "2024-06-11 07:31:25 UTC", description: "Bump enumflags2 from 0.7.9 to 0.7.10", pr_number: 20628, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "843f1864343463630a1a6108e4c53bdece0e115f", date: "2024-06-11 08:52:12 UTC", description: "Bump bufbuild/buf-setup-action from 1.32.1 to 1.32.2", pr_number: 20578, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5b59039b4a4176bd777a5ef3b37dcd1c8ad20e3f", date: "2024-06-11 08:52:24 UTC", description: "Bump serde from 1.0.202 to 1.0.203", pr_number: 20563, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "ceca9a102aa510fbec15f4ad1d56523e1dd51025", date: "2024-06-11 08:52:37 UTC", description: "Bump encoding_rs from 0.8.33 to 0.8.34", pr_number: 20283, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b266a95c881bcefb6b0da4821b3018f93b2c4afb", date: "2024-06-11 08:53:27 UTC", description: "Bump the clap group across 1 directory with 2 updates", pr_number: 20626, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 11, deletions_count: 11},
		{sha: "48a29b3c1363f54998fd3fb2a4cac204b33a1c35", date: "2024-06-11 11:53:50 UTC", description: "add caching to remap vrl compilation", pr_number: 20555, scopes: ["remap transform"], type: "enhancement", breaking_change: false, author: "Zettroke", files_count: 3, insertions_count: 72, deletions_count: 27},
		{sha: "c2fc5ef43a19b803a613e5a3d163da635d45e644", date: "2024-06-11 01:53:59 UTC", description: "Make healthcheck topic configurable", pr_number: 20373, scopes: ["kafka sink"], type: "enhancement", breaking_change: false, author: "yalinglee", files_count: 5, insertions_count: 59, deletions_count: 9},
		{sha: "9bbb991dcc6d47aa2d3bbd06bcba745f1803d9f9", date: "2024-06-11 08:54:21 UTC", description: "Bump docker/build-push-action from 5.3.0 to 5.4.0", pr_number: 20632, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "58114f1345c1fe33807d14f0c8f8ae9087919d19", date: "2024-06-11 06:58:53 UTC", description: "bump rust version to 1.78", pr_number: 20624, scopes: [], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 57, insertions_count: 127, deletions_count: 297},
		{sha: "4f5c99d1afbab12e7d86349f7f269c74fcc7814b", date: "2024-06-11 07:58:45 UTC", description: "Remove `enterprise` feature", pr_number: 20468, scopes: ["enterprise"], type: "chore", breaking_change: true, author: "Bruce Guenter", files_count: 21, insertions_count: 30, deletions_count: 1787},
		{sha: "36abc4578c1b825f5222e8a59fce9c80142cd5be", date: "2024-06-12 03:25:18 UTC", description: "update to crate version to 0.16.0", pr_number: 20634, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 8, insertions_count: 110, deletions_count: 48},
		{sha: "9be2eeb111e8de36937770c55b81c13b1dd7b681", date: "2024-06-12 05:42:27 UTC", description: "Compute proper validate endpoint", pr_number: 20644, scopes: ["datadog sinks"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 44, deletions_count: 34},
		{sha: "f6527ec5869b4502ea1dbb68e85634a331d2eb3d", date: "2024-06-12 09:20:17 UTC", description: "Update sasl2-sys to fix building on GCC 14+ / CLang environments", pr_number: 20645, scopes: ["dev"], type: "chore", breaking_change: false, author: "Scott Balmos", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "3aeaf456cdaece7c05b8ee2c77b57d86cf15c73e", date: "2024-06-12 13:23:04 UTC", description: "Bump clap from 4.5.6 to 4.5.7 in the clap group", pr_number: 20639, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "1a137cf625e7881b02ef287febb80dc4edb244bb", date: "2024-06-12 13:23:13 UTC", description: "Bump the aws group with 3 updates", pr_number: 20638, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 10},
		{sha: "974970e4a62986b56721fb37508c24983935a27e", date: "2024-06-12 13:23:44 UTC", description: "Bump roaring from 0.10.4 to 0.10.5", pr_number: 20630, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9a18b0fd12c02208da4f6edd9067a95ad37ff3d0", date: "2024-06-12 13:23:49 UTC", description: "Bump the graphql group with 2 updates", pr_number: 20627, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 13, deletions_count: 13},
		{sha: "6c8889eba7b313874cd356bb220b437c6b8ba959", date: "2024-06-12 13:24:12 UTC", description: "Bump braces from 3.0.2 to 3.0.3 in /website", pr_number: 20636, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "662d1d09501601a4c7356a4698dc7f3b4dc980d4", date: "2024-06-12 14:10:43 UTC", description: "Bump url from 2.5.0 to 2.5.1", pr_number: 20642, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 296, deletions_count: 6},
		{sha: "30901f08ae5b46a04af94c02a68aebc285638035", date: "2024-06-13 00:23:46 UTC", description: "Fix regex typo in computation of API endpoint", pr_number: 20656, scopes: ["datadog sinks"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 56, deletions_count: 3},
		{sha: "b81118ab581f853791bae9c8d891aeaaa861ee51", date: "2024-06-13 00:13:29 UTC", description: "Allow unicode-3 license", pr_number: 20647, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "ebec09578e49e97019b13fc8297b432e4ae9bd49", date: "2024-06-13 00:35:14 UTC", description: "Correct documentation of algorithms supported by `decrypt`", pr_number: 20658, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 12, deletions_count: 3},
		{sha: "3a03a54bf3121226db74d00dc3adeccbdb4b692e", date: "2024-06-13 01:16:22 UTC", description: "Ensure prometheus::remote_write::Errors is appropriately gated", pr_number: 20657, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "094d2899de9277b713b3b006a036bc1c512c3d5b", date: "2024-06-13 03:39:59 UTC", description: "Bump aws-sigv4 from 1.2.1 to 1.2.2 in the aws group", pr_number: 20650, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "90f8a70aca6f01ee9c893d2ec3ba382c293bf193", date: "2024-06-13 03:40:08 UTC", description: "Bump console-subscriber from 0.2.0 to 0.3.0", pr_number: 20641, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 40, deletions_count: 12},
		{sha: "e8fd823c9eed5df3b05c7cd86a799ce49880d206", date: "2024-06-13 23:03:17 UTC", description: "Bump the aws group with 2 updates", pr_number: 20659, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "7714c684e35c64505101c1dc0afd333b09a7e663", date: "2024-06-14 06:56:47 UTC", description: "Update action for previews", pr_number: 20661, scopes: ["website"], type: "fix", breaking_change: false, author: "Devin Ford", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "0de41f9fc018c74d30333a5c0233aab568f14d4f", date: "2024-06-14 05:36:21 UTC", description: "Update minikube to v1.33.1", pr_number: 20672, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "99d203503896cec835ec0b84d27d72cd38d1dc91", date: "2024-06-14 06:12:27 UTC", description: "Bump Alpine Linux base image to 3.20", pr_number: 20668, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 4, deletions_count: 3},
		{sha: "bcbcd402c1c48b848f397f9ee1d878c25c7ec2f7", date: "2024-06-14 13:12:30 UTC", description: "Bump bufbuild/buf-setup-action from 1.32.2 to 1.33.0", pr_number: 20667, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "d9c16a27bb8ef0ad6abfac662df07d9c895225cf", date: "2024-06-14 06:12:44 UTC", description: "Bump timeout for publishing new environment", pr_number: 20646, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6db92acaa085dbb271b650adc42c2e8b36d53fa3", date: "2024-06-14 15:13:00 UTC", description: "add docs for `psl` argument for `parse_etld`", pr_number: 20542, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 3, insertions_count: 15809, deletions_count: 0},
		{sha: "0e034ee3c52fafb7d81923c3ac1d2050ae5b6358", date: "2024-06-14 09:13:09 UTC", description: "New setting for reduce transform: end_every_period_ms", pr_number: 20440, scopes: ["reduce transform"], type: "enhancement", breaking_change: false, author: "Charles Connell", files_count: 5, insertions_count: 40, deletions_count: 0},
		{sha: "b11ca5d138292e85bba9e110442ee5f1d7a75abf", date: "2024-06-14 23:20:16 UTC", description: "Bump Rust version to 1.79", pr_number: 20670, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 27, insertions_count: 56, deletions_count: 89},
	]
}
