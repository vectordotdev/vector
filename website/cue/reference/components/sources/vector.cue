package metadata

components: sources: vector: {
	_port_v1: 9000
	_port_v2: 6000

	title: "Vector"

	description: """
		Receives data from another upstream Vector instance using the Vector sink.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
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
					port:      _port_v2
					protocols: ["http"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: enabled: false
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
		acknowledgements: configuration._acknowledgements
		address: {
			description: """
				The HTTP address to listen for connections on. It _must_ include a port.
				"""
			groups: ["v2"]
			required: true
			warnings: []
			type: string: {
				examples: ["0.0.0.0:\(_port_v2)"]
				syntax: "literal"
			}
		}
		address: {
			description: """
				The TCP address to listen for connections on, or `systemd#N` to use the Nth socket passed by systemd
				socket activation. If an address is used it _must_ include a port.
				"""
			groups: ["v1"]
			required: true
			warnings: []
			type: string: {
				examples: ["0.0.0.0:\(_port_v1)", "systemd", "systemd#1"]
				syntax: "literal"
			}
		}
		shutdown_timeout_secs: {
			common:      false
			description: "The timeout before a connection is forcefully closed during shutdown."
			groups: ["v1", "v2"]
			required: false
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

	telemetry: metrics: {
		events_in_total:              components.sources.internal_metrics.output.metrics.events_in_total
		protobuf_decode_errors_total: components.sources.internal_metrics.output.metrics.protobuf_decode_errors_total
	}
}
