package metadata

components: sinks: vector: {
	_port: 6000

	title: "Vector"

	description: """
		Sends data to another downstream Vector instance via the Vector source.
		"""

	classes: {
		delivery:      "best_effort"
		development:   "stable"
		egress_method: "batch"
		service_providers: []
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
			compression: enabled: false
			encoding: enabled:    false
			proxy: enabled:       true
			request: {
				enabled: true
				headers: false
			}

			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      false // sink allows both scheme or `enabled` to be used
			}
			to: {
				service: services.vector

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
		notices: []
	}

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

	configuration: generated.components.sinks.vector.configuration

	how_it_works: {
		authentication: {
			title: "Authentication"
			body: """
				You can require authentication between Vector instances by setting the `auth`
				option, which supports the `bearer`, `basic`, and `custom` strategies.

				The sink sends the credentials with every request, including the health check.
				The downstream `vector` source must be configured with the same credentials or it
				rejects the request.

				Because the token is a normal configuration value, you can pull it from a secrets
				backend, for example `token: "SECRET[backend.vector_token]"`. The value is read
				when the configuration loads and again on reload, so rotate it by reloading
				Vector.

				Enable TLS when using authentication so the credentials are not sent in
				plaintext.
				"""
		}
	}

	telemetry: metrics: {}
}
