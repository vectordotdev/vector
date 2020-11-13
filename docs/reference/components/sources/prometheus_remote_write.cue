package metadata

components: sources: prometheus_remote_write: {
	title:       "Prometheus remote_write"
	description: "FIXME [Prometheus](\(urls.prometheus)) is a pull-based monitoring system that scrapes metrics from configured endpoints, stores them efficiently, and supports a powerful query language to compose dynamic information from a variety of otherwise unrelated data points."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				name:     "Prometheus remote_write client"
				thing:    "a \(name)"
				url:      urls.prometheus_remote_write
				versions: null

				interface: socket: {
					api: {
						title: "Prometheus"
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
				can_enable:             true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
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

	configuration: {
		address: {
			description: "The address to accept connections on. The address _must_ include a port."
			required:    true
			type: string: examples: ["0.0.0.0:9090"]
		}
		auth: configuration._http_basic_auth
	}

	output: metrics: {
		counter: output._passthrough_counter
		gauge:   output._passthrough_gauge
	}
}
