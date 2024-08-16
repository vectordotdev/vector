package metadata

releases: "0.37.1": {
	date:     "2024-04-08"
	codename: ""

	whats_next: []

	description: """
		This patch release contains fixes for regressions in 0.37.0.

		**Note:** Please see the release notes for [`v0.37.0`](/releases/0.37.0/) for additional
		changes if upgrading from `v0.36.X`. In particular, see the upgrade guide for breaking
		changes.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				Fixed an issue where `GeoLite2-City` MMDB database type was not supported.
				"""
			contributors: ["esensar"]
		},
	]

	commits: [
		{sha: "dd984ea225b453c5a22f59cbff87f1fa6919237a", date: "2024-03-29 03:48:25 UTC", description: "note for 0.37 about incorrect ddtags parsing behavior", pr_number: 20186, scopes: ["docs"], type: "chore", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 12, deletions_count: 0},
		{sha: "716160dba256255bd43a897ec57ca8359ec44f0c", date: "2024-03-27 00:51:35 UTC", description: "Remove package deprecation banner", pr_number: 20181, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a81f3b3c5039e818989d425ed787e885304c8df1", date: "2024-03-29 04:16:51 UTC", description: "Add breaking change note for dnstap source mode", pr_number: 20202, scopes: ["dnstap source", "releasing"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 7, deletions_count: 0},
		{sha: "3f4859f86cb345149a167c0aaef38b8bda2ec912", date: "2024-04-02 08:34:02 UTC", description: "fix type cardinality docs", pr_number: 20209, scopes: [], type: "docs", breaking_change: false, author: "Michael サイトー 中村 Bashurov", files_count: 3, insertions_count: 7, deletions_count: 9},
		{sha: "17fb71c10950357fbcfff818458b6cbd4fe6e0fa", date: "2024-03-28 08:18:40 UTC", description: "bring back support for `GeoLite2-City` db", pr_number: 20192, scopes: ["enrichment_tables"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "b7495c271be5138dfb677ea05cbbfad0ce623584", date: "2024-03-28 05:35:54 UTC", description: "peg `fakeintake` docker image", pr_number: 20196, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 8, deletions_count: 4},
		{sha: "4496b3dba977444e39d8fdfbba56574ea5fecb7a", date: "2024-03-28 23:46:26 UTC", description: "Drop `apt-get upgrade`", pr_number: 20203, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "2cb35f197f322daec498fd763706577b224a09c4", date: "2024-03-29 02:31:59 UTC", description: "Remove pip install of modules", pr_number: 20204, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 0, deletions_count: 4},
		{sha: "1cb0b96459ba23aa821fee2725680a42146944c6", date: "2024-03-30 05:14:41 UTC", description: "Only use one label for selecting GHA runner", pr_number: 20210, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 11, insertions_count: 26, deletions_count: 26},
		{sha: "8c99fd31e51934647b1e7e4e341992b4edee4070", date: "2024-03-29 05:32:26 UTC", description: "align `ddtags` parsing with DD logs intake", pr_number: 20184, scopes: ["datadog_agent"], type: "fix", breaking_change: true, author: "neuronull", files_count: 5, insertions_count: 16, deletions_count: 37},
		{sha: "e3831a86e77057acfb4ecccb1625d7a5c381fd8c", date: "2024-04-02 21:45:46 UTC", description: "reconstruct `ddtags` if not already in format expected by DD logs intake", pr_number: 20198, scopes: ["datadog_logs sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 195, deletions_count: 28},
		{sha: "7f0dc8fc63ec982a16682a462fc10584f21cdbba", date: "2024-04-05 21:31:59 UTC", description: "Update h2", pr_number: 20236, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 10, deletions_count: 10},

	]
}
