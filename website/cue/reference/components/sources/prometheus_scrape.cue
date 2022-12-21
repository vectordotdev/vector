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
		acknowledgements: false
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

	configuration: {
		endpoints: {
			description: "Endpoints to scrape metrics from."
			required:    true
			warnings: ["You must explicitly add the path to your endpoints. Vector will _not_ automatically add `/metrics`."]
			type: array: {
				items: type: string: {
					examples: ["http://localhost:9090/metrics"]
				}
			}
		}
		scrape_interval_secs: {
			common:      true
			description: "The interval between scrapes, in seconds."
			required:    false
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
			type: string: {
				default: null
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
			type: string: {
				default: null
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
			type: bool: {
				default: false
			}
		}
		query: {
			common: false
			description: """
				Custom parameters for the scrape request query string.
				One or more values for the same parameter key can be provided.
				The parameters provided in this option are appended to any parameters manually provided in the `endpoints` option.
				This option is especially useful when scraping the `/federate` endpoint.
				"""
			required: false
			type: object: {
				examples: [{"match[]": [#"{job="somejob"}"#, #"{__name__=~"job:.*"}"#]}]
				options: {
					"*": {
						common:      false
						description: "Any query key"
						required:    false
						type: array: {
							default: null
							examples: [[
								#"{job="somejob"}"#,
								#"{__name__=~"job:.*"}"#,
							]]
							items: type: string: {
								examples: [
									#"{job="somejob"}"#,
									#"{__name__=~"job:.*"}"#,
								]
								syntax: "literal"
							}
						}
					}
				}
			}
		}
		auth: configuration._http_auth & {_args: {
			password_example: "${PROMETHEUS_PASSWORD}"
			username_example: "${PROMETHEUS_USERNAME}"
		}}
	}

	how_it_works: {
		duplicate_tag_names: {
			title: "Duplicate tag names"
			body: """
				Multiple tags with the same name are invalid within Prometheus. Prometheus
				itself will reject a metric with duplicate tags. Vector will accept the metric,
				but will only take the last value for each tag name specified.
				"""
		}
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
