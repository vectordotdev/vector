package metadata

components: sinks: datadog_events: {
	title: "Datadog Events"

	classes: sinks._datadog.classes & {
		development: "beta"
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      false
				common:       false
				timeout_secs: 0
			}
			compression: enabled: false
			encoding: enabled:    false
			proxy: enabled:       true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
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
		default_api_key: {
			description: "Default Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication), if an event has a key set in its metadata it will prevail over the one set here."
			required:    true
			warnings: []
			type: string: {
				examples: ["${DATADOG_API_KEY_ENV_VAR}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		endpoint: sinks._datadog.configuration.endpoint
		site:     sinks._datadog.configuration.site
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
	}
}
