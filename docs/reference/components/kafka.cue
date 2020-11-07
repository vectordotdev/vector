package metadata

components: _kafka: {
	description: "[Apache Kafka][urls.kafka] is an open-source project for a distributed publish-subscribe messaging system rethought as a distributed commit log. Kafka stores messages in topics that are partitioned and replicated across multiple brokers in a cluster. Producers send messages to topics from which consumers read. These features make it an excellent candidate for durably storing logs and metrics data."

	features: {
		service: {
			name:     "Kafka"
			thing:    "\(name) topics"
			url:      urls.kafka
			versions: ">= 0.8"

			interface: {
				socket: {
					api: {
						title: "Influx HTTP API"
						url:   urls.influxdb_http_api_v2
					}
					direction: "outgoing"
					protocols: ["http"]
					ssl: "optional"
				}
			}
		}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		bootstrap_servers: {
			description: "A comma-separated list of host and port pairs that are the addresses of the Kafka brokers in a \"bootstrap\" Kafka cluster that a Kafka client connects to initially to bootstrap itself."
			required:    true
			warnings: []
			type: string: {
				examples: ["10.14.22.123:9092,10.14.23.332:9092"]
			}
		}
		librdkafka_options: {
			common:      false
			description: "Advanced options. See [librdkafka documentation][urls.librdkafka_config] for details.\n"
			required:    false
			warnings: []
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
			warnings: []
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
			body: """
				The `kafka` sink uses [`librdkafka`][urls.librdkafka] under the hood. This
				is a battle tested, high performance, and reliable library that facilitates
				communication with Kafka. And because Vector produces static MUSL builds,
				this dependency is packaged with Vector, meaning you do not need to install it.
				"""
		}
	}

	telemetry: metrics: {
		vector_consumer_offset_updates_failed_total: {
			description: "The total number of failures to update a Kafka consumer offset."
			type:        "counter"
			tags:        _component_tags
		}
		vector_events_failed_total: {
			description: "The total number of failures to read a Kafka message."
			type:        "counter"
			tags:        _component_tags
		}
	}
}
