package metadata

releases: "0.40.1": {
	date:     "2024-08-26"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.40.1!

		This is the first release with packages signed with the new GPG key as part of [Datadog's key
		rotation](https://docs.datadoghq.com/agent/guide/linux-key-rotation-2024/?tab=debianubuntu).
		This should be a transparent change for users as the package repository setup script
		[setup.vector.dev](https://setup.vector.dev) has already been importing the new key;
		however, if you were manually managing the trusted GPG keys, you will need to update to
		the newer ones (either [apt](https://keys.datadoghq.com/DATADOG_APT_KEY_C0962C7D.public) or
		[rpm](https://keys.datadoghq.com/DATADOG_RPM_KEY_B01082D3.public).

		This release also fixes a regression in Vector v0.40.0.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				Fixes a Vector v0.40.0 regression where the `reduce` transform would not group top
				level objects correctly.
				"""
		},
	]

	commits: [
		{sha: "e9455f6596a138fc8d0ea978897b7a697c77fbb3", date: "2024-08-17 05:29:36 UTC", description: "use the correct merge strategy for top level objects", pr_number: 21067, scopes: ["reduce transform"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 332, deletions_count: 36},
	]
}
