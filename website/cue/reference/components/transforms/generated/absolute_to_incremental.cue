package metadata

generated: components: transforms: absolute_to_incremental: configuration: cache: {
	description: """
		Configuration for the internal metrics cache used to normalize a stream of absolute
		metrics into incremental metrics.

		By default, absolute metrics are evicted after 5 minutes of not being updated. The next
		absolute value will be reset.
		"""
	required: false
	type: object: options: {
		max_bytes: {
			description: "The maximum size in bytes of the events in the metrics normalizer cache, excluding cache overhead."
			required:    false
			type: uint: unit: "bytes"
		}
		max_events: {
			description: "The maximum number of events of the metrics normalizer cache"
			required:    false
			type: uint: unit: "events"
		}
		time_to_live: {
			description: "The maximum age of a metric not being updated before it is evicted from the metrics normalizer cache."
			required:    false
			type: uint: {
				default: 300
				unit:    "seconds"
			}
		}
	}
}
