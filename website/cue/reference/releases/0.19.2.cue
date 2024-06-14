package metadata

releases: "0.19.2": {
	date:     "2022-02-08"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.19.2!

		This patch release contains a few bug fixes for regressions in 0.19.0.

		**Note:** Please see the release notes for [`v0.19.0`](/releases/0.19.0/) for additional changes if upgrading from
		`v0.18.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["sources", "codecs"]
			description: """
				Continue to process data when decoding fails in a source that is using `decoding.codec`. This was
				a regression in `v0.19.0`.
				"""
			pr_numbers: [11254]
		},
		{
			type: "fix"
			scopes: ["codecs"]
			description: """
				`encoding.only_fields` now correctly deserializes again for sinks that used fixed encodings (i.e. those
				that don't have `encoding.codec`). This was a regression in `v0.18.0`.
				"""
			pr_numbers: [11198]
		},
		{
			type: "fix"
			scopes: ["buffers"]
			description: """
				Report correct `buffer_events_total` metric `drop_newest` is used for buffers. Previously, this was
				counting discarded events. This was a regression in `v0.19.0`.
				"""
			pr_numbers: [11159]
		},
		{
			type: "fix"
			scopes: ["transforms"]
			description: """
				All transforms again correctly publish metrics with their component span tags (like `component_id`).
				This was a regression in `v0.19.0`.
				"""
			pr_numbers: [11241]
		},
	]

	commits: [
		{sha: "134a942144308d83a477771fcc72b2c699bf3250", date: "2022-01-26 06:26:12 UTC", description: "Fix example in CSV enrichment highlight", pr_number: 11026, scopes: ["enrichment"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "495b158ea1bcd9831e25d775415fce7389ec3ab7", date: "2022-01-26 09:37:28 UTC", description: "Remove documented elasticsearch sink options that have been removed from code", pr_number: 11028, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count: 27},
		{sha: "b4d9bcce6841ad41ee6b7f7584bc8d71bfb8ca6b", date: "2022-02-05 09:26:10 UTC", description: "Signup Functionality (website)", pr_number: 10904, scopes: ["website"], type: "feat", breaking_change: false, author: "David Weid II", files_count: 10, insertions_count: 311, deletions_count: 28},
		{sha: "f72776059e66c956266c5c2a762519aa24536367", date: "2022-02-08 06:12:10 UTC", description: "Marketo Styles (website)", pr_number: 11228, scopes: ["website"], type: "fix", breaking_change: false, author: "David Weid II", files_count: 3, insertions_count: 80, deletions_count: 65},
		{sha: "c747ddc5df9124b22e03777d968a8d80bb067307", date: "2022-02-09 01:54:02 UTC", description: "continue on error", pr_number: 11254, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 9, deletions_count: 2},
		{sha: "78c9bab4e5105a753b74be5bbe0fd32359fa368f", date: "2022-02-08 22:25:46 UTC", description: "Propagate concurrent transform spans", pr_number: 11241, scopes: ["observability"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "740d8a11bf7536e5102d14c57ee4afebee70ee0c", date: "2022-02-05 06:35:55 UTC", description: "Fix deserialization of `only_fields` for fixed encodings", pr_number: 11198, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 30, deletions_count: 18},
		{sha: "1382998ee6318a00cc6c572e338ad6ebc03f8792", date: "2022-02-03 08:13:05 UTC", description: "fix buffer metrics when using DropNewest", pr_number: 11159, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 12, insertions_count: 135, deletions_count: 47},
	]
}
