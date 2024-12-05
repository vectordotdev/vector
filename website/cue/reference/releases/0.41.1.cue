package metadata

releases: "0.41.1": {
	date:     "2024-09-11"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.41.1!

		This release includes a bug fix for a regression in the `vector` source and native codecs
		where an error log was spuriously reported when receiving events from older Vector versions.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				A regression in the `vector` source and native codecs was fixed where an error log was being
				spuriously reported when receiving events from older Vector versions.
				"""
		},
	]

	commits: [
		{sha: "702b22128f25477948a386920048b7333ff369c4", date: "2024-09-11 05:54:00 UTC", description: "Remove error log when source_event_id is not present", pr_number: 21257, scopes: ["proto"], type: "fix", breaking_change: false, author: "ArunPiduguDD", files_count: 3, insertions_count: 24, deletions_count: 10},
	]
}
