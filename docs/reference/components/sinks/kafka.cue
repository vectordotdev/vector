package metadata

components: sinks: kafka: {
	title:       "Kafka"
	description: components._kafka.description

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		service_providers: ["AWS", "Confluent"]
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
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
					default: null
					enum: ["json", "text"]
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
			to: components._kafka.features.service
		}
	}

	support: components._kafka.support

	configuration: {
		bootstrap_servers: components._kafka.configuration.bootstrap_servers
		key_field: {
			description: "The log field name to use for the topic key. If unspecified, the key will be randomly generated. If the field does not exist on the log, a blank value will be used."
			required:    true
			warnings: []
			type: string: {
				examples: ["user_id"]
			}
		}
		librdkafka_options: components._kafka.configuration.librdkafka_options
		message_timeout_ms: {
			common:      false
			description: "Local message timeout."
			required:    false
			warnings: []
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
		socket_timeout_ms: components._kafka.configuration.socket_timeout_ms
		topic: {
			description: "The Kafka topic name to write events to."
			required:    true
			warnings: []
			type: string: {
				examples: ["topic-1234", "logs-{{unit}}-%Y-%m-%d"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: components._kafka.how_it_works

	telemetry: metrics: {
		vector_consumer_offset_updates_failed_total: {
			description: "The total number of failures to update a Kafka consumer offset."
			type:        "counter"
			tags: _component_tags
		}
		vector_events_failed_total: {
			description: "The total number of failures to read a Kafka message."
			type:        "counter"
			tags: _component_tags
		}
	}
}
