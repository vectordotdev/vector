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
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       true
				max_bytes:    10_000_000
				timeout_secs: 1
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
					batched: true
					enum: ["json", "ndjson", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: true
			}
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
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
		notices: []
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
		healthcheck: type: object: options: uri: {
			common: false
			description: """
				The full URI to make HTTP health check request to. This should include the protocol and host,
				but can also include the port, path, and any other valid part of a URI.
				"""
			required: false
			type: string: {
				default: null
				examples: ["https://10.22.212.22:9000/health"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
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
