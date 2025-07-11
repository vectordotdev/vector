package metadata

releases: "0.26.0": {
	date:     "2022-11-30"
	codename: ""

	whats_next: []

	known_issues: [
		"""
			Annotating namespace labels in the `kubernetes_logs` source cannot be disabled by
			setting `namespace_labels` to `""`. Fixed in v0.27.0.
			""",
		"""
			The `log_schema` config options do not allow configuration of nested paths using `.`s.
			Instead it treats them as flat. Fixed in v0.27.1.
			""",
	]

	description: """
		The Vector team is pleased to announce version 0.26.0!

		Be sure to check out the [upgrade guide](/highlights/2022-11-07-0-26-0-upgrade-guide) for
		breaking changes in this release.

		This is a smaller release primarily including bug fixes and small enhancements.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["source: mongodb_metrics"]
			description: """
				The `mongodb_metrics` sink now uses 64 bit integers rather than 32 bit integers for
				all integer metric values to avoid overflow issues.
				"""
			pr_numbers: [14628]
			contributors: ["KernelErr"]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				The VRL `parse_key_value` function now allows tabs to be used as the delimiter.
				"""
			pr_numbers: [14790]
		},
		{
			type: "fix"
			scopes: ["sources: kafka"]
			description: """
				The `kafka` source now flushes commits when a rebalance event occurs. This avoids
				issues with committing offsets to partitions the Vector instance no longer is
				consuming.
				"""
			pr_numbers: [10434]
		},
		{
			type: "fix"
			scopes: ["sinks: aws_s3"]
			description: """
				Template support was added for the `aws_s3` `ssekms_key_id` parameter to template
				this value based an event field.
				"""
			pr_numbers: [14639]
			contributors: ["fluetm"]
		},
		{
			type: "chore"
			scopes: ["transforms: route"]
			breaking: true
			description: """
				The deprecated `swimlane` alias for the `route` transform was removed.

				See [the upgrade
				guide](/highlights/2022-11-07-0-26-0-upgrade-guide#lanes-parameter-route-transform-removal)
				for more details.
				"""
			pr_numbers: [14903]
		},
		{
			type: "chore"
			scopes: ["transforms: route"]
			breaking: true
			description: """
				The deprecated `check_fields` condition type was removed.

				See [the upgrade
				guide](/highlights/2022-11-07-0-26-0-upgrade-guide#check-fields-removal)
				for more details.
				"""
			pr_numbers: [14903]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				Vector's internal telemetry for bytes processed by components was updated to be an
				estimate of the number of bytes in each event's JSON representation. This is
				expected to give a more accurate and consistent measure of bytes flowing across
				components. This affects:

				- `component_received_event_bytes_total`
				- `component_sent_event_bytes_total`

				These used to measure the number of bytes of the in-memory representation of the
				event, but we found that this measure was not particularly useful for users.
				"""
			pr_numbers: [14139]
		},
		{
			type: "chore"
			scopes: ["transforms: tag_cardinality_limit"]
			description: """
				A fix for the `tag_cardinality_limit` transform removes a case where it would
				inadvertently drop events that shouldn't have been dropped, due to differences in
				the order tags appear on incoming events.
				"""
			pr_numbers: [14889]
		},
		{
			type: "enhancement"
			scopes: ["config"]
			description: """
				Vector now validates field paths specified in configuration (for example in
				templates) at compile time, to return an error if the path is invalid.
				Previously invalid field paths would be silently ignored at runtime.
				"""
			pr_numbers: [14887]
		},
		{
			type: "fix"
			scopes: ["file"]
			description: """
				The `file` source now fails to start if `include` option is not specified. The
				option was always required by the source, but no warnings or errors were being
				emitted if the option was not specified.
				"""
			pr_numbers: [14956]
		},
		{
			type: "feat"
			scopes: ["sinks: elasticsearch"]
			description: """
				The `elasticsearch` sink now supports an
				[`api_version`](/docs/reference/configuration/sinks/elasticsearch/#api_version)
				option to specify the API version the targeted Elasticsearch instance exposes. This
				replaces and deprecates the `suppress_type_name` option which was previously used
				for controlling Elasticsearch version compatibility.

				It can be set to `auto` to attempt to automatically determine the Elasticsearch
				version by querying the Elasticsearch version endpoint.
				"""
			pr_numbers: [14918, 15082]
			contributors: ["ktff"]
		},
		{
			type: "feat"
			scopes: ["vrl: stdlib", "vrl"]
			description: """
				A new [`decode_mime_q`](/docs/reference/vrl/functions/#decode_mime_q) function was
				added to VRL to decode data in
				[`encoded-word`](https://datatracker.ietf.org/doc/html/rfc2047#section-2) format.
				"""
			pr_numbers: [14813]
			contributors: ["ktff"]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				The performance of VRL programs that use regular expression matching was improved.
				"""
			pr_numbers: [15079]
		},
		{
			type: "fix"
			scopes: ["file"]
			description: """
				The `file` source no longer repeats source lines across restarts when line
				aggregation is being used.
				"""
			pr_numbers: [14858]
			contributors: ["jches"]
		},
		{
			type: "fix"
			scopes: ["vrl: stdlib", "vrl"]
			description: """
				The `parse_cef` VRL function now correctly handles UTF-8 escape characters.
				"""
			pr_numbers: [15092]
			contributors: ["ktff"]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Vector now outputs an error message when starting with an invalid thread count (0)
				rather than just exiting silently.
				"""
			pr_numbers: [15111]
			contributors: ["zamazan4ik"]
		},
		{
			type: "fix"
			scopes: ["sinks: blackhole"]
			description: """
				The `blackhole` sink now emits the `component_sent_bytes_total` metric.
				"""
			pr_numbers: [15109]
			contributors: ["zamazan4ik"]
		},
		{
			type: "feat"
			scopes: ["sinks: splunk_hec"]
			description: """
				The `splunk_hec_logs` sink has a new
				[`auto_extract_timestamp`](/docs/reference/configuration/sinks/splunk_hec_logs/#auto_extract_timestamp)
				option to tell Splunk to parse the timestamp out of the message rather than Vector
				sending a timestamp.
				"""
			pr_numbers: [15261]
		},
		{
			type: "fix"
			scopes: ["sources: aws_s3"]
			description: """
				The `aws_s3` source now correctly ignores `s3:TestEvent` messages. This was meant to
				be included in v0.25.0 but there was an issue in the deserialization implementation.
				"""
			pr_numbers: [15331]
			contributors: ["nrhtr"]
		},
	]

	commits: [
		{sha: "bd0cab24cfc25aa19d499b7b703f9d2d7514f3f5", date: "2022-10-06 09:17:31 UTC", description: "bump arbitrary from 1.1.6 to 1.1.7", pr_number: 14738, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "6e3beccf4eac42601286e674c1e61dacb83b4d0b", date: "2022-10-06 14:32:40 UTC", description: "Add pacman / Arch Linux", pr_number: 14716, scopes: ["setup"], type: "docs", breaking_change: false, author: "Justin Kromlinger", files_count: 15, insertions_count: 69, deletions_count: 15},
		{sha: "d33e1833b7132399b986a9fc3140b3c84071fba1", date: "2022-10-07 00:53:31 UTC", description: "Correct multiple issues with APM stats calculation", pr_number: 14694, scopes: ["datadog_traces sink"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 4, insertions_count: 222, deletions_count: 40},
		{sha: "a9b8099e29fb0cdd7fb92b8dbf36d93d7fe637f6", date: "2022-10-07 03:12:38 UTC", description: "Add MVP web UI to run VRL program without access to browser console", pr_number: 14727, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Jonathan Padilla", files_count: 3, insertions_count: 172, deletions_count: 28},
		{sha: "5e34065f5ec2247774d67fd381147e4e3420f98f", date: "2022-10-07 22:18:25 UTC", description: "bump tracing-subscriber from 0.3.15 to 0.3.16", pr_number: 14759, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 97, deletions_count: 81},
		{sha: "77fc20a04a0a7f20c694e2b1ad2f7b7b406bb93d", date: "2022-10-07 22:18:57 UTC", description: "bump syn from 1.0.101 to 1.0.102", pr_number: 14760, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "c3037991f031afc4976d6ea9d7060880d1f359c0", date: "2022-10-08 03:48:57 UTC", description: "bump async-graphql from 4.0.14 to 4.0.15", pr_number: 14766, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 12},
		{sha: "614c441660444f1d82e4f9e4c7a772f45131a8a6", date: "2022-10-08 04:01:07 UTC", description: "bump smallvec from 1.9.0 to 1.10.0", pr_number: 14677, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "57b7d96bc02b531e9dcff1ecd27abb1f7c07b2e0", date: "2022-10-08 06:57:43 UTC", description: "Comply to `*EventsDropped` instrumentation spec in `splunk_hec_metrics` sink", pr_number: 14514, scopes: ["splunk_hec_metrics sink"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "29672f4b3a2fd407527760e312591b80d7862f27", date: "2022-10-08 05:46:36 UTC", description: "bump pest from 2.3.1 to 2.4.0", pr_number: 14678, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0567e9fe55b8fa6e3fb63f482127cb98d75a9758", date: "2022-10-08 04:04:45 UTC", description: "bump test-case from 2.2.1 to 2.2.2", pr_number: 14681, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "88444f047ccde225f0f9f1ffeb461f99f78c7edf", date: "2022-10-08 12:07:40 UTC", description: "bump clap from 4.0.9 to 4.0.10", pr_number: 14746, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "7e37dc17ad186c2b7e252e5cb3d55391041313d8", date: "2022-10-08 12:11:11 UTC", description: "bump ordered-float from 3.1.0 to 3.2.0", pr_number: 14680, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 19, deletions_count: 19},
		{sha: "68aae832298e0b373a38e6faeb77ee213c370f15", date: "2022-10-08 12:14:32 UTC", description: "bump schemars from 0.8.10 to 0.8.11", pr_number: 14679, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "538ab7c384b46b6cfc200657dc6a78f2610929e2", date: "2022-10-12 11:15:15 UTC", description: "Change all i32 metrics to i64", pr_number: 14628, scopes: ["mongodb_metrics source"], type: "fix", breaking_change: false, author: "Rui Li", files_count: 1, insertions_count: 71, deletions_count: 71},
		{sha: "9190eb6f46d82f9e0541be1d6c5effd3b4d89fbf", date: "2022-10-12 04:24:32 UTC", description: "allow tabs to be used as delimiter in parse_key_value", pr_number: 14790, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 8, deletions_count: 2},
		{sha: "a157ce0ff354c1c149ee81dc0582e0a92f312e4b", date: "2022-10-12 04:30:22 UTC", description: "bump async-graphql-warp from 4.0.14 to 4.0.15", pr_number: 14780, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "58e7416944fa9b5d68e82e1c2d297af827b335a7", date: "2022-10-12 04:31:09 UTC", description: "bump pest_derive from 2.3.1 to 2.4.0", pr_number: 14783, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "044d3637d27434ce296899e72ded6479a7f8086b", date: "2022-10-12 04:31:21 UTC", description: "bump snafu from 0.7.1 to 0.7.2", pr_number: 14784, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 10, deletions_count: 10},
		{sha: "e7b3bff0880fb2df1d2e3b07e09ecdbf0bc02507", date: "2022-10-12 04:31:35 UTC", description: "bump serde_json from 1.0.85 to 1.0.86", pr_number: 14785, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "a25107520ac99cca93b6d94e6b67f24d37d7fdf0", date: "2022-10-12 04:31:55 UTC", description: "bump clap from 4.0.10 to 4.0.12", pr_number: 14793, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 12, deletions_count: 12},
		{sha: "b54e9f2989ef159c16717fe121118a9ed437e156", date: "2022-10-12 04:32:10 UTC", description: "bump actions/github-script from 6.3.1 to 6.3.2", pr_number: 14798, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 9, deletions_count: 9},
		{sha: "62b4773e6ad901fc525aad1ed19aa45e7d28bb90", date: "2022-10-12 12:32:20 UTC", description: "bump wiremock from 0.5.14 to 0.5.15", pr_number: 14791, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e7291eb34fa8e7d13be823fa539de7f3cdd37806", date: "2022-10-12 12:38:52 UTC", description: "bump libc from 0.2.134 to 0.2.135", pr_number: 14792, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "38402fa0004d5c2b86cfb88515b4010897788fb9", date: "2022-10-12 13:27:01 UTC", description: "bump uuid from 1.1.2 to 1.2.1", pr_number: 14786, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 11, deletions_count: 11},
		{sha: "a4aea744efdd2ff7b1acb0ba24b4e7337d702434", date: "2022-10-12 13:30:54 UTC", description: "bump mlua from 0.8.3 to 0.8.4", pr_number: 14781, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "ca33f9b8dfd6ef0096eebab2fbf5394be45eafc1", date: "2022-10-13 03:40:25 UTC", description: "Work around race condition initializing test metrics", pr_number: 14811, scopes: ["tests"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 34, deletions_count: 26},
		{sha: "fd9db5ec15ab38656b91408012a66981b6939219", date: "2022-10-13 05:46:29 UTC", description: "bump docker/setup-buildx-action from 2.0.0 to 2.1.0", pr_number: 14806, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 7, deletions_count: 7},
		{sha: "42558b96d203effd19f00b407889ab6028420a0a", date: "2022-10-13 05:46:46 UTC", description: "bump docker/setup-qemu-action from 2.0.0 to 2.1.0", pr_number: 14807, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "2e6a559e417841c495151462599d77461dcc7025", date: "2022-10-13 05:46:57 UTC", description: "bump docker/build-push-action from 3.1.1 to 3.2.0", pr_number: 14808, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "4f4fcd5248827e173eb51460fbe1c83103d57384", date: "2022-10-13 05:47:19 UTC", description: "bump docker/login-action from 2.0.0 to 2.1.0", pr_number: 14809, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "19f024fad4188ac4842b79e6b597599824488b4c", date: "2022-10-13 05:50:21 UTC", description: "bump styfle/cancel-workflow-action from 0.10.1 to 0.11.0", pr_number: 14810, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "c7b62541df780cca476edf3d041dec7a74453171", date: "2022-10-13 22:12:55 UTC", description: "bump tokio-stream from 0.1.10 to 0.1.11", pr_number: 14805, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "4c120d57c1d15ddea3e975e9328b2f6b880c9596", date: "2022-10-13 22:13:08 UTC", description: "bump clap from 4.0.12 to 4.0.14", pr_number: 14826, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "f32eb264184bcee4f635984f7e7fef22dd741380", date: "2022-10-13 22:13:19 UTC", description: "bump float_eq from 1.0.0 to 1.0.1", pr_number: 14827, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "a43bafd7a7769f3ad3493c37183ecaedfff43d41", date: "2022-10-15 01:58:18 UTC", description: "bump docker/metadata-action from 4.0.1 to 4.1.0", pr_number: 14834, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "de4c5170f1a19a72e840e808533d6ab1f72fbe6c", date: "2022-10-15 01:58:28 UTC", description: "bump clap from 4.0.14 to 4.0.15", pr_number: 14839, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 12, deletions_count: 12},
		{sha: "fac05f8005e564883b4388764c29e66a9913f93f", date: "2022-10-15 01:58:51 UTC", description: "bump actions/github-script from 6.3.2 to 6.3.3", pr_number: 14848, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 9, deletions_count: 9},
		{sha: "ee3afe0a81d5bb20c3d8f6e6c8850a040265442d", date: "2022-10-17 22:11:40 UTC", description: "Flush commits on rebalance events", pr_number: 10434, scopes: ["kafka source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 312, deletions_count: 106},
		{sha: "e72f126af85cc8f577d8c893a641da6476089f39", date: "2022-10-18 00:35:20 UTC", description: "Template support for ssekms_key_id", pr_number: 14639, scopes: ["aws_s3 sink"], type: "enhancement", breaking_change: false, author: "Matt Fluet", files_count: 9, insertions_count: 155, deletions_count: 29},
		{sha: "e4e8d639121be5b075a1d6558f443db188e4687e", date: "2022-10-18 06:28:45 UTC", description: "Small tweaks to event processing", pr_number: 14862, scopes: ["prometheus_exporter sink"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 16, deletions_count: 17},
		{sha: "b8c8ce9290511fe292efab5f92b04c782d4f07fa", date: "2022-10-18 12:05:31 UTC", description: "create script for generating Cue documentation output from configuration schema", pr_number: 14863, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 21, insertions_count: 1137, deletions_count: 29},
		{sha: "d8502c211c857fbb1a220e3b976552628d30e669", date: "2022-10-19 01:33:40 UTC", description: "Avoid extra work in bad requests", pr_number: 14869, scopes: ["prometheus_exporter sink"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 33, deletions_count: 29},
		{sha: "4e1e4d4434a448be124551c676284ed9a20a9ecc", date: "2022-10-19 06:16:32 UTC", description: "Remove unused protobuf definitions", pr_number: 14879, scopes: ["opentelemetry source"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 0, deletions_count: 344},
		{sha: "4173127b81659802e821bb65ee7a56fca58dd01d", date: "2022-10-19 09:27:00 UTC", description: "Convert the MetricTags alias into a newtype wrapper", pr_number: 14835, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 52, insertions_count: 533, deletions_count: 502},
		{sha: "40d181f21c52dd2b6d89b659341db686076ca30e", date: "2022-10-20 23:10:44 UTC", description: "various improvements to the generated configuration schema", pr_number: 14888, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 47, insertions_count: 515, deletions_count: 406},
		{sha: "9225175be4c6f32446012a0cc43b8512729c6dc9", date: "2022-10-20 22:24:43 UTC", description: "Change `Value::to_string_lossy` to return `Cow`", pr_number: 14892, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 23, insertions_count: 110, deletions_count: 72},
		{sha: "78a057ff33dec73eae04e04e62f9fabf9e834ead", date: "2022-10-21 00:34:04 UTC", description: "Add `Kind::remove`", pr_number: 14775, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 14, insertions_count: 1093, deletions_count: 388},
		{sha: "eeb54fdc4ece88797e41183657c07bacd0f82e06", date: "2022-10-21 01:10:34 UTC", description: "Display VRL diagnostic messages in web UI", pr_number: 14850, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Jonathan Padilla", files_count: 2, insertions_count: 37, deletions_count: 17},
		{sha: "79387ae865ec11450bcf052ec02035cf1cc0b077", date: "2022-10-21 01:24:55 UTC", description: "Pre-parse all templates", pr_number: 14902, scopes: ["log_to_metric transform"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 35, deletions_count: 73},
		{sha: "0ca07a3350a9d73218511b2131d594b3fb6edff6", date: "2022-10-22 01:52:23 UTC", description: "remove `check_fields` condition and `lanes` alias in `route` transform", pr_number: 14903, scopes: ["filtering", "transforms"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 11, insertions_count: 80, deletions_count: 1253},
		{sha: "65cb55b75e4558beb5ef48bf415c37bf892061ff", date: "2022-10-22 05:39:28 UTC", description: "bump clap from 4.0.15 to 4.0.18", pr_number: 14910, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "c8396380e46033e92072b69e787d668ed2725177", date: "2022-10-22 05:41:30 UTC", description: "bump serde_json from 1.0.86 to 1.0.87", pr_number: 14895, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "8e09892eb13311951b87348c11d4e46fa632df48", date: "2022-10-22 05:42:58 UTC", description: "bump futures-util from 0.3.24 to 0.3.25", pr_number: 14894, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 16, deletions_count: 16},
		{sha: "a3ddbbbad6c67359ccb2db3f4927386e7d764f2d", date: "2022-10-22 05:47:44 UTC", description: "bump docker/metadata-action from 4.1.0 to 4.1.1", pr_number: 14877, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "49ebd7ac58320031011eeee886e6f3ce2b112571", date: "2022-10-22 05:51:18 UTC", description: "bump async-compression from 0.3.14 to 0.3.15", pr_number: 14871, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "26e743c209102a7984b87912e770231db96a0b23", date: "2022-10-22 06:19:22 UTC", description: "bump redis from 0.21.6 to 0.22.1", pr_number: 14883, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5adb1af163b91d5b1bfc5f33f3048aaa2e0967bb", date: "2022-10-25 04:27:16 UTC", description: "estimated JSON-encoded event size", pr_number: 14139, scopes: ["vector"], type: "enhancement", breaking_change: false, author: "Jean Mertz", files_count: 21, insertions_count: 1233, deletions_count: 51},
		{sha: "4ee03f0dcdbda48dd6fd11cbe9bd421ae5ed0a2f", date: "2022-10-24 20:43:26 UTC", description: "bump docker/setup-buildx-action from 2.1.0 to 2.2.1", pr_number: 14878, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 7, deletions_count: 7},
		{sha: "95681a7914d7011059a8b3aa3280cff0533ca54a", date: "2022-10-24 20:43:44 UTC", description: "bump async-trait from 0.1.57 to 0.1.58", pr_number: 14884, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "62cfddd97b96e6d6a7234519c919f4e5158d1cd7", date: "2022-10-24 20:46:25 UTC", description: "bump serde_yaml from 0.9.13 to 0.9.14", pr_number: 14923, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "0da8155bf09754ab4457873b4e6c88e59b0ad729", date: "2022-10-25 04:55:22 UTC", description: "bump ordered-float from 3.2.0 to 3.3.0", pr_number: 14885, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 19, deletions_count: 19},
		{sha: "4c4dd8e56b0633cd938cb6b86d9e8de26f942c8f", date: "2022-10-25 05:00:13 UTC", description: "bump jsonschema from 0.16.0 to 0.16.1", pr_number: 14924, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 23, deletions_count: 33},
		{sha: "6d8086710f896227d47a728e0d42c7b33c031e92", date: "2022-10-25 05:47:25 UTC", description: "bump proc-macro2 from 1.0.46 to 1.0.47", pr_number: 14856, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "41af6f772a02e9af8c3f765f6747f188bb6ee67b", date: "2022-10-25 06:03:44 UTC", description: "bump syn from 1.0.102 to 1.0.103", pr_number: 14920, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "cec64da2744849c4a059f6e07802ca10aacd0788", date: "2022-10-25 06:26:52 UTC", description: "bump futures from 0.3.24 to 0.3.25", pr_number: 14897, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 36, deletions_count: 36},
		{sha: "546e1ac6db2c614bc7707b42a9faa4ef3c1ab72d", date: "2022-10-25 06:30:20 UTC", description: "bump snafu from 0.7.2 to 0.7.3", pr_number: 14922, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 10, deletions_count: 10},
		{sha: "9205aed4cd495ed005241a1a508de8579f15860f", date: "2022-10-25 02:00:10 UTC", description: "Unify common config and render bits", pr_number: 14909, scopes: ["log_to_metric transform"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 56, deletions_count: 193},
		{sha: "648e5bd27a6a4e9956f7aef39b0f19fb1674857e", date: "2022-10-25 08:03:54 UTC", description: "bump serde from 1.0.145 to 1.0.147", pr_number: 14921, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "90728e428b7aac1e139806cb79d1dcee9464da6f", date: "2022-10-25 03:08:08 UTC", description: "Handle drop event edge case", pr_number: 14889, scopes: ["tag_cardinality_limit transform"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 84, deletions_count: 37},
		{sha: "b58fa1325874b6759c49cc66e7dc9d1cfc5c692b", date: "2022-10-25 03:33:16 UTC", description: "bump chrono-tz from 0.6.3 to 0.7.0", pr_number: 14896, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 9, deletions_count: 19},
		{sha: "8fd45eb09c177b1ced9102ba693b7f7699ecd243", date: "2022-10-25 05:47:26 UTC", description: "Get the playground up-and-running on https://playground.vrl.dev", pr_number: 14905, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jonathan Padilla", files_count: 3, insertions_count: 22, deletions_count: 4},
		{sha: "9a7c16a52796b091df60abb626539705ec63530e", date: "2022-10-25 05:39:50 UTC", description: "Use new interface in soak test comment posting", pr_number: 14934, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "58b5932b817ea50b88ba905d68a9cd0518a8d641", date: "2022-10-26 02:33:01 UTC", description: "add js feat to getrandom", pr_number: 14949, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jonathan Padilla", files_count: 2, insertions_count: 2, deletions_count: 0},
		{sha: "8dd021bf339199a3170006a5191374d398a5d4cd", date: "2022-10-26 02:28:10 UTC", description: "bump arbitrary from 1.1.7 to 1.2.0", pr_number: 14936, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "cb3b2cc942896514281506d89646d3730e01fc8b", date: "2022-10-26 02:29:04 UTC", description: "bump async-graphql from 4.0.15 to 4.0.16", pr_number: 14937, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 12},
		{sha: "6bbb23127e33e28a18588c4b57ea41ca677e788e", date: "2022-10-26 02:29:40 UTC", description: "bump ryu from 1.0.9 to 1.0.11", pr_number: 14939, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "0f5fe533b2688dcbe76e7754dbb99cf662d16d92", date: "2022-10-26 02:30:28 UTC", description: "bump assert_cmd from 2.0.4 to 2.0.5", pr_number: 14940, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "db3f430ecaa440e5a974c62eedd10ff22814ccfa", date: "2022-10-26 02:31:16 UTC", description: "bump axum from 0.5.16 to 0.5.17", pr_number: 14943, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "5d370177c18e19afe51f5acd5a0fe4c6a2867df5", date: "2022-10-26 02:32:38 UTC", description: "bump libc from 0.2.135 to 0.2.136", pr_number: 14944, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d21c420e33ccf603dd69bb9b20d9f4b278a53d6b", date: "2022-10-26 06:42:23 UTC", description: "Check lookup path syntax at startup", pr_number: 14887, scopes: ["config"], type: "enhancement", breaking_change: false, author: "Nathan Fox", files_count: 21, insertions_count: 715, deletions_count: 356},
		{sha: "6cafa536a6f2a3b9623b4dc84622491729b2a932", date: "2022-10-26 23:39:18 UTC", description: "integrate config schema-based documentation into existing Cue docs (transforms)", pr_number: 14929, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 75, insertions_count: 1845, deletions_count: 1023},
		{sha: "cb996f3a8797a31962399427cb6c02a43c1be305", date: "2022-10-27 00:32:03 UTC", description: "import machine-generated Cue documentation for sources and sinks", pr_number: 14967, scopes: ["config", "docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 96, insertions_count: 23405, deletions_count: 0},
		{sha: "60164e47f7c6042fb66304681ba4f265da92b52b", date: "2022-10-26 23:14:17 UTC", description: "bump cidr-utils from 0.5.7 to 0.5.8", pr_number: 14962, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "16326ab695afd73b3d08455d89e356ba8d4a0d10", date: "2022-10-26 23:15:05 UTC", description: "bump anyhow from 1.0.65 to 1.0.66", pr_number: 14960, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4bf467280ae6732f51b6762aca80ce9cb49bea51", date: "2022-10-26 23:15:57 UTC", description: "bump async-graphql-warp from 4.0.15 to 4.0.16", pr_number: 14959, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "73e86e05eda7050874474f50bb966e259c63ac0d", date: "2022-10-26 23:18:52 UTC", description: "bump base64 from 0.13.0 to 0.13.1", pr_number: 14958, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "d63d75bd54ed320b3a5908b3125d15ad3f8907e4", date: "2022-10-27 06:47:20 UTC", description: "bump getrandom from 0.2.6 to 0.2.8", pr_number: 14961, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 14, deletions_count: 14},
		{sha: "0c6c8c9be77e4405d17515981921144efa781787", date: "2022-10-27 02:07:25 UTC", description: "swap out pretty_assertions for similar-asserts", pr_number: 14954, scopes: ["deps"], type: "chore", breaking_change: false, author: "Luke Steensen", files_count: 41, insertions_count: 96, deletions_count: 75},
		{sha: "0c4d878a4574d4571a0ec7fb3990940034a42fc1", date: "2022-10-27 01:28:44 UTC", description: "bump aws-smithy-http-tower from 0.49.0 to 0.51.0", pr_number: 14957, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 99, deletions_count: 52},
		{sha: "81a002df76dd34edf4425acfa195cf413d1047c5", date: "2022-10-27 07:00:40 UTC", description: "Introduce 'regression detection' flow", pr_number: 14935, scopes: ["ci"], type: "feat", breaking_change: false, author: "Brian L. Troutwine", files_count: 83, insertions_count: 10574, deletions_count: 0},
		{sha: "82806d2a2c689512875e8703f2a47adf289d5d28", date: "2022-10-27 06:36:15 UTC", description: "require the `include` config option since it is actually required for the source to function", pr_number: 14956, scopes: ["file source"], type: "fix", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 54, deletions_count: 12},
		{sha: "8f6f0e7ad3cdbaee79d43c8504c9b8c5096e8635", date: "2022-10-28 03:16:54 UTC", description: "RFC for enhanced metric tags storage", pr_number: 14838, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 368, deletions_count: 0},
		{sha: "0b838602fbe0bb40cdd03f08695b9d50569e9fc0", date: "2022-10-28 07:03:02 UTC"
			description: "Small cleanup on existing log namespace feature", pr_number: 14979, scopes: ["http_client source"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count: 7
		},
		{sha: "5979dd4f752185849fb47d4075867e04edb5ae3b", date: "2022-10-29 01:06:40 UTC", description: "Add clarifying note to `tag_cardinality_limit` transform. ", pr_number: 14970, scopes: ["external docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "e46d861967fdd465cc84fa248c4a874ab3bf87d9", date: "2022-10-29 03:48:46 UTC", description: "Fix file source defaults", pr_number: 15002, scopes: ["file source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 41, deletions_count: 19},
		{sha: "4e45127b863849e64976a9bdaa74c314f268862a", date: "2022-10-29 12:51:52 UTC", description: "Add `api_version` option", pr_number: 14918, scopes: ["elasticsearch sink"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 8, insertions_count: 343, deletions_count: 87},
		{sha: "af18849f409e10dc5e93a5256de988a46391f8a3", date: "2022-10-29 10:29:39 UTC", description: "Revert add `api_version` option", pr_number: 15006, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 8, insertions_count: 87, deletions_count: 343},
		{sha: "ac69a471ebdb92837c32c20c57c443770a79051a", date: "2022-10-31 05:19:04 UTC", description: "Add `decode_mime_q` function", pr_number: 14813, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 7, insertions_count: 354, deletions_count: 0},
		{sha: "a5d7cea8f8ed626a136cdca49c35115b76aae7e7", date: "2022-11-01 00:59:59 UTC", description: "Standardize on `cfg(windows)`", pr_number: 15000, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 18, deletions_count: 18},
		{sha: "30706de871a16c039d2e3249fb7e71ba9597b3cd", date: "2022-11-01 04:11:55 UTC", description: "revamp the documentation for end-to-end acknowledgements", pr_number: 14986, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 92, insertions_count: 1499, deletions_count: 389},
		{sha: "a12d4788226cf408b4f17a652f4355e944805ce1", date: "2022-11-01 04:54:31 UTC", description: "Upgrade AWS SDK crates to 0.21.0 and Smithy to 0.51.0", pr_number: 14988, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 121, deletions_count: 204},
		{sha: "bc75cafbafb8697edfb3454d35d38e5cd9b41498", date: "2022-11-01 05:32:30 UTC", description: "Switch rdkafka back to crates.io", pr_number: 15040, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 7, deletions_count: 5},
		{sha: "ed5d0cc1266bb679cc94f09937c49458439afb1d", date: "2022-11-01 07:01:52 UTC", description: "Add timeouts to CI jobs that hung recently", pr_number: 15042, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 0},
		{sha: "ebf673404c0b424ab8048b57f2d5ee7de80c5def", date: "2022-11-01 14:38:46 UTC", description: "bump libc from 0.2.136 to 0.2.137", pr_number: 14974, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "72e40289f0d5426e54cd909791637989cbac6efe", date: "2022-11-01 20:55:17 UTC", description: "bump aes from 0.8.1 to 0.8.2", pr_number: 15048, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "28139cfc06072ae893a88dd664c70a2ebc16d710", date: "2022-11-01 20:55:44 UTC", description: "bump infer from 0.9.0 to 0.11.0", pr_number: 15044, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "4bf9243cba704e81b05c8c90516db7d8a6eadf67", date: "2022-11-01 20:55:55 UTC", description: "bump once_cell from 1.15.0 to 1.16.0", pr_number: 15045, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 11, deletions_count: 11},
		{sha: "4f6a9b5f2cb73828fd7d418bf31961afc5b881b2", date: "2022-11-01 20:56:09 UTC", description: "bump mlua from 0.8.4 to 0.8.5", pr_number: 15046, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "c899b021a2fa76c6040365cab3948f21a04cb8ab", date: "2022-11-01 20:56:20 UTC", description: "bump hyper from 0.14.20 to 0.14.22", pr_number: 15047, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "b0559bbb3e9de6738f4d0f272ac372dd599b8b4a", date: "2022-11-01 20:57:02 UTC", description: "bump roxmltree from 0.15.0 to 0.15.1", pr_number: 15050, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "265d9cfa599bc5969efd33f9f4c400f63a9aae18", date: "2022-11-02 00:25:50 UTC", description: "add log namespace support to filter transform", pr_number: 15036, scopes: ["filter transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "87f6cca73469e5b22e3251007a097190eda7a96d", date: "2022-11-02 00:26:47 UTC", description: "add log namespace support to throttle transform", pr_number: 15033, scopes: ["throttle transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "943c6165523d91c89ed5c2dd5fa52ace83c93022", date: "2022-11-02 01:02:46 UTC", description: "Fix roadmap voting link in readme", pr_number: 15055, scopes: [], type: "docs", breaking_change: false, author: "Michael Warkentin", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4271431ba8fa6c3b34e7b5315cff3a3daa7fd228", date: "2022-11-02 07:40:17 UTC", description: "Retry whole payload on partial bulk failure", pr_number: 14891, scopes: ["elasticsearch sink"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 3, insertions_count: 123, deletions_count: 20},
		{sha: "5e582447cb3919eba12891c9178b365423075356", date: "2022-11-02 02:57:58 UTC", description: "add `LogNamespace` getter functions", pr_number: 15054, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 37, deletions_count: 0},
		{sha: "3b7916b82e28979a1121508f1926164fe03f60e9", date: "2022-11-02 01:44:05 UTC", description: "Update usages of deprecated set-output in GitHub Actions", pr_number: 15058, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "01b7b3c03767b61d64f2a7a6a48cf0e13fde7888", date: "2022-11-02 05:23:51 UTC", description: "Remove `mut` requirement of `LogNamespace` getters", pr_number: 15060, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f7f4b640719513ab594e324f64b493e315581091", date: "2022-11-02 05:47:03 UTC", description: "move smp crate version to variable", pr_number: 15062, scopes: ["ci"], type: "chore", breaking_change: false, author: "Geoffrey Oxberry", files_count: 1, insertions_count: 23, deletions_count: 17},
		{sha: "373e1c8dd902b49bc18dd7448d12c1a5c11494ff", date: "2022-11-02 09:33:57 UTC", description: "Add log namespace and schema support", pr_number: 14985, scopes: ["aws_s3 source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 240, deletions_count: 80},
		{sha: "8d6ff2d7bac7872c6c0323187f806082137c4e08", date: "2022-11-02 22:36:02 UTC", description: "Bump version", pr_number: 15069, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "0e686508e604c7e7fdb4e20737b15ad425c825fd", date: "2022-11-03 00:34:10 UTC", description: "fix issues in ci-metadata-script", pr_number: 15074, scopes: ["ci"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 7, deletions_count: 5},
		{sha: "976687f9378331dcddc31008350a5742467ce770", date: "2022-11-03 05:14:05 UTC", description: "add analysis from smp to regression workflow", pr_number: 15063, scopes: ["ci"], type: "feat", breaking_change: false, author: "Geoffrey Oxberry", files_count: 3, insertions_count: 66, deletions_count: 50},
		{sha: "df96668d8be3aa344bd523208349b3802d9db007", date: "2022-11-03 07:38:40 UTC", description: "drop PartialOrd from Event", pr_number: 13277, scopes: ["data model"], type: "chore", breaking_change: false, author: "Luke Steensen", files_count: 12, insertions_count: 63, deletions_count: 52},
		{sha: "0f0fdeeceded29a4f9f301dc20cb362125f5e558", date: "2022-11-03 05:54:17 UTC", description: "Revert config secrets scanning", pr_number: 15081, scopes: ["config"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 14, insertions_count: 104, deletions_count: 382},
		{sha: "717a6403201eca38337612c405a13a68002d0033", date: "2022-11-03 12:07:09 UTC", description: "Convert metric tags tests to use metric_tags! macro", pr_number: 15072, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 25, insertions_count: 246, deletions_count: 507},
		{sha: "f94aaed201f373b03ced3273cbd24d2a4d33c8da", date: "2022-11-03 13:05:06 UTC", description: "Fix merging of configured timezones", pr_number: 15077, scopes: ["config"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 54, deletions_count: 9},
		{sha: "7e213e26a194a283ca54372f720eeda0b0261018", date: "2022-11-04 03:06:32 UTC", description: "improve `LogNamespace` helper functions", pr_number: 15068, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 16, insertions_count: 498, deletions_count: 325},
		{sha: "2ba50e44e5ace7ec095a1c18a3b94db839d4b1f0", date: "2022-11-04 02:12:52 UTC", description: "bump cidr-utils from 0.5.8 to 0.5.9", pr_number: 15065, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7da94438c6e2fc714cfe4f65972fd513079153ab", date: "2022-11-04 02:13:57 UTC", description: "bump webbrowser from 0.8.0 to 0.8.1", pr_number: 15066, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 5, deletions_count: 55},
		{sha: "ca202ba4823cdb792fe79bbcdee6f2b93f1de217", date: "2022-11-04 02:14:57 UTC", description: "bump chrono-tz from 0.7.0 to 0.8.0", pr_number: 15088, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 8, deletions_count: 8},
		{sha: "981f44971ad934b0dd7aae05100d4468c634c555", date: "2022-11-04 04:48:19 UTC", description: "Make Regex `Arc` in VRL to avoid copies", pr_number: 15079, scopes: ["vrl performance"], type: "enhancement", breaking_change: false, author: "Brian Floersch", files_count: 3, insertions_count: 24, deletions_count: 9},
		{sha: "4c5ff8f516654273de82512ef7626a7cb9b19718", date: "2022-11-04 04:58:09 UTC", description: "Add config tests exercising all global options", pr_number: 15085, scopes: ["tests"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 8, insertions_count: 103, deletions_count: 1},
		{sha: "cf7372979be3b818f1cece9dae286a50a1554bf9", date: "2022-11-04 08:21:42 UTC", description: "Include PR number in the regression target tag", pr_number: 15095, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 9, deletions_count: 4},
		{sha: "8937704b68a0f0e2dd31a3df6ae6573b55a0d795", date: "2022-11-04 23:28:40 UTC", description: "add log namespace support to sample transform", pr_number: 15034, scopes: ["sample transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "4656107fe4e9f6b97d8eafd2ee0f2338e4129d8e", date: "2022-11-04 23:29:15 UTC", description: "Remove the deprecated `geoip` transform", pr_number: 15090, scopes: ["geoip transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 13, insertions_count: 12, deletions_count: 1007},
		{sha: "7eea71bef7fb9bbc1377fa8fd5b4ec6ae7ed6610", date: "2022-11-04 23:30:19 UTC", description: "add log namespace support to route transform", pr_number: 15035, scopes: ["route transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 11, deletions_count: 3},
		{sha: "75c60451848c8b5406dd12b7bf0264344a131778", date: "2022-11-04 23:30:45 UTC", description: "add log namespace support to lua transform", pr_number: 15032, scopes: ["lua transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 13, deletions_count: 6},
		{sha: "10ecf20edab4ead7e9adcd2fd1d977dcaf4a19b5", date: "2022-11-05 03:08:11 UTC", description: "add constant that can be used instead of `None` for `LegacyKey`", pr_number: 15105, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "69d0a2d9d64e99bda057b5a7393b4161e39bcfc6", date: "2022-11-05 04:31:22 UTC", description: "add log namespace support to `aws_ec2_metadata` transform", pr_number: 15107, scopes: ["aws_ec2_metadata transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 9, insertions_count: 233, deletions_count: 135},
		{sha: "14f91fb9b02d062ab6c2edb5659b1b6e948c2268", date: "2022-11-05 04:32:00 UTC", description: "add log namespace support to dedupe transform", pr_number: 15089, scopes: ["dedupe transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 7, deletions_count: 2},
		{sha: "e16371ee65da21b9e99334c73363be1ad5d6edb8", date: "2022-11-05 06:54:07 UTC", description: "line_agg preserves the context of the last line added", pr_number: 14858, scopes: ["file source"], type: "fix", breaking_change: false, author: "j chesley", files_count: 2, insertions_count: 147, deletions_count: 74},
		{sha: "3e96d95b81e43551b65116ae9477a7f93abd471e", date: "2022-11-05 05:23:54 UTC", description: "Re-sync docs from config schema", pr_number: 15119, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 24, deletions_count: 7},
		{sha: "d9a533d0e265452e893a49c232d8068a1dc6dbf4", date: "2022-11-05 13:43:29 UTC", description: "Fix parsing escape character for `parse_cef` ", pr_number: 15092, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 1, insertions_count: 19, deletions_count: 1},
		{sha: "3e27faad85f192847d82fdb11e994b2cdfac2222", date: "2022-11-05 16:19:14 UTC", description: "error message with threads=0", pr_number: 15111, scopes: ["config"], type: "fix", breaking_change: false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 4, deletions_count: 1},
		{sha: "24f3137c8bb4f99cf7d8de95817d29a3255ba519", date: "2022-11-05 08:11:41 UTC", description: "re-add support `Bearer` Auth config option", pr_number: 15112, scopes: ["prometheus_remote_write sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 27, deletions_count: 0},
		{sha: "ba02a47b4cb4271e48d66c47a90b6ee3d6d7ef8e", date: "2022-11-08 10:30:55 UTC", description: "change default scrape_interval to 1", pr_number: 15124, scopes: ["internal_metrics"], type: "fix", breaking_change: false, author: "Alexander Zaitsev", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "f4a363f46b990a0f1bf39104243f316d01f474a5", date: "2022-11-08 03:38:17 UTC", description: "APM stats payloads are sent independent of trace payloads and at a set interval.", pr_number: 15084, scopes: ["datadog_traces sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 15, insertions_count: 1591, deletions_count: 1235},
		{sha: "68264018de60b0a48519cdfa3ba4f5e3db2d4cfb", date: "2022-11-09 07:38:23 UTC", description: "add missing space", pr_number: 15138, scopes: [], type: "docs", breaking_change: false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3e5cdc9e8954f696a2d603af57655c9a62b0f01c", date: "2022-11-09 11:18:56 UTC", description: "add missing BytesSent metric", pr_number: 15109, scopes: ["blackhole"], type: "fix", breaking_change: false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 9, deletions_count: 1},
		{sha: "c93f3a08e2674d0a90f59b8695fb9a384059753d", date: "2022-11-09 06:38:58 UTC", description: "configure Netlify ignore builds options", pr_number: 14951, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jonathan Padilla", files_count: 4, insertions_count: 130, deletions_count: 42},
		{sha: "14ba2eb9d5015944b78eb7faf4a1b68a89d13494", date: "2022-11-09 07:31:10 UTC", description: "Remove reference from `LegacyKey` usage to make it easier to use", pr_number: 15141, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 14, deletions_count: 14},
		{sha: "1f29c1debf7890c5cfd5d1e739ca7856595ed940", date: "2022-11-09 16:31:51 UTC", description: "update Avro scheme example", pr_number: 15145, scopes: [], type: "docs", breaking_change: false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "067795eabedea44db8d5d069bc8db1a4646dcc9d", date: "2022-11-09 13:41:47 UTC", description: "Introduce trusted/untrusted regression workflow split", pr_number: 15142, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 2, insertions_count: 568, deletions_count: 225},
		{sha: "c17294b8b2c7a99b43a6749f238a4606d5af7d21", date: "2022-11-10 00:48:54 UTC", description: "Correct github.actions.rest to github.rest.actions", pr_number: 15153, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "7e845c02ed80b605f2cb347ec38931a1761e44b9", date: "2022-11-10 02:15:42 UTC", description: "Add log namespace and schema support", pr_number: 15043, scopes: ["redis source"], type: "feat", breaking_change: false, author: "David Huie", files_count: 3, insertions_count: 166, deletions_count: 66},
		{sha: "3eb5f56100170cb33604c465f86182443e70b505", date: "2022-11-10 06:16:41 UTC", description: "Remove dependabot restriction from comment step", pr_number: 15161, scopes: ["ci"], type: "fix", breaking_change: false, author: "George Hahn", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "31383efe11da5a479acd8e6b9a4c6777ae29885b", date: "2022-11-10 06:45:36 UTC", description: "const init thread locals", pr_number: 15158, scopes: ["core"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "225eac5a4214a62322021eedbc8d27ef6e24f717", date: "2022-11-10 06:49:22 UTC", description: "add tracking allocations", pr_number: 14995, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 11, insertions_count: 204, deletions_count: 47},
		{sha: "0719f3439f9605bfeccc761ccf61fce7cc6c3b67", date: "2022-11-10 13:41:11 UTC", description: "Reintroduce `api_version` option ", pr_number: 15082, scopes: ["elasticsearch sink"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 8, insertions_count: 358, deletions_count: 87},
		{sha: "3b6db2c39193d2a1fc264e2cbec60f4112bd0e72", date: "2022-11-10 05:40:37 UTC", description: "Hide the `log_namespace` option", pr_number: 15120, scopes: ["docs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 11, insertions_count: 7, deletions_count: 26},
		{sha: "b11263dee4fa8013b3f46ca62b94c4b1ba6e0558", date: "2022-11-10 10:52:27 UTC", description: "Update lading in the soak tests", pr_number: 15159, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 108, insertions_count: 451, deletions_count: 598},
		{sha: "58f5ac63f8afd99636293b54243e143aee8cc291", date: "2022-11-11 00:47:04 UTC", description: "Add `OptionalValuePath`", pr_number: 15167, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 43, deletions_count: 2},
		{sha: "b26dede3e399d639b952a3da557785e6e2618a0f", date: "2022-11-11 04:33:20 UTC", description: "Convert metric tag value store to an IndexSet", pr_number: 14984, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 14, insertions_count: 576, deletions_count: 133},
		{sha: "616958bbaab8e709ebf7db6f04c5bc91aca31ad3", date: "2022-11-11 07:41:22 UTC", description: "Add helper to insert all vector source metadata", pr_number: 15101, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 134, deletions_count: 81},
		{sha: "a1b259090a5f889e547122a34bcdb0733368436e", date: "2022-11-11 08:39:47 UTC", description: "fix http pipelines soaks", pr_number: 15179, scopes: ["ci"], type: "fix", breaking_change: false, author: "Arshia Soleimani", files_count: 2, insertions_count: 0, deletions_count: 2},
		{sha: "72a5a8f6be52365b72e77cab52afb7016bea3f76", date: "2022-11-12 10:16:53 UTC", description: "typo", pr_number: 15188, scopes: [], type: "docs", breaking_change: false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3dbc6ff91cac2b24d65553c66596164b4a8e6d42", date: "2022-11-12 03:38:03 UTC", description: "add schema definition test utility", pr_number: 15166, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 13, insertions_count: 312, deletions_count: 79},
		{sha: "b178980dfaabc1172863707dbe19ff67018156f1", date: "2022-11-12 05:49:23 UTC", description: "Add log namespace and schema def support", pr_number: 15174, scopes: ["file source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 360, deletions_count: 74},
		{sha: "0a5fe35d35463fab20c9751da8e82c4886fb9a7d", date: "2022-11-12 06:42:25 UTC", description: "Add log namespace and schema def", pr_number: 15189, scopes: ["aws_kinesis_firehose source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 5, insertions_count: 237, deletions_count: 45},
		{sha: "3bf89a7cf5190a9e67e6b50e36ae63ef5fb2d223", date: "2022-11-12 04:03:21 UTC", description: "Regenerate docs", pr_number: 15192, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 38, deletions_count: 2},
		{sha: "540d490289fa2c2fb9cbc774805461aebb654edb", date: "2022-11-12 06:44:13 UTC", description: "Tweaks to markdown", pr_number: 15097, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Steve Hall", files_count: 4, insertions_count: 20, deletions_count: 20},
		{sha: "5b471406aad01669776b8934838465b94db73a9b", date: "2022-11-14 10:08:09 UTC", description: "Separate out iterating over single and multi-value tags", pr_number: 15182, scopes: ["metrics"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 14, insertions_count: 52, deletions_count: 79},
		{sha: "bf62470e1b8caaa14c0d1b8ecd92a31cfae0b0c9", date: "2022-11-15 02:51:00 UTC", description: "bump ordered-float from 3.3.0 to 3.4.0", pr_number: 15134, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 19, deletions_count: 19},
		{sha: "cf901434d434639f578d3bf4b8285ca27fdb00ca", date: "2022-11-15 01:18:02 UTC", description: "Make allocation tracking scale ", pr_number: 15168, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 5, insertions_count: 63, deletions_count: 72},
		{sha: "66a8828116697a36553633899e94a8c86251d397", date: "2022-11-15 05:21:11 UTC", description: "Add CI for compiling core VRL crates to WASM", pr_number: 15146, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jonathan Padilla", files_count: 6, insertions_count: 60, deletions_count: 0},
		{sha: "3485fb0d7b6cb5b23c6d2265b7cd3f8f5cd1dab5", date: "2022-11-15 06:10:43 UTC", description: "refactor `aws_kinesis_firehose` and `aws_kinesis_streams` sinks to extract redundant code", pr_number: 14893, scopes: ["aws_kinesis sinks"], type: "chore", breaking_change: false, author: "neuronull", files_count: 28, insertions_count: 692, deletions_count: 742},
		{sha: "5de9912f4fe9fb5a78ce1a0a593e03fe4f9338c9", date: "2022-11-15 07:34:13 UTC", description: "bump env_logger from 0.9.1 to 0.9.3", pr_number: 15137, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "7c74af2f09a23a175b6006ad899283db06fadfb0", date: "2022-11-15 07:34:39 UTC", description: "bump clap from 4.0.18 to 4.0.23", pr_number: 15201, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "551e329b19d50e6403c525b5d8d0011d805eb973", date: "2022-11-15 09:49:46 UTC", description: "Cleanup match used for legacy timestamp handling", pr_number: 15205, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 17, deletions_count: 25},
		{sha: "8d6cb1f43ae868fb217ccd684da271720546e77e", date: "2022-11-15 12:56:07 UTC", description: "Pass the same environment variables in Regression Detector as Soak", pr_number: 15209, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 9, deletions_count: 2},
		{sha: "b1f90cac3fe52fa5b987d7dac8c6bd631ab0d963", date: "2022-11-16 02:15:02 UTC", description: "add log_namespace to dnstap", pr_number: 15052, scopes: ["dnstap source"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 4, insertions_count: 825, deletions_count: 597},
		{sha: "44dcb3fa67cb5d29d89d162d53bcd152d997ba82", date: "2022-11-16 02:06:40 UTC", description: "add CI check to enforce up-to-date machine-generated component docs", pr_number: 15162, scopes: ["ci"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 12, insertions_count: 59, deletions_count: 30},
		{sha: "2d812572fd7acc248c4eaed172eca3d2951dbae5", date: "2022-11-16 01:15:25 UTC", description: "bump assert_cmd from 2.0.5 to 2.0.6", pr_number: 15129, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 4},
		{sha: "d48c1c3765732b2ad665c0d3e68ab9e3fc7ceb01", date: "2022-11-16 01:15:41 UTC", description: "bump hyper from 0.14.22 to 0.14.23", pr_number: 15131, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "86cca5e23e7749c8508a9c28dd0d885da9ecea42", date: "2022-11-16 01:16:29 UTC", description: "bump webbrowser from 0.8.1 to 0.8.2", pr_number: 15212, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "acab56d838bca6a2a1d28b1c564f2f3fad220e0b", date: "2022-11-16 02:32:37 UTC", description: "add log namespace support to `reduce` transform", pr_number: 15198, scopes: ["reduce transform"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 134, deletions_count: 6},
		{sha: "64d211b4d50d25d259779f4684a59d8def945b40", date: "2022-11-16 02:16:08 UTC", description: "bump pest from 2.4.0 to 2.4.1", pr_number: 15133, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "ff904a10ab60e88848f2eccf4df19e7c77ba8f5c", date: "2022-11-16 02:28:00 UTC", description: "bump uuid from 1.2.1 to 1.2.2", pr_number: 15211, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 11, deletions_count: 11},
		{sha: "30bfdf315a5946b09a620ce2456308416c391853", date: "2022-11-16 04:29:50 UTC", description: "Use commit statuses, not checks in Regression Detector workflow", pr_number: 15213, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 88, deletions_count: 124},
		{sha: "9bb5bc7e30fa46498b8eee378eeabc27d66bcb5f", date: "2022-11-16 01:32:25 UTC", description: "Add log namespace and schema support", pr_number: 15125, scopes: ["socket source"], type: "feat", breaking_change: false, author: "David Huie", files_count: 10, insertions_count: 580, deletions_count: 138},
		{sha: "5517286a2f1a2ecbec34831e299d51ba8121526c", date: "2022-11-16 07:44:27 UTC", description: "synchronize machine-generated Cue documentation", pr_number: 15222, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 3, insertions_count: 11, deletions_count: 1},
		{sha: "d5d0ed928477d355cc1e050fc33dfb99a5841916", date: "2022-11-16 07:59:20 UTC", description: "bump nats from 0.23.0 to 0.23.1", pr_number: 15210, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "ecdc5ab9fd940eaa7c019c146c2c0e012b88e655", date: "2022-11-16 09:57:06 UTC", description: "bump regex from 1.6.0 to 1.7.0", pr_number: 15132, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 8, deletions_count: 8},
		{sha: "595153d6e7ea7d3af528bb0984c9b8fe021cdc37", date: "2022-11-16 23:08:50 UTC", description: "remove logging dep for generate-component-docs script", pr_number: 15224, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 55, deletions_count: 10},
		{sha: "13a7bd8b99f2d14862fcd4bb44fd7081094edc89", date: "2022-11-17 01:38:10 UTC", description: "Polish web playground", pr_number: 15204, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Jonathan Padilla", files_count: 6, insertions_count: 585, deletions_count: 167},
		{sha: "c744c989e68086fe8ed7bdf82ef15194d70f4610", date: "2022-11-17 01:25:48 UTC", description: "Allow for bare tags in the metric tag value set", pr_number: 15177, scopes: ["metrics"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 7, insertions_count: 328, deletions_count: 156},
		{sha: "cf66017a0c06a4627602a6eb0585bdfaccbc9c5f", date: "2022-11-17 07:51:07 UTC", description: "bump mlua from 0.8.5 to 0.8.6", pr_number: 15231, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "67d4943e9cf41097d08b8371681b47daca422850", date: "2022-11-17 10:26:29 UTC", description: "miscellaneous fixes to generate config schema and derived docs output", pr_number: 15270, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 22, insertions_count: 374, deletions_count: 71},
		{sha: "5f2fb096892d3c9ff4fe1a763a7e4ee0b18fc2bc", date: "2022-11-17 08:13:44 UTC", description: "Add log namespace and schema support", pr_number: 15223, scopes: ["exec source"], type: "feat", breaking_change: false, author: "David Huie", files_count: 1, insertions_count: 240, deletions_count: 24},
		{sha: "a1568a3852a53b203aa7070d9d7799c9bd3ec058", date: "2022-11-18 06:46:53 UTC", description: "added auto_extract_timestamp option", pr_number: 15261, scopes: ["splunk_hec sink"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 13, insertions_count: 145, deletions_count: 5},
		{sha: "8c6d4c18f5e27fcf2776c93db8893790ceff5248", date: "2022-11-18 01:11:20 UTC", description: "Refactor native proto tests", pr_number: 15269, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2049, insertions_count: 137, deletions_count: 145},
		{sha: "9bfe6c865488975b36e7d08b8257a293af77449d", date: "2022-11-18 01:16:32 UTC", description: "bump reqwest from 0.11.12 to 0.11.13", pr_number: 15279, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "04cb157480a6cc84a08599973e83c990ea9234e4", date: "2022-11-18 01:18:01 UTC", description: "bump serde_with from 2.0.1 to 2.1.0", pr_number: 15278, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 12},
		{sha: "798c9907626e4db4df25fb12d317ecd55f4efe3b", date: "2022-11-18 01:52:04 UTC", description: "bump clap from 4.0.23 to 4.0.26", pr_number: 15277, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 12, deletions_count: 12},
		{sha: "3eb59669604ec9d73b923e7976d19097ea67478e", date: "2022-11-18 02:24:03 UTC", description: "bump memmap2 from 0.5.7 to 0.5.8", pr_number: 15228, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5de33ff1fa2f9dd36266550b75283a29eb453348", date: "2022-11-18 03:33:43 UTC", description: "bump pest_derive from 2.4.0 to 2.4.1", pr_number: 15227, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "767310a7909be85c57a34eabaf4abff4ec6f1c8a", date: "2022-11-18 12:55:58 UTC", description: "remove auto_extract_timestamp integration tests", pr_number: 15288, scopes: ["splunk_hec sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 6, deletions_count: 2},
		{sha: "6099e0d3eace517d1a6b2cda5daab61c70a89ac2", date: "2022-11-18 10:27:49 UTC", description: "Correct Regression Detector variant checkout", pr_number: 15289, scopes: ["ci"], type: "fix", breaking_change: false, author: "Brian L. Troutwine", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "04aaf1ac398db373d3f7cc17891f075f45b98018", date: "2022-11-19 09:25:16 UTC", description: "auto_extract_timestamp only works for version 8 and above", pr_number: 15294, scopes: ["splunk_hec sink"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 5, insertions_count: 81, deletions_count: 65},
		{sha: "408b59e0c81273d913664d7582c26e569561c95f", date: "2022-11-19 07:11:38 UTC", description: "Fix flaky `topology_disk_buffer_conflict` test", pr_number: 15297, scopes: ["topology"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "f37e22c6759e06690db13a04b6be942eb498de66", date: "2022-11-22 00:44:21 UTC", description: "bump serde_json from 1.0.87 to 1.0.88", pr_number: 15301, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "3d06f14017ce1f043735f84ebdbbf277dfda65c1", date: "2022-11-22 00:49:25 UTC", description: "bump snap from 1.0.5 to 1.1.0", pr_number: 15300, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "66bd273e0805f824f66fc7492604ea0b7fdf2f45", date: "2022-11-22 00:58:13 UTC", description: "bump crossbeam-queue from 0.3.6 to 0.3.7", pr_number: 15303, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "a7a68536ac340d32a4783a961b5cad8aa71cf2a8", date: "2022-11-22 01:09:46 UTC", description: "bump aws-actions/configure-aws-credentials from 1.pre.node16 to 1.7.0", pr_number: 15285, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "1971d8d88698af9f9feb8f282a5988200c0e4d07", date: "2022-11-22 08:42:43 UTC", description: "bump indexmap from 1.9.1 to 1.9.2", pr_number: 15293, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 8, deletions_count: 8},
		{sha: "0f5a2afd8bd2b81f426f4c71c523cee34a1a6a1c", date: "2022-11-22 03:07:46 UTC", description: "Add log namespace and schema def support", pr_number: 15206, scopes: ["fluent source"], type: "feat", breaking_change: true, author: "neuronull", files_count: 5, insertions_count: 287, deletions_count: 195},
		{sha: "3615ed0f93d0421566ea4b92757be9a4ef15f27e", date: "2022-11-22 12:41:57 UTC", description: "bump crossbeam-utils from 0.8.12 to 0.8.13", pr_number: 15302, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "aea2826582f899d786581fa3d1f61740c5f60f5c", date: "2022-11-22 07:49:41 UTC", description: "Update Privacy Link", pr_number: 15305, scopes: ["template website"], type: "chore", breaking_change: false, author: "David Weid II", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "af8ed73dafa57e625b317ce250d4c01dbf320f8f", date: "2022-11-22 10:17:23 UTC", description: "Bump prost, prost-build, and prost-types to 0.11.2", pr_number: 15311, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 40, deletions_count: 38},
		{sha: "642a820478740e0c96d13bc67074cb1bdbb8fabf", date: "2022-11-22 09:02:10 UTC", description: "Add log namespace and schema support", pr_number: 15307, scopes: ["file_descriptor", "stdin source"], type: "feat", breaking_change: false, author: "David Huie", files_count: 3, insertions_count: 183, deletions_count: 24},
		{sha: "4184aba152453962b4c1cb0cae866d2a28a450ee", date: "2022-11-23 02:49:16 UTC", description: "Fix naming of backwards-compatible fixtures", pr_number: 15313, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2049, insertions_count: 9, deletions_count: 9},
		{sha: "ea464dbb68546b9257231e03d977575b76a8b66b", date: "2022-11-23 01:10:08 UTC", description: "Allocation tracking runtime toggle", pr_number: 15221, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Arshia Soleimani", files_count: 6, insertions_count: 90, deletions_count: 74},
		{sha: "0e855a4a5347a6d7dc4be66a598087ad8d9bebfa", date: "2022-11-23 03:46:35 UTC", description: "bump tokio from 1.21.2 to 1.22.0", pr_number: 15317, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "269a200acb4a689df6dcbaebc74a79de2520e7db", date: "2022-11-23 10:20:05 UTC", description: "update schema definition for amqp source", pr_number: 15321, scopes: ["amqp source"], type: "feat", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 87, deletions_count: 0},
		{sha: "4bec2d57debbbce6f8091d9da29f287fc222477f", date: "2022-11-23 03:55:07 UTC", description: "Add log namespace and schema def support", pr_number: 15290, scopes: ["logstash source"], type: "feat", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 166, deletions_count: 25},
		{sha: "ef7ea06a5914b675ec99502fb04a03330e55df87", date: "2022-11-23 05:47:56 UTC", description: "bump bytes from 1.2.1 to 1.3.0", pr_number: 15316, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 13, insertions_count: 83, deletions_count: 83},
		{sha: "b9c19a6074a40c8840da64234ee380001880e267", date: "2022-11-24 01:49:20 UTC", description: "bump crossbeam-utils from 0.8.13 to 0.8.14", pr_number: 15328, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "09e9f9bf235de494f5f503b63853e3509bb41753", date: "2022-11-24 18:59:27 UTC", description: "fix s3:TestEvent serialization", pr_number: 15331, scopes: ["aws_s3 source"], type: "fix", breaking_change: false, author: "Jeremy Parker", files_count: 1, insertions_count: 21, deletions_count: 1},
		{sha: "63ce446931e8679b9e5217483d6ffbb607be93ac", date: "2022-11-24 05:34:16 UTC", description: "bump crossbeam-queue from 0.3.7 to 0.3.8", pr_number: 15329, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "188a565160c1351d05434c0ea1d4ca661c7f6854", date: "2022-11-24 06:01:26 UTC", description: "Add log namespace and schema def support", pr_number: 15207, scopes: ["internal_logs source"], type: "feat", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 167, deletions_count: 20},
		{sha: "eb97d50b8d47c02fc6935135d3aeadf4032da06d", date: "2022-11-24 06:30:00 UTC", description: "Add log namespace and schema def support", pr_number: 15274, scopes: ["http_server source"], type: "feat", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 360, deletions_count: 123},
		{sha: "2ab07ae60653ba91c1d873bfca8c7499171ef840", date: "2022-11-24 07:30:29 UTC", description: "bump serde_json from 1.0.87 to 1.0.89", pr_number: 15327, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "134b70a5224d57754d080e607774775150ab1909", date: "2022-11-24 08:08:54 UTC", description: "Add log namespace and schema def support", pr_number: 15296, scopes: ["heroku_logs source"], type: "feat", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 302, deletions_count: 114},
		{sha: "5d610b6f3e9935315cf8ab059fbee3f3f1a0fee4", date: "2022-11-25 00:13:59 UTC", description: "bump openssl from 0.10.42 to 0.10.43", pr_number: 15338, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "bf7c87f98702da8f604c44f10aa994fafae90326", date: "2022-11-26 01:49:49 UTC", description: "bump clap from 4.0.26 to 4.0.27", pr_number: 15350, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 123, deletions_count: 15},
		{sha: "87cbfb927e355f731ba55e058410c240e4d82f1b", date: "2022-11-26 01:53:22 UTC", description: "bump flate2 from 1.0.24 to 1.0.25", pr_number: 15349, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "3fc976d2c91e53729dfd932e5d3460d730c7b188", date: "2022-11-26 01:56:27 UTC", description: "bump env_logger from 0.9.3 to 0.10.0", pr_number: 15347, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "240738b0839e455c04b8e61b6c3fc0d3950e4a4f", date: "2022-11-26 02:23:23 UTC", description: "bump rust_decimal from 1.26.1 to 1.27.0", pr_number: 15351, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 78, deletions_count: 8},
		{sha: "128dbf818208d8d8dfd77a57cb8e82c3dfd6e9f5", date: "2022-11-29 08:04:22 UTC", description: "typo", pr_number: 15355, scopes: [], type: "docs", breaking_change: false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "783a0ba4468b0207a5849e26cf7dbbc084569457", date: "2022-11-29 01:23:37 UTC", description: "bump syn from 1.0.103 to 1.0.104", pr_number: 15361, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f4b031266d94765c58ca799a9fe8caa60738de36", date: "2022-11-29 01:25:10 UTC", description: "bump serde from 1.0.147 to 1.0.148", pr_number: 15359, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "6fe5400eb48f9efe38effe1de64828ca55374af1", date: "2022-11-29 08:41:43 UTC", description: "Add log namespace and schema def support", pr_number: 15354, scopes: ["gcp source"], type: "feat", breaking_change: false, author: "Stephen Wakely", files_count: 3, insertions_count: 172, deletions_count: 14},
		{sha: "f8d91bd270c19a7a147220863b9ca764eb44c61d", date: "2022-12-02 08:17:56 UTC", description: "refactor JSON-encoded event size calculations", pr_number: 15051, scopes: [], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 65, insertions_count: 890, deletions_count: 1245},
		{sha: "336babb72cdadebef2fa5d86bb8e8dadd18d6d93", date: "2022-12-01 07:52:48 UTC", description: "align object prefixes with DD rehydration", pr_number: 15387, scopes: ["datadog_archives sink"], type: "fix", breaking_change: false, author: "Vladimir Zhuk", files_count: 2, insertions_count: 8, deletions_count: 2},
		{sha: "1651dd874f63327921e8adef81f1d28519725ed4", date: "2022-12-03 16:05:34 UTC", description: "use in-memory size for JSON encoded size of metrics", pr_number: 15438, scopes: [], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 1, insertions_count: 3, deletions_count: 1},
	]
}
