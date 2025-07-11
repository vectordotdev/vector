package metadata

releases: "0.40.0": {
	date:     "2024-07-29"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.40.0!

		Be sure to check out the [upgrade guide](/highlights/2024-07-29-0-40-0-upgrade-guide) for
		breaking changes in this release.

		This release contains a mix of enhancements and bug fixes. See the changelog below.
		"""

	known_issues: [
		"A regression in the `reduce` transform caused it to not group top level objects correctly (see [#21065](https://github.com/vectordotdev/vector/issues/21065)). This is fixed in v0.40.1.",
		"A regression in the `reduce` transform caused it to not reduce events containing keys with special characters correctly (see [#21165](https://github.com/vectordotdev/vector/issues/21165)). This is fixed in v0.40.2.",
	]

	changelog: [
		{
			type: "enhancement"
			description: """
				VRL was updated to v0.17.0. This includes the following changes:

				Breaking Changes & Upgrade Guide

				- `parse_logfmt` now processes 3 escape sequences when parsing: `\n`, `\"` and `\\`.
				  This means that for example, `\n` in the input will be replaced with an actual
				  newline character in parsed keys or values.
				  (https://github.com/vectordotdev/vrl/pull/777)
				"""
		},
		{
			type: "fix"
			description: """
				Proxy configuration now supports URL-encoded values.
				"""
		},
		{
			type: "enhancement"
			description: """
				Improves GcpAuthenticator token regeneration to avoid encountering 401 responses when a tick is missed.
				"""
			contributors: ["garethpelly"]
		},
		{
			type: "enhancement"
			description: """
				Introduced support for decoding InfluxDB line protocol messages, allowing these messages to be deserialized into the Vector metric format.
				"""
			contributors: ["MichaHoffmann", "sebinsunny"]
		},
		{
			type: "enhancement"
			description: """
				The AWS S3 source can now be configured with the optional timeout settings: `sqs.read_timeout_seconds`, `sqs.connect_timeout_seconds`, and `sqs.operation_timeout_seconds`.
				"""
			contributors: ["andjhop"]
		},
		{
			type: "fix"
			description: """
				Batching records for AWS Kinesis Data Streams and AWS Firehose became independent of the partition key, improving efficiency significantly.
				"""
			contributors: ["steven-aerts"]
		},
		{
			type: "enhancement"
			description: """
				The `statsd` source now has a configuration option to disable key sanitization: `sanitize`. The default is `true` to maintain backwards compatibility.
				"""
			contributors: ["to-m"]
		},
		{
			type: "fix"
			description: """
				The kafka source does not deadlock or cause consumer group rebalancing during `vector validate`.
				"""
			contributors: ["jches"]
		},
		{
			type: "enhancement"
			description: """
				Allow the `datadog_agent` source to accept payloads that have been compressed with `zstd`.
				"""
		},
		{
			type: "fix"
			description: """
				Loki sink now drops events with non-parsable timestamps.
				"""
			contributors: ["suikammd"]
		},
		{
			type: "chore"
			description: """
				Now the GELF codec with stream-based sources uses null byte (`\\0`) by default as messages delimiter instead of newline (`\\n`) character. This better matches GELF server behavior.

				### Configuration changes

				In order to maintain the previous behavior, you must set the `framing.method` option to the `character_delimited` method and the `framing.character_delimited.delimiter` option to `\\n` when using GELF codec with stream-based sources.

				### Example configuration change for socket source

				#### Previous

				```yaml
				sources:
				  my_source_id:
				    type: "socket"
				    address: "0.0.0.0:9000"
				    mode: "tcp"
				    decoding:
				      codec: "gelf"
				```

				#### Current

				```yaml
				sources:
				  my_source_id:
				    type: "socket"
				    address: "0.0.0.0:9000"
				    mode: "tcp"
				    decoding:
				      codec: "gelf"
				    framing:
				      method: "character_delimited"
				    character_delimited:
				      delimiter: "\n"
				```
				"""
			breaking: true
			contributors: ["jorgehermo9"]
		},
		{
			type: "chore"
			description: """
				Reduce transforms can now properly aggregate nested fields.

				This is a breaking change because previously, merging object elements used the
				"discard" strategy. The new behavior is to use the default strategy based on the
				element type.

				### Example

				#### Config

				```toml
				group_by = [ "id" ]
				merge_strategies.id = "discard"
				merge_strategies."a.b[0]" = "array"
				```

				#### Event 1

				```json
				{
				  "id": 777,
				  "an_array": [
				    {
				      "inner": 1
				    }
				  ],
				  "message": {
				    "a": {
				      "b": [1, 2],
				      "num": 1
				    }
				  }
				}
				```

				#### Event 2

				```json
				{
				  "id": 777,
				  "an_array": [
				    {
				      "inner": 2
				    }
				  ],
				  "message": {
				    "a": {
				      "b": [3, 4],
				      "num": 2
				    }
				  }
				}
				```

				#### Reduced Event

				Old behavior:

				```json
				{
				  "id": 777,
				  "an_array": [
				    {
				      "inner": 2
				    }
				  ],
				  "message": {
				    "a": {
				      "b": [1, 2],
				      "num": 1
				    }
				  }
				}
				```

				New behavior:

				```json
				{
				  "id": 777,
				  "an_array": [
				    {
				      "inner": 1
				    }
				  ],
				  "message": {
				    "a": {
				      "b": [
				        [1, 2],
				        [3,4]
				      ],
				      "num": 3
				    }
				  }
				}
				```
				"""
			breaking: true
		},
		{
			type: "feat"
			description: """
				Add possibility to use NATS JetStream in NATS sink. Can be turned on/off via `jetstream` option (default is false).
				"""
			contributors: ["whatcouldbepizza"]
		},
		{
			type: "fix"
			description: """
				Fixes sink retry parameter validation to prevent panic at runtime.
				"""
			contributors: ["dhable"]
		},
		{
			type: "enhancement"
			description: """
				The `demo_logs` source now adds `host` (or the configured `log_schema.host_key`) with the value of
				`localhost` to emitted logs.
				"""
		},
		{
			type: "fix"
			description: """
				Templates using strftime format specifiers now correctly use the semantic timestamp rather than
				always looking for the `log_schema` timestamp. This is required when `log_namespacing` is enabled.
				"""
		},
		{
			type: "chore"
			description: """
				Vector no longer supports running on CentOS 7 since it is now end-of-life
				"""
			breaking: true
		},
		{
			type: "enhancement"
			description: """
				The `vector tap` command now has an optional `duration_ms` flag that allows you to specify the duration of the
				tap. By default, the tap will run indefinitely, but if a duration is specified (in milliseconds) the tap will
				automatically stop after that duration has elapsed.
				"""
			contributors: ["ArunPiduguDD"]
		},
		{
			type: "fix"
			description: """
				The `set_secret` and `remove_secret` VRL functions no longer complain about their return value not
				being consumed. These functions don't return any value.
				"""
		},
	]

	commits: [
		{sha: "199f88ae39f5bbcba392907a063dedd30eb79235", date: "2024-06-15 01:26:33 UTC", description: "Bump cue to 0.9.1", pr_number: 20666, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 6, deletions_count: 3},
		{sha: "b3276b4cc73dee6d3854469562f1b1fcf15a419c", date: "2024-06-15 04:38:05 UTC", description: "Run cue fmt with 0.9.0", pr_number: 20678, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 87, insertions_count: 8682, deletions_count: 8708},
		{sha: "146de92dfd90d20451308d77f4da0c8afc3a4e45", date: "2024-06-18 03:36:54 UTC", description: "make multiple modules public", pr_number: 20683, scopes: ["elasticsearch"], type: "chore", breaking_change: false, author: "Tess Neau", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "9084efa434885d2618656713390e12c9a30bf3d6", date: "2024-06-18 02:03:20 UTC", description: "Bump k8s manifests to v0.34.0 of the chart", pr_number: 20686, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "21c3e68bfd2406e703b6587ed474e4e08a3c9c0f", date: "2024-06-19 12:22:17 UTC", description: "Bump curve25519-dalek from 4.1.1 to 4.1.3", pr_number: 20692, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 10},
		{sha: "9ac371d8543c1dc80db1de4c509a767b78783c64", date: "2024-06-21 23:58:48 UTC", description: "Bump proc-macro2 from 1.0.85 to 1.0.86", pr_number: 20704, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 74, deletions_count: 74},
		{sha: "7f4a4c2436f75b601c42f694a8d8f3086d994e52", date: "2024-06-21 23:59:10 UTC", description: "Bump aws-types from 1.3.1 to 1.3.2 in the aws group", pr_number: 20702, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 4},
		{sha: "0cd2710f500cd39fb807bfe1451bb9ca4d1b2952", date: "2024-06-22 03:59:41 UTC", description: "Bump clap_complete from 4.5.5 to 4.5.6 in the clap group", pr_number: 20700, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "064c77bd11632de654207ce3dec8273f936566f7", date: "2024-06-22 03:59:58 UTC", description: "Bump dashmap from 5.5.3 to 6.0.0", pr_number: 20696, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 20, deletions_count: 6},
		{sha: "b99ffcba01d6d6997d92e13bf0e65613d5ae0c92", date: "2024-06-22 04:00:07 UTC", description: "Bump cargo_toml from 0.20.2 to 0.20.3", pr_number: 20695, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4918991f9711f3f7587c35768083efe2c3f44f37", date: "2024-06-22 04:03:42 UTC", description: "Bump memchr from 2.7.2 to 2.7.4", pr_number: 20673, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "52759aa7bf9ec0785e186b978adcb0a6103cc70a", date: "2024-06-22 04:46:26 UTC", description: "Bump mlua from 0.9.8 to 0.9.9", pr_number: 20693, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 9, deletions_count: 9},
		{sha: "c8be561b5c4f3e9778275339d4a81de2a7abd02f", date: "2024-06-26 00:33:12 UTC", description: "Remove reference to the deleted workload checks workflow", pr_number: 20734, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 8},
		{sha: "d0dcf356ed922917179451a6920b389cadd0768e", date: "2024-06-26 02:58:18 UTC", description: "Restrict comment trigger workflow to repository", pr_number: 20736, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "37e0c1dc5c532d8dc3b4c175566663c057158c2a", date: "2024-06-26 04:37:32 UTC", description: "Validate PR author instead of repository", pr_number: 20741, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 9, deletions_count: 3},
		{sha: "099b04304ccc5e7f69cae991edc18161ae4e2d0c", date: "2024-06-26 06:39:51 UTC", description: "Restrict integration comment workflow to PRs from maintainers", pr_number: 20743, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "476016b28890df879789c5408dfab5c4eb80c33e", date: "2024-06-26 23:09:58 UTC", description: "consumer subscribe in main kafka source task", pr_number: 20698, scopes: ["kafka source"], type: "fix", breaking_change: false, author: "j chesley", files_count: 2, insertions_count: 9, deletions_count: 2},
		{sha: "3de6f0b8175c8a08da1974d6b6e9634075244a06", date: "2024-06-29 02:21:20 UTC", description: "Add `host` to emitted logs", pr_number: 20754, scopes: ["demo_logs source"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 27, deletions_count: 0},
		{sha: "951d726a70df7a2045524e7ef9517fdc6423d9f2", date: "2024-07-02 18:47:07 UTC", description: "Update the base image used for x86_64 artifacts to Ubuntu 16.04", pr_number: 20765, scopes: ["releasing"], type: "chore", breaking_change: true, author: "Jesse Szwedko", files_count: 4, insertions_count: 4, deletions_count: 18},
		{sha: "be6b42ba1d4fe86b12b1f395747188b2b786b0fd", date: "2024-07-02 23:21:22 UTC", description: "Bump serde_json from 1.0.117 to 1.0.120", pr_number: 20766, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a6f82fb740bc3d763581b906df6cfc777c4c338b", date: "2024-07-02 23:22:26 UTC", description: "Bump ordered-float from 4.2.0 to 4.2.1", pr_number: 20758, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 13, deletions_count: 13},
		{sha: "29d8dbef8bad0531713981517a6a50600c3461df", date: "2024-07-03 03:22:38 UTC", description: "Bump the clap group with 2 updates", pr_number: 20757, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 10},
		{sha: "9bbf21fc44c0ecdcb62c8d5e0346a6b26705d7a0", date: "2024-07-03 03:23:29 UTC", description: "Bump actions/add-to-project from 1.0.1 to 1.0.2", pr_number: 20738, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e1c08014fc5decec32ff8f26e51423dd11491d91", date: "2024-07-03 03:23:48 UTC", description: "Bump serde_bytes from 0.11.14 to 0.11.15", pr_number: 20729, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e982f6679e9d1526d856efa462e728774b52cf34", date: "2024-07-03 03:23:57 UTC", description: "Bump dashmap from 6.0.0 to 6.0.1", pr_number: 20728, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "34e35c6633a6031bce7d440d6a9646d4332fadd2", date: "2024-07-03 04:04:46 UTC", description: "Bump serde_with from 3.8.1 to 3.8.2", pr_number: 20760, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 27, deletions_count: 18},
		{sha: "b2d3fe18d13bc4c5160e41810f87da09308967f6", date: "2024-07-03 04:10:19 UTC", description: "Bump url from 2.5.1 to 2.5.2", pr_number: 20694, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 296},
		{sha: "73dd278b89b3835a3a393065db4b51fed94d1cce", date: "2024-07-03 04:21:04 UTC", description: "Clarify the removal of the `enterprise` configuration in 0.39.0", pr_number: 20772, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 125, deletions_count: 125},
		{sha: "361a32b16de67c7b67587fa5d2e0c8d2ca8b6667", date: "2024-07-03 07:05:50 UTC", description: "Add a note to CONTRIBUTING.md about running clippy", pr_number: 20775, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "d778c6f016f862a2d05ba8fda435d0c9cceb2715", date: "2024-07-03 07:44:04 UTC", description: "Run `cue fmt`", pr_number: 20777, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "def8b7f251ec60acc4ec673f5c6acc4065e99e18", date: "2024-07-03 13:01:54 UTC", description: "enable zstd decompression", pr_number: 20732, scopes: ["datadog_agent source"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 8, deletions_count: 0},
		{sha: "b256ba283270e5996a11231de5c4f4f3775ed882", date: "2024-07-04 03:14:37 UTC", description: "Bump docker/build-push-action from 5.4.0 to 6.2.0", pr_number: 20748, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0088883e0c21a15e0228d2dd53d94494a382c21d", date: "2024-07-04 04:57:22 UTC", description: "Revert \"Add trailing slash to aws endpoint examples\"", pr_number: 20791, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 12, insertions_count: 24, deletions_count: 24},
		{sha: "8834741a4dc182618a5c7801782814d5923d1507", date: "2024-07-04 13:58:14 UTC", description: "fix batching of requests #20575 #1407", pr_number: 20653, scopes: ["aws_kinesis sink"], type: "fix", breaking_change: false, author: "Steven Aerts", files_count: 2, insertions_count: 6, deletions_count: 35},
		{sha: "e144ac674973649dd786ec2be4b4bad4bea17163", date: "2024-07-04 16:58:59 UTC", description: "add option to disable key sanitization", pr_number: 20717, scopes: ["statsd source"], type: "feat", breaking_change: false, author: "to-m", files_count: 6, insertions_count: 217, deletions_count: 117},
		{sha: "bfcf95b4eeeb67cd50364d7e684903419c7de9b8", date: "2024-07-08 23:11:37 UTC", description: "Regenerate component docs", pr_number: 20789, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "8325300a767bf342f455ad796fe01c9722bb8fd9", date: "2024-07-09 10:02:13 UTC", description: "replace // with / when merging base url and object key", pr_number: 20810, scopes: ["gcs"], type: "fix", breaking_change: false, author: "Vladimir Zhuk", files_count: 1, insertions_count: 35, deletions_count: 3},
		{sha: "28b7e35bf9cbb105d1fecc18fc852f78b78f0c5b", date: "2024-07-09 06:51:22 UTC", description: "Fix link to unit tests", pr_number: 20819, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "bf916643212c542059513b265d2344e01540ebf3", date: "2024-07-09 12:26:15 UTC", description: "Use NodeJS v16 for package verification workflows", pr_number: 20818, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "c448b237e0c5ac649e538230a7f85cb1a9b6bab5", date: "2024-07-09 23:24:11 UTC", description: "Mark file_to_blackhole soak as erratic", pr_number: 20822, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "43cfd5a316b2a392241270aeb7bf14186f88b066", date: "2024-07-09 23:29:12 UTC", description: "bump VRL to 0.16.1", pr_number: 20821, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "0953a90118474e3affd09dc91f886e32ddcf643b", date: "2024-07-10 01:45:30 UTC", description: "Increases default warmup duration for Regression Detector jobs", pr_number: 20828, scopes: [], type: "chore", breaking_change: false, author: "Scott Opell", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "2d9b1c434478dc969a93028e7b21b0401d0c6d74", date: "2024-07-10 03:30:43 UTC", description: "reduce values for nested fields", pr_number: 20800, scopes: ["reduce"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 349, deletions_count: 88},
		{sha: "60552ab7c7e6ec0569115c01e3caadbc27d46647", date: "2024-07-10 03:59:12 UTC", description: "Adds a trailing newline escape for smp job submit", pr_number: 20829, scopes: [], type: "chore", breaking_change: false, author: "Scott Opell", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "fce0fbfddc08c1334cabd9ca2a48947d9d38eeb3", date: "2024-07-10 05:15:41 UTC", description: "Mark set_secret and remove_secret as impure", pr_number: 20820, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 4, deletions_count: 2},
		{sha: "a6f45862049def7f4ab52c97bdfb3a9ab28a0e47", date: "2024-07-10 06:33:06 UTC", description: "Bump proptest from 1.4.0 to 1.5.0", pr_number: 20720, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 7, deletions_count: 7},
		{sha: "fadf0a903d35102abbfa24ab40a0e0299c41d952", date: "2024-07-10 06:40:23 UTC", description: "Bump syn from 2.0.66 to 2.0.70", pr_number: 20826, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 48, deletions_count: 48},
		{sha: "ecf3762bb292f12199656b9e2700d872fb2bf691", date: "2024-07-10 10:41:10 UTC", description: "Bump serde from 1.0.203 to 1.0.204", pr_number: 20804, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "4307dad3b567373116d13bd5ee5330e7bde23b2d", date: "2024-07-10 10:41:32 UTC", description: "Bump serde_with from 3.8.2 to 3.8.3", pr_number: 20795, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "5a54444dcc7bc809b79cd7fa7e047f41f1cdcd62", date: "2024-07-10 10:41:41 UTC", description: "Bump docker/build-push-action from 6.2.0 to 6.3.0", pr_number: 20788, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "84d87f4041343cf9f77bb26abf3ed93ce19f0964", date: "2024-07-10 10:42:23 UTC", description: "Bump ratatui from 0.26.3 to 0.27.0", pr_number: 20730, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 10},
		{sha: "ef218fa0c922b4d2048f02f887fd34386fc52b8d", date: "2024-07-10 10:42:37 UTC", description: "Bump bufbuild/buf-setup-action from 1.33.0 to 1.34.0", pr_number: 20723, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0a72db7b74f935f4040d18273d4a78dedc25c5a1", date: "2024-07-10 11:27:24 UTC", description: "Bump uuid from 1.8.0 to 1.9.1", pr_number: 20727, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 11},
		{sha: "e3a2abde44a0999f4b9c20ea811f14f0c9b9a71d", date: "2024-07-11 03:50:45 UTC", description: "Bump async-trait from 0.1.80 to 0.1.81", pr_number: 20803, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "25441075689e044f911c620baff86825f1f6f017", date: "2024-07-11 03:51:00 UTC", description: "Bump docker/setup-qemu-action from 3.0.0 to 3.1.0", pr_number: 20787, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "e52f312d208dc1b2f49127a8a5786cdf9f9b5912", date: "2024-07-11 00:14:04 UTC", description: "Update documentation for community_id to mention ICMP", pr_number: 20677, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "6680b15d9e4018f5ec7967910a6737d45bee4029", date: "2024-07-11 00:29:32 UTC", description: "Run component docs check if component cue files change", pr_number: 20793, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "1579627ee3c63347fa68fe8ab3e0105bbe6ff3d9", date: "2024-07-11 05:20:20 UTC", description: "Add duration flag to vector tap", pr_number: 20815, scopes: ["cli"], type: "feat", breaking_change: false, author: "ArunPiduguDD", files_count: 4, insertions_count: 55, deletions_count: 23},
		{sha: "be37a33a2e6294051e86ad50674e2857fb188985", date: "2024-07-17 07:35:30 UTC", description: "Refactor vector tap into library", pr_number: 20850, scopes: ["cli"], type: "chore", breaking_change: false, author: "ArunPiduguDD", files_count: 7, insertions_count: 293, deletions_count: 175},
		{sha: "7916ad5c55a8ad0265d58429272a9c0d1e6c9c2c", date: "2024-07-17 10:29:11 UTC", description: "Temporarily disable eventstoredb integration tests", pr_number: 20869, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 10, deletions_count: 11},
		{sha: "6d179e523164d1e2332ac644746104bbdfdfed22", date: "2024-07-18 11:10:53 UTC", description: "Fix loki event timestamp out of range panic", pr_number: 20780, scopes: ["loki sinks"], type: "fix", breaking_change: false, author: "Suika", files_count: 4, insertions_count: 67, deletions_count: 2},
		{sha: "e9b4fe15b5a84ee9cd2459a70ca92e6c634d519a", date: "2024-07-17 23:18:06 UTC", description: "support url-encoded auth values", pr_number: 20868, scopes: ["proxy"], type: "fix", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 10, deletions_count: 3},
		{sha: "ef4f1752b40af6a405df718320feff55e042e0b4", date: "2024-07-18 06:57:52 UTC", description: "Implement async output channel type for vector-tap lib", pr_number: 20876, scopes: ["tap"], type: "feat", breaking_change: false, author: "ArunPiduguDD", files_count: 4, insertions_count: 213, deletions_count: 63},
		{sha: "103240846e93b48213cda50e0025c756d217647c", date: "2024-07-19 07:16:31 UTC", description: "Fix and reenable eventstoredb integration test", pr_number: 20873, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 68, deletions_count: 13},
		{sha: "037229e717b6c25acbcd4d485ee13465be1cb073", date: "2024-07-20 03:52:00 UTC", description: "Bump databend-client from 0.18.3 to 0.19.3", pr_number: 20887, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4ec2b4c2d5e5e363ee687b0914d56d8083ebb7e8", date: "2024-07-20 03:52:09 UTC", description: "Bump thiserror from 1.0.61 to 1.0.63", pr_number: 20880, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "46100625a7007f6c791be5bb5fd2aeee6cb59f8f", date: "2024-07-20 07:52:26 UTC", description: "Bump toml from 0.8.14 to 0.8.15", pr_number: 20881, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "7c56a11cffdc8a26cda0389e3052e5ef898bafe7", date: "2024-07-20 07:52:55 UTC", description: "Bump docker/build-push-action from 6.3.0 to 6.4.1", pr_number: 20877, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "22141b725c125daa5558b7ab5c41d5f5b5a7defa", date: "2024-07-20 07:53:25 UTC", description: "Bump lapin from 2.3.4 to 2.4.0", pr_number: 20870, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 14, deletions_count: 14},
		{sha: "4d6e44f28285a44f50ecefed949dd7c4fd8a86dc", date: "2024-07-20 07:53:34 UTC", description: "Bump serde_with from 3.8.3 to 3.9.0", pr_number: 20857, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "535bd0ded17f4ac21a2a06918471010ebe46a77d", date: "2024-07-20 07:53:41 UTC", description: "Bump tikv-jemallocator from 0.5.4 to 0.6.0", pr_number: 20856, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "9771a45ba69418612910c4e58239c12d444fae4d", date: "2024-07-20 07:53:52 UTC", description: "Bump the graphql group with 2 updates", pr_number: 20854, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 15, deletions_count: 15},
		{sha: "d1799d8f3ed6cbe7e9ef17c28883a2907319ff74", date: "2024-07-20 07:54:01 UTC", description: "Bump the clap group across 1 directory with 2 updates", pr_number: 20841, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "3d630d99b907b80c2c4522a2d404fd333e6f2283", date: "2024-07-20 07:54:04 UTC", description: "Bump docker/setup-buildx-action from 3.3.0 to 3.4.0", pr_number: 20838, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "9260083004431f882400faca4fa355a26acbbc0f", date: "2024-07-20 07:54:13 UTC", description: "Bump the aws group across 1 directory with 4 updates", pr_number: 20832, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 11},
		{sha: "81211bab72a607d83ee9b494e8628e3764d239a4", date: "2024-07-20 07:54:17 UTC", description: "Bump roaring from 0.10.5 to 0.10.6", pr_number: 20781, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "0c1d3b1ca1250e73237ba7b9e20218c2e53cdfad", date: "2024-07-20 07:54:22 UTC", description: "Bump log from 0.4.21 to 0.4.22", pr_number: 20751, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b6792451711511585d33db991a446765f1d4d723", date: "2024-07-20 06:35:16 UTC", description: "For templates using stftime specifiers use semantic timestamp", pr_number: 20851, scopes: ["config"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 32, deletions_count: 7},
		{sha: "8ffc0db2e58901e8f2cc791baf5e1dd9219ba290", date: "2024-07-20 06:42:50 UTC", description: "add comment to the configuration because it conflicts with what is mentioned in the helm deployment docs", pr_number: 19523, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Patrick Carney", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "fd35e1c54c33e297cdfd48ecfeae344947f303fb", date: "2024-07-20 13:00:59 UTC", description: "Fix link to VRL playground in SUPPORT.md", pr_number: 20846, scopes: [], type: "docs", breaking_change: false, author: "Matthijs Kooijman", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b2aea48a374259fd289f3ee3bc9a23eb0446b025", date: "2024-07-22 21:47:31 UTC", description: "Bump metrics from 0.21.1 to 0.22.0", pr_number: 19463, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 96, insertions_count: 857, deletions_count: 721},
		{sha: "cd6069795de7d735b366b52862b4d639a68223b8", date: "2024-07-23 04:30:59 UTC", description: "Bump openssl from 0.10.64 to 0.10.66", pr_number: 20896, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "67a5e4682138721575b22369cf5ba629cd86ce55", date: "2024-07-23 00:01:14 UTC", description: "Add helper function to add a semantic meaning to an event metadata", pr_number: 20439, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 22, deletions_count: 1},
		{sha: "3b6a73845ed5bbd6be12e36b113127db589196f0", date: "2024-07-23 11:18:08 UTC", description: "Reinstall rustup in MacOS bootstrap", pr_number: 20911, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "7685b6f1306350d610370dbd7c8cc36034102454", date: "2024-07-24 04:01:28 UTC", description: "Bump wiremock from 0.5.22 to 0.6.1", pr_number: 20908, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 16},
		{sha: "587009971ed6fcbbc915c4440255e3a262bdbd4f", date: "2024-07-24 04:01:45 UTC", description: "Bump docker/build-push-action from 6.4.1 to 6.5.0", pr_number: 20907, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2c75df4f829970d248441b1201d7272e296d9e15", date: "2024-07-24 04:01:57 UTC", description: "Bump docker/setup-qemu-action from 3.1.0 to 3.2.0", pr_number: 20905, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "6a77cc3ff72f9016a0816f7a347b1bb8eb056742", date: "2024-07-24 00:40:29 UTC", description: "Clarify behavior of sources when any sink has acks enabled", pr_number: 20910, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 57, insertions_count: 114, deletions_count: 114},
		{sha: "fcafdd2c4e0bd847796bd342059c8840520de132", date: "2024-07-24 07:40:57 UTC", description: "Bump tokio-postgres from 0.7.10 to 0.7.11", pr_number: 20900, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "3a172066c8e30438308e5bb56f2abd6566d5cdba", date: "2024-07-24 07:41:09 UTC", description: "Bump cargo_toml from 0.20.3 to 0.20.4", pr_number: 20899, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ab497bf5b9bfbd43d976c14c0b5cb2872e725396", date: "2024-07-24 07:41:21 UTC", description: "Bump nkeys from 0.4.1 to 0.4.2", pr_number: 20898, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "16884c394f73e6d6b12b2ce3b73f26c3e496258d", date: "2024-07-24 07:41:54 UTC", description: "Bump aws-types from 1.3.2 to 1.3.3 in the aws group", pr_number: 20894, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5256de14796c29a1cbe116495215cd61fbec17b7", date: "2024-07-24 02:42:04 UTC", description: "disallow zero values for sink retry parameters", pr_number: 20891, scopes: ["config"], type: "fix", breaking_change: false, author: "Dan Hable", files_count: 2, insertions_count: 14, deletions_count: 11},
		{sha: "77ea00f558d6b69fab9d13457d62d8f70c481a5a", date: "2024-07-24 09:42:30 UTC", description: "Fix link to commit scopes in CONTRIBUTING.md", pr_number: 20847, scopes: [], type: "docs", breaking_change: false, author: "Matthijs Kooijman", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "dea7e2bf3e69cd297d56c313363566d540c05416", date: "2024-07-24 07:42:31 UTC", description: "Bump bufbuild/buf-setup-action from 1.34.0 to 1.35.0", pr_number: 20915, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "18fad409ec4f01841ec98c705a955ff3276b2104", date: "2024-07-24 07:49:00 UTC", description: "Bump docker/setup-buildx-action from 3.4.0 to 3.5.0", pr_number: 20906, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "787e1ec3a3c5d89e6fb3fd28201b847d873c78f5", date: "2024-07-24 09:56:01 UTC", description: "Bump databend-client from 0.19.3 to 0.19.5", pr_number: 20913, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "56bd0dd57e76c4bb0f4ad43370e44ed74f962a47", date: "2024-07-25 04:55:09 UTC", description: "Add possibility to use nats jetstream in nats sink", pr_number: 20834, scopes: ["new sink"], type: "feat", breaking_change: false, author: "Dmitriy", files_count: 7, insertions_count: 101, deletions_count: 10},
		{sha: "3fbb66c8c94f9d95986e4a695a21681736025c93", date: "2024-07-25 06:08:37 UTC", description: "influxdb line protcol decoder", pr_number: 19637, scopes: ["codec", "sources"], type: "feat", breaking_change: false, author: "Michael Hoffmann", files_count: 27, insertions_count: 655, deletions_count: 2},
		{sha: "621f843c08b420c4b8ad708ef60c0192bf8d9361", date: "2024-07-25 01:09:28 UTC", description: "Bump env_logger from 0.11.3 to 0.11.4", pr_number: 20918, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "12f37413c5aab79f24a912ef5cc28396123b1cb8", date: "2024-07-25 08:12:17 UTC", description: "Bump bytes from 1.6.0 to 1.6.1", pr_number: 20855, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 89, deletions_count: 89},
		{sha: "a4124ce1354b1476ae9cd60cd5cf2004a3ce6f63", date: "2024-07-25 08:12:20 UTC", description: "Bump uuid from 1.9.1 to 1.10.0", pr_number: 20842, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "930006c26e2989182feb939772a843b3592e25da", date: "2024-07-25 08:55:51 UTC", description: "Bump tokio from 1.38.0 to 1.39.1", pr_number: 20916, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 12, insertions_count: 37, deletions_count: 26},
		{sha: "84692d489fafa83b994be8326a281d7cd176c6de", date: "2024-07-25 10:40:09 UTC", description: "Bump the clap group with 2 updates", pr_number: 20917, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "70eb470b647b703472bbdd26b173203f6855d5fc", date: "2024-07-25 04:27:36 UTC", description: "Upgrade VRL to v0.17.0", pr_number: 20922, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e89661ce9866b2f2ad5d80303144db1abdccd5bc", date: "2024-07-25 12:04:05 UTC", description: "Bump syn from 2.0.70 to 2.0.72", pr_number: 20895, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
	]
}
