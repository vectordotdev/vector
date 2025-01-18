package metadata

releases: "0.43.1": {
	date:     "2024-12-10"
	codename: ""

	whats_next: []

	description: """
		This patch release contains fixes for regressions in 0.43.0.
		"""

	known_issues: [
		"""
			The `vector-0.43.1-x86_64-apple-darwin.tar.gz` executable has the wrong architecture, see
			[#22129](https://github.com/vectordotdev/vector/issues/22129). This will be fixed in
			`v0.44`.
			""",
	]

	changelog: [
		{
			type: "fix"
			description: """
				Update to VRL v0.20.1 which reverts to previous `to_float` behavior for non-normal floats.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				Emit `build_info` gauge on an interval to avoid expiration.
				"""
			contributors: ["jszwedko"]
		},
		{
			type: "fix"
			description: """
				Fix `reduce` transform to quote invalid paths by default. Quoting make those paths valid.
				"""
			contributors: ["pront"]
		},
	]

	commits: [
		{sha: "ca3abef14605dfbdca6060bbcd038fd7abfec6f0", date: "2024-12-09 23:11:21 UTC", description: "enable quoting for invalid fields", pr_number: 21989, scopes: ["reduce transform"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 46, deletions_count: 7},
		{sha: "43b8916fa5b381cc90a22c787a574c8fc75550b5", date: "2024-12-10 08:25:25 UTC", description: "Emit `build_info` gauge on an interval", pr_number: 21991, scopes: ["internal_metrics source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 13, deletions_count: 10},
	]
}
