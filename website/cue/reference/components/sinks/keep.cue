package metadata

components: sinks: keep: {
	title: "Keep"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Keep"]
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
				max_events:   1000
				max_bytes:    1_048_576
				timeout_secs: 1.0
			}
			compression: {
				enabled: false
			}
			encoding: {
				enabled: true
				codec: enabled: false
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
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.keep

				interface: {
					socket: {
						api: {
							title: "Keep API"
							url:   urls.keep
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

	configuration: generated.components.sinks.keep.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body:  """
				1. Register for a free account at [platform.keephq.dev](\(urls.keep_platform))

				2. Go to providers tab and setup vector as a provider
				"""
		}

		configuration: {
			title: "Configuration"
			body: """
				In vector configuration source name needs to be "prometheus_alertmanager"
				"""
		}
	}
}
