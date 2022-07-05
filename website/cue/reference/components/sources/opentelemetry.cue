package metadata

components: sources: opentelemetry: {
	_port: 6788

	title: "Opentelemetry"

	description: """
		Collect opentelemetry data over grpc (currently, only log is supported).
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: true
		multiline: enabled: false
		receive: {
			from: {
				service: services.opentelemetry

				interface: socket: {
					direction: "incoming"
					port:      _port
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: enabled: false
			keepalive: enabled:            true
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
		address: {
			description: """
				The grpc address to listen for connections on. It _must_ include a port.
				"""
			required: true
			type: string: {
				examples: ["0.0.0.0:\(_port)"]
			}
		}
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
	]

	output: {
		logs: event: {
			description: "An individual event from a batch of events received through a grpc request sent by opentemetry sdk"
			fields: {
				attributes: {
					description: "attributes for each log record"
					required:    false
					common:      true
					type: object: {
						examples: [{"k1": "v1"}]
					}
				}
				resources: {
					description: "resources for resourceLogs"
					required:    false
					common:      true
					type: object: {
						examples: [{"k1": "v1"}]
					}
				}
				message: {
					description: "log body"
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["hello world"]
					}
				}
				trace_id: {
					description: "trace_id"
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["37e7518fe2e2fcaf22b41c2dac059221"]
					}
				}
				span_id: {
					description: "span_id"
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["05abe7510db73b88"]
					}
				}
				severity_number: {
					description: "severity_number"
					required:    false
					common:      true
					type: uint: {
						default: null
						unit:    null
						examples: [9]
					}
				}
				severity_text: {
					description: "log level"
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["info"]
					}
				}
				flags: {
					description: "trace flags defined in W3C Trace Context specification"
					required:    false
					common: true
					type: uint: {
						default: null
						unit:    null
					}
				}
				timestamp: {
					description: "log generated timestamp in nano seconds"
					required:    true
					type: uint: {
						unit: null
					}
				}
				observed_time_unix_nano: {
					description: "timestamp in nano seconds when collector received the event"
					required:    true
					type: uint: {
						unit: null
					}
				}
				dropped_attributes_count: {
					description: "dropped_attributes_count in opentelemetry spec"
					required:    true
					type: uint: {
						unit: null
					}
				}
			}
		}
		metrics: {
			counter:      output._passthrough_counter
			distribution: output._passthrough_distribution
			gauge:        output._passthrough_gauge
			histogram:    output._passthrough_histogram
			set:          output._passthrough_set
		}
	}

	telemetry: metrics: {
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		protobuf_decode_errors_total:         components.sources.internal_metrics.output.metrics.protobuf_decode_errors_total
	}
}
