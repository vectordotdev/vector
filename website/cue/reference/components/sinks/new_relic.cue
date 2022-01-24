package metadata

components: sinks: new_relic: {
	title: "New Relic"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["New Relic"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   50
				timeout_secs: 30
			}
			compression: {
				enabled: true
				default: "gzip"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
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
				service: services.new_relic

				interface: {
					socket: {
						api: {
							title: "New Relic Event, Metric and Log API"
							url:   urls.new_relic_apis
						}
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

	configuration: {
		license_key: {
			description: "Your New Relic license key."
			required:    true
			warnings: []
			type: string: {
				examples: ["xxxx", "${NEW_RELIC_LICENSE_KEY}"]
				syntax: "literal"
			}
		}
		account_id: {
			description: "Your New Relic account ID."
			required:    true
			warnings: []
			type: string: {
				examples: ["xxxx", "${NEW_RELIC_ACCOUNT_ID}"]
				syntax: "literal"
			}
		}
		region: {
			common:      true
			description: "The region to send data to."
			required:    false
			warnings: []
			type: string: {
				default: "us"
				enum: {
					us: "United States"
					eu: "Europe"
				}
				syntax: "literal"
			}
		}
		api: {
			description: "The API selected to send data to."
			required:    true
			warnings: []
			type: string: {
				enum: {
					events:  "Event API"
					metrics: "Metric API"
					logs:    "Log API"
				}
				syntax: "literal"
			}
		}
	}

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
	}

	telemetry: components.sinks.http.telemetry
}
