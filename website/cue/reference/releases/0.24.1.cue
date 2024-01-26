package metadata

releases: "0.24.1": {
	date:     "2022-09-12"
	codename: ""

	whats_next: []

	description: """
		This patch release contains a few fixes for regressions in 0.24.0.

		**Note:** Please see the release notes for [`v0.24.0`](/releases/0.24.0/) for additional changes if upgrading from
		`v0.23.X`. In particular, the upgrade guide for breaking changes.
		"""
	changelog: [
		{
			type: "fix"
			scopes: ["host_metrics source"]
			description: """
				The new `host_metrics` metrics for physical and logical CPU counts were moved to
				their own new gauges: `logical_cpus` and `physical_cpus`.
				"""
			pr_numbers: [14328]
		},
		{
			type: "fix"
			scopes: ["http sink"]
			description: """
				The `http` sink again accepts `metrics` and `traces` as events depending on the
				configured codec. This was a regression in behavior in 0.23.0 where it was
				incorrectly restricted.
				"""
			pr_numbers: [14320]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				The newly added `expire_metrics` option was corrected to be named
				`expire_metrics_secs` to match other configuration options. It was also corrected to
				take a fractional number of seconds where `expire_metrics` required configuration of
				the number of seconds and nanoseconds separately.
				"""
			pr_numbers: [14247, 14363, 14338]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				Internal metrics as reported by `vector top` and some sinks, like `file`, was
				corrected by reverting the internal metrics registry to again use absolute values.
				"""
			pr_numbers: [14251]
		},
	]

	commits: [
		{sha: "8a4b4f572a1ad0a55ead721b74ac4fe1cb690ee6", date: "2022-08-31 05:21:02 UTC", description: "Fix link in 0.24.0 upgrade guide", pr_number:                          14178, scopes: [], type:                      "docs", breaking_change:  false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count:  2},
		{sha: "146b5eb674ee39a492df686b3d37702094d442b0", date: "2022-08-31 03:49:14 UTC", description: "Fix permission for healthcheck", pr_number:                            14179, scopes: ["aws_s3 sink"], type:         "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:  1},
		{sha: "b5d084698baf037898ca45b0e8eab26224d05896", date: "2022-09-01 08:05:16 UTC", description: "Remove confusion", pr_number:                                          14186, scopes: [], type:                      "docs", breaking_change:  false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 1, deletions_count:  1},
		{sha: "477840e46ac71bbb92be7c2161d6158ef14773a0", date: "2022-09-01 04:40:45 UTC", description: "Document support for GELF codec", pr_number:                           14231, scopes: ["codecs"], type:              "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   4, insertions_count: 6, deletions_count:  2},
		{sha: "f849672fdf0576801f4e2753d0bf15410d2e7185", date: "2022-09-01 21:42:43 UTC", description: "Fix documentation of tls.enabled", pr_number:                          14242, scopes: ["sources", "sinks"], type:    "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   4, insertions_count: 10, deletions_count: 8},
		{sha: "49374352b318157fa4bd61c3b8f12c4497f2ebfc", date: "2022-09-09 23:25:25 UTC", description: "Fix the tags used in the `adaptive_concurrency_*` metrics", pr_number: 14351, scopes: [], type:                      "docs", breaking_change:  false, author: "Bruce Guenter", files_count:   1, insertions_count: 4, deletions_count:  4},
		{sha: "27e725769a74181392a4661f8a5757b1a36d3e40", date: "2022-09-01 05:55:18 UTC", description: "Fix type of global `expire_metrics` setting", pr_number:               14224, scopes: ["config"], type:              "fix", breaking_change:   false, author: "Bruce Guenter", files_count:   4, insertions_count: 31, deletions_count: 14},
		{sha: "0a145b4b5491a9e6afb5bbcad602aa87e819d428", date: "2022-09-02 07:06:31 UTC", description: "Convert internal metric counts back to absolute", pr_number:           14251, scopes: ["observability"], type:       "fix", breaking_change:   false, author: "Bruce Guenter", files_count:   3, insertions_count: 9, deletions_count:  25},
		{sha: "be3d1b578158e9e74b31176611a7808372985870", date: "2022-09-03 06:29:39 UTC", description: "Rename global `expire_metrics` to `expire_metrics_secs`", pr_number:   14247, scopes: ["config"], type:              "fix", breaking_change:   false, author: "Bruce Guenter", files_count:   3, insertions_count: 37, deletions_count: 2},
		{sha: "c3640a7cd17daab9896483ffb6e02515d71f3459", date: "2022-09-03 06:21:51 UTC", description: "Pass span context to spawned tasks", pr_number:                        14254, scopes: ["aws_sqs source"], type:      "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:   1, insertions_count: 20, deletions_count: 13},
		{sha: "740b7c479b65beb27b27f7f13bb7ae2168014702", date: "2022-09-08 04:00:50 UTC", description: "Expand input type support to match codec's", pr_number:                14320, scopes: ["http sink"], type:           "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:   2, insertions_count: 13, deletions_count: 7},
		{sha: "e5bb550f12154318f6cac0c6268f4ac3f5ba1e1e", date: "2022-09-08 23:14:42 UTC", description: " `logical_cpus` and `physical_cpus` separate gauges", pr_number:       14328, scopes: ["host_metrics source"], type: "fix", breaking_change:   false, author: "Kyle Criddle", files_count:    1, insertions_count: 51, deletions_count: 34},
		{sha: "d5b43120b0c2da89de46e68e7d18df9257424e86", date: "2022-09-13 00:42:31 UTC", description: "wilcards -> wildcards", pr_number:                                     14375, scopes: [], type:                      "docs", breaking_change:  false, author: "Johan Bergström", files_count: 1, insertions_count: 1, deletions_count:  1},
		{sha: "e4a8c26cce476c7d2382be07b4b6ef30a7db7882", date: "2022-09-13 02:04:16 UTC", description: "Add release note about expiring internal metrics", pr_number:          14338, scopes: [], type:                      "docs", breaking_change:  false, author: "Bruce Guenter", files_count:   1, insertions_count: 4, deletions_count:  0},
		{sha: "1ca92df8b631cb85d0e089e38071507a15324d94", date: "2022-09-12 22:11:53 UTC", description: "add cpu count metrics to `host_metrics` cue docs", pr_number:          14366, scopes: ["docs"], type:                "fix", breaking_change:   false, author: "Kyle Criddle", files_count:    1, insertions_count: 8, deletions_count:  0},
		{sha: "5cec2a14905ecbdb7b4c42754f55f5b5f5a7fdbd", date: "2022-09-13 01:32:00 UTC", description: "Restore `expire_metrics` back to original type", pr_number:            14363, scopes: ["config"], type:              "chore", breaking_change: false, author: "Bruce Guenter", files_count:   5, insertions_count: 61, deletions_count: 10},
	]
}
