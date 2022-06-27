package metadata

components: sources: datadog_agent: {
	_port: 8080

	title: "Datadog Agent"

	description: """
		Receives observability data from a Datadog Agent over HTTP or HTTPS.
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
		acknowledgements: true
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
		acknowledgements: configuration._source_acknowledgements
		address:          sources.http.configuration.address
		multiple_outputs: {
			common: false
			description: """
				If this setting is set to `true` logs, metrics and traces will be sent to different ouputs. For a source
				component named `agent` the received logs, metrics, and traces can then be accessed by specifying
				`agent.logs`, `agent.metrics`, and `agent.traces`, respectively, as the input to another component.
				"""
			required: false
			type: bool: default: false
		}
		disable_logs: {
			common:      false
			description: "If this settings is set to `true`, logs won't be accepted by the component."
			required:    false
			type: bool: default: false
		}
		disable_metrics: {
			common:      false
			description: "If this settings is set to `true`, metrics won't be accepted by the component."
			required:    false
			type: bool: default: false
		}
		disable_traces: {
			common:      false
			description: "If this settings is set to `true`, traces won't be accepted by the component."
			required:    false
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
		{
			name: "traces"
			description: """
				If [multiple_outputs](#multiple_outputs) is enabled, received trace events will go to this output stream. Use `<component_id>.traces` as an input to downstream transforms and sinks.
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
		traces: {
			description: "A trace received through an HTTP POST request sent by a Datadog Trace Agent."
			fields: {
				spans: {
					description: "The list of spans composing the trace."
					required:    true
					type: array: items: type: object: options: {}
				}
			}
		}
	}

	how_it_works: {
		decompression: {
			title: "Configuring the Datadog Agent"
			body:  """
				Sending logs or metrics to Vector requires the [Datadog Agent](\(urls.datadog_agent_doc)) v7.35/6.35 or greater.

				To send logs from a Datadog Agent to this source, the [Datadog Agent](\(urls.datadog_agent_doc)) configuration
				must be updated to use:

				```yaml
				vector:
					logs.enabled: true
					logs.url: http://"<VECTOR_HOST>:<SOURCE_PORT>" # Use https if SSL is enabled in Vector source configuration
				```

				In order to send metrics the [Datadog Agent](\(urls.datadog_agent_doc)) configuration must be updated with the
				following options:

				```yaml
				vector:
					metrics.enabled: true
					metrics.url: http://"<VECTOR_HOST>:<SOURCE_PORT>" # Use https if SSL is enabled in Vector source configuration
				```

				"""
		}
		trace_support: {
			title: "Trace support"
			body: """
				The `datadog_agent` source is capable of receiving traces from the Datadog Agent for versions < 6/7.33.
				We are working on adding support for the newer agent versions as well as support for passing along APM
				statistics used by Datadog.
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
