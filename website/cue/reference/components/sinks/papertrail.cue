package metadata

components: sinks: papertrail: {
	title: "Papertrail"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		service_providers: ["Papertrail"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			send_buffer_bytes: enabled: true
			keepalive: enabled:         true
			request: enabled:           false
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      false
			}
			to: {
				service: services.papertrail

				interface: {
					socket: {
						api: {
							title: "Syslog"
							url:   urls.syslog
						}
						direction: "outgoing"
						protocols: ["tcp"]
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

	configuration: base.components.sinks.papertrail.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body: """
				1. Register for a free account at [Papertrailapp.com](https://papertrailapp.com/signup?plan=free)

				2. [Create a Log Destination](https://papertrailapp.com/destinations/new) to get a Log Destination
				and ensure that TCP is enabled.

				3. Set the log destination as the `endpoint` option and start shipping your logs!
				"""
		}
	}
}
