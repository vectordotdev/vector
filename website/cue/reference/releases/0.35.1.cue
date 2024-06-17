package metadata

releases: "0.35.1": {
	date:     "2024-02-06"
	codename: ""

	whats_next: []

	description: """
		This patch release contains fixes for regressions in 0.35.0.

		**Note:** Please see the release notes for [`v0.35.0`](/releases/0.35.0/) for additional changes if upgrading from
		`v0.34.X`. In particular, see the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["http provider"]
			description: """
				The HTTP server-based sources that support the new
				`keepalive.max_connection_age_secs` now only apply this setting for HTTP/0.9,
				HTTP/1.0, and HTTP/1.1 connections, since the implementation, which sends
				a `Connection: Close` header, does not apply to HTTP/2 and HTTP/3 connections.
				"""
			pr_numbers: [19801]
		},
	]

	commits: [
		{sha: "48eac298802a0e483b4ea935478f87915fb99c25", date: "2024-01-26 07:42:41 UTC", description: "Add banner alerting people of package migration", pr_number: 19714, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 6, deletions_count: 23},
		{sha: "58be28d5b0a144de58522aa2ec67700b7878049d", date: "2024-01-27 00:29:39 UTC", description: "Add documentation data", pr_number: 19715, scopes: ["aws_sns sink"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 111, deletions_count: 0},
		{sha: "360ee55f8d2f6dcceaff27bcaa18c5f04c9b6ad9", date: "2024-02-06 06:55:52 UTC", description: "Conditionally send Connection: Close header based on HTTP version", pr_number: 19801, scopes: ["http_server source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 10, insertions_count: 91, deletions_count: 13},
		{sha: "8bfb1e51b5918ee18c398113c0a5c647dc8e08f8", date: "2024-02-07 04:45:20 UTC", description: "Update docs for disabling `max_connection_age_secs`", pr_number: 19802, scopes: ["docs", "http_server source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 10, insertions_count: 31, deletions_count: 19},
	]
}
