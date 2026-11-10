package metadata

generated: components: sources: internal_metrics: configuration: {
	emit_incremental: {
		description: """
			Emit counters and histograms as incremental metrics holding the change since the previous
			scrape, instead of absolute totals since Vector started. Gauges stay absolute.

			Sinks that convert absolute metrics to incremental, such as `datadog_metrics`, drop the
			first value they see for each series, so counters and histograms observed within a single
			scrape interval are never sent. Enabling this option avoids that loss.

			This is off by default because it changes what other sinks receive:

			- Counts start from when the source is built, not from when Vector started.
			- Sinks that write metrics as-is, such as `splunk_hec_metrics` or codec-based sinks, report
			  per-scrape deltas instead of running totals. So do `influxdb_metrics` and
			  `greptimedb_metrics` for histograms.
			- The `opentelemetry` sink and the `otlp` codec switch to delta temporality.
			- Sinks that require an interval on incremental counters, such as `new_relic`, drop them.

			When `expire_metrics_secs` is set, using several `internal_metrics` sources with this option
			enabled is not recommended: a series expired and recreated between two scrapes can be
			under-counted.
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
