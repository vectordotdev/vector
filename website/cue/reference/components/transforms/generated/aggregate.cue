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
	clock: {
        description: """
            Aggregation clock source.

            Determines whether buckets are aligned by Vector's processing time
            or by the event's own timestamp.
            """
        required: false
        type: string: {
            default: "Processing"
            enum: {
                Processing: "Buckets are driven by Vector's wall clock (processing time)."
                Event:      "Buckets are driven by each event's own timestamp (event time)."
            }
        }
    }
    allowed_lateness_ms: {
        description: """
            Allowed lateness for event-time processing.

            Specifies how long to wait for late or out-of-order samples before closing an event-time bucket.
            """
        required: false
        type: uint: default: 120000
    }
    emit_ts: {
        description: """
            Output timestamp mode.

            Controls whether the emitted metric's timestamp is set to the start
            or to the end of the bucket window.
            """
        required: false
        type: string: {
            default: "BucketStart"
            enum: {
                BucketStart: "Stamp the output at the start of the bucket window."
                BucketEnd:   "Stamp the output at the end of the bucket window."
            }
        }
    }
}
