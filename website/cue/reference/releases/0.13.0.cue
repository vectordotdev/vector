package metadata

releases: "0.13.0": {
	date: "2021-04-21"

	description: """
		The Vector team is pleased to announce version 0.13.0!

		This release brings a new `datadog_logs` source to accept logs from [dd-agents](\(urls.datadog_agent)), a new
		`gcp_stackdriver_metrics` sink to send metrics to [GCP Stackdriver](\(urls.gcp_stackdriver)), and a new
		subcommand, `vector tap` that allows for inspecting events flowing out of a component. It also brings a number
		of smaller enhancements, particularly around the [Vector Remap Language](\(urls.vrl_reference)) used by our
		`remap`, `filter`, and `route` transforms.

		Check out the [highlights](/releases/0.13.0#highlights) and [changelog](/releases/0.13.0#changelog) for more
		details.
		"""

	whats_next: [
		{
			title: "End-to-end acknowledgements"
			description: """
				We've heard from a number of users that they'd like improved delivery guarantees for events flowing
				through Vector. We are working on a feature to allow, for components that are able to support it, to
				only acknowledging data flowing into source components after that data has been sent by any associated
				sinks. For example, this would avoid acknowledging messages in Kafka until the data in those messages
				has been sent via all associated sinks.
				"""
		},
		{
			title: "Mapping and iteration in VRL"
			description: """
				Shortly you will be able to iterate over data in VRL to:

				- allow processing of fields containing arrays of unknown length
				- map over keys and values in objects to transform them
				"""
		},
		{
			title:       "Kubernetes aggregator role"
			description: """
				We are hard at work at expanding the ability to run Vector as an [aggregator in
				Kubernetes](\(urls.vector_aggregator_role)). This will allow you to build end-to-end observability
				pipelines in Kubernetes with Vector. Distributing processing on the edge, centralizing it with an
				aggregator, or both. If you are interested in beta testing, please [join our chat](\(urls.vector_chat))
				and let us know.

				This was mentioned in the release notes for 0.11.0, but didn't quite make it for 0.12 as anticipated. We
				do expect it to be released in the next few months.
				"""
		},
	]

	commits: [
		{sha: "81e045e3d3b33d40399e5757ebb30c55c3da901f", date: "2021-03-15 01:43:25 UTC", description: "Add new VRL comparison benchmark", pr_number: 6387, scopes: ["ci"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count: 7, insertions_count: 582, deletions_count: 2},
		{sha: "ef32bab16a9256782a853bb82edc8b0d54a8ea71", date: "2021-03-15 20:49:32 UTC", description: "Use next_addr instead of fixed addresses for prometheus_exporter tests", pr_number: 6766, scopes: ["tests"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 10, deletions_count: 16},
		{sha: "c3f4a50c1ab5e0f0589db60c24a4177ff090e94c", date: "2021-03-18 23:03:32 UTC", description: "Add `events_in_total` internal metric to sources", pr_number: 6758, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 51, insertions_count: 280, deletions_count: 46},
		{sha: "ab0beeeb5b2e7736816a529dce8f18bef8342414", date: "2021-03-18 23:23:54 UTC", description: "Correct the glibc requirements for packages", pr_number: 6774, scopes: ["releasing"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 24, deletions_count: 8},
		{sha: "14d604b66a964fe4a5c828eb502ebb6c8efe6980", date: "2021-03-19 22:24:45 UTC", description: "Use regular markdown image syntax in reference section", pr_number: 6835, scopes: ["internal docs"], type: "fix", breaking_change: false, author: "Steve Hall", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7faefad7c1677a1c1e7c30ecc5289a6a79fa26a2", date: "2021-03-20 06:44:17 UTC", description: "Fix workflow for packaging for Debian", pr_number: 6824, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 24, deletions_count: 22},
		{sha: "65168841912190058e5ef08fef2f1f7ca9f466ca", date: "2021-03-22 17:13:43 UTC", description: "New `datadog_logs` source", pr_number: 6744, scopes: ["new source"], type: "feat", breaking_change: false, author: "Pierre Rognant", files_count: 8, insertions_count: 538, deletions_count: 109},
		{sha: "ef8a63b14bae0f8ecb90434358af83823d201f53", date: "2021-03-22 17:16:24 UTC", description: "Explicitly mount docker socket for AWS ECS metadata mock container", pr_number: 6833, scopes: ["tests"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b2008a948a93e8c89296a7d3f8199e8c8ac81c92", date: "2021-03-22 23:01:46 UTC", description: "`--no-topology` has been removed from `vector validate`", pr_number: 6851, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Robin Schneider", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "c6526367d9a822ccb7cfb73b7a171bfe61279260", date: "2021-03-26 23:03:30 UTC", description: "Add `abort` statement to halt processing", pr_number: 6723, scopes: ["remap"], type: "feat", breaking_change: false, author: "Jean Mertz", files_count: 19, insertions_count: 207, deletions_count: 64},
		{sha: "927656ca4d644519bb3c1f992e3f40252f9da77d", date: "2021-03-29 19:34:36 UTC", description: "Add `parse_query_string` function", pr_number: 6796, scopes: ["remap"], type: "feat", breaking_change: false, author: "Vladimir Zhuk", files_count: 5, insertions_count: 218, deletions_count: 0},
		{sha: "36fb3a6232b8d6e3f15066a123203a47ac37ae61", date: "2021-03-30 08:26:15 UTC", description: "No longer hang indefinitely on invalid TCP data", pr_number: 6864, scopes: ["syslog source"], type: "fix", breaking_change: false, author: "FungusHumungus", files_count: 1, insertions_count: 265, deletions_count: 38},
		{sha: "81f9579d1d0a30de82bd89e32e2a851462fdc493", date: "2021-03-30 23:59:32 UTC", description: "New `parse_linux_authorization` function", pr_number: 6883, scopes: ["remap"], type: "feat", breaking_change: false, author: "Vladimir Zhuk", files_count: 5, insertions_count: 97, deletions_count: 2},
		{sha: "d50b7040f07956f1f0ba8ff342ac40eff182811e", date: "2021-03-30 21:05:37 UTC", description: "Support VECTOR_LOG_FORMAT and VECTOR_COLOR environment variables", pr_number: 6930, scopes: ["cli"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 2},
		{sha: "4c42b7d9d259114251a1a1b3c2b7fb213cc04c49", date: "2021-03-19 22:24:45 UTC", description: "Use regular markdown image syntax in reference section", pr_number: 6835, scopes: ["internal docs"], type: "fix", breaking_change: false, author: "Steve Hall", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f5feb66575950a1e022dbd884f73dbbb78ef4c1b", date: "2021-03-31 17:31:21 UTC", description: "Support newlines in if-statement", pr_number: 6893, scopes: ["remap"], type: "enhancement", breaking_change: false, author: "Jean Mertz", files_count: 2, insertions_count: 44, deletions_count: 3},
		{sha: "9fa47280c97c4ca8b252f93e220a61350b6c876a", date: "2021-03-31 21:09:51 UTC", description: "Allow newlines in REPL", pr_number: 6894, scopes: ["remap"], type: "fix", breaking_change: false, author: "Jean Mertz", files_count: 1, insertions_count: 35, deletions_count: 15},
		{sha: "5ea5350fb49728142794ccfb3eb90aa969090728", date: "2021-03-31 21:11:07 UTC", description: "Allow non-matching brackets in REPL; would previously hang", pr_number: 6896, scopes: ["remap"], type: "fix", breaking_change: false, author: "Jean Mertz", files_count: 1, insertions_count: 21, deletions_count: 25},
		{sha: "6a35516fb7b64b690620f53758d1e563c37f00ff", date: "2021-03-31 21:12:19 UTC", description: "Support dropping events when remap fails or is aborted via new `abort` statement", pr_number: 6722, scopes: ["remap"], type: "feat", breaking_change: false, author: "Jean Mertz", files_count: 8, insertions_count: 195, deletions_count: 19},
		{sha: "f6f3aeaa6463d95acc62d0f67dfb3efef940b78b", date: "2021-03-31 21:47:46 UTC", description: "Add defaults for metadata keys so that they are added by default", pr_number: 6928, scopes: ["kafka source"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 2, insertions_count: 51, deletions_count: 43},
		{sha: "e7be8796a3d0a5d0526838d0f855e5760dff7724", date: "2021-03-31 22:40:18 UTC", description: "`route` transform can now process metrics", pr_number: 6927, scopes: ["route transform"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 2, insertions_count: 38, deletions_count: 6},
		{sha: "b7d7c4a9d3e0b8f79171cc92bef593b74a737a58", date: "2021-03-31 22:58:23 UTC", description: "introduce `is_TYPE` helper functions such as `is_string`", pr_number: 6775, scopes: ["remap"], type: "enhancement", breaking_change: false, author: "Pierre Rognant", files_count: 21, insertions_count: 1310, deletions_count: 0},
		{sha: "9c9a7a33519b849c2d12ea6cd4ffd9ad1af2be83", date: "2021-04-01 04:09:46 UTC", description: "Typo on the expectation for building regexps", pr_number: 6951, scopes: ["exceptions"], type: "fix", breaking_change: false, author: "Jérémie Drouet", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "4279482cda8cf782bca476759ccb84bb15be3f4e", date: "2021-03-31 22:10:02 UTC", description: "Fix release-homebrew workflow dependency", pr_number: 6945, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 6},
		{sha: "69065a1836bb50e540219ae5723948c7fb59c67d", date: "2021-03-31 22:10:42 UTC", description: "Fix package naming for latest artifacts", pr_number: 6947, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "a7d3f047dbfc2ca2df1fa56ae187d504e8113c8a", date: "2021-03-31 22:01:44 UTC", description: "Export default container tool setting", pr_number: 6954, scopes: ["tests"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 3},
		{sha: "ea07266a8c5d5035d7fd0ffa5659dd60676ece41", date: "2021-04-01 20:22:31 UTC", description: "Add `parse_csv` function", pr_number: 6853, scopes: ["remap"], type: "feat", breaking_change: false, author: "Vladimir Zhuk", files_count: 6, insertions_count: 205, deletions_count: 0},
		{sha: "7cba83bb490fb2698e585f7d6a27ef3da96da3c5", date: "2021-04-03 00:01:28 UTC", description: "Upgrade reqwest in k8s-e2e-tests", pr_number: 6990, scopes: ["deps"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 101, deletions_count: 294},
		{sha: "a2079ced236971df522f8bda64ffee29fd3a44c3", date: "2021-04-03 01:06:59 UTC", description: "Retry errors in Kafka integration tests", pr_number: 6992, scopes: ["tests"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 9, deletions_count: 6},
		{sha: "d37e5e7c0dd14f772ed6b1ff69bcec202bc55d07", date: "2021-04-05 23:34:07 UTC", description: "Fix filtering of filesystem metrics by device", pr_number: 6921, scopes: ["host_metrics source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 62, deletions_count: 60},
		{sha: "344c358cde696385252656ccbea82c631c386f5e", date: "2021-04-06 00:47:38 UTC", description: "Ensure /etc/default/vector is not automatically overwritten", pr_number: 7007, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "1157d519fcca2189949d34f02a2d7829d210ab80", date: "2021-04-07 00:45:26 UTC", description: "Add gcp_stackdriver_metrics sink.", pr_number: 6666, scopes: ["new sink"], type: "feat", breaking_change: false, author: "Yo Eight", files_count: 7, insertions_count: 613, deletions_count: 8},
		{sha: "e06248659c5fa55e8829562295c4522cc058692d", date: "2021-04-07 03:35:04 UTC", description: "Additions to GraphQL API to allow tapping the events flowing out of a component", pr_number: 6610, scopes: ["graphql api"], type: "enhancement", breaking_change: false, author: "Lee Benson", files_count: 16, insertions_count: 1107, deletions_count: 104},
		{sha: "52daacdeb03e0a74965b7e86ee43dc353ec99921", date: "2021-04-07 03:40:42 UTC", description: "Vector `tap` subcommand to view events flowing out of a component", pr_number: 6871, scopes: ["observability"], type: "feat", breaking_change: false, author: "Lee Benson", files_count: 18, insertions_count: 346, deletions_count: 98},
		{sha: "56e47ed678ea43b5377bd57dc272d48d7f8b52d7", date: "2021-04-07 14:29:00 UTC", description: "Add `json` to `Log` type to expose the log event as JSON", pr_number: 6915, scopes: ["graphql api"], type: "enhancement", breaking_change: false, author: "Lee Benson", files_count: 4, insertions_count: 14, deletions_count: 3},
		{sha: "f8023c763e7c0dfcb0f004201ad4dffa17f29a8a", date: "2021-04-07 21:20:28 UTC", description: "Add `parse_nginx_log` function for parsing common nginx log formats", pr_number: 6973, scopes: ["remap"], type: "feat", breaking_change: false, author: "Jérémie Drouet", files_count: 7, insertions_count: 420, deletions_count: 2},
		{sha: "6431d0b8d1f2b93c0771d126b7b3e5dba1ccadb6", date: "2021-04-07 21:32:30 UTC", description: "Fix formatting of timestamps in tests", pr_number: 7036, scopes: ["tests"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 25, deletions_count: 26},
		{sha: "6c0df5614a10c4ef7d8c5e18d3bba289c6a7a0b5", date: "2021-04-08 20:48:26 UTC", description: "Kafka message headers are now added as fields on the event", pr_number: 7030, scopes: ["kafka source"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 55, deletions_count: 3},
		{sha: "2f63eb954e72e323203abff679ba45892adb7adf", date: "2021-04-09 20:26:50 UTC", description: "Fix aws_ec2_metadata transform test", pr_number: 7062, scopes: ["tests"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "31d56e71529d19a95667c22479d27a4855943c6d", date: "2021-04-12 23:30:11 UTC", description: "`parse_regex` now suppresses the numeric capture groups by default but they can be re-added via `numeric_groups: true` in the function call", pr_number: 7069, scopes: ["remap"], type: "enhancement", breaking_change: true, author: "FungusHumungus", files_count: 8, insertions_count: 164, deletions_count: 75},
		{sha: "03cee23491318b30a57f8143339e727311baff30", date: "2021-04-13 02:02:24 UTC", description: "Prevent lexer getting stuck on an unterminated literal which would cause VRL to hang", pr_number: 7037, scopes: ["remap"], type: "fix", breaking_change: false, author: "FungusHumungus", files_count: 3, insertions_count: 102, deletions_count: 30},
		{sha: "0047abd456a62268ed444f3810b6a9f98d082f9a", date: "2021-04-12 23:14:39 UTC", description: "Update homebrew when bootstrapping OSX", pr_number: 7095, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "0fb0184fd291880729cde1eb01381889e3a70683", date: "2021-04-13 01:40:49 UTC", description: "Support reading the channel from the `channel` query parameter rather than just the `X-Splunk-Request-Channel` header", pr_number: 7067, scopes: ["splunk_hec source"], type: "enhancement", breaking_change: false, author: "Or Ricon", files_count: 2, insertions_count: 105, deletions_count: 21},
		{sha: "8b962a5ff9618835273241f8ac43fbb722638e88", date: "2021-04-13 18:53:31 UTC", description: "Document existence of region parameter", pr_number: 7093, scopes: ["datadog service"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 4, deletions_count: 0},
		{sha: "48d2a84b1b11ba54db7bd892944f2a479238edb4", date: "2021-04-14 02:33:05 UTC", description: "Remove socket file on source shutdown which would otherwise prevent Vector from restarting", pr_number: 7047, scopes: ["socket source"], type: "fix", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 5, insertions_count: 147, deletions_count: 32},
		{sha: "01ea5d1db9e35aa951059cbcac8d172f5d67acd3", date: "2021-04-14 14:33:44 UTC", description: "Clickhouse sink now correctly encodes fields containing an array", pr_number: 7081, scopes: ["clickhouse sink"], type: "fix", breaking_change: false, author: "舍我其谁", files_count: 1, insertions_count: 14, deletions_count: 9},
		{sha: "e4bf41dab7ae6f21631ed609c34524cb712e8f2d", date: "2021-04-14 23:40:40 UTC", description: "Can now read logs from containers that have an attached TTY", pr_number: 7111, scopes: ["docker_logs source"], type: "fix", breaking_change: false, author: "Jean Prat", files_count: 1, insertions_count: 56, deletions_count: 6},
		{sha: "4f1eeb6596c5b73441b6c352b5c4787ab25c3efb", date: "2021-04-15 18:16:47 UTC", description: "Ensure all benchmark artifacts are included", pr_number: 7113, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 3},
		{sha: "5f461184640db8b74f5bc4494acdb453f705af75", date: "2021-04-16 17:47:34 UTC", description: "Preserve type defs when assigning fields to avoid unnecessary type assertions and confusing error messages", pr_number: 7118, scopes: ["remap"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 13, deletions_count: 12},
		{sha: "ff969e9ab5bf71d8649ae8da00f6f89ae70e89ff", date: "2021-04-16 19:44:45 UTC", description: "Actually use the checkpoint files to avoid re-ingesting logs when Vector is restarted", pr_number: 7140, scopes: ["kubernetes_logs source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 16, deletions_count: 11},
		{sha: "ce0064b13d7eb4e91955722f528e9d16bc023723", date: "2021-04-16 22:00:07 UTC", description: "Defaults for `case_sensitive` parameter for string matching functions (like `starts_with`) were updated to default to case sensitive matching to match docs", pr_number: 7091, scopes: ["remap"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 40, deletions_count: 38},
		{sha: "593c37bc982c1dcdbdefe9bb4f23b9a0fb18583f", date: "2021-04-17 00:57:06 UTC", description: "Fix compression handling for `aws_s3`, `aws_kinesis_firehose`, `splunk_hec`, and `http` sources to handle multi-part files", pr_number: 7138, scopes: ["compression"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 13, insertions_count: 80, deletions_count: 21},
		{sha: "75332526486427b0ce26386f3894fba26e633b15", date: "2021-04-19 17:36:36 UTC", description: "Ensure docker labels are not injected as nested objects if they contain a `.`", pr_number: 7152, scopes: ["docker_logs source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 52, deletions_count: 15},
		{sha: "b25e0cb3a74da87d205b7673dedda2fdeac1307a", date: "2021-04-20 18:24:32 UTC", description: "Refactor stream and invocation errors to support recoverable error types", pr_number: 6816, scopes: ["kubernetes_logs source"], type: "fix", breaking_change: false, author: "Ian Henry", files_count: 5, insertions_count: 218, deletions_count: 15},
		{sha: "87a3a18c619dab59ec37f42231e74ee4f81bec14", date: "2021-04-20 20:01:42 UTC", description: "Add `to_regex` function  to convert strings to regexes", pr_number: 7074, scopes: ["remap"], type: "feat", breaking_change: false, author: "Jake He", files_count: 5, insertions_count: 129, deletions_count: 0},
		{sha: "81b48281aefc2bf4117d58385204711494266954", date: "2021-04-21 03:49:19 UTC", description: "Support identifiers with leading numeric characters as fields in paths", pr_number: 7045, scopes: ["remap"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 4, insertions_count: 36, deletions_count: 6},
	]
}
