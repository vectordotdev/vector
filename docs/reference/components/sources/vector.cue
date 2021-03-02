package metadata

components: sources: vector: {
	_port: 9000

	title: "Vector"

	description: """
		Receives data from another upstream Vector instance	using the Vector sink.
		"""

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				service: services.vector

				interface: socket: {
					direction: "incoming"
					port:      _port
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: enabled: true
			keepalive: enabled:            true
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		address: {
			description: "The TCP address to listen for connections on, or `systemd#N to use the Nth socket passed by systemd socket activation. If an address is used it _must_ include a port."
			required:    true
			warnings: []
			type: string: {
				examples: ["0.0.0.0:\(_port)", "systemd", "systemd#1"]
				syntax: "literal"
			}
		}
		shutdown_timeout_secs: {
			common:      false
			description: "The timeout before a connection is forcefully closed during shutdown."
			required:    false
			warnings: []
			type: uint: {
				default: 30
				unit:    "seconds"
			}
		}
	}

	output: {
		logs: event: {
			description: "A Vector event"
			fields: {
				"*": {
					description: "Vector transparently forwards data from another upstream Vector instance. The `vector` source will not modify or add fields."
					required:    true
					type: "*": {}
				}
			}
		}
		metrics: {
			counter:      output._passthrough_counter
			distribution: output._passthrough_distribution
			gauge:        output._passthrough_gauge
			histogram:    output._passthrough_histogram
			set:          output._passthrough_set
		}
	}

	how_it_works: {
		encoding: {
			title: "Encoding"
			body:  """
				Data is encoded via Vector's [event protobuf](\(urls.event_proto))
				before it is sent over the wire.
				"""
		}
		communication_protocol: {
			title: "Communication Protocol"
			body: """
				Upstream Vector instances forward data to downstream Vector
				instances via the TCP protocol.
				"""
		}
		message_acknowledgement: {
			title: "Message Acknowledgement"
			body: """
				Currently, Vector does not perform any application level message
				acknowledgement. While rare, this means the individual message
				could be lost.
				"""
		}

	}

	telemetry: metrics: {
		protobuf_decode_errors_total: components.sources.internal_metrics.output.metrics.protobuf_decode_errors_total
	}
}
