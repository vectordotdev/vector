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
	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
	}

	support: components._kafka.support

	installation: {
		platform_name: null
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
		bootstrap_servers: components._kafka.configuration.bootstrap_servers
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
		topic_key: {
			common:      false
			description: "The log field name to use for the Kafka topic. If unspecified, the key would not be added to the log event."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["topic"]
			}
		}
		partition_key: {
			common:      false
			description: "The log field name to use for the Kafka partition name. If unspecified, the key would not be added to the log event."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["partition"]
			}
		}
		offset_key: {
			common:      false
			description: "The log field name to use for the Kafka offset. If unspecified, the key would not be added to the log event."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["offset"]
			}
		}
		librdkafka_options: components._kafka.configuration.librdkafka_options
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
		socket_timeout_ms: components._kafka.configuration.socket_timeout_ms
		topics: {
			description: "The Kafka topics names to read events from. Regex is supported if the topic begins with `^`.\n"
			required:    true
			warnings: []
			type: array: items: type: string: examples: ["^(prefix1|prefix2)-.+", "topic-1", "topic-2"]
		}
	}

	output: logs: record: {
		description: "An individual Kafka record"
		fields: {
			message: {
				description: "The raw line from the Kafka record."
				required:    true
				type: string: examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
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
				type: string: examples: ["partition"]
			}
			timestamp: fields._current_timestamp & {
				description: "If the [Splunk HEC event endpoint](\(urls.splunk_hec_event_endpoint)) is used then the value of the `time` field will be used. If the [Splunk HEC raw endpoint](\(urls.splunk_hec_raw_endpoint)) is used, then the current time the event was received will be used."
			}
			topic: {
				description: "The Kafka topic that the record came from."
				required:    true
				type: string: examples: ["topic"]
			}
		}
	}

	how_it_works: components._kafka.how_it_works
}
