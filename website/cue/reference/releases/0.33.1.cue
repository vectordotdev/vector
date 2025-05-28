package metadata

releases: "0.33.1": {
	date:     "2023-10-30"
	codename: ""

	whats_next: []

	description: """
		This patch release contains fixes for regressions in 0.33.0 and fixes an issues with the Debian release artifacts.

		**Note:** Please see the release notes for [`v0.33.0`](/releases/0.33.0/) for additional changes if upgrading from
		`v0.32.X`. In particular, see the upgrade guide for breaking changes.
		"""

	changelog: [
		{
			type: "fix"
			scopes: ["releasing", "debian"]
			description: """
				Debian packages again avoid overwriting existing configuration files when upgrading.
				"""
			pr_numbers: [18718]
		},
		{
			type: "fix"
			scopes: ["datadog_metrics sink"]
			description: """
				The performance of the Datadog Metrics sink was greatly improved when the incoming
				metric stream contains mostly counters.
				"""
			pr_numbers: [18759]
		},
		{
			type: "fix"
			scopes: ["dnstap source"]
			description: """
				The `dnstap` source can again parse DNSSEC/RRSIG RRs records.
				"""
			pr_numbers: [18878]
		},
		{
			type: "fix"
			scopes: ["kafka sink"]
			description: """
				A performance regression in the `kafka` sink was corrected.
				"""
			pr_numbers: [18770]
		},
	]

	commits: [
		{sha: "eae7b827fb885af5af12419b3451c841df06abdf", date: "2023-09-30 03:07:13 UTC", description: "Add known issue for 0.33.0 debian packaging regression", pr_number:               18727, scopes: ["releasing"], type:            "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count: 3, deletions_count:   1},
		{sha: "4b72f7e13c7607705fe16227259bd7b1429fc1f7", date: "2023-10-04 06:52:37 UTC", description: "Set download page dropdown to latest version", pr_number:                         18758, scopes: ["website"], type:              "chore", breaking_change: false, author: "Devin Ford", files_count:      1, insertions_count: 1, deletions_count:   1},
		{sha: "9c31f322df7114b70231b843fcd975087ede2a5d", date: "2023-09-27 10:13:36 UTC", description: "Bump tokio-tungstenite from 0.20.0 to 0.20.1", pr_number:                         18661, scopes: ["deps"], type:                 "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 9, deletions_count:   9},
		{sha: "45a9fcf60e51ac213f3cd018183b5890feb5a317", date: "2023-10-04 05:19:08 UTC", description: "Bump webpki from 0.22.1 to 0.22.2", pr_number:                                    18744, scopes: ["deps"], type:                 "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count:   1},
		{sha: "abe84489cc5cfa19490d83576f867073a30f62da", date: "2023-09-30 05:01:39 UTC", description: "Bump warp from 0.3.5 to 0.3.6", pr_number:                                        18704, scopes: ["deps"], type:                 "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count:   38},
		{sha: "1b9a012cd4590b3f5f40b3190b3577ea9eb53046", date: "2023-09-30 05:16:11 UTC", description: "Re-add `conf-files` directive for `cargo-deb`", pr_number:                        18726, scopes: ["debian platform"], type:      "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:   0},
		{sha: "b8b6f9ed76141de290307b3d414c785dd4230ce1", date: "2023-10-06 05:45:13 UTC", description: "improve aggregation performance", pr_number:                                      18759, scopes: ["datadog_metrics sink"], type: "fix", breaking_change:   false, author: "Doug Smith", files_count:      5, insertions_count: 173, deletions_count: 201},
		{sha: "05527172d1ab16d4c4481d392bdc26c89528beab", date: "2023-10-21 02:31:29 UTC", description: "Update example YAML config data_dir", pr_number:                                  18896, scopes: ["releasing"], type:            "fix", breaking_change:   false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:   1},
		{sha: "1d4487c3b12033dba8dc58ae4199706552e3014b", date: "2023-10-21 01:33:10 UTC", description: "support DNSSEC RRSIG record data", pr_number:                                     18878, scopes: ["dnstap source"], type:        "fix", breaking_change:   false, author: "neuronull", files_count:       2, insertions_count: 52, deletions_count:  1},
		{sha: "b246610fd53f62ec07682a22f79d664f5ad031bc", date: "2023-10-25 07:21:51 UTC", description: "Make KafkaService return `Poll::Pending` when producer queue is full", pr_number: 18770, scopes: ["kafka sink"], type:           "fix", breaking_change:   false, author: "Doug Smith", files_count:      2, insertions_count: 78, deletions_count:  26},
		{sha: "391067761a210341f68ad3a4db8fcd0cfa42e578", date: "2023-09-28 06:28:14 UTC", description: "Remove or replace mentions of vector in functions doc", pr_number:                18679, scopes: ["external docs"], type:        "chore", breaking_change: false, author: "May Lee", files_count:         9, insertions_count: 21, deletions_count:  21},
		{sha: "9ca6c7b186e73605359844db0bb20946bfdc6390", date: "2023-10-24 05:48:36 UTC", description: "add new dedicated page for TLS configuration", pr_number:                         18844, scopes: ["tls"], type:                  "docs", breaking_change:  false, author: "Hugo Hromic", files_count:     7, insertions_count: 164, deletions_count: 3},
		{sha: "0e0f6411608b8f37c311c3923c352e58f88d869b", date: "2023-09-28 11:44:20 UTC", description: "Add SHA256 checksums file to GH releases", pr_number:                             18701, scopes: [], type:                       "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 6, deletions_count:   0},
		{sha: "5037fe756a2216cfa340b842387f745af2c29363", date: "2023-10-04 05:46:40 UTC", description: "Add a test to assert conf files aren't overwritten", pr_number:                   18728, scopes: ["ci"], type:                   "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   2, insertions_count: 33, deletions_count:  15},
		{sha: "b4262335582c4466bbef5a9371433bdfbefaf587", date: "2023-10-12 06:27:08 UTC", description: "Bump MacOS unit test runners to 13", pr_number:                                   18823, scopes: ["ci"], type:                   "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:   1},
		{sha: "792a1b541aaa1b34bef605bb9be4f0787b35afab", date: "2023-10-03 06:52:13 UTC", description: "Fix cookie banner style issues", pr_number:                                       18745, scopes: ["ci"], type:                   "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   3, insertions_count: 11, deletions_count:  6},
	]
}
