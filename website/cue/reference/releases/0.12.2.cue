package metadata

releases: "0.12.2": {
	date: "2021-03-30"

	description: """
		This release includes a few critical bug fixes and a an update to OpenSSL to 1.1.1k resolve CVE-2021-3450 and
		CVE-2021-3449.
		"""

	whats_next: []

	commits: [
		{sha: "f95aba79b1de3d5dcc2e267f3548feb9573a68f6", date: "2021-03-17 18:41:15 UTC", description: "Remap function `to_timestamp` panics on an out of range integer", pr_number: 6777, scopes: ["remap"], type: "fix", breaking_change: false, author: "Vladimir Zhuk", files_count: 1, insertions_count: 54, deletions_count: 8},
		{sha: "372e409227d751ba299bfd1be8575b6b263c2791", date: "2021-03-19 01:39:50 UTC", description: "`vector validate` checks healthchecks again", pr_number: 6810, scopes: ["config"], type: "fix", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "128cc3815cc856f5e663e3ff5c940ae1102eb9bf", date: "2021-03-22 20:03:59 UTC", description: "Parse timestamps from scraped prometheus metrics", pr_number: 6827, scopes: ["prometheus_scrape source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 236, deletions_count: 167},
		{sha: "c4e8055f706ab7890c7293eebde197e1d99a11fe", date: "2021-03-23 17:42:40 UTC", description: "Fix VRL CLI segfault", pr_number: 6852, scopes: ["remap"], type: "fix", breaking_change: false, author: "FungusHumungus", files_count: 6, insertions_count: 153, deletions_count: 15},
		{sha: "4c4d0b0638fc8fb8ee0f0359c33748010763a1f0", date: "2021-03-23 23:10:26 UTC", description: "Default timestamp to now rather than 0", pr_number: 6823, scopes: ["prometheus_remote_write sink"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 109, deletions_count: 81},
		{sha: "a049089d9d6cb39564f0f15ae039b2c585030dd1", date: "2021-03-29 19:37:00 UTC", description: "Upgrade OpenSSL to 1.1.1k", pr_number: 6886, scopes: ["deps"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "348cf496a3181eeecda4385c77768ffcdcb00582", date: "2021-03-29 22:20:24 UTC", description: "Parse µs durations with `parse_duration`", pr_number: 6885, scopes: ["remap"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 10, deletions_count: 3},
		{sha: "e13e9b28572d05f09b4711519a097ea6fc9098c2", date: "2021-03-30 06:28:58 UTC", description: "Configs using route transform are able to be reloaded now", pr_number: 6880, scopes: ["route transform"], type: "fix", breaking_change: false, author: "FungusHumungus", files_count: 2, insertions_count: 55, deletions_count: 2},
	]
}
