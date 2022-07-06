package metadata

components: sinks: statsd: {
	title: "Statsd"

	classes: sinks.socket.classes

	features: {
		acknowledgements: sinks.socket.features.acknowledgements
		healthcheck:      sinks.socket.features.healthcheck
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
		traces: false
	}

	configuration: {
		address: {
			description:   "The address to connect to. The address _must_ include a port."
			relevant_when: "mode = `tcp` or `udp`"
			required:      true
			type: string: {
				examples: ["92.12.333.224:5000"]
			}
		}
		mode: {
			description: "The type of socket to use."
			required:    true
			type: string: {
				enum: {
					tcp:  "TCP socket"
					udp:  "UDP socket"
					unix: "Unix domain socket"
				}
			}
		}
		path: {
			description:   "The unix socket path. This should be the absolute path."
			relevant_when: "mode = `unix`"
			required:      true
			type: string: {
				examples: ["/path/to/socket"]
			}
		}
		default_namespace: {
			common: true
			description: """
				Used as a namespace for metrics that don't have it.
				A namespace will be prefixed to a metric's name.
				"""
			required: false
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
