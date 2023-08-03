package metadata

base: components: sources: internal_metrics: configuration: {
	namespace: {
		description: "Overrides the default namespace for the metrics emitted by the source."
		required:    false
		type: string: default: "vector"
	}
	scrape_interval_secs: {
		description: "The interval between metric gathering, in seconds."
		required:    false
		type: float: {
			default: 1.0
			unit:    "seconds"
		}
	}
	tags: {
		description: "Tag configuration for the `internal_metrics` source."
		required:    false
		type: object: options: {
			host_key: {
				description: """
					Overrides the name of the tag used to add the peer host to each metric.

					The value is the peer host's address, including the port. For example, `1.2.3.4:9000`.

					By default, the [global `log_schema.host_key` option][global_host_key] is used.

					Set to `""` to suppress this key.

					[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
					"""
				required: false
				type: string: default: "host"
			}
			pid_key: {
				description: """
					Sets the name of the tag to use to add the current process ID to each metric.

					By default, this is not set and the tag is not automatically added.
					"""
				required: false
				type: string: examples: ["pid"]
			}
		}
	}
}
