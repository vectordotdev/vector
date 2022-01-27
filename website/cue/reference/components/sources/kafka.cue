package metadata

components: sources: kafka: {
	title: "Kafka"

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

	configuration: {
		acknowledgements: configuration._acknowledgements
		auto_offset_reset: {
			common:      false
			description: """
				If offsets for consumer group do not exist, set them using this strategy. See the
				[librdkafka documentation](\(urls.librdkafka_config)) for the `auto.offset.reset` option for further
				clarification.
				"""
			required:    false
			type: string: {
				default: "largest"
				examples: ["smallest", "earliest", "beginning", "largest", "latest", "end", "error"]
			}
		}
		bootstrap_servers: components._kafka.configuration.bootstrap_servers
		commit_interval_ms: {
			common:      false
			description: "The frequency that the consumer offsets are committed (written) to offset storage."
			required:    false
			type: uint: {
				default: 5000
				examples: [5000, 10000]
				unit: "milliseconds"
			}
		}
		fetch_wait_max_ms: {
			common:      false
			description: "Maximum time the broker may wait to fill the response."
			required:    false
			type: uint: {
				default: 100
				examples: [50, 100]
				unit: "milliseconds"
			}
		}
		group_id: {
			description: "The consumer group name to be used to consume events from Kafka."
			required:    true
			type: string: {
				examples: ["consumer-group-name"]
			}
		}
		key_field: {
			common:      true
			description: "The log field name to use for the Kafka message key."
			required:    false
			type: string: {
				default: "message_key"
				examples: ["message_key"]
			}
		}
		topic_key: {
			common:      false
			description: "The log field name to use for the Kafka topic."
			required:    false
			type: string: {
				default: "topic"
				examples: ["topic"]
			}
		}
		partition_key: {
			common:      false
			description: "The log field name to use for the Kafka partition name."
			required:    false
			type: string: {
				default: "partition"
				examples: ["partition"]
			}
		}
		offset_key: {
			common:      false
			description: "The log field name to use for the Kafka offset."
			required:    false
			type: string: {
				default: "offset"
				examples: ["offset"]
			}
		}
		headers_key: {
			common:      false
			description: "The log field name to use for the Kafka headers."
			required:    false
			type: string: {
				default: "headers"
				examples: ["headers"]
			}
		}
		librdkafka_options: components._kafka.configuration.librdkafka_options
		sasl: {
			common:      false
			description: "Options for SASL/SCRAM authentication support."
			required:    false
			type: object: {
				examples: []
				options: {
					enabled: {
						common:      true
						description: "Enable SASL/SCRAM authentication to the remote (not supported on Windows at this time)."
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
		session_timeout_ms: {
			common:      false
			description: "The Kafka session timeout in milliseconds."
			required:    false
			type: uint: {
				default: 10000
				examples: [5000, 10000]
				unit: "milliseconds"
			}
		}
		socket_timeout_ms: components._kafka.configuration.socket_timeout_ms
		topics: {
			description: "The Kafka topics names to read events from. Regex is supported if the topic begins with `^`."
			required:    true
			type: array: items: type: string: {
				examples: ["^(prefix1|prefix2)-.+", "topic-1", "topic-2"]
			}
		}
	}

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
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
	}

	how_it_works: components._kafka.how_it_works
}
