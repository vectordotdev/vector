package metadata

releases: "0.14.1": {
	date:     "2021-07-19"
	codename: ""

	description: """
		This release contains a bug fix for a regression to checkpointing for the `file` source for `0.14.0`.  If you
		are upgrading from `0.13.X` and are using the `file` source, please upgrade to this release instead.
		"""

	whats_next: []

	commits: [
		{sha: "721ca6679ba49b999cee659fdf884359cbb9fe5b", date: "2021-07-19 03:44:52 UTC", description: "Backport fingerprint fix", pr_number: 8225, scopes: ["file source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 240, deletions_count: 29},
	]
}
