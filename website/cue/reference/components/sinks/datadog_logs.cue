package metadata

components: sinks: datadog_logs: {
	title: "Datadog Logs"

	classes: sinks._datadog.classes

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    4_250_000
				max_events:   1000
				timeout_secs: 5.0
			}
			compression: {
				enabled: true
				default: "gzip"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.datadog_logs

				interface: {
					socket: {
						api: {
							title: "Datadog logs API"
							url:   urls.datadog_logs_endpoints
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

	configuration: base.components.sinks.datadog_logs.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		attributes: {
			title: "Attributes"
			body: """
				Datadog's logs API has special handling for the following fields: `ddsource`, `ddtags`, `hostname`, `message`, and `service`.
				If your event contains any of these fields they will be used as described by the [API reference](https://docs.datadoghq.com/api/latest/logs/#send-logs).
				"""
		}
	}
}
