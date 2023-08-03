package metadata

releases: "0.20.1": {
	date:     "2022-04-06"
	codename: ""

	description: """
		The Vector team is pleased to announce version 0.20.1!

		This patch release contains a few fixes for regressions in in 0.20.0.

		**Note:** Please see the release notes for [`v0.20.0`](/releases/0.20.0/) for additional changes if upgrading from
		`v0.19.X`. In particular, the upgrade guide for breaking changes.
		"""

	whats_next: []

	changelog: [
		{
			type: "fix"
			scopes: ["unit tests"]
			description: """
				Unit tests no longer panic when `extract_from` or `no_outputs_from` has an invalid input.
				"""
			pr_numbers: [11340]
		},
		{
			type: "fix"
			scopes: ["unit tests"]
			description: """
				Unit tests no longer output warnings for unconsumed outputs.
				"""
			pr_numbers: [11345]
		},
		{
			type: "fix"
			scopes: ["socket source", "observability"]
			description: """
				The `socket` source `component_received_events_total`, `component_received_event_bytes_total` metrics were corrected.
				"""
			pr_numbers: [11490]
		},
		{
			type: "fix"
			scopes: ["buffer"]
			description: """
				Configuring a sink buffer with a value of `0` for `max_events` or `max_size` now returns a boot time error rather than panicking.
				"""
			pr_numbers: [11829]
		},
		{
			type: "fix"
			scopes: ["buffer"]
			description: """
				When using `buffer.on_full` of `drop_newest`, buffers were dropping events before they were actually
				full. This error has been corrected and now buffers will wait until full before dropping.
				"""
			pr_numbers: [11832, 11988]
		},
		{
			type: "fix"
			scopes: ["socket source"]
			description: """
				Previously idle connections would cause the `socket` source to cease processing due to Vector's rate
				limiting behavior. Now these connections are excluded from the rate limiting calculation.
				"""
			pr_numbers: [11549]
		},
	]

	commits: [
		{sha: "8ab7f12b5f4350d6b2ab06a0276de69449681eac", date: "2022-02-12 09:01:00 UTC", description: "fix markup", pr_number:                                                 11330, scopes: [], type:                   "docs", breaking_change:  false, author: "Tshepang Lekhonkhobe", files_count: 1, insertions_count:  1, deletions_count:    1},
		{sha: "819789d0be1fc538ab91953c02c1632a6e8a68fc", date: "2022-02-16 07:38:23 UTC", description: "Update vector source/sink highlight timeline", pr_number:               11400, scopes: [], type:                   "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  4, deletions_count:    5},
		{sha: "a18e6e52eb3647592f949d56da07b2f8ce3aa2e6", date: "2022-02-12 07:45:39 UTC", description: "Error on nonexistent extract_from, no_outputs_from targets", pr_number: 11340, scopes: ["unit tests"], type:       "fix", breaking_change:   false, author: "Will", files_count:                 7, insertions_count:  128, deletions_count:  15},
		{sha: "b02ad3b0b53c8324d4c05b07f738a202c18fb37f", date: "2022-02-15 03:46:25 UTC", description: "Avoid warning logs for unconsumed outputs", pr_number:                  11345, scopes: ["unit tests"], type:       "chore", breaking_change: false, author: "Will", files_count:                 2, insertions_count:  64, deletions_count:   4},
		{sha: "c534215f2f44b9e077494866d49f3ac2a05a5e17", date: "2022-02-16 07:39:07 UTC", description: "Add known issue for unit test warnings", pr_number:                     11409, scopes: ["releasing"], type:        "chore", breaking_change: false, author: "Jesse Szwedko", files_count:        1, insertions_count:  2, deletions_count:    1},
		{sha: "77449373433604260cff8e93fa8c005b919520ac", date: "2022-02-18 09:27:16 UTC", description: "we have just one Runtime transform now", pr_number:                     11412, scopes: [], type:                   "docs", breaking_change:  false, author: "Tshepang Lekhonkhobe", files_count: 1, insertions_count:  3, deletions_count:    3},
		{sha: "b12533a04a6a49d569b998717116072200b9b1ab", date: "2022-02-18 06:31:26 UTC", description: "Ensure `buffer` configuration is always documented", pr_number:         10872, scopes: ["sinks"], type:            "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        47, insertions_count: 64, deletions_count:   90},
		{sha: "4be35d4f28e1abcea274949f3060f0cb73409ee6", date: "2022-02-24 04:05:22 UTC", description: "Fix docker quickstart instructions", pr_number:                         11506, scopes: ["external docs"], type:    "fix", breaking_change:   false, author: "Spencer Gilbert", files_count:      1, insertions_count:  2, deletions_count:    2},
		{sha: "e37a2ae00c84ebca51159176bc85145ec9241341", date: "2022-02-24 06:15:03 UTC", description: "Render encoding as required when codecs are enabled", pr_number:        11535, scopes: ["external docs"], type:    "fix", breaking_change:   false, author: "Spencer Gilbert", files_count:      2, insertions_count:  15, deletions_count:   2},
		{sha: "fc5b97bb8a42a06c49407aad3d191f5d22d444ad", date: "2022-03-01 08:01:37 UTC", description: "Correct option name in throttle documentation", pr_number:              11617, scopes: [], type:                   "chore", breaking_change: false, author: "Spencer Gilbert", files_count:      1, insertions_count:  6, deletions_count:    6},
		{sha: "96309701b9fc938436497bbb56f12a134327876c", date: "2022-03-01 05:58:30 UTC", description: "Add note to releases about stepping through versions", pr_number:       11613, scopes: [], type:                   "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  23, deletions_count:   0},
		{sha: "2eed4910d014e2d56786f1533173bc3c18476c6f", date: "2022-03-02 10:58:12 UTC", description: "Port the \"going-to-prod\" docs from Notion to the website", pr_number: 11596, scopes: [], type:                   "docs", breaking_change:  false, author: "Ari", files_count:                  45, insertions_count: 2524, deletions_count: 1426},
		{sha: "0f21ecdda8fbbf761255150d1594ce045f96810b", date: "2022-03-04 00:37:40 UTC", description: "Unabbreviate High Availability", pr_number:                             11664, scopes: [], type:                   "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  1, deletions_count:    1},
		{sha: "8f52066726f7fdc6bef7c3496450034625b7f88a", date: "2022-03-04 00:40:28 UTC", description: "Update instance type recommendation", pr_number:                        11669, scopes: [], type:                   "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  1, deletions_count:    1},
		{sha: "e3aa2f5648cfa12010005494ec7a8aa2b1583052", date: "2022-03-08 01:01:08 UTC", description: "Fix example type handling", pr_number:                                  11707, scopes: ["reduce transform"], type: "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        2, insertions_count:  2, deletions_count:    2},
		{sha: "1a7f8fb968fc0a1e32cdb6a2eb90bdf06b1355f0", date: "2022-03-08 01:36:24 UTC", description: "Clarify endpoint path", pr_number:                                      11709, scopes: ["loki sink"], type:        "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  1, deletions_count:    1},
		{sha: "414dd28c70143c189c4f8a58c0c306efa098ba5b", date: "2022-03-10 02:01:22 UTC", description: "The `filter` parameter of `redact` is required", pr_number:             11751, scopes: ["vrl"], type:              "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  1, deletions_count:    1},
		{sha: "3204de5b42e9f7e096ccae32e5f282eaea10b304", date: "2022-03-15 23:58:02 UTC", description: "don't drop events when we yield for non-capacity reasons", pr_number:   11832, scopes: ["buffers"], type:          "chore", breaking_change: false, author: "Toby Lawrence", files_count:        4, insertions_count:  60, deletions_count:   21},
		{sha: "64877388290ed5b10c61e3f828b54f9bf9abf785", date: "2022-03-23 09:48:36 UTC", description: "minor fixes/improvements to literals documentation", pr_number:         11928, scopes: ["vrl"], type:              "docs", breaking_change:  false, author: "Hugo Hromic", files_count:          2, insertions_count:  10, deletions_count:   10},
		{sha: "92beefc35c9c0e59f34ef6d2728ac71736b9d73d", date: "2022-03-10 16:24:58 UTC", description: "add encoding configuration examples", pr_number:                        11645, scopes: [], type:                   "docs", breaking_change:  false, author: "Will Li", files_count:              2, insertions_count:  36, deletions_count:   32},
		{sha: "909dfbe2b6eabf38851d0778df983663128e7f11", date: "2022-02-12 07:45:39 UTC", description: "Error on nonexistent extract_from, no_outputs_from targets", pr_number: 11340, scopes: ["unit tests"], type:       "fix", breaking_change:   false, author: "Will", files_count:                 1, insertions_count:  2, deletions_count:    2},
		{sha: "4ab3c378b5a7d889874f4c04be2ef4a21502a054", date: "2022-02-23 03:10:25 UTC", description: "emit metric with good count for tcp", pr_number:                        11490, scopes: ["socket source"], type:    "fix", breaking_change:   false, author: "Jérémie Drouet", files_count:       1, insertions_count:  4, deletions_count:    3},
		{sha: "84cdc25a5447df2768c6457e59a7f2a1bc95774b", date: "2022-03-16 09:45:26 UTC", description: "Require buffer sizes to be non-zero", pr_number:                        11829, scopes: ["buffers"], type:          "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:        15, insertions_count: 141, deletions_count:  71},
		{sha: "37fb5955431c8dcac355abaaa38535cdaf80cfb8", date: "2022-02-26 00:48:51 UTC", description: "Prevent idle connections from hoarding permits ", pr_number:            11549, scopes: ["socket source"], type:    "fix", breaking_change:   false, author: "Nathan Fox", files_count:           1, insertions_count:  8, deletions_count:    0},
	]
}
