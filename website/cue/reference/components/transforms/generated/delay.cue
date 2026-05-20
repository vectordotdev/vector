package metadata

generated: components: transforms: delay: configuration: {
	delay_milliseconds: {
		description: "Time to delay each event, in milliseconds."
		required:    true
		type: uint: unit: "milliseconds"
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
					the acceptance/consumption of events. This may cause the system to degenerate if this
					component blocks for too long.
					"""
				drop_newest: """
					Drops the event instead of waiting for free space in the queue.

					The event will be intentionally dropped. This mode is typically used when performance is the
					highest priority, and it is preferable to temporarily lose events rather than cause a
					slowdown in the acceptance/consumption of events.
					"""
				forward: "Forward the event without any delay to next component."
			}
		}
	}
	queue_capacity: {
		description: "Optional limit for number of items in the delay queue."
		required:    false
		type: uint: {}
	}
}
