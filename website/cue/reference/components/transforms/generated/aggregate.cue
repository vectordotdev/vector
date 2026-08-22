package metadata

generated: components: transforms: aggregate: configuration: {
	event_time: {
		description: """
			Event-time aggregation settings.

			When present, metrics are grouped into buckets based on their timestamps rather than when
			they are processed. Omit this block to keep the default system-time behavior.
			"""
		required: false
		type: object: options: {
			allowed_lateness_ms: {
				description: """
					Grace period for late-arriving events, in milliseconds.

					Each bucket accepts events until the system clock reaches
					`bucket_end + allowed_lateness_ms`, where `bucket_end` is the exclusive end of the
					event-time window. That cutoff is enforced when events are recorded, not only when a
					periodic flush runs. Once a bucket is emitted it is closed permanently; any later
					events whose timestamp falls inside it are dropped and counted via
					`component_discarded_events_total`.

					Set to 0 for strict ordering (no late events allowed).
					"""
				required: false
				type: uint: {
					default: 0
					examples: [0, 5000, 30000]
				}
			}
			max_future_ms: {
				description: """
					Maximum allowed time drift for future events, in milliseconds.

					Acts as a clock-skew guard: events whose timestamp is further in the future than this
					many milliseconds (relative to the current system time) are dropped and counted via
					`component_discarded_events_total`. Defaults to 10 seconds.

					Set to 0 to allow events at any future time.
					"""
				required: false
				type: uint: {
					default: 10000
					examples: [0, 60000, 300000]
				}
			}
			missing_timestamp: {
				description: """
					How to handle events with missing timestamps.

					Metrics that pass through unchanged for the configured mode do not require a timestamp.
					For metrics that would be bucketed:
					- `drop` (default) discards the event and increments `component_discarded_events_total`
					- `use_system_time` synthesizes a timestamp from the current system clock
					"""
				required: false
				type: string: {
					default: "drop"
					enum: {
						drop:            "Drop the event and count it via `component_discarded_events_total`."
						use_system_time: "Use the current system time as the event timestamp."
					}
				}
			}
		}
	}
	interval_ms: {
		description: """
			The interval between flushes, in milliseconds.

			Must be greater than zero. During this time frame, metrics (beta) with the same series data
			(name, namespace, tags, and so on) are aggregated.
			"""
		required: false
		type: uint: default: 10000
	}
	mode: {
		description: """
			Function to use for aggregation.

			Some of the functions may only function on incremental and some only on absolute metrics.
			"""
		required: false
		type: string: {
			default: "Auto"
			enum: {
				Auto:   "Default mode. Sums incremental metrics and uses the latest value for absolute metrics."
				Count:  "Counts metrics for incremental and absolute metrics"
				Diff:   "Returns difference between latest value for absolute; incremental metrics pass through unchanged."
				Latest: "Returns the latest value for absolute metrics; incremental metrics pass through unchanged."
				Max:    "Max value of absolute metric; incremental metrics pass through unchanged."
				Mean:   "Mean value of absolute metric; incremental metrics pass through unchanged."
				Min:    "Min value of absolute metric; incremental metrics pass through unchanged."
				Stdev:  "Stdev value of absolute metric; incremental metrics pass through unchanged."
				Sum:    "Sums incremental metrics; absolute metrics pass through unchanged."
			}
		}
	}
}
