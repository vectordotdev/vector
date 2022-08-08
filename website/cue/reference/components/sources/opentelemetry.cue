package metadata

components: sources: opentelemetry: {
	_grpc_port: 4317
	_http_port: 4318

	title: "OpenTelemetry"

	description: """
		Collect OpenTelemetry data over gRPC and HTTP (currently, only logs are supported).
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "aggregator"]
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
					port:      _grpc_port
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
			tls: {
				// enabled per listener below
				enabled: false
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
		grpc: {
			description: "Configuration options for the gRPC server."
			required:    true
			type: object: {
				examples: [{address: "0.0.0.0:\(_grpc_port)"}]
				options: {
					address: {
						description: """
						The gRPC address to listen for connections on. It _must_ include a port.
						"""
						required: true
						type: string: {
							examples: ["0.0.0.0:\(_grpc_port)"]
						}
					}
					tls: configuration._tls_accept & {_args: {
						can_verify_certificate: true
						enabled_default:        false
					}}
				}
			}
		}
		http: {
			description: "Configuration options for the HTTP server."
			required:    true
			type: object: {
				examples: [{address: "0.0.0.0:\(_http_port)"}]
				options: {
					address: {
						description: """
							The HTTP address to listen for connections on. It _must_ include a port.
							"""
						required: true
						type: string: {
							examples: ["0.0.0.0:\(_http_port)"]
						}
					}
					tls: configuration._tls_accept & {_args: {
						can_verify_certificate: true
						enabled_default:        false
					}}
				}
			}
		}
	}

	outputs: [
		{
			name: "logs"
			description: """
				Received log events will go to this output stream. Use `<component_id>.logs` as an input to downstream transforms and sinks.
				"""
		},
	]

	output: {
		logs: event: {
			description: "An individual event from a batch of events received through a gRPC request sent by OpenTelemetry SDK"
			fields: {
				attributes: {
					description: "Attributes that describe the specific event occurrence."
					required:    false
					common:      true
					type: object: {
						examples: [{"k1": "v1"}]
					}
				}
				resources: {
					description: "Set of attributes that describe the resource."
					required:    false
					common:      true
					type: object: {
						examples: [{"k1": "v1"}]
					}
				}
				message: {
					description: "Contains the body of the log record."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["hello world"]
					}
				}
				trace_id: {
					description: "Request trace id as defined in W3C Trace Context. Can be set for logs that are part of request processing and have an assigned trace id."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["37e7518fe2e2fcaf22b41c2dac059221"]
					}
				}
				span_id: {
					description: "Can be set for logs that are part of a particular processing span."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["05abe7510db73b88"]
					}
				}
				severity_number: {
					description: "Numerical value of the severity. Smaller numerical values correspond to less severe events (such as debug events), larger numerical values correspond to more severe events (such as errors and critical events)."
					required:    false
					common:      true
					type: uint: {
						default: null
						unit:    null
						examples: [9]
					}
				}
				severity_text: {
					description: "Severity text (also known as log level)."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["info"]
					}
				}
				flags: {
					description: "Trace flag as defined in W3C Trace Context specification."
					required:    false
					common:      true
					type: uint: {
						default: null
						unit:    null
					}
				}
				timestamp: {
					description: "The UTC Datetime when the event occurred."
					required:    true
					type: uint: {
						unit: null
					}
				}
				observed_timestamp: {
					description: "The UTC Datetime when the event was observed by the collection system."
					required:    true
					type: uint: {
						unit: null
					}
				}
				dropped_attributes_count: {
					description: "Counts for attributes dropped due to collection limits."
					required:    true
					type: uint: {
						unit: null
					}
				}
			}
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

	how_it_works: {
		tls: {
			title: "Transport Layer Security (TLS)"
			body:  """
				  Vector uses [OpenSSL](\(urls.openssl)) for TLS protocols. You can
				  adjust TLS behavior via the `grpc.tls.*` and `http.tls.*` options.
				  """
		}
	}
}
