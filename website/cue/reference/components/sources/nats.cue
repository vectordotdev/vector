package metadata

components: sources: nats: {
	title: "NATS"

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: components._nats.features.collect.from
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
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	support: components._nats.support

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.nats.configuration

	output: logs: record: {
		description: "An individual NATS record."
		fields: {
			message: {
				description: "The raw line from the NATS message."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["nats"]
				}
			}
			subject: {
				description: "The subject from the NATS message."
				required:    true
				type: string: {
					examples: ["nats.subject"]
				}
			}
		}
	}

	how_it_works: components._nats.how_it_works
}
