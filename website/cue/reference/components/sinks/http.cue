package metadata

components: sinks: http: {
	title: "HTTP"

	classes: {
		commonly_used: true
		service_providers: []
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: {
			enabled:  true
			uses_uri: true
		}
		send: {
			batch: {
				enabled:      true
				common:       true
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip"]
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
			proxy: enabled: true
			request: {
				enabled: true
				headers: true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: {
					name:     "HTTP"
					thing:    "an \(name) server"
					url:      urls.http_server
					versions: null
				}

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: ["Input type support can depend on configured `encoding.codec`"]
	}

	configuration: base.components.sinks.http.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			summary:      true
			set:          true
		}
		traces: true
	}
}
