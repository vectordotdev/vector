package metadata

base: components: sink: {
	configuration: {
		inputs: {
			description: """
				A list of upstream [source](\(urls.vector_sources)) or [transform](\(urls.vector_transforms))
				IDs. Wildcards (`*`) are supported.

				See [configuration](\(urls.vector_configuration)) for more info.
				"""
			required:    true
			sort:        -1
			type: array: items: type: string: {
				examples: [
					"my-source-or-transform-id",
					"prefix-*",
				]
			}
		}

		buffer: {
			common:      false
			description: """
				Configures the sink specific buffer behavior.

				More information about the individual buffer types, and buffer behavior, can be found in the [Buffering Model](\(urls.vector_buffering_model)) section.
				"""
			required:    false
			type: object: {
				examples: []
				options: {
					max_events: {
						common:        true
						description:   "The maximum number of [events](\(urls.vector_data_model)) allowed in the buffer."
						required:      false
						relevant_when: "type = \"memory\""
						type: uint: {
							default: 500
							unit:    "events"
						}
					}
					max_size: {
						description: """
							The maximum size of the buffer on the disk. Must be at least ~256 megabytes (268435488 bytes).
							"""
						required:      true
						relevant_when: "type = \"disk\""
						type: uint: {
							examples: [268435488]
							unit: "bytes"
						}
					}
					type: {
						common:      true
						description: "The type of buffer to use."
						required:    false
						type: string: {
							default: "memory"
							enum: {
								memory: """
									Events are buffered in memory.

									This is more performant, but less durable. Data will be lost if Vector is restarted forcefully or crashes.
									"""
								disk: """
									Events are buffered on disk.

									This is less performant, but more durable. Data that has been synchronized to disk will not be lost if Vector is restarted forcefully or crashes.

									Data is synchronized to disk every 500ms.
									"""
							}
						}
					}
					when_full: {
						common:      false
						description: "The behavior when the buffer becomes full."
						required:    false
						type: string: {
							default: "block"
							enum: {
								block: """
									Waits for capacity in the buffer.

									This will cause backpressure to propagate to upstream components, which can cause data to pile up on the edge.
									"""
								drop_newest: """
									Drops the event without waiting for capacity in the buffer.

									The data is lost. This should only be used when performance is the highest priority.
									"""
							}
						}
					}
				}
			}
		}

		healthcheck: {
			common:      true
			description: "Health check options for the sink."
			required:    false
			type: object: {
				examples: []
				options: {
					enabled: {
						common:      true
						description: "Enables/disables the healthcheck upon Vector boot."
						required:    false
						type: bool: default: true
					}
				}
			}
		}

		proxy: base.configuration._proxy
	}
}
