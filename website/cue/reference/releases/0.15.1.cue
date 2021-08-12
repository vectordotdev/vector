package metadata

releases: "0.15.1": {
	date:     "2021-07-27"
	codename: ""

	description: """
		The Vector team is pleased to announce version 0.15.1!

		This release simply contains a bug fix for an RPM packaging regression in 0.15.0 where the RPM would not
		properly install if a previous version of the RPM was installed.

		See the release notes for 0.15.0 for additional changes if upgrading from 0.14.X.
		"""

	commits: [
		{sha: "d936fe1e28aac0702a5cbc4f88bbee6d63adebaa", date: "2021-07-15 04:47:47 UTC", description: "Upgrade Tailwind to 2.24", pr_number:                                     8295, scopes: ["css website"], type:    "enhancement", breaking_change: false, author: "Luc Perkins", files_count:     2, insertions_count: 160, deletions_count: 388},
		{sha: "e6798a528403790a805a10774a11d154a2179ed1", date: "2021-07-16 01:00:57 UTC", description: "Fix overflow scroll behavior on component pages", pr_number:              8321, scopes: ["css website"], type:    "fix", breaking_change:         false, author: "Luc Perkins", files_count:     1, insertions_count: 2, deletions_count:   2},
		{sha: "e40031568ba9136f74a8108d67f8b3139ef56e9b", date: "2021-07-16 04:06:49 UTC", description: "Add documentation for missing VRL functions", pr_number:                  8265, scopes: ["external docs"], type:  "fix", breaking_change:         false, author: "Luc Perkins", files_count:     5, insertions_count: 86, deletions_count:  78},
		{sha: "918084f476480660e87703a1fe3fafc402598dc3", date: "2021-07-16 22:54:31 UTC", description: "Replace SystemD service file with previous", pr_number:                   8342, scopes: ["releasing"], type:      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:   2, insertions_count: 57, deletions_count:  32},
		{sha: "477a4499be1a4125d4a6d6650018d5206ffa4d7b", date: "2021-07-16 04:21:46 UTC", description: "Add missing changes from the old docs/ directory", pr_number:             8324, scopes: [], type:                 "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   3, insertions_count: 192, deletions_count: 3},
		{sha: "3ea0aaf6100a23210997eec24000e29c67c59250", date: "2021-07-16 05:03:39 UTC", description: "Upgrade Algolia search widget", pr_number:                                8303, scopes: ["search website"], type: "enhancement", breaking_change: false, author: "Luc Perkins", files_count:     5, insertions_count: 252, deletions_count: 135},
		{sha: "9480188ba105407751426e9ea6441a0c898a78f2", date: "2021-07-16 05:14:36 UTC", description: "Fix broken links in getting started guide", pr_number:                    8327, scopes: ["external docs"], type:  "fix", breaking_change:         false, author: "Luc Perkins", files_count:     2, insertions_count: 9, deletions_count:   8},
		{sha: "ebf0d296a918fbbb7d5ad2add0909a643b1f0bd6", date: "2021-07-16 22:20:34 UTC", description: "Clarify `prometheus_remote_write` usage", pr_number:                      8269, scopes: [], type:                 "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   3, insertions_count: 19, deletions_count:  1},
		{sha: "994d81239a9b5ea4eae3e92530e601629b593cc6", date: "2021-07-16 23:10:37 UTC", description: "clarify remote_write flag for prometheus", pr_number:                     8343, scopes: [], type:                 "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   1, insertions_count: 1, deletions_count:   1},
		{sha: "1d51dd9adbc4d42aadcc140f513f8e7783281c6d", date: "2021-07-17 00:27:50 UTC", description: "Add 0.15.0 highlight for emitting multiple events from remap", pr_number: 8346, scopes: [], type:                 "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   1, insertions_count: 100, deletions_count: 0},
		{sha: "bc15debcac84a0fc4c052a5ea90fa6f01cf62854", date: "2021-07-17 08:49:45 UTC", description: "Only use term \"component name\" not \"component id\"", pr_number:        8344, scopes: [], type:                 "docs", breaking_change:        false, author: "Robin Schneider", files_count: 8, insertions_count: 14, deletions_count:  14},
		{sha: "b5e3ad82d3c0ef8daf6bfae028fd882fa62ace68", date: "2021-07-17 00:48:12 UTC", description: "Provide redirects for all integration guides", pr_number:                 8332, scopes: ["external docs"], type:  "fix", breaking_change:         false, author: "Luc Perkins", files_count:     4, insertions_count: 113, deletions_count: 22},
		{sha: "aaf9a65ae802ee91ce9503851ed3d9c2abaf438e", date: "2021-07-17 03:29:58 UTC", description: "Move highlights up on release page", pr_number:                           8348, scopes: [], type:                 "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:   1, insertions_count: 14, deletions_count:  14},
		{sha: "142427f16d01b341c566e506fbaa6bfd563d655e", date: "2021-07-26 22:02:02 UTC", description: "Remove creation of vector group from rpm", pr_number:                     8446, scopes: ["releasing"], type:      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:   4, insertions_count: 28, deletions_count:  9},

	]

	whats_next: [
		{
			title:       "Enabling Adaptive Concurrency Control by default"
			description: """
				We released [Adaptive Concurrency Control](\(urls.adaptive_request_concurrency_post)) in version 0.11.0
				of Vector, but, up until now, this feature has been opt-in. We've been collecting user feedback, making
				enhancements, and expect to enable this feature as the default in 0.16.0. Users will still be able to
				configure static concurrency controls as they do now.
				"""
		},
		{
			title: "End to end acknowledgements"
			description: """
				We've heard from a number of users that they'd like improved delivery guarantees for events flowing
				through Vector. We are working on a feature to allow, for components that are able to support it, to
				only acknowledging data flowing into source components after that data has been sent by any associated
				sinks. For example, this would avoid acknowledging messages in Kafka until the data in those messages
				has been sent via all associated sinks.

				This release includes support in additional  source and sink components that support acknowledgements,
				but it has not yet been fully documented and tested. We expect to officially release this with 0.16.0.
				"""
		},
		{
			title:       "Kubernetes aggregator role"
			description: """
				We are hard at work at expanding the ability to run Vector as an [aggregator in
				Kubernetes](\(urls.vector_aggregator_role)). This will allow you to build end-to-end observability
				pipelines in Kubernetes with Vector. Distributing processing on the edge, centralizing it with an
				aggregator, or both. If you are interested in beta testing, please [join our chat](\(urls.vector_chat))
				and let us know.

				We do expect this to be released with 0.16.0.
				"""
		},
	]
}
