package metadata

components: sinks: datadog_logs: {
	title:       "Datadog Logs"
	description: "[Datadog][urls.datadog] is a monitoring service for cloud-scale applications, providing monitoring of servers, databases, tools, and services, through a SaaS-based data analytics platform."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		service_providers: ["Datadog"]
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: codec: {
				enabled: true
				default: null
				enum: ["json", "text"]
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

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		api_key: {
			description: "Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication)"
			required:    true
			warnings: []
			type: string: {
				examples: ["${DATADOG_API_KEY_ENV_VAR}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		endpoint: {
			common:      false
			description: "The endpoint to send logs to."
			required:    false
			type: string: {
				default: "intake.logs.datadoghq.com:10516"
				examples: ["127.0.0.1:8080", "example.com:12345"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body: #"""
				1. Register for a free account at [datadoghq.com](https://www.datadoghq.com/)

				2. Fetch your logs api key by going to the [other](https://app.datadoghq.com/logs/onboarding/other) options
				and selecting the `fluentd` option, it should then present you an `api_key`. This api key can now be used
				with Vector!
				"""#
		}
	}
}
