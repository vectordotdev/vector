package metadata

components: sources: datadog_agent: {
	_port: 8080

	title: "Datadog Agent"

	description: """
		Receives observability data from a Datadog Agent over HTTP or HTTPS. For now, this is limited to logs and metrics
		but will be expanded in the future cover traces.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator", "sidecar"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "bytes"
		}
		receive: {
			from: {
				service: services.datadog_agent

				interface: socket: {
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
		multiple_outputs: {
			common: false
			description: """
				If this setting is set to `true` metrics and logs will be sent to different ouputs. For a source component
				named `agent` the received logs and metrics can then be accessed by specifying `agent.logs` and `agent.metrics`,
				respectively, as the input to another component.
				"""
			required: false
			type: bool: default: false
		}
		store_api_key: {
			common:      false
			description: "When incoming events contain a Datadog API key, if this setting is set to `true` the key will kept in the event metadata and will be used if the event is sent to a Datadog sink."
			required:    false
			type: bool: default: true
		}
	}

	outputs: [
		{
			name: components._default_output.name
			description: """
				Default output stream of the component. Use this component's ID as an input to downstream transforms and sinks. Only active if [multiple_outputs](#multiple_outputs) is disabled.
				"""
		},
		{
			name: "logs"
			description: """
				If [multiple_outputs](#multiple_outputs) is enabled, received log events will go to this output stream. Use `<component_id>.logs` as an input to downstream transforms and sinks.
				"""
		},
		{
			name: "metrics"
			description: """
				If [multiple_outputs](#multiple_outputs) is enabled, received metric events will go to this output stream. Use `<component_id>.metrics` as an input to downstream transforms and sinks.
				"""
		},
	]

	output: {
		logs: line: {
			description: "An individual event from a batch of events received through an HTTP POST request sent by a Datadog Agent."
			fields: {
				message: {
					description: "The message field, containing the plain text message."
					required:    true
					type: string: {
						examples: ["Hi from erlang"]
					}
				}
				status: {
					description: "The status field extracted from the event."
					required:    true
					type: string: {
						examples: ["info"]
					}
				}
				timestamp: fields._current_timestamp
				hostname:  fields._local_host
				service: {
					description: "The service field extracted from the event."
					required:    true
					type: string: {
						examples: ["backend"]
					}
				}
				ddsource: {
					description: "The source field extracted from the event."
					required:    true
					type: string: {
						examples: ["java"]
					}
				}
				ddtags: {
					description: "The coma separated tags list extracted from the event."
					required:    true
					type: string: {
						examples: ["env:prod,region:ap-east-1"]
					}
				}
			}
		}
		metrics: {
			counter:      output._passthrough_counter
			distribution: output._passthrough_distribution
			gauge:        output._passthrough_gauge
		}
	}

	how_it_works: {
		decompression: {
			title: "Configuring the Datadog Agent"
			body:  """
				To send logs from a Datadog Agent to this source, the [Datadog Agent](\(urls.datadog_agent_doc)) configuration
				must be updated to use:

				```yaml
				logs_config:
					dd_url: "<VECTOR_HOST>:<SOURCE_PORT>"
					use_v2_api: false # source does not yet support new v2 API
					use_http: true # this source only supports HTTP/HTTPS
					logs_no_ssl: true|false # should match source SSL configuration.
				```
				"""
		}
	}

	telemetry: metrics: {
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
	}
}
