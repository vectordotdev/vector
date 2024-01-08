package metadata

components: sources: prometheus_pushgateway: {
	title: "Prometheus Pushgateway"

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
						title: "Prometheus Pushgateway"
						url:   urls.prometheus_pushgateway
					}
					direction: "incoming"
					port:      9091
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

	configuration: base.components.sources.prometheus_pushgateway.configuration

	output: metrics: {
		counter:   output._passthrough_counter
		gauge:     output._passthrough_gauge
		histogram: output._passthrough_histogram
		summary:   output._passthrough_summary
	}

	how_it_works: {
		post_vs_put: {
			title: "HTTP Methods - POST vs PUT"
			body: """
					The official Prometheus Pushgateway implementation supports `POST` and
					`PUT` requests for pushing metrics to a grouping key, with slightly
					different semantics.

					When metrics are sent via a `POST` request, only metrics with the same
					name are replaced. When they're sent via a `PUT` request, all metrics
					within the grouping key are replaced.

					Due to the difficulty of supporting the `PUT` semantics in Vector's
					architecture, only `POST` has been implemented.
				"""
		}

		protobuf: {
			title: "Protobuf"
			body: """
					The Prometheus Protobuf format is currently unsupported. Metrics can only
					be pushed in the text exposition format.
				"""
		}

		aggregation: {
			title: "Metric aggregation"
			body: """
					When `aggregate_metrics` is enabled only counters and histograms will be
					summed as it doesn't make sense to sum gauges or summaries from separate
					pushes.
				"""
		}
	}

	telemetry: metrics: {
		http_server_handler_duration_seconds: components.sources.internal_metrics.output.metrics.http_server_handler_duration_seconds
		http_server_requests_received_total:  components.sources.internal_metrics.output.metrics.http_server_requests_received_total
		http_server_responses_sent_total:     components.sources.internal_metrics.output.metrics.http_server_responses_sent_total
	}
}
