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
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		processed_bytes_total:                components.sources.internal_metrics.output.metrics.processed_bytes_total
		request_read_errors_total:            components.sources.internal_metrics.output.metrics.request_read_errors_total
		requests_received_total:              components.sources.internal_metrics.output.metrics.requests_received_total
	}
}
