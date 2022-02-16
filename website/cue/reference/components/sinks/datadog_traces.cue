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
				max_events:   100_000
				timeout_secs: 2
			}
			compression: enabled: false
			encoding: enabled:    false
			proxy: enabled:       true
			request: {
				enabled:                    true
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               60
				headers:                    false
			}
			tls: enabled: false
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
		api_key:  sinks._datadog.configuration.api_key
		endpoint: sinks._datadog.configuration.endpoint
		region:   sinks._datadog.configuration.region
		site:     sinks._datadog.configuration.site
		default_namespace: {
			common: true
			description: """
				Used as a namespace for traces that don't have it.
				A namespace will be prefixed to a trace's name.
				"""
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["service"]
			}
		}
	}

	input: {
		logs: false
		traces: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    false
			set:          false
			summary:      false
		}
	}

	telemetry: traces: {
		component_sent_events_total:      components.sources.internal_traces.output.traces.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_traces.output.traces.component_sent_event_bytes_total
	}
}
