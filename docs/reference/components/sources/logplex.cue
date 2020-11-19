package metadata

components: sources: logplex: {
	_port: 80

	title:       "Heroku Logplex"
	description: "[Heroku’s Logplex](\(urls.logplex)) router is responsible for collating and distributing the log entries generated by Heroku apps and other components of the Heroku platform. It makes these entries available through the Logplex public API and the Heroku command-line tool."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				name:     "Heroku"
				thing:    "a \(name) app"
				url:      urls.logplex
				versions: null

				interface: socket: {
					api: {
						title: "Syslog 6587"
						url:   urls.syslog_6587
					}
					port: _port
					protocols: ["http"]
					ssl: "optional"
				}

				setup: [
					"""
						Create a [Heroku log drain](\(urls.heroku_http_log_drain)) that
						points to your Vector instance's address:

						```bash
						heroku drains:add https://<user>:<pass>@<address> -a <app>
						```
						""",
				]
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
				type: string: examples: ["erlang"]
			}
			host: fields._local_host
			message: {
				description: "The message field, containing the plain text message."
				required:    true
				type: string: examples: ["Hi from erlang"]
			}
			proc_id: {
				description: "The procid field extracted from log message."
				required:    true
				type: string: examples: ["console"]
			}
			timestamp: fields._current_timestamp
		}
	}

	telemetry: metrics: {
		request_read_errors_total: components.sources.internal_metrics.output.metrics.request_read_errors_total
		requests_received_total:   components.sources.internal_metrics.output.metrics.requests_received_total
	}
}
