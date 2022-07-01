package metadata

components: sinks: datadog_events: {
	title: "Datadog Events"

	classes: sinks._datadog.classes & {
		development: "beta"
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: enabled:       false
			compression: enabled: false
			encoding: enabled:    false
			proxy: enabled:       true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.datadog_events

				interface: {
					socket: {
						api: {
							title: "Datadog events API"
							url:   urls.datadog_events_endpoints
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: sinks._datadog.support

	configuration: {
		default_api_key: sinks._datadog.configuration.default_api_key
		endpoint:        sinks._datadog.configuration.endpoint
		site:            sinks._datadog.configuration.site
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
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
	}
}
