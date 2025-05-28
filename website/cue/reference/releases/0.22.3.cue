package metadata

releases: "0.22.3": {
	date:     "2022-06-27"
	codename: ""

	whats_next: []

	description: """
		This patch release contains a few fixes for regressions in 0.22.0.

		**Note:** Please see the release notes for [`v0.22.0`](/releases/0.22.0/) for additional changes if upgrading from
		`v0.20.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["gcp_pubsub source"]
			description: """
				The `gcp_pubsub` source has improved throughput due to not only issuing requests to the server when it
				has acknowledgements to provide.
				"""
			pr_numbers: [13203]
		},
		{
			type: "fix"
			scopes: ["aws provider", "observability", "sinks"]
			description: """
				AWS sinks to longer include an `endpoint` tag on internal metrics. This was causing cardinality issues
				as the `aws_s3` source includes the full object key in its request path.
				"""
			pr_numbers: [24385]
		},
	]

	commits: [
		{sha: "5bcdf44cf2c129b6eefbc9a45744358f67913889", date: "2022-06-23 21:39:47 UTC", description: "Document that metrics are at /metrics", pr_number:   13306, scopes: ["prometheus_exporter sink"], type:     "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count:  1},
		{sha: "85d038dda85b05b57b6712a7fb54f686dd9b7f9b", date: "2022-06-23 21:40:14 UTC", description: "Fix default version documentation", pr_number:       13305, scopes: ["vector source", "vector sink"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count:  2},
		{sha: "952e6d1813a1f964d06f95fbea83b4d38673eab8", date: "2022-06-18 04:34:18 UTC", description: "Fix handling of streaming pull requests", pr_number: 13203, scopes: ["gcp_pubsub source"], type:            "fix", breaking_change:  false, author: "Bruce Guenter", files_count: 1, insertions_count: 50, deletions_count: 31},
		{sha: "6ac4bc2e2d39a48c1bb02427964967764aea8822", date: "2022-06-23 00:39:52 UTC", description: "Remove `endpoint` from metrics tags", pr_number:     13285, scopes: ["aws provider"], type:                 "fix", breaking_change:  false, author: "Jesse Szwedko", files_count: 3, insertions_count: 3, deletions_count:  8},
		{sha: "5a6512b29f065ff53f8c0747362f044231a19091", date: "2022-06-28 02:58:45 UTC", description: "Document disk buffer minimum size", pr_number:       13352, scopes: ["buffers"], type:                      "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count:  1},
	]
}
