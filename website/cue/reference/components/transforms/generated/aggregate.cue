package metadata

generated: components: transforms: aggregate: configuration: {
	allowed_lateness_ms: {
		description: """
			Grace period for late-arriving events when using event-time aggregation.

			Events with timestamps older than the watermark but within this grace period will still be accepted.
			Set to 0 for strict ordering (no late events allowed).
			Only applies when `time_source` is set to `EventTime`.
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

			Events with timestamps further in the future than this value will be dropped.
			Set to 0 to allow events at any future time.
			Only applies when `time_source` is set to `EventTime`.
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
			default: "SystemTime"
			enum: {
				EventTime: """
					Use event timestamps for aggregation windows.

					Events are grouped into buckets based on their timestamps. Events arriving out of order
					(after their bucket has been flushed) are rejected.
					"""
				SystemTime: """
					Use system clock time for aggregation windows (default).

					Events are aggregated based on when they are processed, not their timestamps.
					"""
			}
		}
	}
	use_system_time_for_missing_timestamps: {
		description: """
			How to handle events with missing timestamps in event-time mode.

			When `true`, events without timestamps will use the current system time as a fallback.
			When `false`, events without timestamps will be dropped.
			Only applies when `time_source` is set to `EventTime`.
			"""
		required: false
		type: bool: default: false
	}
}
