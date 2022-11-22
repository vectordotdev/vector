package metadata

components: sinks: warp10: {
	title: "Warp10"

	classes: {
		commonly_used: false
		service_providers: ["Warp10"]
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       true
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: enabled:    false
			request: enabled:     false
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			proxy: enabled: true
			to: {
				service: {
					name:     "Warp10"
					thing:    "a \(name) server"
					url:      urls.warp10
					versions: null
				}

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

	configuration: {
		token: {
			description: "The Warp10 write token"
			required:    true
			type: string: {}
		}
		uri: {
			description: """
				The full URI of the Warp10 __update__ endpoint.
				"""
			required: true
			type: string: {
				examples: ["https://127.0.0.1:8080/api/v0/update"]
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			summary:      true
			set:          true
		}
		traces: false
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		http_bad_requests_total:          components.sources.internal_metrics.output.metrics.http_bad_requests_total
		processed_bytes_total:            components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:           components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
