package metadata

components: sources: opcua: {
	title: "opcua"

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: components._opcua.features.collect.from
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
		}
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "bytes"
		}
	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	support: components._opcua.support

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.opcua.configuration

	how_it_works: components._opcua.how_it_works
}
