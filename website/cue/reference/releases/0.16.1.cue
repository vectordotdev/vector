package metadata

releases: "0.16.1": {
	date:     "2021-08-26"
	codename: ""

	description: """
		The Vector team is pleased to announce version 0.16.1!

		This release contains two bug fixes from the 0.16.0 release:

		* Fixing an issue where Vector would crash when loading disk buffers
		* Fixing an issue where the `vector` sink would incorrectly try to use `http://` when `tls` was enabled

		**Note:** Please see the release notes for [0.16.0](/releases/0.16.0) for additional changes if upgrading from
		0.15.X. In particular, the upgrade guide for breaking changes.
		"""

	whats_next: []

	commits: [
		{sha: "5e4f6eeed35dcd9208b9d0a927d67d87569385a0", date: "2021-08-26 19:47:01 UTC", description: "Remove debug build verification for release build", pr_number:  8883, scopes: ["releasing"], type:   "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count:   3},
		{sha: "35fa22814641b66a5e2b4cf34ff7372dd019abc6", date: "2021-08-27 00:31:19 UTC", description: "disk buffer performs overlapping read from leveldb", pr_number: 8845, scopes: ["buffers"], type:     "fix", breaking_change:   false, author: "Toby Lawrence", files_count: 5, insertions_count: 177, deletions_count: 2},
		{sha: "8dc98d16d456e594449525f71ea73d9ca000cfe3", date: "2021-08-27 06:50:06 UTC", description: "set correct default scheme for TLS connections", pr_number:     8901, scopes: ["vector sink"], type: "fix", breaking_change:   false, author: "Jean Mertz", files_count:    1, insertions_count: 32, deletions_count:  10},
	]
}
