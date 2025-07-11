package metadata

releases: "0.21.2": {
	date:     "2022-05-04"
	codename: ""

	whats_next: []

	description: """
		This patch release contains a few fixes for regressions in 0.21.0.

		**Note:** Please see the release notes for [`v0.21.0`](/releases/0.21.0/) for additional changes if upgrading from
		`v0.20.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				Vector's VRL REPL no longer loses variable assignments when the expression errors.
				"""
			pr_numbers: [12405]
		},
		{
			type: "fix"
			scopes: ["docker platform"]
			description: """
				Vector's docker images no longer need a volume mounted at `/var/lib/docker` to run correctly with the
				default `data_dir` configuration.
				"""
			pr_numbers: [12414]
		},
		{
			type: "fix"
			scopes: ["aws provider"]
			description: """
				AWS components now allow configuration of the load timeout for credentials (`load_timeout_secs`). In
				0.21.0, the default load timeout was inadvertently changed to the new default timeout of the new Rust
				AWS SDK which dropped it from 30 seconds to 5 seconds. This new configuration option allows
				configuring it a higher value when necessary.
				"""
			pr_numbers: [12422]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				`vector generate` now works correctly again when `datadog_agent` is used as a source rather than logging
				an error.
				"""
			pr_numbers: [12470]
		},
		{
			type: "fix"
			scopes: ["aws provider"]
			description: """
				AWS components now check for a region at Vector start-up time so that they return an error rather than
				starting but failing to make requests later due to the lack of region configuration.
				"""
			pr_numbers: [12475]
		},
	]

	commits: [
		{sha: "8655f0fa73a69a0aec9e723c1ae108bb3558d8bd", date: "2022-04-23 02:53:19 UTC", description: "Fix version typo in 0.21.1 release notes", pr_number: 12371, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "19e2cff5f3b8a3e727d2fa0f11ab7e2903116ccb", date: "2022-04-25 22:29:21 UTC", description: "Note version support for trace ingestion", pr_number: 12378, scopes: ["datadog_agent source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 14, deletions_count: 6},
		{sha: "3f52ab15cecb2dc1891dbadad8575a647831e7e8", date: "2022-04-25 22:38:39 UTC", description: "Note new lack of IMDSv1 support for authentication", pr_number: 12377, scopes: ["aws provider"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 10, deletions_count: 3},
		{sha: "b0e83fe1a9877122b8b3f6140e0a03f23012fa50", date: "2022-04-26 06:44:45 UTC", description: "improve `datadog_agent` source doc", pr_number: 10371, scopes: ["external docs"], type: "enhancement", breaking_change: false, author: "Pierre Rognant", files_count: 2, insertions_count: 16, deletions_count: 6},
		{sha: "37e703cb18779a6a33a92e79a15d88619e2ab9cc", date: "2022-04-30 10:10:41 UTC", description: "Add Attributes note to datadog_logs sink", pr_number: 12499, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 10, deletions_count: 0},
		{sha: "f79b62b7a873a6164653e8a16b80232259d7a4f3", date: "2022-04-26 13:13:55 UTC", description: "remember local state after compilation error in REPL", pr_number: 12405, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Jean Mertz", files_count: 5, insertions_count: 43, deletions_count: 23},
		{sha: "f3b75a6a8523d1b1708c91eae6ce608c02e2eb5d", date: "2022-04-27 07:26:29 UTC", description: "add empty `/var/lib/vector` directory to Docker images", pr_number: 12414, scopes: ["docker platform"], type: "fix", breaking_change: false, author: "Hugo Hromic", files_count: 6, insertions_count: 14, deletions_count: 0},
		{sha: "6cb290664dd53652e769d6e9d961ce522cef5a0b", date: "2022-04-28 02:39:24 UTC", description: "Allow configuring the load timeout for AWS credentials", pr_number: 12422, scopes: ["aws provider"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 72, deletions_count: 17},
		{sha: "1f42e076f902d089dcbc255da3c761e5ca87da6f", date: "2022-04-29 06:48:27 UTC", description: "add sleep_impl to client builder", pr_number: 12462, scopes: ["aws provider"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 10, insertions_count: 87, deletions_count: 2},
		{sha: "5f799e7a9138e48ca60ff1714ac8b7ae0cc79377", date: "2022-04-30 00:19:10 UTC", description: "Submit source to inventory", pr_number: 12470, scopes: ["datadog_agent source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "d4e5f425a68554362325499e908e96624a2a5a37", date: "2022-05-05 04:32:04 UTC", description: "Use default region provider if region unspecified", pr_number: 12475, scopes: ["aws provider"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 13, insertions_count: 76, deletions_count: 33},
		{sha: "8e371f24c534dc9c78ac7c4dcf565e4fe1640af8", date: "2022-05-06 01:00:52 UTC", description: "Disable retries", pr_number: 12611, scopes: ["aws provider"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 8, insertions_count: 64, deletions_count: 0},
	]
}
