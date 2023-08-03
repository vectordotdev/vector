package metadata

components: sinks: honeycomb: {
	title: "Honeycomb"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Honeycomb"]
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
				max_bytes:    100_000
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: enabled: false
			to: {
				service: services.honeycomb

				interface: {
					socket: {
						api: {
							title: "Honeycomb batch events API"
							url:   urls.honeycomb_batch
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

	configuration: base.components.sinks.honeycomb.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body:  """
				1. Register for a free account at [honeycomb.io](\(urls.honeycomb_signup))

				2. Once registered, create a new dataset and when presented with log shippers select the
				curl option and use the key provided with the curl example.
				"""
		}
	}
}
