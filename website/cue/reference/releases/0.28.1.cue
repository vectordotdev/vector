package metadata

releases: "0.28.1": {
	date:     "2023-03-06"
	codename: ""

	description: """
		This patch release contains a few fixes for regressions in 0.28.0.

		**Note:** Please see the release notes for [`v0.28.0`](/releases/0.28.0/) for additional changes if upgrading from
		`v0.27.X`. In particular, the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["aws provider"]
			description: """
				All AWS components now correctly sign requests again. This impacted all AWS
				components except the `aws_s3` sink and caused the components to output AWS service
				errors for each request.
				"""
			pr_numbers: [16651]
		},
		{
			type: "fix"
			scopes: ["socket source"]
			description: """
				The `framing.*.max_length` configuration options can again be used on the `socket`
				source. Previously they returned an error that they conflicted with the deprecated
				top-level `max_length` configuration option.
				"""
			pr_numbers: [16631]
		},
	]

	commits: [
		{sha: "764c542e36a13d2846c01a2d64b6a2d033fa7797", date: "2023-03-01 15:34:20 UTC", description: "Add known issues for 0.28.0.", pr_number:                                    16649, scopes: ["releasing"], type:     "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count:  12, deletions_count:  0},
		{sha: "366d34eb93e5852350b6675d96a2701d5d4611e1", date: "2023-03-01 16:22:29 UTC", description: "Add a links to footer for to the /releases and /downloads pages", pr_number: 16441, scopes: ["website"], type:       "chore", breaking_change: false, author: "neuronull", files_count:       3, insertions_count:  16, deletions_count:  1},
		{sha: "08406152030236d35130899454d4a934449923ac", date: "2023-03-01 22:46:02 UTC", description: "Revert AWS-SDK updates", pr_number:                                          16651, scopes: ["aws service"], type:   "fix", breaking_change:   false, author: "Spencer Gilbert", files_count: 27, insertions_count: 421, deletions_count: 447},
		{sha: "73bf8728e7d8c74d71730ffb554597440b273ce5", date: "2023-03-01 00:28:22 UTC", description: "remove default_max_length default", pr_number:                               16631, scopes: ["socket source"], type: "fix", breaking_change:   false, author: "Stephen Wakely", files_count:  3, insertions_count:  1, deletions_count:   6},
	]
}
