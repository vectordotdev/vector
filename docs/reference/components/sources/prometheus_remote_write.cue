package metadata

components: sources: prometheus_remote_write: {
	title:       "Prometheus remote_write"
	description: "The [Prometheus](\(urls.prometheus)) remote_write protocol is a protobuf based encoding for sending metrics efficiently between agents."

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
				service: {
					name:     "Prometheus remote_write client"
					thing:    "a \(name)"
					url:      urls.prometheus_remote_integrations
					versions: null
				}

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
		targets: {
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

	installation: {
		platform_name: null
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

	how_it_works: {
		metric_types: {
			title: "Metric type interpretation"
			body: """
				The remote_write protocol used by this source transmits
				only the metric tags, timestamp, and numerical value. No
				explicit information about the original type of the
				metric (ie counter, histogram, etc) is included. As
				such, this source makes a guess as to what the original
				metric type was.

				For metrics named with a suffix of `_total`, this source
				emits the value as a counter metric. All other metrics
				are emitted as gauges.
				"""
		}
	}
}
