package metadata

releases: "0.24.2": {
	date:     "2022-10-12"
	codename: ""

	whats_next: []

	description: """
		This patch release contains a few fixes for regressions and bugs in new functionality from 0.24.0.

		**Note:** Please see the release notes for [`v0.24.0`](/releases/0.24.0/) for additional changes if upgrading from
		`v0.23.X`. In particular, the upgrade guide for breaking changes.
		"""
	changelog: [
		{
			type: "fix"
			scopes: ["datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now properly encodes rate metrics received from the
				`datadog_agent` source.
				"""
			pr_numbers: [14741]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				The `expire_metrics_secs` setting is now correctly applied to expire timeseries not
				seen in the configured number of seconds.
				"""
			pr_numbers: [14741]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Options that take a field name (like the global `timestamp_key` option) can now be
				disabled again by supplying an empty string (`""`) for the config option.
				"""
			pr_numbers: [14421]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				VRL now longer returns an incorrect invalid type error for closures ("block returns
				invalid value type").
				"""
			pr_numbers: [14797]
		},
	]

	commits: [
		{sha: "08d4be802206ed01c2b78e5d3675eaf847145f11", date: "2022-09-13 21:40:39 UTC", description: "Remove duplicate release bullet", pr_number:                                                14395, scopes: ["releasing"], type:            "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   1, insertions_count: 0, deletions_count:  10},
		{sha: "218f4a3f856cb7df8c8b461dd45b24cbd5a382e6", date: "2022-09-14 02:34:02 UTC", description: "fix wrong default value for `suppress_timestamp` in `prometheus_exporter` sink", pr_number: 14389, scopes: ["docs"], type:                 "fix", breaking_change:   false, author: "Hugo Hromic", files_count:     1, insertions_count: 1, deletions_count:  1},
		{sha: "5c769f87870b6f2d1c0cd6c277cd6a832c37d0b9", date: "2022-09-14 04:46:48 UTC", description: "Fix labels for uptime_seconds", pr_number:                                                  14403, scopes: ["internal_metrics"], type:     "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:  1},
		{sha: "b3578c5dd8aefc03b7146b256ba6d77d3da9271e", date: "2022-09-17 02:34:21 UTC", description: "Re-add `tls.enabled` option", pr_number:                                                    14457, scopes: ["vector sink"], type:          "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:  1},
		{sha: "23a344532f0e344e6b4b7df17fc6cfbec04060e4", date: "2022-09-23 06:29:12 UTC", description: "Fix description of load1/5/15 metrics", pr_number:                                          14527, scopes: ["host_metrics source"], type:  "docs", breaking_change:  false, author: "Bruce Guenter", files_count:   1, insertions_count: 3, deletions_count:  3},
		{sha: "8d5fcac7f7fcf5e4f305a3bf1e16fe3575fca5bc", date: "2022-09-24 02:10:19 UTC", description: "Clarify behavior of expiring internal counters", pr_number:                                 14534, scopes: ["observability"], type:        "docs", breaking_change:  false, author: "Bruce Guenter", files_count:   1, insertions_count: 5, deletions_count:  0},
		{sha: "b58227e3c42eb802738118743bf063b4ad0b4787", date: "2022-09-24 03:29:59 UTC", description: "Making the example `endpoint` valid format", pr_number:                                     14543, scopes: ["external docs"], type:        "chore", breaking_change: false, author: "Kyle Criddle", files_count:    1, insertions_count: 1, deletions_count:  1},
		{sha: "7a367e795bbd04eb0581dda52f4b0b90a21aab13", date: "2022-09-24 04:38:34 UTC", description: "Remove hanging paragraph about config watching", pr_number:                                 14545, scopes: ["reload"], type:               "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:  5},
		{sha: "f2654c58e25f98f23ff17afc158dc63afb361cc0", date: "2022-10-03 20:38:36 UTC", description: "Update AWS documentation", pr_number:                                                       14675, scopes: [], type:                       "docs", breaking_change:  false, author: "Alexander Chen", files_count:  1, insertions_count: 0, deletions_count:  13},
		{sha: "a3007780cb0a5f7636429f3c8d36d96df5e83095", date: "2022-10-05 03:20:49 UTC", description: "Re-add concurrency diagram", pr_number:                                                     14717, scopes: [], type:                       "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   1, insertions_count: 2, deletions_count:  0},
		{sha: "c72c660316eeef9b492a5a0d0e1c95df5795e57d", date: "2022-10-07 02:10:02 UTC", description: "Fix documented name of `source_lag_time_seconds`", pr_number:                               14755, scopes: ["observability"], type:        "docs", breaking_change:  false, author: "Bruce Guenter", files_count:   2, insertions_count: 7, deletions_count:  7},
		{sha: "0ec568b65a7532d86fd25c9562d5a3702a8714ef", date: "2022-09-15 03:57:07 UTC", description: "allow empty log schema keys", pr_number:                                                    14421, scopes: ["config"], type:               "fix", breaking_change:   false, author: "Nathan Fox", files_count:      2, insertions_count: 35, deletions_count: 20},
		{sha: "0ce1ea44108883e07229adebcb8ac45fb1680ad9", date: "2022-09-15 05:28:03 UTC", description: "Handle both expire_metrics settings when merging configs", pr_number:                       14423, scopes: ["config"], type:               "fix", breaking_change:   false, author: "Bruce Guenter", files_count:   2, insertions_count: 19, deletions_count: 0},
		{sha: "0cc2b905e0593e8b432c42b9a77291a50ddf169f", date: "2022-10-06 05:55:18 UTC", description: "properly encode rate metrics", pr_number:                                                   14741, scopes: ["datadog_metrics sink"], type: "fix", breaking_change:   false, author: "Toby Lawrence", files_count:   1, insertions_count: 46, deletions_count: 7},
		{sha: "b32c61e064c720c462507211fa01966204848352", date: "2022-10-06 22:50:36 UTC", description: "website doesn't render deprecation warn", pr_number:                                        14751, scopes: ["docs"], type:                 "fix", breaking_change:   false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count:  0},
		{sha: "79ae6f4d2023e646475212762c80dc71f34287b8", date: "2022-10-12 05:10:41 UTC", description: "Document `method` option", pr_number:                                                       14802, scopes: ["http sink"], type:            "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   1, insertions_count: 12, deletions_count: 0},
		{sha: "1ed431f0915292678cdd8ac04c299cd07205cb64", date: "2022-10-13 00:50:53 UTC", description: "fix closure block type calculation", pr_number:                                             14797, scopes: ["vrl"], type:                  "fix", breaking_change:   false, author: "Nathan Fox", files_count:      2, insertions_count: 3, deletions_count:  1},
		{sha: "1f8859f663079c5c26435060e59d655e16f607de", date: "2022-10-05 06:12:32 UTC", description: "Check before installing cargo-deny", pr_number:                                             14721, scopes: ["tests"], type:                "fix", breaking_change:   false, author: "Bruce Guenter", files_count:   2, insertions_count: 4, deletions_count:  2},
	]
}
