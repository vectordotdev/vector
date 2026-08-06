package metadata

releases: "0.11.1": {
	date: "2020-12-17"

	whats_next: []

	commits: [
		{sha: "d6de536d361c5edbbe056945b8d809e5a1999c50", date: "2020-12-03 07:38:50 UTC", description: "Change logs level on request", pr_number: 5337, scopes: ["prometheus sink"], type: "fix", breaking_change: false, author: "Kirill Fomichev", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "7e7bf612dd86867db774c560c1cf95dc6c390a1f", date: "2020-12-04 06:27:26 UTC", description: "Set content encoding header when compression is on", pr_number: 5355, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Samuel Gabel", files_count: 1, insertions_count: 20, deletions_count: 0},
		{sha: "90ac946d7f40f18cfde563bf0814fa963148010c", date: "2020-12-05 09:58:28 UTC", description: "Include config format test only with required features", pr_number: 5356, scopes: ["tests"], type: "fix", breaking_change: false, author: "Kirill Fomichev", files_count: 1, insertions_count: 11, deletions_count: 6},
		{sha: "4a59b403b0304036566af27c4ce9a6cd475b5f11", date: "2020-12-09 01:29:43 UTC", description: "Set Accept-Encoding to identity for HTTP client", pr_number: 5442, scopes: ["networking"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 48, deletions_count: 6},
		{sha: "19efdca02e8644d8a375b6f4a30a3d66e1fba0bc", date: "2020-12-09 21:42:10 UTC", description: "Add support for detecting glibc version to installer script", pr_number: 5421, scopes: ["setup"], type: "fix", breaking_change: false, author: "James Turnbull", files_count: 1, insertions_count: 27, deletions_count: 3},
		{sha: "543439ec44390ca265a26d175739ea16df4893d1", date: "2020-12-10 08:15:35 UTC", description: "Reuse buffers", pr_number: 5344, scopes: ["topology"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 16, insertions_count: 347, deletions_count: 108},
		{sha: "e070bb7a307e1d71a2be32adda99839581325c9d", date: "2020-12-17 08:11:48 UTC", description: "Fix wrong log level", pr_number: 5558, scopes: ["coercer transform"], type: "fix", breaking_change: false, author: "Duy Do", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b82d330fb492968a9aa6d8539e7228acd6c547ac", date: "2020-12-17 22:17:46 UTC", description: "Remove duplicated event", pr_number: 5451, scopes: ["vector sink"], type: "fix", breaking_change: false, author: "Kirill Fomichev", files_count: 2, insertions_count: 0, deletions_count: 17},
		{sha: "2215e49422b6d93b307ce29cbc4d89511b1f8a89", date: "2020-12-18 04:46:43 UTC", description: "Update hyper to work around the docker EOF errors", pr_number: 5561, scopes: ["docker_logs source"], type: "fix", breaking_change: false, author: "MOZGIII", files_count: 2, insertions_count: 4, deletions_count: 2},
	]
}
