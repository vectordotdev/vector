package metadata

components: sinks: datadog_logs: {
	title: "Datadog Logs"

	description: sinks._datadog.description
	classes:     sinks._datadog.classes

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					default: null
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
			}
			to: {
				name:     "Datadog logs"
				thing:    "a \(name) account"
				url:      urls.datadog_logs
				versions: null

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

	support: support: sinks._datadog.support

	configuration: {
		api_key:  sinks._datadog.configuration.api_key
		endpoint: sinks._datadog.configuration.endpoint
	}

	input: {
		logs:    true
		metrics: null
	}
}
