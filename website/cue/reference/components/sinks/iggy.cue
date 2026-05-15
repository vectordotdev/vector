package metadata

components: sinks: iggy: {
	title: "Iggy"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
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
			request: enabled: false
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: components._iggy.features.send.to
		}
	}

	support: components._iggy.support

	configuration: generated.components.sinks.iggy.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: components._iggy.how_it_works
}
