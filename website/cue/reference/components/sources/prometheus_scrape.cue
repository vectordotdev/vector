package metadata

components: sources: prometheus_scrape: {
	title: "Prometheus Scrape"
	alias: "prometheus"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.prometheus_client

				interface: socket: {
					api: {
						title: "Prometheus"
						url:   urls.prometheus_text_based_exposition_format
					}
					direction: "outgoing"
					protocols: ["http"]
					ssl: "optional"
				}
			}
			proxy: enabled: true
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
		}
		multiline: enabled: false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		endpoints: {
			description: "Endpoints to scrape metrics from."
			required:    true
			warnings: ["You must explicitly add the path to your endpoints. Vector will _not_ automatically add `/metics`."]
			type: array: {
				items: type: string: {
					examples: ["http://localhost:9090/metrics"]
					syntax: "literal"
				}
			}
		}
		scrape_interval_secs: {
			common:      true
			description: "The interval between scrapes, in seconds."
			required:    false
			warnings: []
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
		instance_tag: {
			category: "Context"
			common:   true
			description: """
				The tag name added to each event representing the scraped instance's host:port.
				"""
			required: false
			warnings: []
			type: string: {
				default: null
				syntax:  "literal"
				examples: ["instance"]
			}
		}
		endpoint_tag: {
			category: "Context"
			common:   true
			description: """
				The tag name added to each event representing the scraped instance's endpoint.
				"""
			required: false
			warnings: []
			type: string: {
				default: null
				syntax:  "literal"
				examples: ["endpoint"]
			}
		}
		honor_labels: {
			category: "Context"
			common:   true
			description: """
				Controls how tag conflicts are handled if the scraped source has tags that Vector would add. If true,
				Vector will not add the new tag if the scraped metric has the tag already. If false, Vector will rename
				the conflicting tag by adding `exported_` to it.  This matches Prometheus's `honor_labels`
				configuration.
				"""
			required: false
			warnings: []
			type: bool: {
				default: false
			}
		}
		auth: configuration._http_auth & {_args: {
			password_example: "${PROMETHEUS_PASSWORD}"
			username_example: "${PROMETHEUS_USERNAME}"
		}}
	}

	output: metrics: {
		_extra_tags: {
			"instance": {
				description: "The host:port of the scraped instance. Only present if `instance_tag` is set."
				examples: ["localhost:9090"]
				required: false
			}
			"endpoint": {
				description: "Any endpoint of the scraped instance. Only present if `endpoint_tag` is set."
				examples: ["http://localhost:9090/metrics"]
				required: false
			}
		}

		counter: output._passthrough_counter & {
			tags: _extra_tags
		}
		gauge: output._passthrough_gauge & {
			tags: _extra_tags
		}
		histogram: output._passthrough_histogram & {
			tags: _extra_tags
		}
		summary: output._passthrough_summary & {
			tags: _extra_tags
		}
	}

	telemetry: metrics: {
		events_in_total:                 components.sources.internal_metrics.output.metrics.events_in_total
		http_error_response_total:       components.sources.internal_metrics.output.metrics.http_error_response_total
		http_request_errors_total:       components.sources.internal_metrics.output.metrics.http_request_errors_total
		parse_errors_total:              components.sources.internal_metrics.output.metrics.parse_errors_total
		processed_bytes_total:           components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:          components.sources.internal_metrics.output.metrics.processed_events_total
		component_received_events_total: components.sources.internal_metrics.output.metrics.component_received_events_total
		requests_completed_total:        components.sources.internal_metrics.output.metrics.requests_completed_total
		request_duration_seconds:        components.sources.internal_metrics.output.metrics.request_duration_seconds
	}
}
