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

	configuration: base.components.sources.http_client.configuration

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
		traces: {
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
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		http_error_response_total:            components.sources.internal_metrics.output.metrics.http_error_response_total
		http_request_errors_total:            components.sources.internal_metrics.output.metrics.http_request_errors_total
		parse_errors_total:                   components.sources.internal_metrics.output.metrics.parse_errors_total
		processed_bytes_total:                components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:               components.sources.internal_metrics.output.metrics.processed_events_total
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		requests_completed_total:             components.sources.internal_metrics.output.metrics.requests_completed_total
		request_duration_seconds:             components.sources.internal_metrics.output.metrics.request_duration_seconds
	}
}
