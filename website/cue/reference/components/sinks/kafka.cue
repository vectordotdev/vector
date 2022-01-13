package metadata

components: sinks: kafka: {
	title: "Kafka"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "dynamic"
		service_providers: ["AWS", "Confluent"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       true
				max_events:   null
				max_bytes:    null
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
					enum: ["json", "text", "ndjson"]
				}
			}
			request: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: false
				can_verify_hostname:    false
				enabled_default:        false
			}
			to: components._kafka.features.send.to
		}
	}

	support: components._kafka.support

	configuration: {
		bootstrap_servers: components._kafka.configuration.bootstrap_servers
		key_field: {
			common:      true
			description: "The log field name or tags key to use for the topic key. If the field does not exist in the log or in tags, a blank value will be used. If unspecified, the key is not sent. Kafka uses a hash of the key to choose the partition or uses round-robin if the record has no key."
			required:    false
			type: string: {
				default: null
				examples: ["user_id"]
			}
		}
		librdkafka_options: components._kafka.configuration.librdkafka_options
		message_timeout_ms: {
			common:      false
			description: "Local message timeout."
			required:    false
			type: uint: {
				default: 300000
				examples: [150000, 450000]
				unit: null
			}
		}
		sasl: {
			common:      false
			description: "Options for SASL/SCRAM authentication support."
			required:    false
			type: object: {
				examples: []
				options: {
					enabled: {
						common:      true
						description: "Enable SASL/SCRAM authentication to the remote. (Not supported on Windows at this time.)"
						required:    false
						type: bool: default: null
					}
					mechanism: {
						common:      true
						description: "The Kafka SASL/SCRAM mechanisms."
						required:    false
						type: string: {
							default: null
							examples: ["SCRAM-SHA-256", "SCRAM-SHA-512"]
						}
					}
					password: {
						common:      true
						description: "The Kafka SASL/SCRAM authentication password."
						required:    false
						type: string: {
							default: null
							examples: ["password"]
						}
					}
					username: {
						common:      true
						description: "The Kafka SASL/SCRAM authentication username."
						required:    false
						type: string: {
							default: null
							examples: ["username"]
						}
					}
				}
			}
		}
		socket_timeout_ms: components._kafka.configuration.socket_timeout_ms
		topic: {
			description: "The Kafka topic name to write events to."
			required:    true
			type: string: {
				examples: ["topic-1234", "logs-{{unit}}-%Y-%m-%d"]
				syntax: "template"
			}
		}
		headers_key: {
			common:      false
			description: "The log field name to use for the Kafka headers. If omitted, no headers will be written."
			required:    false
			type: string: {
				default: null
				examples: ["headers"]
			}
		}
	}

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
	}

	how_it_works: components._kafka.how_it_works

	telemetry: metrics: {
		component_sent_events_total:         components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total:    components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		component_sent_bytes_total:          components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		events_discarded_total:              components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total:             components.sources.internal_metrics.output.metrics.processing_errors_total
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
	}
}
