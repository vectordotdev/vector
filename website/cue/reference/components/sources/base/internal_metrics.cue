package metadata

base: components: sources: internal_metrics: configuration: {
	namespace: {
		description: """
			Overrides the default namespace for the metrics emitted by the source.

			Overrides the default namespace.
			"""
		required: false
		type: string: {}
	}
	scrape_interval_secs: {
		description: "The interval between metric gathering, in seconds."
		required:    false
		type: float: default: 1.0
	}
	tags: {
		description: "Tag configuration for the `internal_metrics` source."
		required:    false
		type: object: options: {
			host_key: {
				description: """
					Sets the name of the tag to use to add the current hostname to each metric.

					By default, the [global `log_schema.host_key` option][global_host_key] is used.
					"""
				required: false
				type: string: {}
			}
			pid_key: {
				description: """
					Sets the name of the tag to use to add the current process ID to each metric.

					By default, this is not set and the tag will not be automatically added.
					"""
				required: false
				type: string: {}
			}
		}
	}
}
