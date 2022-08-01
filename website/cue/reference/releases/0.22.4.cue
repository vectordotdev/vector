package metadata

releases: "0.22.4": {
	date:     "2022-08-01"
	codename: ""

	whats_next: []
	description: """
		The Vector team is pleased to announce version 0.22.4!

		**Note:** Please see the release notes for [`v0.22.0`](/releases/0.22.0/) for additional changes if upgrading from
		`v0.21.X`. In particular, the upgrade guide for breaking changes.
		"""
	changelog: [
		{
			type: "fix"
			scopes: ["codec"]
			description: """
				Vector no longer shuts down when a configured source codec (`decoding.codec`) receives invalid data.
				"""
			pr_numbers: [13737]
		},
	]

	commits: [
		{sha: "5c4a12c6515debe9a159b22dc5406223e1c4dbb7", date: "2022-06-07 22:39:18 UTC", description: "fix if statement type definitions", pr_number:                                                 12954, scopes: ["vrl"], type:    "fix", breaking_change:  false, author: "Nathan Fox", files_count:      17, insertions_count: 257, deletions_count: 35},
		{sha: "98447e2b596779ebc4567a78821aa0ea43afce0d", date: "2022-07-14 00:52:33 UTC", description: "Include breaking change for watch-config in 0.21", pr_number:                                  13536, scopes: [], type:         "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count:  15, deletions_count:  2},
		{sha: "b8517902c5da893eef9b914fc92e8ad56f095f73", date: "2022-07-30 14:03:59 UTC", description: "Fix bug where Vector shuts down when a decoding error is returned to `FramedRead`", pr_number: 13737, scopes: ["codecs"], type: "fix", breaking_change:  false, author: "Pablo Sichert", files_count:   3, insertions_count:  44, deletions_count:  2},
	]
}
