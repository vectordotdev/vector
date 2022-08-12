package metadata

releases: "0.23.3": {
	date:     "2022-08-10"
	codename: ""

	whats_next: []

	description: """
		This patch release contains a few fixes for regressions in 0.23.0.

		This is the first release after 0.23.0, versions 0.23.1 and 0.23.2 were inadvertently skipped due to a [version
		mismatch during release](https://github.com/vectordotdev/vector/pull/13930).

		**Note:** Please see the release notes for [`v0.23.0`](/releases/0.23.0/) for additional changes if upgrading from
		`v0.22.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["codec"]
			description: """
				Vector no longer shutsdown when `encoding.codec` is configured on a source and the
				incoming data does not match the codec.
				"""
			pr_numbers: [13737]
		},
		{
			type: "fix"
			scopes: ["vrl stdlib"]
			description: """
				The `parse_groks` VRL function now correctly removes empty fields, matching the
				expected behavior as of Vector v0.23.0 when the `remove_empty` parameter was
				dropped.
				"""
			pr_numbers: [13721]
		},
		{
			type: "fix"
			scopes: ["codec", "elasticsearch sink"]
			description: """
				For the `elasticsearch` sink, the `encoding.only_fields` and
				`encoding.except_fields` are now correctly applied after any templated strings are
				evaluated.
				"""
			pr_numbers: [13734]
		},
	]

	commits: [
		{sha: "d72f1d8963538e29af1689aebb47ef8fa3db84ef", date: "2022-07-12 06:01:39 UTC", description: "typoes", pr_number:                                                                            13489, scopes: [], type:                     "docs", breaking_change:        false, author: "Tshepang Mbambo", files_count: 4, insertions_count:  5, deletions_count:   5},
		{sha: "c05afb2200c74a3bf84c70b34be06e3c36732644", date: "2022-07-12 06:02:25 UTC", description: "typo", pr_number:                                                                              13488, scopes: [], type:                     "docs", breaking_change:        false, author: "Tshepang Mbambo", files_count: 1, insertions_count:  1, deletions_count:   1},
		{sha: "1d19df127f5e73721525aebcbd2f63e81ce40aa5", date: "2022-07-11 23:11:16 UTC", description: "Correct length_delimited header documentation", pr_number:                                     13491, scopes: ["codecs"], type:             "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   2, insertions_count:  2, deletions_count:   2},
		{sha: "1126a5e12d90cc8045f5ebeb322bbb026ed3072a", date: "2022-07-12 05:53:53 UTC", description: "Correct changelog entry", pr_number:                                                           13509, scopes: ["gcp_pubsub source"], type:  "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   1, insertions_count:  1, deletions_count:   1},
		{sha: "94063f094d47393484bb99777dd816a555ba0d28", date: "2022-07-13 00:02:21 UTC", description: "correct links to open new issues on troubleshooting page", pr_number:                          13510, scopes: [], type:                     "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   1, insertions_count:  2, deletions_count:   2},
		{sha: "21d8683f80f629d7ccc6049922563ba90670bc67", date: "2022-07-13 00:39:25 UTC", description: "Fix typos in datadog_agent docs", pr_number:                                                   13515, scopes: [], type:                     "docs", breaking_change:        false, author: "Spencer Gilbert", files_count: 1, insertions_count:  2, deletions_count:   2},
		{sha: "f80fe013a4d70873ddd523f3a444f8bb3be23c5b", date: "2022-07-13 03:05:52 UTC", description: "Update tags and description for dd_agent", pr_number:                                          13517, scopes: [], type:                     "docs", breaking_change:        false, author: "Spencer Gilbert", files_count: 1, insertions_count:  2, deletions_count:   2},
		{sha: "52390c7f038bec3838914813e5f58bfa4b8ab65d", date: "2022-07-13 01:09:56 UTC", description: "Remove note about external buffers", pr_number:                                                13522, scopes: [], type:                     "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   1, insertions_count:  0, deletions_count:   1},
		{sha: "bbb5e978ef2a7914fc999d22e7cef808939e44e0", date: "2022-07-13 02:14:53 UTC", description: "Regenerate k8s manifests for 0.23.0", pr_number:                                               13523, scopes: ["releasing"], type:          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:   17, insertions_count: 23, deletions_count:  21},
		{sha: "c35618170c1a4a696e0e66915fbb1eb73643a285", date: "2022-07-14 00:52:33 UTC", description: "Include breaking change for watch-config in 0.21", pr_number:                                  13536, scopes: [], type:                     "docs", breaking_change:        false, author: "Spencer Gilbert", files_count: 1, insertions_count:  15, deletions_count:  2},
		{sha: "ef5592cb7d137ff7f569eb1117899f4c3b9fe625", date: "2022-07-15 02:30:19 UTC", description: "fix minimum size value for `buffer.max_size`", pr_number:                                      13557, scopes: ["buffers"], type:            "fix", breaking_change:         false, author: "Toby Lawrence", files_count:   1, insertions_count:  1, deletions_count:   1},
		{sha: "a267c58b4cf422fb8dc1f931cda64e7d3ff77343", date: "2022-07-15 22:05:37 UTC", description: "Fix key_field docs to mention templating", pr_number:                                          13561, scopes: ["throttle transform"], type: "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   1, insertions_count:  6, deletions_count:   4},
		{sha: "8c169a70666adcb0094e0df99a85b2edf205c370", date: "2022-07-27 07:08:56 UTC", description: "Add note to 0.23.0 about libc/libc++ changes", pr_number:                                      13724, scopes: ["releasing"], type:          "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   2, insertions_count:  29, deletions_count:  4},
		{sha: "5ea6ceb4fdfb2081e7fffaabceec47d92b4e1834", date: "2022-08-03 23:24:21 UTC", description: "Fix description for aggregator role", pr_number:                                               13827, scopes: [], type:                     "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   1, insertions_count:  1, deletions_count:   1},
		{sha: "36a7742cc60699928dd3946172c952f1ea9f05c0", date: "2022-07-28 05:35:03 UTC", description: "Do not include empty matches for `grok` pattern", pr_number:                                   13721, scopes: ["vrl"], type:                "fix", breaking_change:         false, author: "Pablo Sichert", files_count:   2, insertions_count:  13, deletions_count:  13},
		{sha: "b6deeffa654a61aebbcc79dabaf7b75ff85dcb13", date: "2022-07-30 14:03:59 UTC", description: "Fix bug where Vector shuts down when a decoding error is returned to `FramedRead`", pr_number: 13737, scopes: ["codecs"], type:             "fix", breaking_change:         false, author: "Pablo Sichert", files_count:   3, insertions_count:  44, deletions_count:  2},
		{sha: "490eac75baeaf87c293c222b62e4e262fa413b93", date: "2022-08-10 01:21:25 UTC", description: "Update v0.23 upgrade guide", pr_number:                                                        13906, scopes: ["docs"], type:               "chore", breaking_change:       false, author: "Kyle Criddle", files_count:    1, insertions_count:  15, deletions_count:  0},
		{sha: "c3b6606144295e02fe69bdd055d51c7fa3e39b35", date: "2022-08-09 04:52:18 UTC", description: "Use leading underscore in component ids", pr_number:                                           13859, scopes: ["enterprise"], type:         "chore", breaking_change:       false, author: "Will", files_count:            1, insertions_count:  9, deletions_count:   9},
		{sha: "aa7b80c206fd841f0c94458824528a2c855d80d0", date: "2022-08-11 06:47:09 UTC", description: "drop application_key", pr_number:                                                              13655, scopes: ["enterprise"], type:         "enhancement", breaking_change: false, author: "Vladimir Zhuk", files_count:   3, insertions_count:  29, deletions_count:  22},
		{sha: "c0b7d7e7adb6d03fcfa7bd4f48cdf854498399c8", date: "2022-07-08 05:56:04 UTC", description: "bump async-graphql / async-graphql-warp from 3.0.38 to 4.0.4", pr_number:                      13471, scopes: ["deps"], type:               "chore", breaking_change:       false, author: "Kyle Criddle", files_count:    4, insertions_count:  131, deletions_count: 27},
		{sha: "ff94708756ed63fa1418521c199ecf1c83b8ef6a", date: "2022-07-19 09:03:22 UTC", description: "bump async-graphql from 4.0.4 to 4.0.5", pr_number:                                            13603, scopes: ["deps"], type:               "chore", breaking_change:       false, author: "dependabot[bot]", files_count: 4, insertions_count:  21, deletions_count:  107},
		{sha: "fa32a43c011043087d983701bdcd72075d192899", date: "2022-08-03 03:33:33 UTC", description: "bump async-graphql from 4.0.5 to 4.0.6", pr_number:                                            13779, scopes: ["deps"], type:               "chore", breaking_change:       false, author: "dependabot[bot]", files_count: 4, insertions_count:  12, deletions_count:  12},
	]
}
