package metadata

releases: "0.17.1": {
	date:     "2021-10-15"
	codename: ""

	description: """
		The Vector team is pleased to announce version `v0.17.1`!

		This release contains a few bug fixes from the `v0.17.0` release to restore compatibility with existing source
		event decoding as well as a fix for the `events_out_total` metric where it was double the value it should have been.

		**Note:** Please see the release notes for [`v0.17.0`](/releases/0.17.0/) for additional changes if upgrading from
		`v0.16.X`. In particular, the upgrade guide for breaking changes.
		"""

	whats_next: []

	commits: [
		{sha: "d73f9fb23d0bd47e49c0bd27024683d61c239e1e", date: "2021-10-09 04:14:55 UTC", description: "Fix log/metric example output", pr_number: 9542, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Luc Perkins", files_count: 2, insertions_count: 36, deletions_count: 25},
		{sha: "6d9a11ef6d269b7dcec2134f9c1210b05b972ef8", date: "2021-10-11 07:17:54 UTC", description: "Add link checking to preview builds for vector.dev", pr_number: 8944, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Luc Perkins", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "27f78c2c671ad073c3748874f773a5608f37f2ec", date: "2021-10-15 02:20:24 UTC", description: "Reset default max_length for character_delimited framing", pr_number: 9594, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "39a40c871093c85773af7e7fbd19b28f44aecd0e", date: "2021-10-15 09:32:47 UTC", description: "Default framing to `bytes` for message based sources and to `newline_delimited` for stream based sources", pr_number: 9567, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Pablo Sichert", files_count: 26, insertions_count: 426, deletions_count: 187},
		{sha: "2ce803f45f61c79aa5ea89b930b90e496239d728", date: "2021-10-15 05:09:08 UTC", description: "Update bytes codec to actually read full message", pr_number: 9613, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 11, deletions_count: 16},
		{sha: "4faa4a550f1c65327da25b531dc35960a1b322fe", date: "2021-10-15 08:08:17 UTC", description: "Re-add max_length configuration", pr_number: 9621, scopes: ["sources"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 6, insertions_count: 121, deletions_count: 62},
		{sha: "10bc1cbeb9331228ce38f9ce5372ac4b45c97f32", date: "2021-10-15 19:23:09 UTC", description: "parse the schema columns using Conversion", pr_number: 9583, scopes: ["enriching"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 118, deletions_count: 42},
		{sha: "6c0cbecd56dcf3966ed3caa4d75c1cc218d5dfa0", date: "2021-10-15 13:56:51 UTC", description: "Fixes enrichment tables in test (again)", pr_number: 9612, scopes: ["enriching"], type: "fix", breaking_change: false, author: "Danny Browning", files_count: 2, insertions_count: 4, deletions_count: 2},
		{sha: "50af759b901b633c1a420f2e8f536658aa121735", date: "2021-10-16 03:35:07 UTC", description: "added behaviour test for enrichment tables", pr_number: 9633, scopes: ["enriching"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 59, deletions_count: 0},
		{sha: "9125688ea2fe822c5626340f95eda2a6b7cff304", date: "2021-10-15 22:57:18 UTC", description: "Drop default max framing length", pr_number: 9625, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 48, deletions_count: 37},
		{sha: "19362a906c85d7be9b58ab0065723432aadc7368", date: "2021-10-16 00:09:30 UTC", description: "Re-add support for encoding", pr_number: 9640, scopes: ["codecs", "http source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 85, deletions_count: 12},
		{sha: "3c7413dedd4826267e1947596c83f0f2b8f52227", date: "2021-10-18 20:59:16 UTC", description: "Remove duplicate counter emit `events_out_total`", pr_number: 9668, scopes: ["observability"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},

	]
}
