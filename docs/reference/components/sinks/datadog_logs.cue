package metadata

components: sinks: datadog_logs: {
	title:             "Datadog Logs"
	short_description: "Streams log events to [Datadog's][urls.datadog] logs via the [TCP endpoint][urls.datadog_logs_endpoints]."
	long_description:  "[Datadog][urls.datadog] is a monitoring service for cloud-scale applications, providing monitoring of servers, databases, tools, and services, through a SaaS-based data analytics platform."

	classes: {
		commonly_used: false
		egress_method: "stream"
		function:      "transmit"
		service_providers: ["Datadog"]
	}

	features: {
		buffer: enabled:      true
		compression: enabled: false
		encoding: codec: {
			enabled: true
			default: null
			enum: ["json", "text"]
		}
		healthcheck: enabled: true
		request: enabled:     false
		tls: {
			enabled:                true
			can_enable:             true
			can_verify_certificate: true
			can_verify_hostname:    true
			enabled_default:        true
		}
	}

	statuses: {
		delivery:    "at_least_once"
		development: "beta"
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
			description: "The endpoint to stream logs to."
			required:    false
			type: string: {
				default: "intake.logs.datadoghq.com:10516"
				examples: ["127.0.0.1:8080", "example.com:12345"]
			}
		}
	}

	input: {
		logs:    true
		metrics: false
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
