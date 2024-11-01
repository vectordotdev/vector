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
		http_server_handler_duration_seconds: components.sources.internal_metrics.output.metrics.http_server_handler_duration_seconds
		http_server_requests_received_total:  components.sources.internal_metrics.output.metrics.http_server_requests_received_total
		http_server_responses_sent_total:     components.sources.internal_metrics.output.metrics.http_server_responses_sent_total
	}
}
