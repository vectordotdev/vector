package metadata

components: sinks: vector: {
	title: "Vector"

	description: """
		Sends data to another downstream Vector instance via the Vector source.
		"""

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			send_buffer_bytes: enabled: true
			keepalive: enabled:         true
			request: enabled:           false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.vector

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

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			summary:      true
			set:          true
		}
	}

	configuration: {
		address: {
			description: "The downstream Vector address to connect to. The address _must_ include a port."
			required:    true
			warnings: []
			type: string: {
				examples: ["92.12.333.224:5000"]
				syntax: "literal"
			}
		}
	}

	how_it_works: components.sources.vector.how_it_works

	telemetry: metrics: {
		processed_bytes_total:        components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:       components.sources.internal_metrics.output.metrics.processed_events_total
		protobuf_decode_errors_total: components.sources.internal_metrics.output.metrics.protobuf_decode_errors_total
	}
}
