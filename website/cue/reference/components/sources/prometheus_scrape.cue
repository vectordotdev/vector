package metadata

components: sources: prometheus_scrape: {
	title: "Prometheus Scrape"
	alias: "prometheus"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		auto_generated:   true
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

	configuration: base.components.sources.prometheus_scrape.configuration & {
		endpoints: warnings: ["You must explicitly add the path to your endpoints. Vector will _not_ automatically add `/metrics`."]
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

		query_params_structure: {
			title: "Query params structure"
			body: """
				In query params, key needs to be `match[]` with array of values

				```yaml
				sources:
					source0:
						query:
							"match[]":
								- '{job="somejob"}'
								- '{__name__=~"job:.*"}'
				```
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
		http_client_responses_total:      components.sources.internal_metrics.output.metrics.http_client_responses_total
		http_client_response_rtt_seconds: components.sources.internal_metrics.output.metrics.http_client_response_rtt_seconds
	}
}
