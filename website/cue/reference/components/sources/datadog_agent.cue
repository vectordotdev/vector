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
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
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

	configuration: base.components.sources.datadog_agent.configuration

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
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["datadog_agent"]
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
					description: "The comma separated tags list extracted from the event."
					required:    true
					type: string: {
						examples: ["env:prod,region:ap-east-1"]
					}
				}
			}
		}
		metrics: {
			_extra_tags: {
				"source_type": {
					description: "The name of the source type."
					examples: ["datadog_agent"]
					required: true
				}
			}
			counter: output._passthrough_counter & {
				tags: _extra_tags
			}
			distribution: output._passthrough_distribution & {
				tags: _extra_tags
			}
			gauge: output._passthrough_gauge & {
				tags: _extra_tags
			}
		}
		traces: {
			description: "A trace received through an HTTP POST request sent by a Datadog Trace Agent."
			fields: {
				spans: {
					description: "The list of spans composing the trace."
					required:    true
					type: array: items: type: object: options: {}
				}
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["datadog_agent"]
					}
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

				In order to send traces the [Datadog Agent](\(urls.datadog_agent_doc)) configuration must be updated with the
				following options:

				```yaml
				vector:
					traces.enabled: true
					traces.url: http://"<VECTOR_HOST>:<SOURCE_PORT>" # Use https if SSL is enabled in Vector source configuration
				```
				"""
		}
		trace_support: {
			title: "Trace support caveats"
			body: """
				The `datadog_agent` source is capable of receiving traces from the Datadog Agent and
				forwarding them to Datadog. In order to have accurate APM statistics, you should
				disable any sampling of traces within the Datadog Agent or client SDKs as Vector
				calculates the metrics that drive the APM statistics (like span hit count and
				duration distribution).
				"""
		}
	}

	telemetry: metrics: {
		http_server_handler_duration_seconds: components.sources.internal_metrics.output.metrics.http_server_handler_duration_seconds
		http_server_requests_received_total:  components.sources.internal_metrics.output.metrics.http_server_requests_received_total
		http_server_responses_sent_total:     components.sources.internal_metrics.output.metrics.http_server_responses_sent_total
	}
}
