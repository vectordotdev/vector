package metadata

generated: components: transforms: aggregate: configuration: {
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
