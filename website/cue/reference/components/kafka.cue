package metadata

components: _kafka: {
	features: {
		collect: from: {
			service: services.kafka
			interface: {
				socket: {
					api: {
						title: "Kafka protocol"
						url:   urls.kafka_protocol
					}
					direction: "incoming"
					port:      9093
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
		}

		send: to: {
			service: services.kafka
			interface: {
				socket: {
					api: {
						title: "Kafka protocol"
						url:   urls.kafka_protocol
					}
					direction: "outgoing"
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
		}
	}

	support: {
		requirements: []
		notices: []
		warnings: []
	}

	configuration: {
		bootstrap_servers: {
			description: "A comma-separated list of host and port pairs that are the addresses of the Kafka brokers in a \"bootstrap\" Kafka cluster that a Kafka client connects to initially to bootstrap itself."
			required:    true
			type: string: {
				examples: ["10.14.22.123:9092,10.14.23.332:9092"]
			}
		}
		librdkafka_options: {
			common:      false
			description: "Advanced options. See [librdkafka documentation](\(urls.librdkafka_config)) for details.\n"
			required:    false
			type: object: {
				examples: [
					{
						"client.id":                "${ENV_VAR}"
						"fetch.error.backoff.ms":   "1000"
						"socket.send.buffer.bytes": "100"
					},
				]
				options: {}
			}
		}
		socket_timeout_ms: {
			common:      false
			description: "Default timeout for network requests.\n"
			required:    false
			type: uint: {
				default: 60000
				examples: [30000, 60000]
				unit: "milliseconds"
			}
		}
	}

	how_it_works: {
		librdkafka: {
			title: "librdkafka"
			body:  """
				The `kafka` sink uses [`librdkafka`](\(urls.librdkafka)) under the hood. This
				is a battle-tested, high performance, and reliable library that facilitates
				communication with Kafka. As Vector produces static MUSL builds,
				this dependency is packaged with Vector, meaning you do not need to install it.
				"""
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
	}
}
