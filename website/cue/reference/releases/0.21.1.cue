package metadata

releases: "0.21.1": {
	date:     "2022-04-21"
	codename: ""

	whats_next: []

	description: """
		This patch release contains a few fixes for regressions in 0.21.0.

		**Note:** Please see the release notes for [`v0.21.0`](/releases/0.21.0/) for additional changes if upgrading from
		`v0.20.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["config", "reload"]
			description: """
				Vector no longer panics when reloading configuration that results in added components.
				"""
			pr_numbers: [12290]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				Vector no longer emits spurious `TRACE` level logs to `internal_logs` whenever `vector top` is run.
				"""
			pr_numbers: [12300]
		},
		{
			type: "fix"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` no longer panics whenever an error is received from the Kubernetes API watch
				stream.
				"""
			pr_numbers: [12248]
		},
		{
			type: "fix"
			scopes: ["elasticsearch sink", "aws provider"]
			description: """
				The `elasticsearch` sink now correctly works again when using AWS authentication to send data to AWS
				OpenSearch. Previously the security token was not included when signing the requests.
				"""
			pr_numbers: [12258]
		},
		{
			type: "fix"
			scopes: ["nats source", "nats sink"]
			description: """
				The `nats` source and sink authentication options are now configurable. Previously Vector was not
				correctly deserializing them.
				"""
			pr_numbers: [12263, 12283]
		},
		{
			type: "fix"
			scopes: ["aws provider"]
			description: """
				AWS components now correctly pass the configured region to AWS STS when `assume_role` is used. This was
				a regression during the switch to the new Rust AWS SDK.
				"""
			pr_numbers: [12315]
		},
		{
			type: "fix"
			scopes: ["aws_cloudwatch_logs sink"]
			description: """
				The `aws_cloudwatch_logs` sink now correctly retries throttled requests again. This was a regression
				during the switch to the new Rust AWS SDK.
				"""
			pr_numbers: [12315]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Vector no longer panics when used with configuration options that take event paths such as `encoding.only_fields`.
				"""
			pr_numbers: [12306]
		},
	]

	commits: [
		{sha: "6f180800e39e6f6be14e4c8c19778aaa99946176", date: "2022-04-16 01:37:51 UTC", description: "Fix deb download link", pr_number:                                                12247, scopes: [], type:                           "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  6, deletions_count:   1},
		{sha: "e48c367536724217041cbecd677b9fa014d9d285", date: "2022-04-19 02:23:33 UTC", description: "note change in option behavior in 0.21.0 highlights", pr_number:                  12259, scopes: [], type:                           "docs", breaking_change:  false, author: "Spencer Gilbert", files_count:      1, insertions_count:  17, deletions_count:  0},
		{sha: "d03cb2310230b114b32d9ca81c6255c532270acb", date: "2022-04-19 07:16:43 UTC", description: "cleanup monitoring doc and add example config", pr_number:                        12265, scopes: [], type:                           "docs", breaking_change:  false, author: "Spencer Gilbert", files_count:      1, insertions_count:  12, deletions_count:  2},
		{sha: "2d4d391b17a88b62aa1fb1af208e1807b785b278", date: "2022-04-20 03:29:18 UTC", description: "Note performance improvement on v0.21.0 release notes", pr_number:                12250, scopes: [], type:                           "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  4, deletions_count:   0},
		{sha: "ef8e62bba5452ea03391d37c918fe8eb94e214ff", date: "2022-04-20 03:30:23 UTC", description: "Document lack of support for `credential_process` for AWS components", pr_number: 12282, scopes: [], type:                           "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        2, insertions_count:  14, deletions_count:  3},
		{sha: "74c0969e05c7c5c927e6a76b1eae6748ac675cc2", date: "2022-04-20 09:35:41 UTC", description: "don't panic when checking inputs for new component during reload", pr_number:     12290, scopes: ["topology"], type:                 "fix", breaking_change:   false, author: "Toby Lawrence", files_count:        4, insertions_count:  126, deletions_count: 48},
		{sha: "ea1ae011f753995c2068e739e028b27c42820602", date: "2022-04-20 07:10:40 UTC", description: "Add known issues to 0.21.0 release", pr_number:                                   12278, scopes: [], type:                           "docs", breaking_change:  false, author: "Spencer Gilbert", files_count:      1, insertions_count:  6, deletions_count:   0},
		{sha: "9ad2a0e7be3949e429ebfc65d42d6f4d6fb69b55", date: "2022-04-21 02:55:58 UTC", description: "Fix docs for tags", pr_number:                                                    12321, scopes: ["internal_metrics"], type:         "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:        1, insertions_count:  11, deletions_count:  16},
		{sha: "281ce5f4296e12b0f7350c2acf426df5f35e7540", date: "2022-04-21 06:14:24 UTC", description: "Bump next version to v0.21.1", pr_number:                                         12323, scopes: ["releasing"], type:                "chore", breaking_change: false, author: "Jesse Szwedko", files_count:        2, insertions_count:  2, deletions_count:   2},
		{sha: "5cc699c15e140be2aa50c3f2cacdb12f3a98a107", date: "2022-04-21 23:54:27 UTC", description: "apply correct log level filtering", pr_number:                                    12300, scopes: ["internal_logs source"], type:     "fix", breaking_change:   false, author: "Toby Lawrence", files_count:        1, insertions_count:  24, deletions_count:  71},
		{sha: "3ed373f50150294171189b0a1881a0adead7d332", date: "2022-04-19 02:45:51 UTC", description: "Handle all outcomes in reflector's select!", pr_number:                           12248, scopes: ["kubernetes_logs source"], type:   "fix", breaking_change:   false, author: "Spencer Gilbert", files_count:      3, insertions_count:  53, deletions_count:  24},
		{sha: "10ed692efbee4ef8786c23bc4a84bdd9082922c7", date: "2022-04-19 22:56:54 UTC", description: "Include security token when signing request", pr_number:                          12258, scopes: ["elasticsearch sink"], type:       "fix", breaking_change:   false, author: "Nathan Fox", files_count:           1, insertions_count:  6, deletions_count:   4},
		{sha: "5eead8a972e01796d318ac78c2f82c9a98b1d7ce", date: "2022-04-19 23:13:48 UTC", description: "Fix handling of auth option naming", pr_number:                                   12263, scopes: ["nats sink", "nats source"], type: "fix", breaking_change:   false, author: "Bruce Guenter", files_count:        3, insertions_count:  239, deletions_count: 163},
		{sha: "0a4ed84367e7178022c1a30c76c8254e5e2d9e0b", date: "2022-04-20 04:03:22 UTC", description: "Unflatten `auth` configuration", pr_number:                                       12283, scopes: ["nats source", "nats sink"], type: "fix", breaking_change:   false, author: "Bruce Guenter", files_count:        2, insertions_count:  1, deletions_count:   2},
		{sha: "8a9f37a122177cb3e3990011271d800600ed39cd", date: "2022-04-21 04:27:19 UTC", description: "Pass configured region to credentials provider", pr_number:                       12315, scopes: ["aws provider"], type:             "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:        5, insertions_count:  35, deletions_count:  14},
		{sha: "9a0fbb47dcd4ae9d0f1224bc390635d07f1b417f", date: "2022-04-21 08:16:39 UTC", description: "Retry ThrottlingException", pr_number:                                            12286, scopes: ["cloudwatch logs sink"], type:     "fix", breaking_change:   false, author: "Nathan Fox", files_count:           4, insertions_count:  59, deletions_count:  76},
		{sha: "d6da5b2d198a12ea71149c656202eefeaaa78ac8", date: "2022-04-21 08:22:25 UTC", description: "Mark `vector config` as experimental", pr_number:                                 12324, scopes: [], type:                           "chore", breaking_change: false, author: "Jesse Szwedko", files_count:        1, insertions_count:  1, deletions_count:   1},
		{sha: "91d6610effe9d651d0040367252051ced76056ec", date: "2022-04-21 22:54:58 UTC", description: "Add support for flags to `vector config`", pr_number:                             12327, scopes: ["config"], type:                   "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:        2, insertions_count:  65, deletions_count:  4},
		{sha: "a5166b6d107762af90920989799094c630f79705", date: "2022-04-21 23:12:13 UTC", description: "Make region required configuration", pr_number:                                   12313, scopes: ["aws provider"], type:             "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:        13, insertions_count: 33, deletions_count:  56},
		{sha: "b142d3d1a721f5714cdf583edca006899a310931", date: "2022-04-22 09:19:20 UTC", description: "Use user-provided array values in vector config --include-defaults", pr_number:   12337, scopes: ["cli"], type:                      "fix", breaking_change:   false, author: "Will", files_count:                 1, insertions_count:  4, deletions_count:   13},
		{sha: "cbf7d6c7f872a06586fafd783ebbc269baea7b46", date: "2022-04-22 09:37:58 UTC", description: "Implement `Serialize`/`Display` for `OwnedPath`", pr_number:                      12306, scopes: ["config"], type:                   "fix", breaking_change:   false, author: "Pablo Sichert", files_count:        2, insertions_count:  92, deletions_count:  3},
		{sha: "c672d1bb238ab68f9e335bf153449dcd50fc1496", date: "2022-04-22 03:03:53 UTC", description: "Fix merge conflict for AWS auth changes", pr_number:                              12344, scopes: ["ci"], type:                       "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:        5, insertions_count:  24, deletions_count:  20},
		{sha: "6110d517b9f22962bd4f9cfd6a5da979de341b37", date: "2022-04-16 06:46:58 UTC", description: "typoes", pr_number:                                                               12238, scopes: [], type:                           "docs", breaking_change:  false, author: "Tshepang Lekhonkhobe", files_count: 1, insertions_count:  2, deletions_count:   2},
	]
}
