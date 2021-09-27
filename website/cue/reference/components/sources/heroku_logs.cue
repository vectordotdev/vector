package metadata

components: sources: heroku_logs: {
	_port: 80

	title: "Heroku Logplex"

	description: """
		Receives log data from Heroku log drains via Heroku's logplex system.
		"""

	alias: "logplex"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				service: services.heroku

				interface: socket: {
					api: {
						title: "Syslog 6587"
						url:   urls.syslog_6587
					}
					direction: "incoming"
					port:      _port
					protocols: ["http"]
					ssl: "optional"
				}
			}

			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		acknowledgements: configuration._acknowledgements
		address:          sources.http.configuration.address
		auth:             sources.http.configuration.auth
		query_parameters: sources.http.configuration.query_parameters
	}

	output: logs: line: {
		description: "An individual event from a batch of events received through an HTTP POST request."
		fields: {
			app_name: {
				description: "The app name field extracted from log message."
				required:    true
				type: string: {
					examples: ["erlang"]
					syntax: "literal"
				}
			}
			host: fields._local_host
			message: {
				description: "The message field, containing the plain text message."
				required:    true
				type: string: {
					examples: ["Hi from erlang"]
					syntax: "literal"
				}
			}
			proc_id: {
				description: "The procid field extracted from log message."
				required:    true
				type: string: {
					examples: ["console"]
					syntax: "literal"
				}
			}
			timestamp: fields._current_timestamp
		}
	}

	telemetry: metrics: {
		events_in_total:                 components.sources.internal_metrics.output.metrics.events_in_total
		processed_bytes_total:           components.sources.internal_metrics.output.metrics.processed_bytes_total
		component_received_events_total: components.sources.internal_metrics.output.metrics.component_received_events_total
		request_read_errors_total:       components.sources.internal_metrics.output.metrics.request_read_errors_total
		requests_received_total:         components.sources.internal_metrics.output.metrics.requests_received_total
	}
}
