package metadata

components: sources: http_client: {
	title: "HTTP Client"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar", "aggregator"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: false
		auto_generated:   true
		codecs: {
			enabled:         true
			default_framing: "`bytes`"
		}
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.http_scrape

				interface: socket: {
					direction: "outgoing"
					protocols: ["http"]
					ssl: "optional"
				}
			}
			proxy: enabled: true
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
		}
		multiline: enabled: false
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.http_client.configuration & {
		endpoint: warnings: ["You must explicitly add the path to your endpoint."]
	}

	output: {
		logs: {
			text: {
				description: "An individual line from a `text/plain` HTTP request"
				fields: {
					message: {
						description:   "The raw line from the incoming payload."
						relevant_when: "encoding == \"text\""
						required:      true
						type: string: {
							examples: ["Hello world"]
						}
					}
					source_type: {
						description: "The name of the source type."
						required:    true
						type: string: {
							examples: ["http_client"]
						}
					}
					timestamp: fields._current_timestamp
				}
			}
			structured: {
				description: "An individual line from an `application/json` request"
				fields: {
					"*": {
						common:        false
						description:   "Any field contained in your JSON payload"
						relevant_when: "encoding == \"json\""
						required:      false
						type: "*": {}
					}
					source_type: {
						description: "The name of the source type."
						required:    true
						type: string: {
							examples: ["http_client"]
						}
					}
					timestamp: fields._current_timestamp
				}
			}
		}
		metrics: {
			_extra_tags: {
				"source_type": {
					description: "The name of the source type."
					examples: ["http_client"]
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
			histogram: output._passthrough_histogram & {
				tags: _extra_tags
			}
			set: output._passthrough_set & {
				tags: _extra_tags
			}
		}
		traces: trace: {
			description: "A trace received through an HTTP request."
			fields: {
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["http_client"]
					}
				}
			}
		}
	}

	telemetry: metrics: {
		http_client_responses_total:      components.sources.internal_metrics.output.metrics.http_client_responses_total
		http_client_response_rtt_seconds: components.sources.internal_metrics.output.metrics.http_client_response_rtt_seconds
	}

	how_it_works: {
		query_params_structure: {
			title: "Query params structure"
			body: """
				Query params can either be single key value pair or a key with multiple values

				```yaml
				sources:
					source0:
						query:
							field: value
							fruit:
								- mango
								- papaya
								- kiwi
							start_time:
								type: vrl
								value: "now()"
				```
				"""
		}
		request_body_generation: {
			title: "Request Body Generation"
			body: """
				The request body can be a static string or a dynamic value generated via VRL.
				Using VRL allows you to construct JSON payloads or other formats dynamically at request time.

				When a body is provided, the `Content-Type` header is automatically set to
				`application/json` unless explicitly overridden in the `headers` configuration.

				**Static Body**

				```yaml
				body: "{"foo": "bar"}"
				```

				**Dynamic VRL Body**

				When `type` is set to `vrl`, the `value` is evaluated as a VRL expression. The result is used as the request body.

				```yaml
				body:
					type: vrl
					value: |
					encode_json({
						"searchStatements": [{"column": "auditAction", "operator": "=", "value": "DELETE"}],
					"timestamp": now()
					})
				```
				"""
		}
	}
}
