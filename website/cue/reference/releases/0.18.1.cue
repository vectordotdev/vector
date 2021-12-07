package metadata

releases: "0.18.1": {
	date:     "2021-11-30"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version `v0.18.1`!

		This patch release contains a few bug fixes for regressions in `v0.18.0`.

		**Note:** Please see the release notes for [`v0.18.0`](/releases/0.18.0/) for additional changes if upgrading from
		`v0.17.X`. In particular, the upgrade guide for breaking changes.

		## Bug Fixes:

		- The new [automatic namespacing](/highlights/2021-11-18-implicit-namespacing/) feature broke usages of
		  `--config-dir` when directories were present that did not match Vector's config schema. Vector now just
		  ignores these directories and only looks at known namespacing directories like `sources/`.
		  [#10173](https://github.com/vectordotdev/vector/pull/10173)
		  [#10177](https://github.com/vectordotdev/vector/pull/10177).
		- The `elasticsearch` sink no longer logs a debug message for each event.
		  [#10117](https://github.com/vectordotdev/vector/pull/10117).
		- The `remap` transform now only creates the `.dropped` output (as part of the new [failed event routing
		  feature](/highlights/2021-11-18-failed-event-routing/)) whenever `reroute_dropped = true`.
		  [#10152](https://github.com/vectordotdev/vector/pull/10152)
		- A change to internal telemetry had caused aggregated histograms emitted by
		  the `prometheus_exporter` and `prometheus_remote_write` sinks to be
		  incorrectly tallied. This was fixed. [#10165](https://github.com/vectordotdev/vector/pull/10165)
		"""

	commits: [
		{sha: "685603b673454be46c7da966670c79b729cc8388", date: "2021-11-20 05:37:27 UTC", description: "Add known issues section to 0.18.0", pr_number:                                             10120, scopes: [], type:                           "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 3, deletions_count:   0},
		{sha: "d0db8c9816474f7e15772e1d3d7ca5c4948c3bb3", date: "2021-11-23 06:47:57 UTC", description: "Fix automatic namespacing example transform name", pr_number:                               10137, scopes: [], type:                           "docs", breaking_change:  false, author: "Jesse Szwedko", files_count:   1, insertions_count: 2, deletions_count:   2},
		{sha: "3f985ac47acdd6c2bfe81610aebd0de77e08ded2", date: "2021-11-25 06:45:40 UTC", description: "Make example in guides/level-up/managing-complex-configs work with Vector 0.18", pr_number: 10168, scopes: ["config"], type:                   "docs", breaking_change:  false, author: "Robin Schneider", files_count: 1, insertions_count: 2, deletions_count:   3},
		{sha: "3abe791b7f3357ad5ec458d8b80953e4d16c1f1e", date: "2021-11-20 04:07:59 UTC", description: "Remove println from es service", pr_number:                                                 10117, scopes: ["elasticsearch sink"], type:       "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 0, deletions_count:   1},
		{sha: "957a57600ba20db26c85bce46b83cad16aa636bb", date: "2021-11-24 06:59:38 UTC", description: "only add dropped output when enabled", pr_number:                                           10152, scopes: ["remap transform"], type:          "fix", breaking_change:   false, author: "Luke Steensen", files_count:   1, insertions_count: 66, deletions_count:  2},
		{sha: "600a0cfb18c7f64852c026a30b5b6c197365b923", date: "2021-11-25 06:09:49 UTC", description: "agg histograms dont encode buckets correctly", pr_number:                                   10165, scopes: ["prometheus_exporter sink"], type: "fix", breaking_change:   false, author: "Toby Lawrence", files_count:   8, insertions_count: 276, deletions_count: 79},
		{sha: "852cd41b7bbb9f8c7e4da97ae32429882c1235d2", date: "2021-11-30 03:32:59 UTC", description: "Install bundler on OSX", pr_number:                                                         10191, scopes: ["ci"], type:                       "chore", breaking_change: false, author: "Jesse Szwedko", files_count:   1, insertions_count: 2, deletions_count:   0},
	]
}
