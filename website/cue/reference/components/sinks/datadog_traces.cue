package metadata

components: sinks: datadog_traces: {
	title: "Datadog Traces"

	classes: sinks._datadog.classes

	features: {
		buffer: enabled:      false
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    2_300_000
				max_events:   1_000
				timeout_secs: 5
			}
			compression: {
				enabled: true
				default: "gzip"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: enabled: false
			proxy: enabled:    true
			request: enabled:  true
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
			}
			to: {
				service: services.datadog_traces

				interface: {
					socket: {
						api: {
							title: "Datadog traces API"
							url:   urls.datadog_traces_endpoints
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
		default_api_key: sinks._datadog.configuration.api_key
		endpoint:        sinks._datadog.configuration.endpoint
		site:            sinks._datadog.configuration.site
	}

	input: {
		logs:    false
		metrics: null
	}

	telemetry: metrics: {
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
	}
}
