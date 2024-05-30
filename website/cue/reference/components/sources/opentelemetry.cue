package metadata

components: sources: opentelemetry: {
	_grpc_port: 4317
	_http_port: 4318

	title: "OpenTelemetry"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		auto_generated:   true
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
		warnings: [
			"""
				The `opentelemetry` source only supports log and trace events at this time.
				""",
		]
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.opentelemetry.configuration

	outputs: [
		{
			name: "logs"
			description: """
				Received log events will go to this output stream. Use `<component_id>.logs` as an input to downstream transforms and sinks.
				"""
		},
		{
			name: "traces"
			description: """
				Received trace events will go to this output stream. Use `<component_id>.traces` as an input to downstream transforms and sinks.
				"""
		},
	]

	output: {
		logs: event: {
			description: "An individual log event from a batch of events received through an OTLP request"
			fields: {
				attributes: {
					description: "Attributes that describe the specific event occurrence."
					required:    false
					common:      true
					type: object: {
						examples: [
							{
								"http.status.code":          500
								"http.url":                  "http://example.com"
								"my.custom.application.tag": "hello"
							},
							{
								"http.scheme":      "https"
								"http.host":        "donut.mycie.com"
								"http.target":      "/order"
								"http.method":      "post"
								"http.status_code": 500
								"http.flavor":      "1.1"
								"http.user_agent":  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_14_0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.149 Safari/537.36"
							},
						]
					}
				}
				resources: {
					description: "Set of attributes that describe the resource."
					required:    false
					common:      true
					type: object: {
						examples: [
							{
								"service.name":    "donut_shop"
								"service.version": "2.0.0"
								"k8s.pod.uid":     "1138528c-c36e-11e9-a1a7-42010a800198"
							},
							{
								"container.name": "vector"
							},
						]
					}
				}
				message: {
					description: "Contains the body of the log record."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["20200415T072306-0700 INFO I like donuts"]
					}
				}
				trace_id: {
					description: "Request trace id as defined in W3C Trace Context. Can be set for logs that are part of request processing and have an assigned trace id."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["66346462623365646437363566363230"]
					}
				}
				span_id: {
					description: "Can be set for logs that are part of a particular processing span."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["43222c2d51a7abe3"]
					}
				}
				severity_number: {
					description: """
						Numerical value of the severity.

						Smaller numerical values correspond to less severe events (such as debug events), larger numerical values correspond to more severe events (such as errors and critical events).
						"""
					required: false
					common:   true
					type: uint: {
						default: null
						unit:    null
						examples: [3, 9, 17, 24]
					}
				}
				severity_text: {
					description: "Severity text (also known as log level)."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["TRACE3", "INFO", "ERROR", "FATAL4"]
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
					description: """
						The UTC Datetime when the event occurred. If this value is unset, or `0`, it will be set to the `observed_timestamp` field.

						This field is converted from the `time_unix_nano` Protobuf field.
						"""
					required: true
					type: timestamp: {}
				}
				observed_timestamp: {
					description: """
						The UTC Datetime when the event was observed by the collection system. If this value is unset, or `0`, it will be set to the current time.

						This field is converted from the `observed_time_unix_nano` Protobuf field.
						"""
					required: true
					type: timestamp: {}
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

	how_it_works: {
		tls: {
			title: "Transport Layer Security (TLS)"
			body:  """
				Vector uses [OpenSSL](\(urls.openssl)) for TLS protocols due to OpenSSL's maturity. You can
				enable and adjust TLS behavior via the `grpc.tls.*` and `http.tls.*` options and/or via an
				[OpenSSL configuration file](\(urls.openssl_conf)). The file location defaults to
				`/usr/local/ssl/openssl.cnf` or can be specified with the `OPENSSL_CONF` environment variable.
				"""
		}
		traces: {
			title: "Ingest OTLP traces"
			body: """
				Trace support is experimental and subject to change as Vector has no strongly-typed structure for traces internally. Instead traces are stored as a key/value map similar to logs. This may change in the future to be a structured format.
				"""
		}
	}

	telemetry: metrics: {
		grpc_server_handler_duration_seconds: components.sources.internal_metrics.output.metrics.grpc_server_handler_duration_seconds
		grpc_server_messages_received_total:  components.sources.internal_metrics.output.metrics.grpc_server_messages_received_total
		grpc_server_messages_sent_total:      components.sources.internal_metrics.output.metrics.grpc_server_messages_sent_total
		http_server_handler_duration_seconds: components.sources.internal_metrics.output.metrics.http_server_handler_duration_seconds
		http_server_requests_received_total:  components.sources.internal_metrics.output.metrics.http_server_requests_received_total
		http_server_responses_sent_total:     components.sources.internal_metrics.output.metrics.http_server_responses_sent_total
	}
}
