package metadata

releases: "0.46.1": {
	date:     "2025-04-14"
	codename: ""

	description: """
			Resolved a regression affecting AWS integrations introduced in version 0.46.0.
			This issue has been addressed by updating to the latest aws-* crate versions.
			No configuration changes are required for this fix.
		"""

	whats_next: []

	changelog: []

	commits: [
		{sha: "5b4e0c94a0fe97cd9aa8e1229c53984ee666d570", date: "2025-04-09 00:39:09 UTC", description: "release prep ", pr_number: 22835, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 37, insertions_count: 626, deletions_count: 205},
		{sha: "a99bf5439bcb166393ed1f36ef44bf200edd2350", date: "2025-04-12 00:05:47 UTC", description: "AWS integrations regression in v0.46.0", pr_number: 22844, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 127, deletions_count: 159},
	]
}
