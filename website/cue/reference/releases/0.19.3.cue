package metadata

releases: "0.19.3": {
	date:     "2022-02-10"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.19.3!

		This patch release contains a one bug fix for a regression in 0.19.0.

		**Note:** Please see the release notes for [`v0.19.0`](/releases/0.19.0/) for additional changes if upgrading from
		`v0.18.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["codecs"]
			description: """
				`encoding.only_fields` YAML and JSON configuration now correctly deserializes again for sinks that used
				fixed encodings (i.e. those that don't have `encoding.codec`). This was a regression in `v0.18.0`. It
				was fixed in `v0.19.2` but only for TOML configuration.
				"""
			pr_numbers: [11312]
		},
	]

	commits: [
		{sha: "2f688630c2306e3fb6900cebea18050a57b7694c", date: "2022-02-11 02:56:35 UTC", description: "Fix deserialization of `only_fields` in YAML for fixed encodings", pr_number: 11312, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
	]
}
