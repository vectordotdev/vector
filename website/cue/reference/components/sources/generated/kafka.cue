package metadata

generated: components: sources: kafka: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::SourceAcknowledgementsConfig"]
	}
	auto_offset_reset: {
		description: """
			If offsets for consumer group do not exist, set them using this strategy.

			See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for the `auto.offset.reset` option for further clarification.
			"""
		required: false
		type: string: {
			default: "largest"
			examples: ["smallest", "earliest", "beginning", "largest", "latest", "end", "error"]
		}
	}
	bootstrap_servers: {
		description: """
			A comma-separated list of Kafka bootstrap servers.

			These are the servers in a Kafka cluster that a client should use to bootstrap its connection to the cluster,
			allowing discovery of all the other hosts in the cluster.

			Must be in the form of `host:port`, and comma-separated.
			"""
		required: true
		type: string: examples: ["10.14.22.123:9092,10.14.23.332:9092"]
	}
	commit_interval_ms: {
		description: "The frequency that the consumer offsets are committed (written) to offset storage."
		required:    false
		type: uint: {
			default: 5000
			examples: [5000, 10000]
			unit: "milliseconds"
		}
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::2c0db0ef1f05c78303bd4391"]
	}
	decompression: {
		description: """
			Configuration for decompressing message payloads that were compressed by the producer.

			This applies to application-level compression, where the producer compressed each message
			payload before sending it. Compression negotiated at the Kafka protocol level is handled
			transparently by the underlying client library and does not require this option.

			Payloads are decompressed before `framing` and `decoding` are applied.
			"""
		required: false
		type: object: options: {
			algorithm: {
				description: "The decompression algorithm."
				required:    true
				type: string: enum: {
					gzip: """
						[Gzip][gzip] decompression.

						[gzip]: https://www.gzip.org/
						"""
					zlib: """
						[Zlib][zlib] decompression.

						[zlib]: https://zlib.net/
						"""
					zstd: """
						[Zstandard][zstd] decompression.

						[zstd]: https://facebook.github.io/zstd/
						"""
				}
			}
			dictionary_path: {
				description: """
					The path to a compression dictionary to use when decompressing payloads.

					The dictionary must be the same as the one used by the producer when compressing the
					payloads. Only supported with the `zstd` algorithm.
					"""
				required: false
				type: string: examples: ["/etc/vector/compression.dict"]
			}
		}
	}
	drain_timeout_ms: {
		description: """
			Timeout to drain pending acknowledgements during shutdown or a Kafka
			consumer group rebalance.

			When Vector shuts down or the Kafka consumer group revokes partitions from this
			consumer, wait a maximum of `drain_timeout_ms` for the source to
			process pending acknowledgements. Must be less than `session_timeout_ms`
			to ensure the consumer is not excluded from the group during a rebalance.

			Default value is half of `session_timeout_ms`.
			"""
		required: false
		type: uint: examples: [2500, 5000]
	}
	fetch_wait_max_ms: {
		description: "Maximum time the broker may wait to fill the response."
		required:    false
		type: uint: {
			default: 100
			examples: [50, 100]
			unit: "milliseconds"
		}
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::FramingConfig::2a4f9b813a8f495175f80ccf"]
	}
	group_id: {
		description: "The consumer group name to be used to consume events from Kafka."
		required:    true
		type: string: examples: ["consumer-group-name"]
	}
	headers_key: {
		description: """
			Overrides the name of the log field used to add the headers to each event.

			The value is the headers of the Kafka message itself.

			By default, `"headers"` is used.
			"""
		required: false
		type: string: {
			default: "headers"
			examples: ["headers"]
		}
	}
	key_field: {
		description: """
			Overrides the name of the log field used to add the message key to each event.

			The value is the message key of the Kafka message itself.

			By default, `"message_key"` is used.
			"""
		required: false
		type: string: {
			default: "message_key"
			examples: ["message_key"]
		}
	}
	librdkafka_options: {
		description: """
			Advanced options set directly on the underlying `librdkafka` client.

			See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.
			"""
		required: false
		type: object: {
			examples: [{
				"client.id":                "${ENV_VAR}"
				"fetch.error.backoff.ms":   "1000"
				"socket.send.buffer.bytes": "100"
			}]
			options: "*": {
				description: "A librdkafka configuration option."
				required:    true
				type: string: {}
			}
		}
	}
	metrics: {
		description: "Metrics (beta) configuration."
		required:    false
		type: object: options: topic_lag_metric: {
			description: "Expose topic lag metrics for all topics and partitions. Metric names are `kafka_consumer_lag`."
			required:    false
			type: bool: default: false
		}
	}
	offset_key: {
		description: """
			Overrides the name of the log field used to add the offset to each event.

			The value is the offset of the Kafka message itself.

			By default, `"offset"` is used.
			"""
		required: false
		type: string: {
			default: "offset"
			examples: [
				"offset"
			]
		}
	}
	partition_key: {
		description: """
			Overrides the name of the log field used to add the partition to each event.

			The value is the partition from which the Kafka message was consumed from.

			By default, `"partition"` is used.
			"""
		required: false
		type: string: {
			default: "partition"
			examples: ["partition"]
		}
	}
	sasl: {
		description: "Configuration for SASL authentication when interacting with Kafka."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::kafka::KafkaSaslConfig>"]
	}
	session_timeout_ms: {
		description: "The Kafka session timeout."
		required:    false
		type: uint: {
			default: 10000
			examples: [5000, 10000]
			unit: "milliseconds"
		}
	}
	socket_timeout_ms: {
		description: "Timeout for network requests."
		required:    false
		type: uint: {
			default: 60000
			examples: [30000, 60000]
			unit: "milliseconds"
		}
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	topic_key: {
		description: """
			Overrides the name of the log field used to add the topic to each event.

			The value is the topic from which the Kafka message was consumed from.

			By default, `"topic"` is used.
			"""
		required: false
		type: string: {
			default: "topic"
			examples: [
				"topic"
			]
		}
	}
	topics: {
		description: """
			The Kafka topics names to read events from.

			Regular expression syntax is supported if the topic begins with `^`.
			"""
		required: true
		type: array: items: type: string: examples: ["^(prefix1|prefix2)-.+", "topic-1", "topic-2"]
	}
}
