package metadata

generated: components: transforms: delay: configuration: {
	delay_per_event: {
		description: "Time to delay each event, in seconds."
		required:    true
		type: float: unit: "seconds"
	}
	delay_until_condition: {
		description: "Delay events in provided delay periods until the condition is met."
		required:    false
		type: condition: {}
	}
	overflow_strategy: {
		description: "Strategy to handle full queue capacity."
		required:    false
		type: string: {
			default: "block"
			enum: {
				block: """
					Wait for free space in the queue.

					This applies backpressure up the topology, signalling that sources should slow down
					the acceptance/consumption of events. This means that while no data is lost, data will pile
					up at the edge.
					"""
				drop_newest: """
					Drops the event instead of waiting for free space in the queue.

					The event will be intentionally dropped. This mode is typically used when performance is the
					highest priority, and it is preferable to temporarily lose events rather than cause a
					slowdown in the acceptance/consumption of events.
					"""
				pass: """
					Passes the event immediately instead of waiting for delay, to not take up the space in the
					queue.
					"""
			}
		}
	}
	queue_capacity: {
		description: "Optional limit for number of items in the delay queue."
		required:    false
		type: uint: {}
	}
}
