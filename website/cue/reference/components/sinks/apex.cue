package metadata

components: sinks: apex: {
	title: "Apex"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["apex.sh"]
		stateful: false
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
			compression: {
				enabled: false
			}
			encoding: {
				enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
			}
			to: {
				service: services.apex

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
		uri: {
			description: "The base URI of the Apex instance. Vector will append `/add_events` to this."
			required:    true
			type: string: {
				examples: ["http://localhost:3100"]
			}
		}
		project_id: {
			description: "The ID of the project to associate reported logs with."
			required:    true
			type: string: {
				examples: ["my-project"]
			}
		}
		api_token: {
			description: "The API token to use to authenticate with Apex."
			required:    true
			type: string: {
				examples: ["${API_TOKEN}"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		processed_bytes_total:            components.sources.internal_metrics.output.metrics.processed_bytes_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
		streams_total:                    components.sources.internal_metrics.output.metrics.streams_total
	}
}
