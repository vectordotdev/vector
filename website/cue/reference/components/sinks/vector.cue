package metadata

components: sinks: vector: {
	_port: 6000

	title: "Vector"

	description: """
		Sends data to another downstream Vector instance via the Vector source.
		"""

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "stable"
		egress_method: "batch"
		service_providers: []
		stateful: false
	}
	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: enabled:    false
			request: {
				enabled: true
				headers: false
			}

			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      false // sink allows both scheme or `enabled` to be used
			}
			to: {
				service: services.vector

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
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
		traces: true
	}

	configuration: base.components.sinks.vector.configuration

	how_it_works: components.sinks.vector.how_it_works

	telemetry: metrics: {
		protobuf_decode_errors_total: components.sources.internal_metrics.output.metrics.protobuf_decode_errors_total
	}
}
