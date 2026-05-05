package metadata

generated: components: transforms: aggregate: configuration: {
	allowed_lateness_ms: {
		description: """
			Grace period for late-arriving events when using event-time aggregation.

			Each bucket is held open for this many milliseconds past the end of its window so late
			events still land in the correct bucket. Once a bucket is emitted it is closed
			permanently; any later events whose timestamp falls inside it are dropped and counted
			via `component_discarded_events_total`.

			Set to 0 for strict ordering (no late events allowed). Only applies when `time_source`
			is set to `event_time`.
			"""
		required: false
		type: uint: {
			default: 0
			examples: [0, 5000, 30000]
		}
	}
	interval_ms: {
		description: """
			The interval between flushes, in milliseconds.

			During this time frame, metrics (beta) with the same series data (name, namespace, tags, and so on) are aggregated.
			"""
		required: false
		type: uint: default: 10000
	}
	max_future_ms: {
		description: """
			Maximum allowed time drift for future events in event-time mode.

			Acts as a clock-skew guard: events whose timestamp is further in the future than this
			many milliseconds (relative to the current system time) are dropped and counted via
			`component_discarded_events_total`. Defaults to 10 seconds.

			Set to 0 to allow events at any future time. Only applies when `time_source` is set
			to `event_time`.
			"""
		required: false
		type: uint: {
			default: 10000
			examples: [0, 60000, 300000]
		}
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
				Diff:   "Returns difference between latest value for absolute, ignores incremental"
				Latest: "Returns the latest value for absolute metrics, ignores incremental"
				Max:    "Max value of absolute metric, ignores incremental"
				Mean:   "Mean value of absolute metric, ignores incremental"
				Min:    "Min value of absolute metric, ignores incremental"
				Stdev:  "Stdev value of absolute metric, ignores incremental"
				Sum:    "Sums incremental metrics, ignores absolute"
			}
		}
	}
	time_source: {
		description: """
			Time source to use for aggregation windows.

			When set to `event_time`, events are grouped into buckets based on their timestamps rather than
			when they are processed. Events arriving out of order (after their bucket has been flushed) are rejected.
			"""
		required: false
		type: string: {
			default: "system_time"
			enum: {
				event_time: """
					Use event timestamps for aggregation windows.

					Events are grouped into buckets based on their timestamps. Events arriving out of order
					(after their bucket has been flushed) are rejected.
					"""
				system_time: """
					Use system clock time for aggregation windows (default).

					Events are aggregated based on when they are processed, not their timestamps.
					"""
			}
		}
	}
	use_system_time_for_missing_timestamps: {
		description: """
			How to handle events with missing timestamps in event-time mode.

			When `true`, events without a timestamp use the current system time as a fallback.
			When `false`, such events are dropped and counted via `component_discarded_events_total`.

			Only applies when `time_source` is set to `event_time`.
			"""
		required: false
		type: bool: default: false
	}
}
