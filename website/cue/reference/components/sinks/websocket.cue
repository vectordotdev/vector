package metadata

components: sinks: websocket: {
	title: "WebSocket"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		buffer: enabled:      false
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.websocket
				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp"]
						ssl: "optional"
					}
				}
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

	configuration: {
		uri: {
			description: """
				The WebSocket URI to connect to. This should include the protocol and host,
				but can also include the port, path, and any other valid part of a URI.
				"""
			required: true
			warnings: []
			type: string: {
				examples: ["ws://127.0.0.1:9000/endpoint"]
				syntax: "literal"
			}
		}
		ping_interval: {
			common:      true
			description: "Send WebSocket pings each this number of seconds."
			required:    false
			warnings: []
			type: uint: {
				default: null
				unit:    "seconds"
			}
		}
		ping_timeout: {
			common:        true
			description:   "Try to reconnect to the WebSocket server if pong not received for this number of seconds."
			relevant_when: "ping_interval is set"
			required:      false
			warnings: ["This parameter is not taken into account if ping_interval is not set"]
			type: uint: {
				default: null
				unit:    "seconds"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		open_connections:             components.sources.internal_metrics.output.metrics.open_connections
		connection_established_total: components.sources.internal_metrics.output.metrics.connection_established_total
		connection_failed_total:      components.sources.internal_metrics.output.metrics.connection_failed_total
		connection_shutdown_total:    components.sources.internal_metrics.output.metrics.connection_shutdown_total
		connection_errors_total:      components.sources.internal_metrics.output.metrics.connection_errors_total
		events_in_total:              components.sources.internal_metrics.output.metrics.events_in_total
		events_out_total:             components.sources.internal_metrics.output.metrics.events_out_total
		processed_bytes_total:        components.sources.internal_metrics.output.metrics.processed_bytes_total
	}
}
