package metadata

releases: "0.34.1": {
	date:     "2023-11-16"
	codename: ""

	whats_next: []

	description: """
		This patch release contains fixes for regressions in 0.34.0 and fixes an issues with the Debian release artifacts.

		**Note:** Please see the release notes for [`v0.34.0`](/releases/0.34.0/) for additional changes if upgrading from
		`v0.33.X`. In particular, see the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["datadog_metrics sink"]
			description: """
				The Datadog Metrics sink was reverted back to using the Datadog Metrics v1 endpoints
				to avoid incorrectly sizing batches for the v2 metrics endpoints. We will revisit
				the switch te the v2 endpoints in the future.
				"""
			pr_numbers: [19148, 19138]
		},
		{
			type: "fix"
			scopes: ["loki sink"]
			description: """
				The Loki sink again correctly sets the `Content-Encoding` header on requests to
				`application/x-protobuf` when the default `snappy` compression is used. Previously
				it would set the content encoding as `application/json` which would result in Loki
				rejecting the requests with an HTTP 400 response.
				"""
			pr_numbers: [19099]
		},
	]

	commits: [
		{sha: "09df599a655a116b7eb6016a28705165519fa3f9", date: "2023-11-08 02:44:54 UTC", description: "Add deprecation note about respositories.timber.io deprecation", pr_number: 19078, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 16, deletions_count: 0},
		{sha: "def235e8e7d67f9461898bd72b55809f6ee09a3a", date: "2023-11-08 03:56:37 UTC", description: "Replace setup.vector.dev references", pr_number: 19080, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "356927197e86f280a2762cc0a2a4ee610650df8b", date: "2023-11-08 07:45:06 UTC", description: "Fix formatting for v0.34.0 release note", pr_number: 19085, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "dba0ba17a5c7888ddb20fe808422532939c57619", date: "2023-11-11 06:12:35 UTC", description: "Add upgrade note about TOML breaking change to v0.34.0", pr_number: 19120, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 13, deletions_count: 0},
		{sha: "cee9d071165a6c9b5d7ba59721e2a838f52fa88b", date: "2023-11-14 04:30:31 UTC", description: "Add known issue for Datadog Metrics sink in v0.34.0", pr_number: 19122, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 9, deletions_count: 0},
		{sha: "9e1ad37179e88de5aed5f78176771133b1c8bde7", date: "2023-11-15 08:06:32 UTC", description: "WEB-4247 | Update references from s3 to setup.vector.dev", pr_number: 19149, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "9a5cb519b21e727a7aedec2ebc9d39367e4c7859", date: "2023-11-07 08:12:15 UTC", description: "fix truncate arguments", pr_number: 19068, scopes: [], type: "docs", breaking_change: false, author: "Mark Johnston", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "a59329aaca2c5d4ae98517fc06fec11728957375", date: "2023-11-10 04:47:38 UTC", description: "update a few more examples to YAML", pr_number: 19103, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 115, deletions_count: 102},
		{sha: "218963a3f8a460cfe8e9c3dd3b3ccabb775745ef", date: "2023-11-14 07:19:08 UTC", description: "update to use the global list of compression algorithms", pr_number: 19099, scopes: ["loki sink"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 5, insertions_count: 25, deletions_count: 66},
		{sha: "7b292cea468bd7894f6f48aedaad11f46ff2a622", date: "2023-11-15 04:44:20 UTC", description: "evaluate series v1 env var at runtime", pr_number: 19148, scopes: ["datadog_metrics sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "3158f46d66297ac9a4a406553038d4641ebf7590", date: "2023-11-16 05:42:47 UTC", description: "Revert to using v1 endpoint by default", pr_number: 19138, scopes: ["datadog_metrics sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 43, deletions_count: 31},
	]
}
