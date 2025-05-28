package metadata

releases: "0.32.2": {
	date:     "2023-09-20"
	codename: ""

	whats_next: []
	description: """
		This patch release contains a fix for a regression in 0.32.0.

		**Note:** Please see the release notes for [`v0.32.0`](/releases/0.32.0/) for additional changes if upgrading from
		`v0.31.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["aws provider"]
			description: """
				AWS components again allow use of `assume_role` for authentication without an
				`external_id`. Previously, as of `v032.0`, Vector would panic when starting.
				"""
			pr_numbers: [18452]
		},
	]

	commits: [
		{sha: "3b9144cb411ea91446c445324db714908ccb814a", date: "2023-08-24 03:26:04 UTC", description: "Fix installer list for MacOS", pr_number:                    18364, scopes: ["website"], type:                       "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:     1, insertions_count: 1, deletions_count:  0},
		{sha: "3040ae250b36e5dedda6fd635d364cbd77d0fef8", date: "2023-08-25 08:29:56 UTC", description: "add PGO information", pr_number:                             18369, scopes: [], type:                                "docs", breaking_change:  false, author: "Alexander Zaitsev", files_count: 3, insertions_count: 36, deletions_count: 0},
		{sha: "1164f5525780a9599864bdda46722e895a20fd4c", date: "2023-08-29 17:50:25 UTC", description: "fix some typos", pr_number:                                  18401, scopes: ["file source"], type:                   "docs", breaking_change:  false, author: "geekvest", files_count:          7, insertions_count: 8, deletions_count:  8},
		{sha: "dd460a0bf91d210e262b1953a6afcaf3aa8f3033", date: "2023-09-07 00:30:03 UTC", description: "Fix docs for `host` field for syslog source", pr_number:     18453, scopes: ["syslog source", "docs"], type:         "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:     1, insertions_count: 8, deletions_count:  2},
		{sha: "9356c56b86817fdca931168986b3e9c88aea1be9", date: "2023-09-09 01:06:01 UTC", description: "Update the AWS authentication documentation", pr_number:     18492, scopes: ["aws provider", "external_docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count:     1, insertions_count: 17, deletions_count: 7},
		{sha: "77d12ee88b17d4d71b3609299b356e050afe651a", date: "2023-09-02 00:24:47 UTC", description: "Don't unwap external_id", pr_number:                         18452, scopes: ["aws provider"], type:                  "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:     1, insertions_count: 19, deletions_count: 10},
		{sha: "badb64f451de9169a2b65cbd541427f0411c7764", date: "2023-08-23 06:04:04 UTC", description: "update `rustls-webpki` due to security advisory", pr_number: 18344, scopes: [], type:                                "chore", breaking_change: false, author: "Nathan Fox", files_count:        3, insertions_count: 5, deletions_count:  2},
	]
}
