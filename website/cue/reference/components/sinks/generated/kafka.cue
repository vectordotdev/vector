package metadata

generated: components: sinks: kafka: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized or compressed.
					"""
				required: false
				type: uint: unit: "bytes"
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: unit: "events"
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 1.0
					unit:    "seconds"
				}
			}
		}
	}
	bootstrap_servers: {
		description: """
			A comma-separated list of Kafka bootstrap servers.

			These are the servers in a Kafka cluster that a client should use to bootstrap its
			connection to the cluster, allowing discovery of all the other hosts in the cluster.

			Must be in the form of `host:port`, and comma-separated.
			"""
		required: true
		type: string: examples: ["10.14.22.123:9092,10.14.23.332:9092"]
	}
	compression: {
		description: "Supported compression types for Kafka."
		required:    false
		type: string: {
			default: "none"
			enum: {
				gzip:   "Gzip."
				lz4:    "LZ4."
				none:   "No compression."
				snappy: "Snappy."
				zstd:   "Zstandard."
			}
		}
	}
	dangerously_allow_unconfined_template_resolution: {
		description: """
			Disable all template confinement checks for this sink.

			**DANGEROUS — disables a security control.**

			Bypasses both startup validation and runtime confinement for every
			templated field on this sink. When enabled, a log producer that
			controls any field used in a template can write to arbitrary keys,
			paths, or routing destinations. This flag is a full opt-out: it
			disables confinement even for templates that have a usable static
			prefix.
			"""
		required: false
		type: bool: default: false
	}
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type:     _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
	}
	headers_key: {
		description: """
			The log field name to use for the Kafka headers.

			If omitted, no headers are written.
			"""
		required: false
		type: string: examples: ["headers"]
	}
	healthcheck_topic: {
		description: """
			The topic name to use for healthcheck. If omitted, `topic` is used.
			This option helps prevent healthcheck warnings when `topic` is templated.

			It is ignored when healthcheck is disabled.
			"""
		required: false
		type: string: {}
	}
	key_field: {
		description: """
			The log field name or tag key to use for the topic key.

			If the field does not exist in the log or in the tags, a blank value is used. If
			unspecified, the key is not sent.

			Kafka uses a hash of the key to choose the partition or uses round-robin if the record has
			no key.
			"""
		required: false
		type: string: examples: ["user_id", ".my_topic", "%my_topic"]
	}
	librdkafka_options: {
		description: """
			A map of advanced options to pass directly to the underlying `librdkafka` client.

			For more information on configuration options, see [Configuration properties][config_props_docs].

			[config_props_docs]: https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md
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
	message_timeout_ms: {
		description: "Local message timeout, in milliseconds."
		required:    false
		type: uint: {
			default: 300000
			examples: [150000, 450000]
			unit: "milliseconds"
		}
	}
	rate_limit_duration_secs: {
		description: "The time window used for the `rate_limit_num` option."
		required:    false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	rate_limit_num: {
		description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
		required:    false
		type: uint: {
			default: 9223372036854775807
			unit:    "requests"
		}
	}
	sasl: {
		description: "Configuration for SASL authentication when interacting with Kafka."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::kafka::KafkaSaslConfig>"]
	}
	socket_timeout_ms: {
		description: "Default timeout, in milliseconds, for network requests."
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
	topic: {
		description: "The Kafka topic name to write events to."
		required:    true
		type: string: {
			examples: ["topic-1234", "logs-{{unit}}-%Y-%m-%d"]
			syntax: "template"
		}
	}
}
