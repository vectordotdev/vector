package metadata

releases: "0.19.1": {
	date:     "2022-01-24"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.19.1!

		This patch release contains a few bug fixes for regressions in 0.19.0.

		**Note:** Please see the release notes for [`v0.19.0`](/releases/0.19.0/) for additional changes if upgrading from
		`v0.18.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["sources", "codecs"]
			description: """
				Fixed regression in `framing.character_delimited.delimiter`
				where it would not deserialize from user configuration properly.
				"""
			pr_numbers: [10829]
		},
		{
			type: "fix"
			scopes: ["sinks", "buffers"]
			description: """
				Fixed regression in disk buffers where Vector would not discover
				and use disk buffers created by < 0.19.0. See [the tracking
				issue](https://github.com/vectordotdev/vector/issues/10430#issue-1078985240)
				for more details.
				"""
			pr_numbers: [10826]
		},
		{
			type: "fix"
			scopes: ["security"]
			description: """
				Fixed CVE-2022-21658 and RUSTSEC-2022-0006 by upgrading dependencies.
				"""
			pr_numbers: [10941, 11001]
		},
		{
			type: "fix"
			scopes: ["buffers", "topology"]
			description: """
				Fixed issue with disk buffers where they would not flush events
				in them until new events more events flow in.
				"""
			pr_numbers: [10948]
		},
	]

	commits: [
		{sha: "53980694089a60a1166e99548e8f10f781c917ba", date: "2021-12-29 02:28:35 UTC", description: "Fix broken 0.19.0 links", pr_number: 10608, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3631139ff77a0b877a87b4ab2460b4bdace7ed3e", date: "2021-12-29 06:36:02 UTC", description: "Document deprecation of `token` for Splunk HEC sinks", pr_number: 10612, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 29, deletions_count: 4},
		{sha: "34c5259db27ba1b68800111d31967af34ee5422e", date: "2022-01-05 02:10:10 UTC", description: "Fix url for discussions redirect", pr_number: 10683, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "ea870e1b5cbfaa8782fe96dc5af06722e58def5a", date: "2022-01-14 05:00:22 UTC", description: "Fix elasticsearch section name in upgrade 0.19 guide", pr_number: 10843, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "bbe4ad4067b1156b4994a06321cddf990f27b190", date: "2022-01-14 04:01:57 UTC", description: "Handle `framing.character_delimiter.delimiter` as `char` in config serialization", pr_number: 10829, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Pablo Sichert", files_count: 2, insertions_count: 65, deletions_count: 0},
		{sha: "12e88e0ad08a4af43430253f6959eb7ee56627ca", date: "2022-01-21 21:01:17 UTC", description: "Add known issue for 0.19.0 for character delimiter", pr_number: 10846, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "e3a163e1d456c64610b58777bc98f3a11f90c8f7", date: "2022-01-14 09:46:22 UTC", description: "correctly migrate old disk v1 buffer data dir when possible", pr_number: 10826, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 7, insertions_count: 499, deletions_count: 20},
		{sha: "31fee59b530437f307f3f2ccb2b583f93d4dc357", date: "2022-01-21 06:44:38 UTC", description: "Upgrade Rust to 1.58.1", pr_number: 10941, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "976940bfa8026893afc5a02dd6cf96d1e46a4602", date: "2022-01-22 02:32:35 UTC", description: "ensure fanout is flushed on idle", pr_number: 10948, scopes: ["topology"], type: "fix", breaking_change: false, author: "Luke Steensen", files_count: 3, insertions_count: 69, deletions_count: 1},
		{sha: "a70117c58d14b7bb36a67f2168f3a6eea1790bf7", date: "2022-01-25 02:56:33 UTC", description: "Update thread_local to 1.1.4", pr_number: 11001, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
	]
}
