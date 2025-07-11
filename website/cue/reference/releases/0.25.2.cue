package metadata

releases: "0.25.2": {
	date:     "2022-11-23"
	codename: ""

	whats_next: []

	description: """
		This patch release contains a few fixes for regressions in 0.25.0.

		**Note:** Please see the release notes for [`v0.25.0`](/releases/0.25.0/) for additional changes if upgrading from
		`v0.24.X`. In particular, the upgrade guide for breaking changes.
		"""
	changelog: [
		{
			type: "fix"
			scopes: ["remap transform, observability"]
			description: """
				Recent internal telemetry work marked the events discarded using the abort function`abort` as "unintentional",
				this caused an increase in ERROR logs emitted. This was corrected to be "intentional" and now logged at DEBUG.
				"""
			pr_numbers: [15165]
		},
		{
			type: "fix"
			scopes: ["azure_blob sink"]
			description: """
				The Azure Blob Storage sink now properly passes the `connection_string` contents, rather than the **REDACTED**
				value.
				"""
			pr_numbers: [15225]
		},
		{
			type: "fix"
			scopes: ["datadog_traces sink"]
			description: """
				The `datadog traces` sink now correctly aggregates (caches) the computed APM stats, and sends the aggregated
				payloads to Datadog at a fixed interval, decoupled from the trace payloads. The stats payloads were previously
				not being cached and instead sent to Datadog as each batch of traces was processed, which is not compatible
				with Datadog's backend APM API. A robust integration test was also added, to validate the correct aggregation
				behavior.
				"""
			pr_numbers: [14694, 14757, 14861, 15084]
		},
	]

	commits: [
		{sha: "8bb11349ed0368ec3a96bf169e4c73246c9d41ab", date: "2022-11-12 01:49:41 UTC", description: "Clean Up Privacy Links", pr_number: 15194, scopes: ["template website"], type: "chore", breaking_change: false, author: "David Weid II", files_count: 1, insertions_count: 9, deletions_count: 19},
		{sha: "840fd8c7633fb56bff2bdfb1296a31b9cd40483a", date: "2022-11-09 23:15:54 UTC", description: "`abort` is an intentional drop", pr_number: 15165, scopes: ["remap transform", "observability"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "eba869fcfd65b11d10e60ab794f02a55875d2939", date: "2022-11-16 20:29:04 UTC", description: "Retrieve .inner() from SensitiveString for Azure ConnectionString", pr_number: 15225, scopes: ["azure service"], type: "fix", breaking_change: true, author: "Arch Oversight", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "43242ed1e7e3328b952572049e815a27b5a04096", date: "2022-10-06 18:53:31 UTC", description: "Correct multiple issues with APM stats calculation", pr_number: 14694, scopes: ["datadog_traces sink"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 4, insertions_count: 223, deletions_count: 41},
		{sha: "9a8073443ed1ba2bce13248cb27e2202bc8f84c8", date: "2022-10-13 16:24:48 UTC", description: "Have a robust APM stats integration test", pr_number: 14757, scopes: ["datadog_traces sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 490, deletions_count: 10},
		{sha: "8cf07939d366e44c1803b2739f399aad45af09aa", date: "2022-10-17 22:55:55 UTC", description: "Add more missing APM stats logic , temporarily replace Error with debug log", pr_number: 14861, scopes: ["datadog_traces sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 31, deletions_count: 36},
		{sha: "b1e647f33270673f9c851806ce1f408c41f35aed", date: "2022-11-07 21:38:17 UTC", description: "APM stats payloads are sent independent of trace payloads and at a set interval.", pr_number: 15084, scopes: ["datadog_traces sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 15, insertions_count: 1591, deletions_count: 1235},
	]
}
