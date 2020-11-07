package metadata

components: sinks: vector: {
	title: "Vector"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
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
			request: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				name:     "Vector source"
				thing:    "a \(name)"
				url:      urls.vector_source
				versions: null

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
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
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
			}
		}
	}

	how_it_works: components.sources.vector.how_it_works

	telemetry: metrics: {
		vector_protobuf_decode_errors_total: _vector_protobuf_decode_errors_total
	}
}
