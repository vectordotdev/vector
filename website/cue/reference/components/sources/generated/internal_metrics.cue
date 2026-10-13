package metadata

generated: components: sources: internal_metrics: configuration: {
	emit_incremental: {
		description: """
			Emit counters and histograms as incremental metrics holding the change since the previous
			scrape, instead of absolute totals since Vector started. Gauges stay absolute.

			Sinks that convert absolute metrics to incremental, such as `datadog_metrics`, drop the
			first value they see for each series, so histograms observed within a single scrape
			interval are never sent. Enabling this option avoids that loss.

			Counts start from when the source is built. Sinks that require an interval on incremental
			counters, such as `new_relic`, drop them.
			"""
		required: false
		type: bool: default: false
	}
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
				type: string: {}
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
