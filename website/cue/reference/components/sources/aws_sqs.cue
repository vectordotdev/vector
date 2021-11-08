package metadata

components: sources: aws_sqs: components._aws & {
	title: "AWS SQS"

//	features: {
//		collect: {
//			checkpoint: enabled: false
//			tls: {
//				enabled:                true
//				can_enable:             true
//				can_verify_certificate: false
//				can_verify_hostname:    false
//				enabled_default:        false
//			}
//			from: components._kafka.features.collect.from
//		}
//		multiline: enabled: false
//		codecs: {
//			enabled:         true
//			default_framing: "bytes"
//		}
//	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: [
			"""
				The AWS SQS source requires an SQS queue.
			""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		acknowledgements: configuration._acknowledgements
		poll_secs: {
			common:      true
			description: "How long to wait when polling SQS for new messages. 0-20 seconds"
			required:    false
			warnings: []
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
		client_concurrency: {
			common: true
			description: "How many clients are receiving / acking SQS messages. Increasing may allow higher throughput."
			required: false
			warnings: []
			type: uint: {
				default: "1 per CPU core"
				unit: "# of clients"
			}
		}
		queue_url: {
			description: "The URL of the SQS queue to receive bucket notifications from."
			required:    true
			warnings: []
			type: string: {
				examples: ["https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"]
				syntax: "literal"
			}
		}
	}

	output: logs: record: {
		description: "An individual SQS record"
		fields: {
			message: {
				description: "The raw message from the SQS record."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					syntax: "literal"
				}
			}
		}
	}

	telemetry: metrics: {
		events_failed_total:                  components.sources.internal_metrics.output.metrics.events_failed_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		consumer_offset_updates_failed_total: components.sources.internal_metrics.output.metrics.consumer_offset_updates_failed_total
		kafka_queue_messages:                 components.sources.internal_metrics.output.metrics.kafka_queue_messages
		kafka_queue_messages_bytes:           components.sources.internal_metrics.output.metrics.kafka_queue_messages_bytes
		kafka_requests_total:                 components.sources.internal_metrics.output.metrics.kafka_requests_total
		kafka_requests_bytes_total:           components.sources.internal_metrics.output.metrics.kafka_requests_bytes_total
		kafka_responses_total:                components.sources.internal_metrics.output.metrics.kafka_responses_total
		kafka_responses_bytes_total:          components.sources.internal_metrics.output.metrics.kafka_responses_bytes_total
		kafka_produced_messages_total:        components.sources.internal_metrics.output.metrics.kafka_produced_messages_total
		kafka_produced_messages_bytes_total:  components.sources.internal_metrics.output.metrics.kafka_produced_messages_bytes_total
		kafka_consumed_messages_total:        components.sources.internal_metrics.output.metrics.kafka_consumed_messages_total
		kafka_consumed_messages_bytes_total:  components.sources.internal_metrics.output.metrics.kafka_consumed_messages_bytes_total
		processed_bytes_total:                components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:               components.sources.internal_metrics.output.metrics.processed_events_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
	}

	how_it_works: components._kafka.how_it_works
}
