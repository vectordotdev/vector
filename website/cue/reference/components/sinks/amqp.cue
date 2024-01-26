package metadata

components: sinks: amqp: {
	title: "AMQP"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "dynamic"
		service_providers: ["AMQP"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      false
				common:       false
				timeout_secs: null
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip", "lz4", "snappy", "zstd"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
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
				can_verify_certificate: false
				can_verify_hostname:    false
				enabled_default:        false
				enabled_by_scheme:      false
			}
			to: components._amqp.features.send.to
		}
	}

	support: components._amqp.support

	configuration: base.components.sinks.amqp.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: components._amqp.how_it_works
}
