package metadata

base: components: sinks: configuration: {
	buffer: {
		description: """
			Buffer configuration.

			Buffers are compromised of stages(*) that form a buffer _topology_, with input items being
			subject to configurable behavior when each stage reaches configured limits.  Buffers are
			configured for sinks, where backpressure from the sink can be handled by the buffer.  This
			allows absorbing temporary load, or potentially adding write-ahead-log behavior to a sink to
			increase the durability of a given Vector pipeline.

			While we use the term "buffer topology" here, a buffer topology is referred to by the more
			common "buffer" or "buffers" shorthand.  This is related to buffers originally being a single
			component, where you could only choose which buffer type to use.  As we expand buffer
			functionality to allow chaining buffers together, you'll see "buffer topology" used in internal
			documentation to correctly reflect the internal structure.
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
					The maximum size of the buffer on disk.

					Must be at least ~256 megabytes (268435488 bytes).
					"""
				relevant_when: "type = \"disk_v1\" or type = \"disk\""
				required:      true
				type: uint: {}
			}
			type: {
				required: false
				type: string: {
					default: "memory"
					enum: {
						disk: """
														Events are buffered on disk. (version 2)

														This is less performant, but more durable. Data that has been synchronized to disk will not
														be lost if Vector is restarted forcefully or crashes.

														Data is synchronized to disk every 500ms.
														"""
						disk_v1: """
														Events are buffered on disk. (version 1)

														This is less performant, but more durable. Data that has been synchronized to disk will not
														be lost if Vector is restarted forcefully or crashes.
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
						overflow: """
														Overflows to the next stage in the buffer topology.

														If the current buffer stage is full, attempt to send this event to the next buffer stage.
														That stage may also be configured overflow, and so on, but ultimately the last stage in a
														buffer topology must use one of the other handling behaviors. This means that next stage may
														potentially be able to buffer the event, but it may also block or drop the event.

														This mode can only be used when two or more buffer stages are configured.
														"""
					}
				}
			}
		}
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
			uri: {
				description: """
					The full URI to make HTTP healthcheck requests to.

					This must be a valid URI, which requires at least the scheme and host. All other
					components -- port, path, etc -- are allowed as well.
					"""
				required: false
				type: string: syntax: "literal"
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
		type: array: items: type: string: {
			examples: ["my-source-or-transform-id", "prefix-*"]
			syntax: "literal"
		}
	}
	proxy: {
		description: """
			Proxy configuration.

			Vector can be configured to proxy traffic through an HTTP(S) proxy when making external requests. Similar to common
			proxy configuration convention, users can set different proxies to use based on the type of traffic being proxied,
			as well as set specific hosts that should not be proxied.
			"""
		required: false
		type: object: options: {
			enabled: {
				description: "Enables proxying support."
				required:    false
				type: bool: default: true
			}
			http: {
				description: """
					Proxy endpoint to use when proxying HTTP traffic.

					Must be a valid URI string.
					"""
				required: false
				type: string: {
					examples: ["http://foo.bar:3128"]
					syntax: "literal"
				}
			}
			https: {
				description: """
					Proxy endpoint to use when proxying HTTPS traffic.

					Must be a valid URI string.
					"""
				required: false
				type: string: {
					examples: ["http://foo.bar:3128"]
					syntax: "literal"
				}
			}
			no_proxy: {
				description: """
					A list of hosts to avoid proxying.

					Multiple patterns are allowed:

					| Pattern             | Example match                                                               |
					| ------------------- | --------------------------------------------------------------------------- |
					| Domain names        | `example.com` matches requests to `example.com`                     |
					| Wildcard domains    | `.example.com` matches requests to `example.com` and its subdomains |
					| IP addresses        | `127.0.0.1` matches requests to `127.0.0.1`                         |
					| [CIDR][cidr] blocks | `192.168.0.0/16` matches requests to any IP addresses in this range     |
					| Splat               | `*` matches all hosts                                                   |

					[cidr]: https://en.wikipedia.org/wiki/Classless_Inter-Domain_Routing
					"""
				required: false
				type: array: {
					default: []
					items: type: string: syntax: "literal"
				}
			}
		}
	}
}
