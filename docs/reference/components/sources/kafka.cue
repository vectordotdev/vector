package metadata

components: sources: kafka: {
	title:       "Kafka"
	description: "[Apache Kafka][urls.kafka] is an open-source project for a distributed publish-subscribe messaging system rethought as a distributed commit log. Kafka stores messages in topics that are partitioned and replicated across multiple brokers in a cluster. Producers send messages to topics from which consumers read. These features make it an excellent candidate for durably storing logs and metrics data."

	features: {
		collect: {
			checkpoint: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: false
				enabled_default:        false
			}
		}
		multiline: enabled: false
	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
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

		requirements: [
			"""
				[Kafka][urls.kafka] version `>= 0.8` is required.
				""",
		]
		warnings: []
		notices: []
	}

	configuration: {
		auto_offset_reset: {
			common:      false
			description: "If offsets for consumer group do not exist, set them using this strategy. [librdkafka documentation][urls.librdkafka_config] for `auto.offset.reset` option for explanation."
			required:    false
			warnings: []
			type: string: {
				default: "largest"
				examples: ["smallest", "earliest", "beginning", "largest", "latest", "end", "error"]
			}
		}
		bootstrap_servers: {
			description: "A comma-separated list of host and port pairs that are the addresses of the Kafka brokers in a \"bootstrap\" Kafka cluster that a Kafka client connects to initially to bootstrap itself."
			required:    true
			warnings: []
			type: string: {
				examples: ["10.14.22.123:9092,10.14.23.332:9092"]
			}
		}
		commit_interval_ms: {
			common:      false
			description: "The frequency that the consumer offsets are committed (written) to offset storage.\n"
			required:    false
			warnings: []
			type: uint: {
				default: 5000
				examples: [5000, 10000]
				unit: "milliseconds"
			}
		}
		fetch_wait_max_ms: {
			common:      false
			description: "Maximum time the broker may wait to fill the response.\n"
			required:    false
			warnings: []
			type: uint: {
				default: 100
				examples: [50, 100]
				unit: "milliseconds"
			}
		}
		group_id: {
			description: "The consumer group name to be used to consume events from Kafka.\n"
			required:    true
			warnings: []
			type: string: {
				examples: ["consumer-group-name"]
			}
		}
		key_field: {
			common:      true
			description: "The log field name to use for the Kafka message key. If unspecified, the key would not be added to the log event. If the message has null key, then this field would not be added to the log event."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["message_key"]
			}
		}
		librdkafka_options: {
			common:      false
			description: "Advanced options. See [librdkafka documentation][urls.librdkafka_config] for details.\n"
			required:    false
			warnings: []
			type: object: {
				examples: [{"client.id": "${ENV_VAR}"}, {"fetch.error.backoff.ms": "1000"}, {"socket.send.buffer.bytes": "100"}]
				options: {}
			}
		}
		sasl: {
			common:      false
			description: "Options for SASL/SCRAM authentication support."
			required:    false
			warnings: []
			type: object: {
				examples: []
				options: {
					enabled: {
						common:      true
						description: "Enable SASL/SCRAM authentication to the remote. (Not supported on Windows at this time.)"
						required:    false
						warnings: []
						type: bool: default: null
					}
					mechanism: {
						common:      true
						description: "The Kafka SASL/SCRAM mechanisms."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["SCRAM-SHA-256", "SCRAM-SHA-512"]
						}
					}
					password: {
						common:      true
						description: "The Kafka SASL/SCRAM authentication password."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["password"]
						}
					}
					username: {
						common:      true
						description: "The Kafka SASL/SCRAM authentication username."
						required:    false
						warnings: []
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
			description: "The Kafka session timeout in milliseconds.\n"
			required:    false
			warnings: []
			type: uint: {
				default: 10000
				examples: [5000, 10000]
				unit: "milliseconds"
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
		topics: {
			description: "The Kafka topics names to read events from. Regex is supported if the topic begins with `^`.\n"
			required:    true
			warnings: []
			type: array: items: type: string: examples: ["^(prefix1|prefix2)-.+", "topic-1", "topic-2"]
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
}
