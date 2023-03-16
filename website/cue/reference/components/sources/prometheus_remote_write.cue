package metadata

components: sources: prometheus_remote_write: {
	title: "Prometheus Remote Write"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		multiline: enabled: false
		receive: {
			from: {
				service: services.prometheus

				interface: socket: {
					api: {
						title: "Prometheus Remote Write"
						url:   urls.prometheus_remote_write
					}
					direction: "incoming"
					port:      9090
					protocols: ["http"]
					ssl: "optional"
				}
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				enabled_default:        false
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

	configuration: base.components.sources.prometheus_remote_write.configuration

	output: metrics: {
		counter: output._passthrough_counter
		gauge:   output._passthrough_gauge
	}

	how_it_works: {
		metric_types: {
			title: "Metric type interpretation"
			body: """
				The remote_write protocol used by this source transmits
				only the metric tags, timestamp, and numerical value. No
				explicit information about the original type of the
				metric (i.e. counter, histogram, etc) is included. As
				such, this source makes a guess as to what the original
				metric type was.

				For metrics named with a suffix of `_total`, this source
				emits the value as a counter metric. All other metrics
				are emitted as gauges.
				"""
		}

		duplicate_tag_names: {
			title: "Duplicate tag names"
			body: """
				Multiple tags with the same name are invalid within Prometheus. Prometheus
				itself will reject a metric with duplicate tags. Vector will accept the metric,
				but will only take the last value for each tag name specified.
				"""
		}
	}

	telemetry: metrics: {
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		parse_errors_total:                   components.sources.internal_metrics.output.metrics.parse_errors_total
		processed_bytes_total:                components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:               components.sources.internal_metrics.output.metrics.processed_events_total
		requests_completed_total:             components.sources.internal_metrics.output.metrics.requests_completed_total
		requests_received_total:              components.sources.internal_metrics.output.metrics.requests_received_total
		request_duration_seconds:             components.sources.internal_metrics.output.metrics.request_duration_seconds
	}
}
