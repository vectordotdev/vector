package metadata

releases: "0.20.1": {
	date:     "2022-02-22"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.20.1!

		This patch release contains a few bug fixes for regressions in unit test behavior in 0.20.0.

		**Note:** Please see the release notes for [`v0.20.0`](/releases/0.20.0/) for additional changes if upgrading from
		`v0.19.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["unit tests"]
			description: """
				Unit tests no longer panic if used with a non-existent output.
				"""
			pr_numbers: [11340]
		},
		{
			type: "fix"
			scopes: ["unit tests"]
			description: """
				Unit tests no longer output warnings for unconsumed outputs (e.g. when unit testing the `route` tranform).
				"""
			pr_numbers: [11345]
		},
	]

	commits: [
		{sha: "8ab7f12b5f4350d6b2ab06a0276de69449681eac", date: "2022-02-12 09:01:00 UTC", description: "fix markup", pr_number:                                                  11330, scopes: [], type:             "docs", breaking_change:  false, author: "Tshepang Lekhonkhobe", files_count: 1, insertions_count: 1, deletions_count:   1},
		{sha: "7272c56ecbb745212f20c9665798ce3a2854c8bd", date: "2022-02-15 03:46:25 UTC", description: "Avoid warning logs for unconsumed outputs", pr_number:                   11345, scopes: ["unit tests"], type: "chore", breaking_change: false, author: "Will", files_count:                 2, insertions_count: 64, deletions_count:  4},
		{sha: "4220aadb3ee128f094e5e54fbe097ddc0716c06f", date: "2022-02-12 07:45:39 UTC", description: "Error on non-existent extract_from, no_outputs_from targets", pr_number: 11340, scopes: ["unit tests"], type: "fix", breaking_change:   false, author: "Will", files_count:                 7, insertions_count: 128, deletions_count: 15},
	]
}
