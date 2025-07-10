package metadata

releases: "0.12.1": {
	date:        "2021-03-12"
	description: """
		This release contains a few fast follow bug fixes from the 0.12.0
		release, primarily centered around the recently released [**Vector Remap
		Language**](\(urls.vrl_reference)) based on user feedback.
		"""

	whats_next: []

	commits: [
		{sha: "865080e08b74dc16d86666ac114b84e16707a973", date: "2021-03-11 04:33:07 UTC", description: "Fix exmalpe config to be runnable", pr_number: 6698, scopes: ["config"], type: "fix", breaking_change: false, author: "Luc Perkins", files_count: 2, insertions_count: 18, deletions_count: 22},
		{sha: "1220402b6e14977886ea85071ea01eb6c1130fc2", date: "2021-03-11 04:36:06 UTC", description: "Fix description of error code 103", pr_number: 6705, scopes: ["remap"], type: "fix", breaking_change: false, author: "Luc Perkins", files_count: 5, insertions_count: 15, deletions_count: 18},
		{sha: "ec18a7c22fa13dc0388bdce11078d341422bf22c", date: "2021-03-12 20:51:00 UTC", description: "Improve the way values are assigned in remap for fallible expressions", pr_number: 6716, scopes: ["remap"], type: "enhancement", breaking_change: false, author: "Jean Mertz", files_count: 9, insertions_count: 80, deletions_count: 33},
		{sha: "b9a433476708cd313dced27129100263578fb5ce", date: "2021-03-12 21:57:39 UTC", description: "Improve the nginx log parsing ergonomics in remap", pr_number: 6717, scopes: ["remap"], type: "enhancement", breaking_change: false, author: "Jean Mertz", files_count: 6, insertions_count: 95, deletions_count: 11},
		{sha: "415c2b05c430d0f661d54d947a349596b1cccac9", date: "2021-03-12 23:27:25 UTC", description: "Do not mutate the event if the remap script fails", pr_number: 6719, scopes: ["remap"], type: "enhancement", breaking_change: false, author: "Jean Mertz", files_count: 4, insertions_count: 118, deletions_count: 24},
		{sha: "ace65b71ff564da61538d388d4e92a62437dae5f", date: "2021-03-12 20:04:46 UTC", description: "Ensure that metric timestamps are updated when values are updated", pr_number: 6738, scopes: ["prometheus_exporter sink"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 100, deletions_count: 12},
		{sha: "9793c2fcb2b71e935edc99420a434cc8df439c2f", date: "2021-03-13 00:50:05 UTC", description: "Rename and document `drop_on_error` remap config option", pr_number: 6750, scopes: ["remap transform"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 32, deletions_count: 22},

	]
}
