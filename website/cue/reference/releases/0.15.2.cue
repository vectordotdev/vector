package metadata

releases: "0.15.2": {
	date:     "2021-08-11"
	codename: ""

	description: """
		The Vector team is pleased to announce version 0.15.2!

		This release contains a fix for `vector validate` to source environment variables indicating configuration
		location: `VECTOR_CONFIG`, `VECTOR_CONFIG_YAML`, `VECTOR_CONFIG_JSON`, and `VECTOR_CONFIG_TOML`.

		In v0.15.0, we released a change the SystemD unit file to run `vector validate` before start-up as part of
		`ExecStartPre`. If users were using, for example, `VECTOR_CONFIG` in `/etc/default/vector` to pass the
		configuration location, this would result in Vector failing to boot.

		See the release notes for 0.15.0 for additional changes if upgrading from 0.14.X.
		"""

	commits: [
		{sha: "0824874dbd0d8c6fe06cd05213c35e375caf28e3", date: "2021-08-04 14:32:23 UTC", description: "Use env vars with validate command", pr_number: 8577, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 19, deletions_count: 4},

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
