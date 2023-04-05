package metadata

releases: "0.28.2": {
	date:     "2023-04-05"
	codename: ""

	description: """
		This patch release contains a few fixes for regressions in 0.28.0 and 0.27.0.

		**Note:** Please see the release notes for [`v0.28.0`](/releases/0.28.0/) for additional changes if upgrading from
		`v0.27.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["socket source", "logstash source", "fluent source"]
			description: """
				TCP-based sources like `socket`, `logstash`, and `fluent` would sometimes panic when
				back-pressure logic calculated a lower limit on the number of incoming
				connections than 2, which is intended to be the minimum limit.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16858]
		},
		{
			type: "fix"
			scopes: ["http_server source"]
			description: """
				The `http_server` source again has the correct default accept `method` of `POST`
				rather than `GET`.
				"""
			pr_numbers: [16746]
		},
		{
			type: "fix"
			scopes: ["elasticsearch sink"]
			description: """
				The `elasticsearch` sink no longer panics when `bulk.index` is unspecified and the
				default `mode` of `bulk` is used. Instead it defaults again to `vector-%Y.%m.%d`.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16723]
		},
		{
			type: "fix"
			scopes: ["syslog source"]
			description: """
				The `syslog` source again correctly inserts the source IP of the connection that
				events come in on as `source_ip` rather than `source_id`.
				"""
			contributors: ["zamazan4ik"]
			pr_numbers: [16836, 16837]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				The `log_schema.timestamp_key` can again be set to `""` to suppress adding of
				a timestamp to events rather than panicking when starting.
				"""
			pr_numbers: [16839]
		},
	]

	commits: [
		{sha: "97d160c69206e0fbcfae2212b93e96047a652785", date: "2023-03-31 01:15:34 UTC", description: "Update stars and contributor counts on the website", pr_number:                           17007, scopes: [], type:                  "chore", breaking_change: false, author: "Spencer Gilbert", files_count:   1, insertions_count:  2, deletions_count:   2},
		{sha: "8693da1dad282287f5ac732d8e3be2769b531441", date: "2023-03-11 00:37:04 UTC", description: "restore POST as the correct default method", pr_number:                                   16746, scopes: ["http sink"], type:       "fix", breaking_change:   false, author: "neuronull", files_count:         2, insertions_count:  10, deletions_count:  25},
		{sha: "53ef8d9abc89cac69786afac47debf7ad62134df", date: "2023-03-14 07:52:20 UTC", description: "Fix panic in parsing `BulkConfig.index` by making `BulkConfig` non-optional.", pr_number: 16723, scopes: ["elasticsearch"], type:   "fix", breaking_change:   false, author: "Alexander Zaitsev", files_count: 6, insertions_count:  82, deletions_count:  72},
		{sha: "23f3cbcbc712a2d07645b81c2cd0998222e7db5c", date: "2023-03-21 03:13:33 UTC", description: "add `source_ip` to `syslog` schema", pr_number:                                           16837, scopes: ["syslog"], type:          "fix", breaking_change:   false, author: "Stephen Wakely", files_count:    2, insertions_count:  27, deletions_count:  2},
		{sha: "6905dc647823364cce76c1fbc2e298bd838f12ac", date: "2023-03-18 05:21:49 UTC", description: "Rename source_id -> source_ip", pr_number:                                                16836, scopes: ["syslog"], type:          "fix", breaking_change:   false, author: "Alexander Zaitsev", files_count: 1, insertions_count:  3, deletions_count:   3},
		{sha: "bbc2a1cd51311a6a6934ec18160025fcdc77c363", date: "2023-03-22 03:39:32 UTC", description: "fix panic under low resources", pr_number:                                                16858, scopes: ["panic"], type:           "fix", breaking_change:   false, author: "Alexander Zaitsev", files_count: 1, insertions_count:  4, deletions_count:   1},
		{sha: "8e278378acd0ea9009dbc36c34fe0a13321fa324", date: "2023-03-28 08:18:39 UTC", description: "use date from 1999 days ago for auto_extracted timestamp integration test", pr_number:    16970, scopes: ["splunk_hec sink"], type: "fix", breaking_change:   false, author: "Stephen Wakely", files_count:    1, insertions_count:  13, deletions_count:  8},
		{sha: "d2cbd3040828c045eddb8a384996311b90b3e982", date: "2023-03-23 00:35:13 UTC", description: "Migrate `LogSchema` `timestamp` to new lookup code", pr_number:                           16839, scopes: ["core"], type:            "chore", breaking_change: false, author: "Nathan Fox", files_count:        59, insertions_count: 810, deletions_count: 380},
		{sha: "89b940dcd099a52dcf401bc2894f6c1a1a5e8490", date: "2023-03-16 23:09:02 UTC", description: "bump openssl from 0.10.45 to 0.10.46", pr_number:                                         16805, scopes: ["deps"], type:            "chore", breaking_change: false, author: "dependabot[bot]", files_count:   4, insertions_count:  33, deletions_count:  23},
		{sha: "721ee1b880395106cb6b5c53c7ba262700100c2b", date: "2023-03-21 00:46:53 UTC", description: "bump openssl from 0.10.46 to 0.10.47", pr_number:                                         16863, scopes: ["deps"], type:            "chore", breaking_change: false, author: "dependabot[bot]", files_count:   3, insertions_count:  6, deletions_count:   6},
		{sha: "a34edf45aaaac0eb57c72640b06b583483875009", date: "2023-03-24 19:43:11 UTC", description: "bump openssl from 0.10.47 to 0.10.48", pr_number:                                         16941, scopes: ["deps"], type:            "chore", breaking_change: false, author: "dependabot[bot]", files_count:   3, insertions_count:  6, deletions_count:   6},
		{sha: "0913470c7b41edffce93dec7b26547ace9ed3853", date: "2023-03-27 23:32:28 UTC", description: "Ignore RUSTSEC-2023-0029", pr_number:                                                     16969, scopes: ["ci"], type:              "chore", breaking_change: false, author: "Jesse Szwedko", files_count:     1, insertions_count:  9, deletions_count:   0},
		{sha: "168a1fc450ca389f297e71345a04406d6dc6b97d", date: "2023-03-01 03:11:16 UTC"
			description:                                      "bump tempfile from 3.3.0 to 3.4.0", pr_number: 16602, scopes: ["deps"]
			type:             "chore", breaking_change: false, author: "dependabot[bot]"
			files_count:      3
			insertions_count: 6, deletions_count: 16
		},
	]
}
