package metadata

base: components: transforms: aggregate: configuration: {
	interval_ms: {
		description: """
			The interval between flushes, in milliseconds.

			During this time frame, metrics (beta) with the same series data (name, namespace, tags, and so on) are aggregated.
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
}
