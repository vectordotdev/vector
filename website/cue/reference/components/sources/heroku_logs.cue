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
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "bytes"
		}
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
				can_verify_certificate: true
				enabled_default:        false
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.heroku_logs.configuration

	output: logs: line: {
		description: "An individual event from a batch of events received through an HTTP POST request."
		fields: {
			app_name: {
				description: "The app name field extracted from log message."
				required:    true
				type: string: {
					examples: ["erlang"]
				}
			}
			host: fields._local_host
			message: {
				description: "The message field, containing the plain text message."
				required:    true
				type: string: {
					examples: ["Hi from erlang"]
				}
			}
			proc_id: {
				description: "The procid field extracted from log message."
				required:    true
				type: string: {
					examples: ["console"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["heroku_logs"]
				}
			}
			timestamp: fields._current_timestamp
		}
	}

	telemetry: metrics: {
		http_server_handler_duration_seconds: components.sources.internal_metrics.output.metrics.http_server_handler_duration_seconds
		http_server_requests_received_total:  components.sources.internal_metrics.output.metrics.http_server_requests_received_total
		http_server_responses_sent_total:     components.sources.internal_metrics.output.metrics.http_server_responses_sent_total
	}
}
