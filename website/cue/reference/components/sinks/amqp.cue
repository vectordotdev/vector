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

	configuration: {
		connection: {
			description: "Connection options for the AMQP sink."
			required:    true
			warnings: []
			type: object: {
				examples: []
				options: {
					connection_string: components._amqp.configuration.connection_string
				}
			}
		}
		exchange: {
			description: "The exchange to publish messages to."
			required:    true
			warnings: []
			type: string: {
				examples: ["message_exchange"]
				syntax: "literal"
			}
		}
		routing_key: {
			common:      false
			description: "Template use to generate a routing key which corresponds to a queue binding."
			required:    false
			warnings: []
			type: string: {
				examples: ["{{ field_a }}-{{ field_b }}"]
				syntax:  "literal"
				default: null
			}
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: components._amqp.how_it_works

	telemetry: metrics: {
		events_discarded_total:  components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
