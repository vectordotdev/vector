package metadata

releases: "0.34.2": {
	date:     "2023-12-21"
	codename: ""

	whats_next: []

	description: """
		This patch release contains fixes for regressions in 0.34.0.

		**Note:** Please see the release notes for [`v0.34.0`](/releases/0.34.0/) for additional changes if upgrading from
		`v0.33.X`. In particular, see the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["datadog_agent source"]
			description: """
				The Datadog Agent source now correctly responds to the empty payload request that
				the Datadog Agent sends on start-up to verify connectivity.
				"""
			pr_numbers: [19093]
		},
		{
			type: "fix"
			scopes: ["codecs"]
			description: """
				The `protobuf` encoder added in v0.34.0 now correctly receives and encodes log
				events. Previously it was silently discarding them.
				"""
			pr_numbers: [19264]
		},
	]

	commits: [
		{sha: "73a668c7988e196a74e8a5d4a171dd2e5eddbed3", date: "2023-11-18 06:23:53 UTC", description: "WEB-4275 | Update Navigation", pr_number:                            19186, scopes: ["website"], type:              "chore", breaking_change: false, author: "Devin Ford", files_count:      1, insertions_count: 1, deletions_count:  1},
		{sha: "1b9fb9b5ac3c99eef2fbe160660401c3797f4254", date: "2023-11-14 08:44:16 UTC", description: "Add alpha to traces and beta to metrics in descriptions", pr_number: 19139, scopes: ["docs"], type:                 "chore", breaking_change: false, author: "May Lee", files_count:         6, insertions_count: 12, deletions_count: 12},
		{sha: "f7c3824c1f6830119ac98b8f4791322fb7e24e50", date: "2023-11-29 06:44:59 UTC", description: "Add known issue for protobuf encoder in v0.34.0", pr_number:         19244, scopes: ["releasing"], type:            "chore", breaking_change: false, author: "Bruce Guenter", files_count:   1, insertions_count: 3, deletions_count:  0},
		{sha: "ab7983a201c7e001317d993f7a291a769af06b38", date: "2023-11-09 04:10:43 UTC", description: "return 200 on empty object payload", pr_number:                      19093, scopes: ["datadog_agent source"], type: "fix", breaking_change:   false, author: "Doug Smith", files_count:      2, insertions_count: 23, deletions_count: 1},
		{sha: "d2fea6580aaa9a0936f11b9f70fc053676872837", date: "2023-12-08 04:34:35 UTC", description: "fix 'ProtobufSerializerConfig' input type", pr_number:               19264, scopes: ["codecs"], type:               "fix", breaking_change:   false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 16, deletions_count: 1},
		{sha: "e27b7bdd997879e6fcc99b60b6165e2e533adf6e", date: "2023-11-30 23:45:09 UTC", description: "Ignore RUSTSEC-2023-0071 for now", pr_number:                        19263, scopes: ["security"], type:             "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count: 6, deletions_count:  1},
		{sha: "74d6cb1effcba4b8f7a7be951907a78f95d39996", date: "2023-12-20 18:01:16 UTC", description: "Bump zerocopy from 0.7.21 to 0.7.31", pr_number:                     19394, scopes: ["deps"], type:                 "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count:  4},
	]
}
