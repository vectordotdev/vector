package metadata

components: sinks: statsd: {
	title: "Statsd"

	classes: sinks.socket.classes

	features: {
		buffer:      sinks.socket.features.buffer
		healthcheck: sinks.socket.features.healthcheck
		send: {
			compression: sinks.socket.features.send.compression
			encoding: enabled: false
			request: sinks.socket.features.send.request
			send_buffer_bytes: {
				enabled:       true
				relevant_when: "mode = `tcp` or mode = `udp`"
			}
			tls: sinks.socket.features.send.tls
			to: {
				service: services.statsd_receiver

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp", "udp", "unix"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: sinks.socket.support

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    false
			set:          true
			summary:      false
		}
	}

	configuration: sinks.socket.configuration & {
		"type": "type": string: enum: statsd: "The type of this component."
		default_namespace: {
			common: true
			description: """
				Used as a namespace for metrics that don't have it.
				A namespace will be prefixed to a metric's name.
				"""
			required: false
			warnings: []
			type: string: {
				default: null
				examples: ["service"]
			}
		}
	}

	telemetry: metrics: {
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
