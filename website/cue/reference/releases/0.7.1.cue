package metadata

releases: "0.7.1": {
	date: "2020-01-24"

	whats_next: []

	commits: [
		{sha: "e5096d0ad09333cdcbf7f7b8fdea71764c61b940", date: "2020-01-22 17:53:08 +0000", description: "Make sorting of blog posts stable", pr_number: 1566, scopes: ["operations"], type: "chore", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "ed11b854a21f8f8f4d0b532d2b946ed0d3a91718", date: "2020-01-22 18:00:10 +0000", description: "Add AWS API key for Windows tests in CI", pr_number: 1565, scopes: ["operations"], type: "chore", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "e7bd9180249751dcef6299f4836b0a82274ec2f9", date: "2020-01-22 18:00:26 +0000", description: "Pass `CIRCLE_SHA1` environment variable to `release-github` job", pr_number: 1567, scopes: ["operations"], type: "chore", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "95373bd03fa70d2bcbcfa9c6b02550bcd65d0623", date: "2020-01-22 11:36:26 +0000", description: "Fix crash when `in_flight_limit` is set to `1`", pr_number: 1569, scopes: ["aws_s3 sink"], type: "fix", breaking_change: false, author: "Lucio Franco", files_count: 1, insertions_count: 31, deletions_count: 1},
		{sha: "678be7404a236bb6f5e596d117f8cadd16e5a690", date: "2020-01-22 19:30:47 +0000", description: "Fix error when socket addresses do not use `IPV4` or `IPV6` addresses", pr_number: 1575, scopes: ["socket sink"], type: "fix", breaking_change: false, author: "Lucio Franco", files_count: 9, insertions_count: 117, deletions_count: 118},
		{sha: "c7de358cb72d38bc82544ba2c42c01a96be77961", date: "2020-01-23 13:28:37 +0000", description: "Fix `aws_kinesis_firehose` sink healthcheck", pr_number: 1573, scopes: ["aws_kinesis_firehose sink"], type: "fix", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 13, deletions_count: 22},
		{sha: "e5a3113f0ddfbcb08c6ce70dda374abbfdbc867d", date: "2020-01-23 14:51:32 +0000", description: "Escape special characters in options descriptions", pr_number: 1580, scopes: ["website"], type: "chore", breaking_change: false, author: "Alexander Rodin", files_count: 26, insertions_count: 53, deletions_count: 53},
		{sha: "e1b6bc834a94066313c2de58e540845476289789", date: "2020-01-23 19:44:22 +0000", description: "Create `vector` user when installing RPM package", pr_number: 1583, scopes: ["rpm platform"], type: "fix", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 11, deletions_count: 2},
		{sha: "dc3275705489d55e86d10f609fd5caf090b65f5d", date: "2020-01-23 21:44:23 +0000", description: "Support bug fixing releases", pr_number: 1587, scopes: ["operations"], type: "chore", breaking_change: false, author: "Alexander Rodin", files_count: 2, insertions_count: 18, deletions_count: 5},
		{sha: "8287f0535d1ddd5e6fadaf1368623dbe3d7579b0", date: "2020-01-23 22:26:44 +0000", description: "Add all generated files to the release commit", pr_number: 1588, scopes: ["operations"], type: "chore", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "18b2d2f0d3b4f6df22883550dd106c2ec8c051d4", date: "2020-01-23 23:31:55 +0000", description: "Do not require `systemd` as an RPM dependency", pr_number: 1590, scopes: ["rpm platform"], type: "fix", breaking_change: false, author: "Alexander Rodin", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "d9052b84a872f6562dfd0318a6c6c887c92fda34", date: "2020-01-23 23:37:55 +0000", description: "Add `release-push` target to the Makefile", pr_number: 1589, scopes: ["operations"], type: "chore", breaking_change: false, author: "Alexander Rodin", files_count: 2, insertions_count: 30, deletions_count: 0},
	]
}
