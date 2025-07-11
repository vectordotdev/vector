package metadata

releases: "0.25.1": {
	date:     "2022-11-04"
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
			scopes: ["config"]
			description: """
				Vector now loads configurations using environment variables outside of string values
				for configuration options again.
				"""
			pr_numbers: [15081]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Vector now loads multi-file configurations using the global `timezone` configuration
				option again.
				"""
			pr_numbers: [15077]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				The `prometheus_remote_write` sink now allows the configuration `auth.bearer` to be provided again.
				"""
			pr_numbers: [15112]
		},
	]

	commits: [
		{sha: "8f43b656d23b013f4da07ddc8422b2140fb56ec9", date: "2022-11-03 00:36:09 UTC", description: "fix link to journalctl discussion", pr_number: 15070, scopes: [], type: "docs", breaking_change: false, author: "Jim Rollenhagen", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4f6a2a84c9093bc65a546b063e9d5914b4165492", date: "2022-11-03 01:55:23 UTC", description: "Add known issues for 0.25.0 release", pr_number: 15076, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 10, deletions_count: 1},
		{sha: "2cafbde09e753abb89af3d3c3416c4e4bdd9466c", date: "2022-11-03 05:54:17 UTC"
			description: "Revert config secrets scanning", pr_number: 15081, scopes: ["config"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 14, insertions_count: 105, deletions_count: 387
		},
		{sha: "3d55887042bbc2ecae9ca0c0bf220f39f1c5b462", date: "2022-11-03 13:05:06 UTC", description: "Fix merging of configured timezones", pr_number: 15077, scopes: ["config"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 82, deletions_count: 15},
		{sha: "296a65099fd3399f7aa0f6e597ff77be997063d0", date: "2022-11-05 05:47:58 UTC", description: "Add another known issue for prometheus remote write source", pr_number: 15122, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "44eebb0c164c5ff815d7f3032d5a3f8f17b3e82f", date: "2022-11-01 22:34:25 UTC", description: "Disable axiom integration test temporarily", pr_number: 15053, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "d632330e3f1fb70586606188a2320cfae2c6a544", date: "2022-11-03 03:14:35 UTC", description: "Explicitly set DD_HOSTNAME for Datadog Agent integration tests", pr_number: 15080, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "51e42510afd1c1e9737f0328e4ff8b28eb7cb1cd", date: "2022-11-05 08:11:41 UTC", description: "re-add support `Bearer` Auth config option", pr_number: 15112, scopes: ["prometheus_remote_write sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 93, deletions_count: 28},
	]
}
