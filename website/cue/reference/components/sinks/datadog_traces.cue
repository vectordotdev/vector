package metadata

components: sinks: datadog_traces: {
	title: "Datadog Traces"

	classes: sinks._datadog.classes

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    2_300_000
				max_events:   1_000
				timeout_secs: 5.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: enabled: false
			proxy: enabled:    true
			request: {
				enabled:                    true
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    300
				timeout_secs:               60
				headers:                    false
			}
			tls: {
				enabled:                true
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
							url:   urls.datadog_traces
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: ["APM stats are not yet supported, so they will dropped when using Vector to send traces received from the Datadog Trace agent to Datadog."]
		notices: []
	}

	configuration: {
		default_api_key: sinks._datadog.configuration.api_key
		endpoint:        sinks._datadog.configuration.endpoint
		site:            sinks._datadog.configuration.site
	}

	input: {
		logs:    false
		metrics: null
		traces:  true
	}

	telemetry: metrics: {
		component_discarded_events_total: components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:           components.sources.internal_metrics.output.metrics.component_errors_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
	}
}
