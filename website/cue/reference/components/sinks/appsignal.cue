package metadata

components: sinks: appsignal: {
	title: "AppSignal"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["AppSignal"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   100
				max_bytes:    450_000_000
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "gzip"
				algorithms: ["gzip"]
				levels: [6]
			}
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled:     true
				concurrency: 100
				headers:     false
			}
			tls: enabled: false
			to: {
				service: services.appsignal

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
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

	configuration: base.components.sinks.appsignal.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    false
			set:          false
			summary:      false
		}
		traces: false
	}

	telemetry: components.sinks.http.telemetry
}
