package metadata

components: sinks: statsd: {
	title: "StatsD"

	classes: sinks.socket.classes

	features: {
		acknowledgements: sinks.socket.features.acknowledgements
		auto_generated:   true
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

	configuration: base.components.sinks.statsd.configuration
}
