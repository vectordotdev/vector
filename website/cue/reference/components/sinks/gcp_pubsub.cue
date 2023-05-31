package metadata

components: sinks: gcp_pubsub: {
	title: "GCP PubSub"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["GCP"]
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
				max_bytes:    10_000_000
				max_events:   1000
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.gcp_pubsub

				interface: {
					socket: {
						api: {
							title: "GCP XML Interface"
							url:   urls.gcp_xml_interface
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

	configuration: base.components.sinks.gcp_pubsub.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "pubsub"

			policies: [
				{
					_action: "topics.get"
					required_for: ["healthcheck"]
				},
				{
					_action: "topics.publish"
					required_for: ["operation"]
				},
			]
		},
	]
}
