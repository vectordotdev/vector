package metadata

releases: "0.13.1": {
	date:     "2021-04-29"
	codename: ""

	description: """
			This release includes a high priority bug fix for a regression in
			0.13.0 that caused very high memory use when using the `kafka`
		    source and backpressure was experienced.
		"""

	whats_next: []

	commits: [
		{sha: "420a74fe57cf81984db72bf319d4e7eba1059e7c", date: "2021-04-29 00:44:52 UTC", description: "Fix runaway memory usage of kafka source", pr_number: 7266, scopes: ["kafka source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 118, deletions_count: 126},
	]
}
