package metadata

components: sinks: openobserve: {
	title: "OpenObserve"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["OpenObserve"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "gzip"
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					default: "json"
				}
				timestamp_format: {
					enabled: true
					default: "rfc3339"
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
				service: services.openobserve

				interface: {
					socket: {
						api: {
							title: "OpenObserve HTTP API"
							url:   urls.openobserve
						}
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
		notices: []
	}

	configuration: base.components.sinks.openobserve.configuration

	input: {
		logs: true
		metrics: false
		traces: false
	}
}
