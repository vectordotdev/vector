package metadata

components: sources: kafka: {
	title: "Kafka"

	features: {
		auto_generated:   true
		acknowledgements: true
		collect: {
			checkpoint: enabled: false
			tls: {
				enabled:                true
				can_verify_certificate: false
				can_verify_hostname:    false
				enabled_default:        false
				enabled_by_scheme:      false
			}
			from: components._kafka.features.collect.from
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
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	support: components._kafka.support

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.kafka.configuration

	output: logs: record: {
		description: "An individual Kafka record"
		fields: {
			message: {
				description: "The raw line from the Kafka record."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
				}
			}
			offset: {
				description: "The Kafka offset at the time the record was retrieved."
				required:    true
				type: uint: {
					examples: [100]
					unit: null
				}
			}
			partition: {
				description: "The Kafka partition that the record came from."
				required:    true
				type: string: {
					examples: ["partition"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["kafka"]
				}
			}
			timestamp: fields._current_timestamp & {
				description: "The timestamp encoded in the Kafka message or the current time if it cannot be fetched."
			}
			topic: {
				description: "The Kafka topic that the record came from."
				required:    true
				type: string: {
					examples: ["topic"]
				}
			}
		}
	}

	telemetry: metrics: {
		kafka_queue_messages:                components.sources.internal_metrics.output.metrics.kafka_queue_messages
		kafka_queue_messages_bytes:          components.sources.internal_metrics.output.metrics.kafka_queue_messages_bytes
		kafka_requests_total:                components.sources.internal_metrics.output.metrics.kafka_requests_total
		kafka_requests_bytes_total:          components.sources.internal_metrics.output.metrics.kafka_requests_bytes_total
		kafka_responses_total:               components.sources.internal_metrics.output.metrics.kafka_responses_total
		kafka_responses_bytes_total:         components.sources.internal_metrics.output.metrics.kafka_responses_bytes_total
		kafka_produced_messages_total:       components.sources.internal_metrics.output.metrics.kafka_produced_messages_total
		kafka_produced_messages_bytes_total: components.sources.internal_metrics.output.metrics.kafka_produced_messages_bytes_total
		kafka_consumed_messages_total:       components.sources.internal_metrics.output.metrics.kafka_consumed_messages_total
		kafka_consumed_messages_bytes_total: components.sources.internal_metrics.output.metrics.kafka_consumed_messages_bytes_total
		kafka_consumer_lag:                  components.sources.internal_metrics.output.metrics.kafka_consumer_lag
	}

	how_it_works: components._kafka.how_it_works
}
