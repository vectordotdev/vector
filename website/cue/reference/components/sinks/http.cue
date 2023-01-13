package metadata

components: sinks: http: {
	title: "HTTP"

	classes: {
		commonly_used: true
		service_providers: []
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: true
		healthcheck: {
			enabled:  true
			uses_uri: true
		}
		send: {
			batch: {
				enabled:      true
				common:       true
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					framing: true
					enum: ["json", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: {
					name:     "HTTP"
					thing:    "an \(name) server"
					url:      urls.http_server
					versions: null
				}

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: ["Input type support can depend on configured `encoding.codec`"]
	}

	configuration: {
		auth: configuration._http_auth & {_args: {
			password_example: "${HTTP_PASSWORD}"
			username_example: "${HTTP_USERNAME}"
		}}
		uri: {
			description: """
				The full URI to make HTTP requests to. This should include the protocol and host,
				but can also include the port, path, and any other valid part of a URI.
				"""
			required: true
			type: string: {
				examples: ["https://10.22.212.22:9000/endpoint"]
			}
		}
		method: {
			description: "The HTTP method to use."
			required:    false
			common:      false
			type: string: {
				default: "POST"
				enum: {
					PUT:  "PUT"
					POST: "POST"
				}
			}
		}
		payload_prefix: {
			description: """
				A string to prefix the payload with.

				This option is ignored if the encoding is not character delimited JSON.
				If specified, the `payload_suffix`must also be specified and together they must produce a valid JSON object.
				"""
			required: false
			type: string: {
				default: ""
				examples: ["{\"data\":"]
			}
		}
		payload_suffix: {
			description: """
				A string to suffix the payload with.

				This option is ignored if the encoding is not character delimited JSON.
				If specified, the `payload_prefix`must also be specified and together they must produce a valid JSON object.
				"""
			required: false
			type: string: {
				default: ""
				examples: ["}"]
			}
		}
	}

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			summary:      true
			set:          true
		}
		traces: true
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		http_bad_requests_total:          components.sources.internal_metrics.output.metrics.http_bad_requests_total
		processed_bytes_total:            components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:           components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
