package metadata

components: sources: iggy: {
	title: "Iggy"

	features: {
		auto_generated:   true
		acknowledgements: true
		collect: {
			checkpoint: enabled: false
			from: components._iggy.features.collect.from
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
		commonly_used: false
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	support: components._iggy.support

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.iggy.configuration

	output: {
		logs: record: {
			description: "An individual Iggy message."
			fields: {
				message: {
					description: "The raw payload from the Iggy message."
					required:    true
					type: string: {
						examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					}
				}
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["iggy"]
					}
				}
				stream: {
					description: "The Iggy stream the message was consumed from."
					required:    true
					type: string: {
						examples: ["vector"]
					}
				}
				topic: {
					description: "The Iggy topic the message was consumed from."
					required:    true
					type: string: {
						examples: ["logs"]
					}
				}
			}
		}
		metrics: "": {
			description: "Metric events that may be emitted by this source."
		}
		traces: "": {
			description: "Trace events that may be emitted by this source."
		}
	}

	telemetry: metrics: {
		iggy_consumer_committed_offset: components.sources.internal_metrics.output.metrics.iggy_consumer_committed_offset
		iggy_consumer_lag_messages:     components.sources.internal_metrics.output.metrics.iggy_consumer_lag_messages
		iggy_consumer_polled_offset:    components.sources.internal_metrics.output.metrics.iggy_consumer_polled_offset
	}

	how_it_works: components._iggy.how_it_works
}
