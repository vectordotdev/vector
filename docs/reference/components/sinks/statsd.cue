package metadata

components: sinks: statsd: {
	title:       "Statsd"
	description: "[StatsD](\(urls.statsd)) is a standard and, by extension, a set of tools that can be used to send, collect, and aggregate custom metrics from any application. Originally, StatsD referred to a daemon written by [Etsy](\(urls.etsy)) in Node."

	classes: sinks.socket.classes

	features: {
		buffer:      sinks.socket.features.buffer
		healthcheck: sinks.socket.features.healthcheck
		send: {
			compression: sinks.socket.features.send.compression
			encoding: {
				enabled: true
				codec: enabled: false
			}
			request: sinks.socket.features.send.request
			tls:     sinks.socket.features.send.tls
			to: {
				name:     "Statsd receiver"
				thing:    "a \(name)"
				url:      urls.statsd
				versions: null

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
			histogram:    true
			set:          true
			summary:      true
		}
	}

	configuration: sinks.socket.configuration & {
		namespace: {
			common:      true
			description: "A prefix that will be added to all metric names."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["service"]
			}
		}
	}
}
