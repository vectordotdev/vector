package metadata

components: sinks: mqtt: {
	title: "MQTT"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: false
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      false
			}
			to: {
				service: services.mqtt
				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp"]
						ssl: "optional"
					}
				}
			}
		}
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

	configuration: base.components.sinks.mqtt.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	telemetry: metrics: {
		open_connections:                     components.sources.internal_metrics.output.metrics.open_connections
		connection_shutdown_total:            components.sources.internal_metrics.output.metrics.connection_shutdown_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_events_count:      components.sources.internal_metrics.output.metrics.component_received_events_count
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_sent_bytes_total:           components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:          components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total:     components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
	}
}
