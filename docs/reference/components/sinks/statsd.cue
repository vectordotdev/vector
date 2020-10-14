package metadata

components: sinks: statsd: {
	title:             "Statsd"
	short_description: "Streams metric events to [StatsD][urls.statsd] metrics service."
	long_description:  "[StatsD][urls.statsd] is a standard and, by extension, a set of tools that can be used to send, collect, and aggregate custom metrics from any application. Originally, StatsD referred to a daemon written by [Etsy][urls.etsy] in Node."

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "stable"
		egress_method: "stream"
		function:      "transmit"
		service_providers: []
	}

	features: {
		buffer: enabled:      false
		compression: enabled: false
		encoding: codec: enabled: false
		healthcheck: enabled: true
		request: enabled:     false
		tls: enabled:         false
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

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

	configuration: {
		address: {
			common:      true
			description: "The UDP socket address to send stats to."
			required:    false
			warnings: []
			type: string: {
				default: "127.0.0.1:8125"
			}
		}
		mode: {
			description: "The type of socket to use."
			groups: ["tcp", "udp", "unix"]
			required: true
			warnings: []
			type: string: {
				enum: {
					tcp:  "TCP Socket."
					udp:  "UDP Socket."
					unix: "Unix Domain Socket."
				}
			}
		}
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
		path: {
			description: "The unix socket path. *This should be absolute path*."
			groups: ["unix"]
			required: true
			warnings: []
			type: string: {
				examples: ["/path/to/socket"]
			}
		}
	}
}
