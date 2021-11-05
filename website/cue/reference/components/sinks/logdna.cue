package metadata

components: sinks: logdna: {
	title: "LogDNA"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["LogDNA"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10490000
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: enabled: false
			to: {
				service: services.logdna

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
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
		api_key: {
			description: "The Ingestion API key."
			required:    true
			type: string: {
				examples: ["${LOGDNA_API_KEY}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		default_app: {
			common:      false
			description: "The default app that will be set for events that do not contain a `file` or `app` field."
			required:    false
			type: string: {
				default: "vector"
				examples: ["vector", "myapp"]
			}
		}
		default_env: {
			common:      false
			description: "The default environment that will be set for events that do not contain an `env` field."
			required:    false
			type: string: {
				default: "production"
				examples: ["staging", "production"]
			}
		}
		endpoint: {
			common:      false
			description: "The endpoint to send logs to."
			required:    false
			type: string: {
				default: "https://logs.logdna.com/logs/ingest"
				examples: ["http://127.0.0.1", "http://example.com"]
			}
		}
		hostname: {
			description: "The hostname that will be attached to each batch of events."
			required:    true
			type: string: {
				examples: ["${HOSTNAME}", "my-local-machine"]
			}
		}
		ip: {
			common:      false
			description: "The IP address that will be attached to each batch of events."
			required:    false
			type: string: {
				default: null
				examples: ["0.0.0.0"]
			}
		}
		mac: {
			common:      false
			description: "The mac address that will be attached to each batch of events."
			required:    false
			type: string: {
				default: null
				examples: ["my-mac-address"]
			}
		}
		tags: {
			common:      false
			description: "The tags that will be attached to each batch of events."
			required:    false
			type: array: {
				default: null
				items: type: string: {
					examples: ["tag1", "tag2"]
				}
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
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
