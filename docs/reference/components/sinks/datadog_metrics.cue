package metadata

components: sinks: datadog_metrics: {
	title: "Datadog Metrics"

	description: sinks._datadog.description
	classes:     sinks._datadog.classes

	features: {
		buffer: enabled:      false
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    null
				max_events:   20
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: enabled:    false
			request: {
				enabled:                    true
				in_flight_limit:            5
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               60
			}
			tls: enabled: false
			to: {
				name:     "Datadog metrics"
				thing:    "a \(name) account"
				url:      urls.datadog_metrics
				versions: null

				interface: {
					socket: {
						api: {
							title: "Datadog metrics API"
							url:   urls.datadog_metrics_endpoints
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
		namespace: {
			common:      true
			description: "A prefix that will be added to all metric names."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["service"]
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    false
			set:          false
			summary:      false
		}
	}
}
