package metadata

components: sinks: postgres: {
	title: "PostgreSQL"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		stateful:      false
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
			request: {
				enabled: true
				headers: false
			}
			compression: enabled: false
			encoding: enabled:    false
			tls: enabled:         false
			to: {
				service: services.postgres
				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp", "unix"]
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

	configuration: base.components.sinks.postgres.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}
}
