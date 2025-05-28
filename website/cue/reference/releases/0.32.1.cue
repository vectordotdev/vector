package metadata

releases: "0.32.1": {
	date:     "2023-08-21"
	codename: ""

	whats_next: []
	description: """
		This patch release contains a fix for a regression in 0.32.0 and fixes a few issues with the release artifacts.

		**Note:** Please see the release notes for [`v0.32.0`](/releases/0.32.0/) for additional changes if upgrading from
		`v0.31.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				A number of sinks were emitting incorrect telemetry for the `component_sent_*` metrics:

				- WebHDFS
				- GCP Cloud Storage
				- AWS S3
				- Azure Blob Storage
				- Azure Monitor Logs
				- Databend
				- Clickhouse
				- Datadog Logs

				This has been corrected.
				"""
			pr_numbers: [18289]
		},
		{
			type: "fix"
			scopes: ["security"]
			description: """
				The newly added `--openssl-legacy-provider` flag in 0.32.0 can now be disabled by
				setting it to `false` via `--openssl-legacy-provider=false`. Previously it would
				complain of extra arguments.
				"""
			pr_numbers: [18276]
		},
		{
			type: "fix"
			scopes: ["opentelemetry source"]
			description: """
				The `opentelemetry` source no longer fails to decode large payloads. This was a regression
				in 0.31.0 when a 4 MB limit was inadvertently applied.
				"""
			pr_numbers: [18306]
		},
	]

	commits: [
		{sha: "1a32e969162d921f00c3ad67c242e8cf047d2c99", date: "2023-08-16 21:58:31 UTC", description: "Add 0.32.0 highlight for legacy OpenSSL provider deprecation", pr_number:      18263, scopes: ["releasing"], type:            "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count: 12, deletions_count:  2},
		{sha: "0f7d6e6798d81bd1cae17c918f53a87406deb383", date: "2023-08-17 07:01:42 UTC", description: "0.32.0.cue typo", pr_number:                                                   18270, scopes: [], type:                       "docs", breaking_change:  false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 1, deletions_count:   1},
		{sha: "91f7612053204f5305ea2991429cf7ccfae4bf26", date: "2023-08-16 22:16:47 UTC", description: "Add note about protobuf codec addition for 0.32.0 release", pr_number:         18275, scopes: ["releasing"], type:            "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count: 2, deletions_count:   0},
		{sha: "38e95b56178197224f3aead2d19050421fdb5464", date: "2023-08-18 04:01:11 UTC", description: "Add known issues for v0.32.0", pr_number:                                      18298, scopes: ["releasing"], type:            "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count: 22, deletions_count:  0},
		{sha: "2dcaf302f52206c516422615f0a52ba45fedae8b", date: "2023-08-18 08:31:54 UTC", description: "add the 'http_client_requests_sent_total'", pr_number:                         18299, scopes: ["docs"], type:                 "fix", breaking_change:   false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 12, deletions_count:  0},
		{sha: "c9ccee0fdcc516af3555e498f8366c3059f1c74d", date: "2023-08-18 05:45:53 UTC", description: "add all events that are being encoded", pr_number:                             18289, scopes: ["observability"], type:        "fix", breaking_change:   false, author: "Stephen Wakely", files_count:  2, insertions_count: 102, deletions_count: 79},
		{sha: "8868b078ac78f66e62657b034d9d03b551bbebef", date: "2023-08-17 03:58:20 UTC", description: "load default and legacy openssl providers", pr_number:                         18276, scopes: ["deps"], type:                 "fix", breaking_change:   false, author: "Doug Smith", files_count:      2, insertions_count: 34, deletions_count:  22},
		{sha: "042fb51dbec93c1e1b644735ab749b9711c2e4c8", date: "2023-08-17 22:46:37 UTC", description: "Make the warning for the deprecated OpenSSL provider more verbose", pr_number: 18278, scopes: ["security"], type:             "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:   1},
		{sha: "a1dfd54b6947f7766756e5eb24f5b6e1bcc46c98", date: "2023-08-16 08:35:28 UTC", description: "Bump `nkeys` to 0.3.2", pr_number:                                             18264, scopes: ["deps"], type:                 "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   2, insertions_count: 85, deletions_count:  114},
		{sha: "56177ebce2797c0015c49775e6fdffd4153cc26f", date: "2023-08-19 01:14:20 UTC", description: "Remove the 4MB default for gRPC request decoding", pr_number:                  18306, scopes: ["opentelemetry source"], type: "fix", breaking_change:   false, author: "neuronull", files_count:       1, insertions_count: 4, deletions_count:   1},
	]
}
