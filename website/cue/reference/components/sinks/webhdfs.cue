package metadata

components: sinks: webhdfs: {
	title: "WebHDFS"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"

		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip", "zstd"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					framing: true
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: enabled:     false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.webhdfs.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}
}
