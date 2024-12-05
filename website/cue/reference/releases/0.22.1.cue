package metadata

releases: "0.22.1": {
	date:     "2022-06-08"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.22.1!

		**Note:** Please see the release notes for [`v0.22.0`](/releases/0.22.0/) for additional changes if upgrading from
		`v0.21.X`. In particular, the upgrade guide for breaking changes.
		"""
	changelog: [
		{
			type: "fix"
			scopes: ["journald source"]
			description: """
				The `journald' source no longer deadlocks immediately.
				"""
			pr_numbers: [12885, 12889]
		},
		{
			type: "fix"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source works with k3s and k3d again rather than erroring with certificate issues.
				"""
			pr_numbers: [13038]
			contributors: ["jaysonsantos"]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Vector no longer panics reloading configuration using `compression` and `concurrency` options.
				"""
			pr_numbers: [12992, 13019]
			contributors: ["jorgebay", "KH-Moogsoft"]
		},
		{
			type: "fix"
			scopes: ["socket source", "syslog source"]
			description: """
				When using a component that creates a unix socket, `vector validate` no longer creates the socket. This
				was causing the default SystemD unit file to fail to start Vector since it runs `vector validate` before
				starting Vector.
				"""
			pr_numbers: [13021]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				VRL now correctly calculates type definitions when conditionals are used. Previously VRL was setting the
				type to whatever was inside the conditional block rather than unioning the type.

				For example:

				```coffeescript
				thing = 5
				if .foo == .bar {
					thing = null
				}
				```

				Was resulting in VRL thinking that `thing` is always `null` even though it is only conditionally `null`
				and so should instead be `null | integer`. This affects later usages which depend on the type of the
				value (like functions).

				The result of this is that you might need to introduce more type coercion using VRL's [type
				functions](/docs/reference/vrl/functions/#type-functions).
				"""
			pr_numbers: [12954]
		},
	]

	commits: [
		{sha: "9cedaea345b4a4d05220de60887f5115230e99c4", date: "2022-06-02 01:30:35 UTC", description: "Fixup Docker quickstart instructions", pr_number: 12915, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 5, deletions_count: 6},
		{sha: "e2f96300eaaf0fda6817578290fccda17106ddf8", date: "2022-06-03 06:24:09 UTC", description: "typoes", pr_number: 12941, scopes: [], type: "docs", breaking_change: false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f7239176a9e0b3f909e4c4ac5884f9be2dba0ff5", date: "2022-06-02 22:48:26 UTC", description: "Fix contributor links on releases", pr_number: 12945, scopes: ["docs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "466e30c9a343af4f44b608422eebdde9b69b997c", date: "2022-06-07 00:26:36 UTC", description: "Remove notice from parse_logfmt intended for encoding", pr_number: 12967, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "7c509190ee450847a73406648bd7235782f67baf", date: "2022-06-06 22:51:15 UTC", description: "Add note about making metrics unique", pr_number: 12975, scopes: ["internal_metrics source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 15, deletions_count: 0},
		{sha: "809e3fa9a6961854809ee36b7b9a6ecfdb51abd9", date: "2022-06-08 05:31:13 UTC", description: "Add known issue for assume role caching to 0.21.0", pr_number: 12997, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "587e2f587b551c3815adf024883076678324bc68", date: "2022-06-09 06:39:52 UTC", description: "that is a shell feature", pr_number: 13025, scopes: [], type: "docs", breaking_change: false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 0, deletions_count: 6},
		{sha: "ced912e68bffce0a2e03fa47bde29335b4762245", date: "2022-05-28 03:18:57 UTC", description: "Several cleanups", pr_number: 12885, scopes: ["journald source"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 136, deletions_count: 117},
		{sha: "e881b96f074cd04b5695a6943c1b3d1a38fbc195", date: "2022-05-31 22:06:09 UTC", description: "Improve handling of shutdown signal", pr_number: 12889, scopes: ["journald source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 23, deletions_count: 11},
		{sha: "8d898f620a54e7ef6d2bae07db225d33ce62b824", date: "2022-06-08 09:01:26 UTC", description: "Compression config map serialization", pr_number: 12992, scopes: ["config"], type: "fix", breaking_change: false, author: "Jorge Bay-Gondra", files_count: 1, insertions_count: 25, deletions_count: 5},
		{sha: "c45134e0f41750b7b6dcb8a37642ccb6dfd36005", date: "2022-06-08 09:38:32 UTC", description: "make Concurrency struct serialization reversible", pr_number: 13019, scopes: ["config"], type: "fix", breaking_change: false, author: "KH-Moogsoft", files_count: 1, insertions_count: 35, deletions_count: 2},
		{sha: "7c7d37c2aac375634e87f981c55b7d077dd76da8", date: "2022-06-09 12:01:28 UTC", description: "Compile kube-rs with openssl", pr_number: 13038, scopes: ["kubernetes_logs"], type: "fix", breaking_change: false, author: "Jayson Reis", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "9951ccbb291952be404122444294b5c1053817a2", date: "2022-06-09 03:54:14 UTC", description: "Avoid creating unix sockets too early", pr_number: 13021, scopes: ["socket source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 52, deletions_count: 14},
		{sha: "3d2178b8d8a03988fd0ebda6af182e0f01b2805d", date: "2022-06-09 03:55:23 UTC", description: "Namespace enterprise metrics with `vector`", pr_number: 13034, scopes: ["observability"], type: "chore", breaking_change: false, author: "Will", files_count: 2, insertions_count: 37, deletions_count: 4},
	]
}
