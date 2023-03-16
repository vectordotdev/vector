package metadata

releases: "0.27.1": {
	date:     "2023-02-21"
	codename: ""

	whats_next: []

	description: """
		This patch release contains a few fixes for regressions in 0.27.1.

		**Note:** Please see the release notes for [`v0.27.0`](/releases/0.27.0/) for additional changes if upgrading from
		`v0.26.X`. In particular, the upgrade guide for breaking changes.
		"""
	changelog: [
		{
			type: "fix"
			scopes: ["internal_metrics", "observability"]
			description: """
				Vector again correctly tags the `component_events_in_total` and
				`component_events_out_total` internal metrics emitted by sources with their
				component tags. This fixes reporting by `vector top`. This was a regression in
				0.27.0.
				"""
			pr_numbers: [16416, 16439]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Vector again allows for nested paths to be used in `log_schema` configuration. There
				was a regression in 0.26.0 that treated them as flat paths.
				"""
			pr_numbers: [16410]
		},
		{
			type: "fix"
			scopes: ["security"]
			description: """
				Vector's OpenSSL dependency was upgraded to `1.1.1t` to resolve
				[CVE-2023-0215](https://cve.mitre.org/cgi-bin/cvename.cgi?name=CVE-2023-0215).
				"""
			pr_numbers: [16355]
		},
	]

	commits: [
		{sha: "e827d3eed9e90df60973355bcace7f0ed8eedad1", date: "2023-02-01 23:44:14 UTC", description: "document VRL playground", pr_number:                                                                     16062, scopes: ["vrl"], type:           "docs", breaking_change:  false, author: "Alexander Zaitsev", files_count: 1, insertions_count: 7, deletions_count:  0},
		{sha: "c320e6c105f0fc891ecd350fc8c16ffbd602cc54", date: "2023-02-01 16:26:33 UTC", description: "Remove extra space", pr_number:                                                                          16233, scopes: [], type:                "chore", breaking_change: false, author: "Spencer Gilbert", files_count:   1, insertions_count: 1, deletions_count:  1},
		{sha: "0d506445738a266a6fc4151534b8c9f7258beea2", date: "2023-02-06 20:56:42 UTC", description: "remove scrollbars from component tag on website", pr_number:                                             16306, scopes: [], type:                "docs", breaking_change:  false, author: "Stephen Wakely", files_count:    1, insertions_count: 1, deletions_count:  1},
		{sha: "f73f0c800d6c5b1bc83e030ef11c26e1dcaa055f", date: "2023-02-14 16:01:20 UTC", description: "fix source metadata paths from being flattened with `insert_standard_vector_source_metadat`", pr_number: 16410, scopes: ["core"], type:          "fix", breaking_change:   false, author: "Nathan Fox", files_count:        1, insertions_count: 31, deletions_count: 2},
		{sha: "79100c8cf6835df0a975f4fe9472205c85ad9304", date: "2023-02-13 23:01:06 UTC", description: "Fix tagging registered events in sources", pr_number:                                                    16416, scopes: ["observability"], type: "fix", breaking_change:   false, author: "Bruce Guenter", files_count:     1, insertions_count: 5, deletions_count:  1},
		{sha: "e1e9d5db6f7a0485443f3eab24368444acb8c741", date: "2023-02-14 21:20:37 UTC", description: "Fix tagging more registered events in sources", pr_number:                                               16439, scopes: ["observability"], type: "fix", breaking_change:   false, author: "Bruce Guenter", files_count:     2, insertions_count: 8, deletions_count:  15},
		{sha: "23cfe73b57e69b40a49cdbca79481379bfdeec37", date: "2023-02-08 01:10:57 UTC", description: "Update openssl-src crate", pr_number:                                                                    16355, scopes: ["deps"], type:          "chore", breaking_change: false, author: "Jesse Szwedko", files_count:     1, insertions_count: 2, deletions_count:  2},
	]
}
