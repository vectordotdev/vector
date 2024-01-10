package metadata

components: sinks: datadog_metrics: {
	title: "Datadog Metrics"

	classes: sinks._datadog.classes & {
		stateful: true
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   100_000
				timeout_secs: 2.0
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
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.datadog_metrics

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

	configuration: base.components.sinks.datadog_metrics.configuration

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
		traces: false
	}
}
