package metadata

releases: "0.8.2": {
	date: "2020-03-06"

	whats_next: []

	commits: [
		{sha: "361f5d1688a1573e9794c4decb0aec26e731de70", date: "2020-03-05 09:25:36 +0000", description: "Enable file sink in generate subcmd", pr_number: 1989, scopes: ["cli"], type: "fix", breaking_change: false, author: "Ashley Jeffs", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "b709ce7a15e1b42bcaae765902968158b10567ac", date: "2020-03-06 11:37:19 +0000", description: "Explicitly call GC in `lua` transform", pr_number: 1990, scopes: ["lua transform"], type: "fix", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 25, deletions_count: 8},
		{sha: "bc81e26f137de5a7ff2b8f893d7839a2052bb8a8", date: "2020-03-06 12:26:59 +0000", description: "Fix broken links", pr_number: null, scopes: [], type: "docs", breaking_change: false, author: "Alexander Rodin", files_count: 5, insertions_count: 9, deletions_count: 7},
		{sha: "ee998b2078c7019481a25881ee71764e1260c6a5", date: "2020-03-06 12:51:52 +0000", description: "Use new Homebrew installer in CI", pr_number: null, scopes: ["testing"], type: "chore", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 1, deletions_count: 1},
	]
}
