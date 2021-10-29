package metadata

components: sources: vector: {
	_port: 9000

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
					port:      _port
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
			required: true
			type: string: {
				examples: ["0.0.0.0:\(_port)"]
			}
		}
		shutdown_timeout_secs: {
			common:      false
			description: "The timeout before a connection is forcefully closed during shutdown."
			required:    false
			type: uint: {
				default: 30
				unit:    "seconds"
			}
		}
		version: {
			description: "Source API version. Specifying this version ensures that Vector does not break backward compatibility."
			common:      true
			required:    false
			warnings: ["Ensure you use the same version for both the source and sink."]
			type: string: {
				enum: {
					"1": "Vector source API version 1"
					"2": "Vector source API version 2"
				}
				default: "1"
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
		events_in_total:                 components.sources.internal_metrics.output.metrics.events_in_total
		protobuf_decode_errors_total:    components.sources.internal_metrics.output.metrics.protobuf_decode_errors_total
		component_received_events_total: components.sources.internal_metrics.output.metrics.component_received_events_total
	}
}
