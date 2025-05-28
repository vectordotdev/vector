package metadata

releases: "0.29.1": {
	date:     "2023-04-20"
	codename: ""

	description: """
		This patch release contains a fix for a regression in 0.29.0 and fixes a few issues with the release artifacts.

		**Note:** Please see the release notes for [`v0.29.0`](/releases/0.29.0/) for additional changes if upgrading from
		`v0.28.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["transforms"]
			description: """
				Certain configurations containing a chain of at least two `remap` transforms no
				longer result in a panic at startup.
				"""
			pr_numbers: [17146]
		},
	]

	commits: [
		{sha: "9246b12c03cc27363d0d67b5b35ea55628a10938", date: "2023-04-17 22:08:57 UTC", description: "Revert transform definitions", pr_number:          17146, scopes: [], type:            "chore", breaking_change: false, author: "Stephen Wakely", files_count:  86, insertions_count: 2005, deletions_count: 2558},
		{sha: "3aabd683e7b21cddf040d9ac6fcd7e43d716182e", date: "2023-04-13 08:50:00 UTC", description: "bump serde_yaml from 0.9.19 to 0.9.21", pr_number: 17120, scopes: ["deps"], type:      "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count:  8, deletions_count:    11},
		{sha: "957687b4c413759f4cb27b972da2d749fad0b0d6", date: "2023-04-12 16:15:26 UTC", description: "Fix release channels", pr_number:                  17133, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   5, insertions_count:  5, deletions_count:    5},
		{sha: "7c8c60199c3f5fe9a4859538f09a5ae5524fdfee", date: "2023-04-12 16:14:21 UTC", description: "Fix homebrew release script", pr_number:           17131, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   2, insertions_count:  36, deletions_count:   26},
	]
}
