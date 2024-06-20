package metadata

releases: "0.22.2": {
	date:     "2022-06-14"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.22.2!

		Note this release does include one potentially impactful behavior change to fix a bug in the `internal_logs`
		source. The `internal_logs` source now, as was intended, applies the same rate limiting logic applied to logs
		that Vector would write to `stderr`. This means, rather than seeing each log individually, duplicate logs in
		small time windows are rolled up and logged like:

		```json
		{"host":"consigliere","message":"Internal log [function call error for \"parse_json\" at (14:34): unable to parse json: trailing characters at line 1 column 8] has been rate limited 282948 times.","metadata":{"kind":"event","level":"ERROR","module_path":"vrl_stdlib::log","target":"vrl_stdlib::log"},"pid":2064943,"source_type":"internal_logs","timestamp":"2022-06-14T22:52:50.329188616Z","vector":{"component_id":"transform0","component_kind":"transform","component_name":"transform0","component_type":"remap"}}
		{"host":"consigliere","internal_log_rate_secs":1,"message":"function call error for \"parse_json\" at (14:34): unable to parse json: trailing characters at line 1 column 8","metadata":{"kind":"event","level":"ERROR","module_path":"vrl_stdlib::log","target":"vrl_stdlib::log"},"pid":2064943,"source_type":"internal_logs","timestamp":"2022-06-14T22:52:50.329201656Z","vector":{"component_id":"transform0","component_kind":"transform","component_name":"transform0","component_type":"remap"},"vrl_position":54}
		{"host":"consigliere","message":"Internal log [function call error for \"parse_json\" at (14:34): unable to parse json: trailing characters at line 1 column 8] is being rate limited.","metadata":{"kind":"event","level":"ERROR","module_path":"vrl_stdlib::log","target":"vrl_stdlib::log"},"pid":2064943,"source_type":"internal_logs","timestamp":"2022-06-14T22:52:50.329212676Z","vector":{"component_id":"transform0","component_kind":"transform","component_name":"transform0","component_type":"remap"}}
		```

		This change was made to bring `internal_logs` in line with Vector's logging and avoid unexpected high bills from
		ingestion in downstream systems when a catastrophic failure occurs.

		**Note:** Please see the release notes for [`v0.22.0`](/releases/0.22.0/) for additional changes if upgrading from
		`v0.21.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["buffers"]
			description: """
				The `disk` buffer max record size was increased to match the max data file size of 128 MB. This makes it
				much less likely that a `panic` will be hit due to large record batches coming in. We have work planned
				to make this more robust by splitting up batches larger than the data file size.
				"""
			pr_numbers: [13121]
		},
		{
			type: "fix"
			scopes: ["internal_logs", "observability"]
			description: """
				The `internal_logs` source now correctly applies the same log rate limiting that is applied to logs
				Vector logs to `stderr`.
				"""
			pr_numbers: [13154]
		},
	]

	commits: [
		{sha: "102c400aa7ab09a8a768c9cfff59972f38cb6ad6", date: "2022-06-10 22:31:00 UTC", description: "A few docs fixes", pr_number: 13094, scopes: ["statsd sink", "statsd source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 32, deletions_count: 6},
		{sha: "d211c2420837f933a179ef5682ca9f68b7a910ae", date: "2022-06-11 00:59:13 UTC", description: "Fix bullets on release changelog in darkmode", pr_number: 13095, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "1613a3c5af5e6b2fcf0c962a56b9c60d003903cb", date: "2022-06-14 00:55:23 UTC", description: "Pixel tracking setup", pr_number: 12995, scopes: ["javascript website"], type: "chore", breaking_change: false, author: "David Weid II", files_count: 3, insertions_count: 18, deletions_count: 1},
		{sha: "e46c4ed0200857df278e33400370b9022b47399e", date: "2022-06-13 23:00:55 UTC", description: "Readability Fixes", pr_number: 13106, scopes: ["various"], type: "docs", breaking_change: false, author: "Ryan Russell", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "8014ea8fd5ded292c0f2cdce926625ff96c442cb", date: "2022-06-15 00:52:00 UTC", description: "Announce intended removal of deprecated transforms", pr_number: 13105, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 452, deletions_count: 1998},
		{sha: "15a3a99de4cb3065f62ecb9bd4416bc9ab48c778", date: "2022-06-14 02:49:58 UTC", description: "Set collectors in enterprise host_metrics", pr_number: 13124, scopes: ["observability"], type: "chore", breaking_change: false, author: "Will", files_count: 1, insertions_count: 13, deletions_count: 1},
		{sha: "822a783d4a7ab4e8811ff5f124829e4527c257d8", date: "2022-06-14 04:03:59 UTC", description: "Remove `--nocapture` from integration tests", pr_number: 13128, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 26, insertions_count: 0, deletions_count: 51},
		{sha: "72f23680a0337dd517a61e1a18bd9643d7e2d66d", date: "2022-06-14 04:01:37 UTC", description: "max default max record size to align with default max data file size", pr_number: 13121, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 4, insertions_count: 90, deletions_count: 33},
		{sha: "de80a4de2e6246de653975ec249333317f3508a8", date: "2022-06-16 02:16:22 UTC", description: "redirect support page to Observability Pipelines product page", pr_number: 13148, scopes: ["website"], type: "chore", breaking_change: false, author: "Brian Deutsch", files_count: 4, insertions_count: 15, deletions_count: 15},
		{sha: "a9586542149e7aa4209a7790f48cbb3af07c995d", date: "2022-06-16 02:05:30 UTC", description: "Fix links in Lua CSV parsing guide", pr_number: 13169, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 4},
		{sha: "8d278e87a4b4e3d6b74715b0dbcfaa9e0cc4c1ef", date: "2022-06-15 11:30:16 UTC", description: "rate limit internal logs", pr_number: 13154, scopes: ["internal_logs source"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 4, deletions_count: 1},
	]
}
