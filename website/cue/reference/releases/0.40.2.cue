package metadata

releases: "0.40.2": {
	date:     "2024-09-09"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.40.2!

		This release also fixes regression in the `reduce` transform in Vector v0.40.0.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				The `reduce` transform can now reduce fields that contain special characters.
				"""
		},
	]

	commits: [
		{sha: "360db351a60fd62671a15822b8aa5e08b915979f", date: "2024-09-04 09:20:32 UTC", description: "surround invalid path segments with quotes", pr_number: 21201, scopes: ["reduce transform"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 48, deletions_count: 107},
	]
}
