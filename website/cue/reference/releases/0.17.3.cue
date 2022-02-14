package metadata

releases: "0.17.3": {
	date:     "2021-10-21"
	codename: ""

	description: """
		The Vector team is pleased to announce version `v0.17.3`!

		This patch release contains a bug fix to ensure that Adaptive Concurrency Control is the default for all
		HTTP-based sinks, as was documented in the release notes for `v0.17.0`.

		**Note:** Please see the release notes for [`v0.17.0`](/releases/0.17.0/) for additional changes if upgrading from
		`v0.16.X`. In particular, the upgrade guide for breaking changes.
		"""

	whats_next: []

	commits: [
		{sha: "e09ba73ab05e46e21bfd0959bbb4633d5295e11b", date: "2021-10-20 00:35:41 UTC", description: "Add warnings to config parameter templates", pr_number: 9708, scopes: ["external docs"], type: "fix", breaking_change:   false, author: "Luc Perkins", files_count:   2, insertions_count: 27, deletions_count: 2},
		{sha: "fcd9bed12245dce13a93f708ce079ac1c5d29a45", date: "2021-10-21 01:53:56 UTC", description: "Fix concurrency defaults", pr_number:                   9724, scopes: [], type:                "docs", breaking_change:  false, author: "Jesse Szwedko", files_count: 5, insertions_count: 2, deletions_count:  7},
		{sha: "f42cfb1eade0926face11c4d1930bd86b0ea1e8b", date: "2021-10-21 03:41:26 UTC", description: "Default sink concurrency to adaptive", pr_number:       9723, scopes: ["sinks"], type:         "fix", breaking_change:   false, author: "Jesse Szwedko", files_count: 2, insertions_count: 11, deletions_count: 5},
		{sha: "5b3e0a75c1050d28a14d2c4f87f86b9228d047af", date: "2021-10-20 19:27:51 UTC", description: "Remove Helm release", pr_number:                        9717, scopes: ["releasing"], type:     "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count:  17},
	]
}
