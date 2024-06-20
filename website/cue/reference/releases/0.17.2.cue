package metadata

releases: "0.17.2": {
	date:     "2021-10-18"
	codename: ""

	description: """
		The Vector team is pleased to announce version `v0.17.2`!

		This release contains one additional bug fix to prefer fields decoded from the incoming event. This only
		mattered if you were using the new `decoding` feature with the `json` codec.

		**Note:** Please see the release notes for [`v0.17.0`](/releases/0.17.0/) for additional changes if upgrading from
		`v0.16.X`. In particular, the upgrade guide for breaking changes.
		"""

	whats_next: []

	commits: [
		{sha: "996c619254f7de97c037d1979ee21f337c83ce0c", date: "2021-10-18 22:30:32 UTC", description: "Upgrade download template", pr_number: 9656, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Luc Perkins", files_count: 4, insertions_count: 102, deletions_count: 145},
		{sha: "f1cf62d810c759a3a6388f6d60f1024fb2805768", date: "2021-10-16 13:38:26 UTC", description: "Add precedence for event data over metadata", pr_number: 9641, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Pablo Sichert", files_count: 20, insertions_count: 225, deletions_count: 92},
		{sha: "04acb94f4d776224d7f58149e2fac8495244a844", date: "2021-10-18 21:36:29 UTC", description: "Ignore RUSTSEC-2020-0071", pr_number: 9674, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "e77e8b21fd10e540c791b40267e26588d4005603", date: "2021-10-09 03:07:37 UTC", description: "`cargo update -p nix`", pr_number: 9532, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "04513f2718df1815a25f0ce65fb6023ef4b5dd6a", date: "2021-10-19 04:51:39 UTC", description: "Ignore related RUSTSEC-2020-0159 for now", pr_number: 9686, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "9b227504b06e4be678818534d556e5d2e8b51c54", date: "2021-10-11 01:28:14 UTC", description: "Fix issue identified by markdownlint that is blocking checks", pr_number: 9545, scopes: [], type: "docs", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 6, deletions_count: 3},
	]
}
