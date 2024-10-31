package metadata

releases: "0.36.1": {
	date:     "2024-03-11"
	codename: ""

	whats_next: []

	description: """
		This patch release contains fixes for regressions in 0.36.0.

		**Note:** Please see the release notes for [`v0.36.0`](/releases/0.36.0/) for additional changes if upgrading from
		`v0.35.X`. In particular, see the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				Fixed gzip and zlib compression performance degradation introduced in v0.34.0.
				"""
			contributors: ["Hexta"]
		},
		{
			type: "fix"
			description: """
				AWS components again support the use of `assume_role`. This was a regression in v0.36.0.
				"""
		},
		{
			type: "fix"
			description: """
				AWS components again support the use of `credential_process` in AWS config files to load AWS
				credentials from an external process. This was a regression in v0.36.0.
				"""
		},
		{
			type: "fix"
			description: """
				AWS components again support auto-detection of the region. This was a regression in v0.36.0.
				"""
		},
		{
			type: "fix"
			description: """
				The `kafka` sink avoids panicking during a rebalance event. This was a regression in v0.36.0.
				"""
		},
	]

	commits: [
		{sha: "a10a137394bda91a97bf6d1731459615af2869ad", date: "2024-02-17 20:44:07 UTC", description: "0.36 changelog fixes", pr_number: 19875, scopes: ["releases website"], type: "chore", breaking_change: false, author: "hdhoang", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "3057ccfd7e0f58b615d756ca6541b5604053cef4", date: "2024-02-21 05:49:55 UTC", description: "Fix `drop_on_abort` docs", pr_number: 19918, scopes: ["remap transform"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 6, deletions_count: 10},
		{sha: "f1f8c1bc998ef98215dba117335b74e8e5e57b68", date: "2024-02-22 15:48:21 UTC", description: "bump openssl version used for links in docs", pr_number: 19880, scopes: ["website"], type: "chore", breaking_change: false, author: "Hugo Hromic", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9def84e0de3831f0add61c9b2cb4e880fcf8aa7d", date: "2024-02-29 09:31:36 UTC", description: "determine region using our http client", pr_number: 19972, scopes: ["aws service"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 25, deletions_count: 3},
		{sha: "54bcee72242d06eacd355451ed62ee1029925a81", date: "2024-03-06 22:45:13 UTC", description: "Update lockfree-object-pool to 0.1.5", pr_number: 20001, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "e4951cc447d8a3b4896c4603a962651350b6ac37", date: "2024-03-08 09:10:35 UTC", description: "Bump whoami to 1.5.0", pr_number: 20018, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 11, deletions_count: 3},
		{sha: "28760fbcdade2353feb506a51ef7288a570d6ca6", date: "2024-03-08 17:46:25 UTC", description: "Enable `credentials-process` for `aws-config`", pr_number: 20030, scopes: ["aws provider"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 1},
		{sha: "63133311baa0df60d08e22bb1e4bec858438e268", date: "2024-03-09 09:25:33 UTC", description: "Fix gzip and zlib performance degradation", pr_number: 20032, scopes: ["compression"], type: "fix", breaking_change: false, author: "Artur Malchanau", files_count: 2, insertions_count: 66, deletions_count: 34},
		{sha: "a8cd2a2df1df26de9e14d51cb84bc0bdd443a195", date: "2024-03-05 22:34:02 UTC", description: "Update mio", pr_number: 20005, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "1ea58e47cadc4acc9d554a60653e76cbdd034105", date: "2024-03-09 04:13:14 UTC", description: "Add missing changelog entries", pr_number: 20041, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 3, deletions_count: 0},
	]
}
