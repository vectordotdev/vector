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
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.opentelemetry.configuration

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
		{
			name: "metrics"
			description: """
				Received metric events will go to this output stream. Use `<component_id>.metrics` as an input to downstream transforms and sinks.
				"""
		},
	]

	output: {
		logs: event: {
			description: "An individual log event from a batch of events received through an OTLP request. The following applies only when the `use_otlp_decoding` option is `false`."
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
				"scope.name": {
					description: "Instrumentation scope name (often logger name)."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["some.module.name"]
					}
				}
				"scope.version": {
					description: "Instrumentation scope version."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["1.2.3"]
					}
				}
				"scope.attributes": {
					description: "Set of attributes that belong to the instrumentation scope."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"attr1": "value1"
								"attr2": "value2"
								"attr3": "value3"
							},
						]
					}
				}
				"scope.dropped_attributes_count": {
					description: "Number of attributes dropped from the instrumentation scope (if not zero)."
					required:    false
					common:      false
					type: uint: {
						unit: null
					}
				}
				"scope.schema_url": {
					description: "The schema URL for the instrumentation scope. Applies to all log records within this scope."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["https://opentelemetry.io/schemas/1.21.0"]
					}
				}
				schema_url: {
					description: "The schema URL for the resource. Applies to the data in the `resources` field."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["https://opentelemetry.io/schemas/1.21.0"]
					}
				}
				resource_dropped_attributes_count: {
					description: "Number of attributes dropped from the resource due to collection limits (if not zero)."
					required:    false
					common:      false
					type: uint: {
						unit: null
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
		metrics: "": {
			description: "Metric events that may be emitted by this source."
		}
		traces: event: {
			description: "An individual trace span event from a batch of events received through an OTLP request. The following applies only when the `use_otlp_decoding` option is `false`. Trace spans are stored as key/value maps at the event root."
			fields: {
				trace_id: {
					description: "A unique identifier for the trace (hex-encoded 16 bytes)."
					required:    true
					type: string: {
						default: null
						examples: ["0123456789abcdef0123456789abcdef"]
					}
				}
				span_id: {
					description: "A unique identifier for the span within the trace (hex-encoded 8 bytes)."
					required:    true
					type: string: {
						default: null
						examples: ["0123456789abcdef"]
					}
				}
				parent_span_id: {
					description: "The span_id of this span's parent span (hex-encoded 8 bytes). Empty for root spans."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["fedcba9876543210"]
					}
				}
				trace_state: {
					description: "W3C trace-state header value conveying vendor-specific trace information."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["rojo=00f067aa0ba902b7"]
					}
				}
				name: {
					description: "A description of the span's operation (e.g. a qualified method name)."
					required:    true
					type: string: {
						default: null
						examples: ["GET /api/users", "mysql.query"]
					}
				}
				kind: {
					description: "The span kind. 0=Unspecified, 1=Internal, 2=Server, 3=Client, 4=Producer, 5=Consumer."
					required:    true
					type: uint: {
						unit: null
						examples: [1, 2, 3]
					}
				}
				start_time_unix_nano: {
					description: "Start time of the span."
					required:    true
					type: timestamp: {}
				}
				end_time_unix_nano: {
					description: "End time of the span."
					required:    true
					type: timestamp: {}
				}
				attributes: {
					description: "Span-level attributes as key/value pairs."
					required:    false
					common:      true
					type: object: {
						examples: [
							{
								"http.method":      "GET"
								"http.status_code": 200
							},
						]
					}
				}
				dropped_attributes_count: {
					description: "Number of span attributes dropped due to collection limits."
					required:    true
					type: uint: {
						unit: null
					}
				}
				events: {
					description: "Time-stamped annotations on the span."
					required:    false
					common:      true
					type: array: items: type: object: options: {}
				}
				dropped_events_count: {
					description: "Number of span events dropped."
					required:    true
					type: uint: {
						unit: null
					}
				}
				links: {
					description: "References from this span to spans in the same or different traces."
					required:    false
					common:      false
					type: array: items: type: object: options: {}
				}
				dropped_links_count: {
					description: "Number of span links dropped."
					required:    true
					type: uint: {
						unit: null
					}
				}
				status: {
					description: "Status of the span with `message` and `code` fields."
					required:    false
					common:      true
					type: object: {
						examples: [
							{
								message: ""
								code:    0
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
								"service.name":    "my-service"
								"service.version": "1.0.0"
							},
						]
					}
				}
				"scope.name": {
					description: "Instrumentation scope name (e.g. tracer library name)."
					required:    false
					common:      true
					type: string: {
						default: null
						examples: ["io.opentelemetry.contrib.mongodb"]
					}
				}
				"scope.version": {
					description: "Instrumentation scope version."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["1.2.3"]
					}
				}
				"scope.attributes": {
					description: "Set of attributes that belong to the instrumentation scope."
					required:    false
					common:      false
					type: object: {
						examples: [
							{
								"attr1": "value1"
							},
						]
					}
				}
				"scope.dropped_attributes_count": {
					description: "Number of attributes dropped from the instrumentation scope (if not zero)."
					required:    false
					common:      false
					type: uint: {
						unit: null
					}
				}
				"scope.schema_url": {
					description: "The schema URL for the instrumentation scope."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["https://opentelemetry.io/schemas/1.21.0"]
					}
				}
				schema_url: {
					description: "The schema URL for the resource."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["https://opentelemetry.io/schemas/1.21.0"]
					}
				}
				resource_dropped_attributes_count: {
					description: "Number of attributes dropped from the resource due to collection limits (if not zero)."
					required:    false
					common:      false
					type: uint: {
						unit: null
					}
				}
				ingest_timestamp: {
					description: "The UTC time when Vector received the trace event."
					required:    true
					type: timestamp: {}
				}
			}
		}
	}

	how_it_works: {
		otlp_decoding: {
			title: "OTLP Decoding Usage"
			body: """
				This section is a usage example for the `use_otlp_decoding` option.
				This setup allows shipping OTLP formatted logs to an OTEL collector without the use of a `remap` transform.
				The same can be done for metrics and traces.

				However, OTLP formatted metrics cannot be converted to Vector's metrics format. As a workaround, the OTLP
				metrics are converted to Vector log events while preserving the OTLP format. This prohibits the use of metric
				transforms like `aggregate` but it enables easy shipping to OTEL collectors.

				The recommended `opentelemetry` sink configuration is the following:
				```yaml
					otel_sink:
						inputs:
							- otel.logs
						type: opentelemetry
						protocol:
							type: http
							uri: http://localhost:5318/v1/logs
						encoding:
							codec: otlp
				```
				"""
		}
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
		metrics: {
			title: "Ingest metrics"
			body: """
				Metrics support is experimental and subject to change due to structural differences between the internal Vector metric data model and OpenTelemetry.
				If aggregation temporality is supported for an OpenTelemetry metric type, it influences the corresponding Vector metric kind as follows: If temporality is set to Delta, the metric kind is Incremental; otherwise, it is Absolute.
				Metric type mappings:
				Gauge is mapped to a Vector Gauge;
				Sum is mapped to a Vector Counter if `is_monotonic` is true, to Vector Gauge if `is_monotonic` is false;
				Histogram is mapped to a Vector AggregatedHistogram;
				Exponential Histogram is also mapped to a Vector AggregatedHistogram, bucket boundaries are reconstructed from the exponential scale;
				Summary is mapped to a Vector Aggregated Summary.
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
