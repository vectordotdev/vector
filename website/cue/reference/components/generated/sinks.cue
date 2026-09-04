package metadata

generated: components: sinks: configuration: {
	buffer: {
		description: """
			Configures the buffering behavior for this sink.

			More information about the individual buffer types, and buffer behavior, can be found in the
			[Buffering Model][buffering_model] section.

			[buffering_model]: /docs/architecture/buffering-model/
			"""
		required: false
		type: object: options: {
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
	}
	graph: {
		description: """
			Extra graph configuration

			Configure output for component when generated with graph command
			"""
		required: false
		type:     _schemaDefinitions["vector::config::dot_graph::GraphConfig"]
	}
	healthcheck: {
		description: "Healthcheck configuration."
		required:    false
		type: object: options: {
			enabled: {
				description: "Whether or not to check the health of the sink when Vector starts up."
				required:    false
				type: bool: default: true
			}
			timeout: {
				description: "Timeout duration for healthcheck in seconds."
				required:    false
				type: float: {
					default: 10.0
					unit:    "seconds"
				}
			}
			uri: {
				description: """
					The full URI to make HTTP healthcheck requests to.

					This must be a valid URI, which requires at least the scheme and host. All other
					components -- port, path, etc -- are allowed as well.
					"""
				required: false
				type: string: {}
			}
		}
	}
	inputs: {
		description: """
			A list of upstream [source][sources] or [transform][transforms] IDs.

			Wildcards (`*`) are supported.

			See [configuration][configuration] for more info.

			[sources]: https://vector.dev/docs/reference/configuration/sources/
			[transforms]: https://vector.dev/docs/reference/configuration/transforms/
			[configuration]: https://vector.dev/docs/reference/configuration/
			"""
		required: true
		type: array: items: type: string: examples: ["my-source-or-transform-id", "prefix-*"]
	}
	proxy: {
		description: """
			Proxy configuration.

			Configure to proxy traffic through an HTTP(S) proxy when making external requests.

			Similar to common proxy configuration convention, you can set different proxies
			to use based on the type of traffic being proxied. You can also set specific hosts that
			should not be proxied.
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::proxy::ProxyConfig"]
	}
}
