package metadata

components: sources: nats: {
	title: "NATS"

	features: {
		collect: {
			checkpoint: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: false
				can_verify_hostname:    false
				enabled_default:        false
			}
			from: components._nats.features.collect.from
		}
		multiline: enabled: false
	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	support: components._nats.support

	installation: {
		platform_name: null
	}

	configuration: {
		url: components._nats.configuration.url
		subject: components._nats.configuration.subject
		name: components._nats.configuration.name
		queue: {
			common:      false
			description: "NATS Queue Group to join"
			required:    false
			type: string: {
				default: "vector"
				examples: ["foo", "API Name Option Example"]
				syntax: "literal"
			}
		}
    }

	output: logs: record: {
		description: "An individual NATS record"
		fields: {
			message: {
				description: "The raw line from the NATS message."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					syntax: "literal"
				}
			}
		}
	}

	telemetry: metrics: {
		events_discarded_total:  components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
		processed_bytes_total:   components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:  components.sources.internal_metrics.output.metrics.processed_events_total
		send_errors_total:       components.sources.internal_metrics.output.metrics.send_errors_total
	}

	how_it_works: components._nats.how_it_works
}
