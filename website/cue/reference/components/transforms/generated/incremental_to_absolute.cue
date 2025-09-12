package metadata

generated: components: transforms: incremental_to_absolute: configuration: cache: {
	description: """
		Configuration for the internal metrics cache used to normalize a stream of incremental
		metrics into absolute metrics.

		By default, incremental metrics are evicted after 5 minutes of not being updated. The next
		incremental value will be reset.
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
