package metadata

_schemaDefinitions: "derived::vector_buffers::config::BufferType::6c60e9549d874ce4350c077a": object: options: {
	max_events: {
		description:   "The maximum number of events allowed in the buffer."
		relevant_when: "type = \"memory\""
		required:      false
		type: uint: default: 500
	}
	max_size: {
		description: """
			The maximum allowed amount of allocated memory the buffer can hold.

			If `type = "disk"` then must be at least ~256 megabytes (268435488 bytes).
			"""
		required: true
		type: uint: unit: "bytes"
	}
	type: {
		description: "The type of buffer to use."
		required:    false
		type: string: {
			default: "memory"
			enum: {
				disk: """
					Events are buffered on disk.

					This is less performant, but more durable. Data that has been synchronized to disk will not
					be lost if Vector is restarted forcefully or crashes.

					Data is synchronized to disk every 500ms.
					"""
				memory: """
					Events are buffered in memory.

					This is more performant, but less durable. Data will be lost if Vector is restarted
					forcefully or crashes.
					"""
			}
		}
	}
	when_full: {
		description: "Event handling behavior when a buffer is full."
		required:    false
		type: string: {
			default: "block"
			enum: {
				block: """
					Wait for free space in the buffer.

					This applies backpressure up the topology, signalling that sources should slow down
					the acceptance/consumption of events. This means that while no data is lost, data will pile
					up at the edge.
					"""
				drop_newest: """
					Drops the event instead of waiting for free space in buffer.

					The event will be intentionally dropped. This mode is typically used when performance is the
					highest priority, and it is preferable to temporarily lose events rather than cause a
					slowdown in the acceptance/consumption of events.
					"""
			}
		}
	}
}
