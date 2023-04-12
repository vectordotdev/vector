package metadata

components: sources: statsd: {
	_port: 8125

	title: "StatsD"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: false
		multiline: enabled: false
		receive: {
			from: {
				service: services.statsd
				interface: socket: {
					api: {
						title: "StatsD"
						url:   urls.statsd_udp_protocol
					}
					direction: "incoming"
					port:      _port
					protocols: ["udp"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: {
				enabled:       true
				relevant_when: "mode = `tcp` or mode = `udp`"
			}
			keepalive: enabled: true
			tls: {
				enabled:                true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
		auto_generated: true
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.statsd.configuration

	output: metrics: {
		counter:      output._passthrough_counter
		distribution: output._passthrough_distribution
		gauge:        output._passthrough_gauge
		set:          output._passthrough_set
	}

	how_it_works: {
		timestamps: {
			title: "Timestamps"
			body:  """
				The StatsD protocol doesn't provide support for sending metric timestamps. You may
				notice that each parsed metric is assigned a `null` timestamp, which is a special
				value indicating a realtime metric (i.e. not a historical metric). Normally, such
				`null` timestamps are substituted with the current time by downstream sinks or
				third-party services during sending/ingestion. See the
				[metric data model](\(urls.vector_metric)) page for more info.
				"""
		}
	}

	telemetry: metrics: {
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		connection_errors_total:              components.sources.internal_metrics.output.metrics.connection_errors_total
		invalid_record_total:                 components.sources.internal_metrics.output.metrics.invalid_record_total
		invalid_record_bytes_total:           components.sources.internal_metrics.output.metrics.invalid_record_bytes_total
		processed_bytes_total:                components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:               components.sources.internal_metrics.output.metrics.processed_events_total
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
	}
}
