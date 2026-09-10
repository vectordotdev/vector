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
		type: object: options: {
			avro: {
				description:   "Apache Avro-specific encoder options."
				relevant_when: "codec = \"avro\""
				required:      true
				type:          _schemaDefinitions["codecs::decoding::format::avro::AvroDeserializerOptions"]
			}
			codec: {
				description: "The codec to use for decoding events."
				required:    false
				type: string: {
					default: "bytes"
					enum: {
						avro: """
															Decodes the raw bytes as an [Apache Avro][apache_avro] message.

															[apache_avro]: https://avro.apache.org/
															"""
						bytes: "Uses the raw bytes as-is."
						gelf: """
															Decodes the raw bytes as a [GELF][gelf] message.

															This codec is experimental for the following reason:

															The GELF specification is more strict than the actual Graylog receiver.
															Vector's decoder adheres more strictly to the GELF spec, with
															the exception that some characters such as `@` are allowed in field names.

															Other GELF codecs, such as Loki's, use a [Go SDK][implementation] that is maintained
															by Graylog and is much more relaxed than the GELF spec.

															Going forward, Vector will use the [Go SDK][implementation] as the reference implementation, which means
															the codec may continue to relax the enforcement of the specification.

															[gelf]: https://docs.graylog.org/docs/gelf
															[implementation]: https://github.com/Graylog2/go-gelf/blob/v2/gelf/reader.go
															"""
						influxdb: """
															Decodes the raw bytes as an [Influxdb Line Protocol][influxdb] message.

															[influxdb]: https://docs.influxdata.com/influxdb/cloud/reference/syntax/line-protocol
															"""
						json: """
															Decodes the raw bytes as [JSON][json].

															[json]: https://www.json.org/
															"""
						native: """
															Decodes the raw bytes as [native Protocol Buffers format][vector_native_protobuf].

															This decoder can output all types of events: logs, metrics, and traces.

															This codec is **[experimental][experimental]**.

															[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
															[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
															"""
						native_json: """
															Decodes the raw bytes as [native JSON format][vector_native_json].

															This decoder can output all types of events: logs, metrics, and traces.

															This codec is **[experimental][experimental]**.

															[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
															[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
															"""
						otlp: """
															Decodes the raw bytes as [OTLP (OpenTelemetry Protocol)][otlp] protobuf format.

															This decoder handles the three OTLP signal types: logs, metrics, and traces.
															It automatically detects which type of OTLP message is being decoded.

															[otlp]: https://opentelemetry.io/docs/specs/otlp/
															"""
						protobuf: """
															Decodes the raw bytes as [protobuf][protobuf].

															[protobuf]: https://protobuf.dev/
															"""
						syslog: """
															Decodes the raw bytes as a Syslog message.

															Decodes either as the [RFC 3164][rfc3164]-style format ("old" style) or the
															[RFC 5424][rfc5424]-style format ("new" style, includes structured data).

															[rfc3164]: https://www.ietf.org/rfc/rfc3164.txt
															[rfc5424]: https://www.ietf.org/rfc/rfc5424.txt
															"""
						vrl: """
															Decodes the raw bytes as a string and passes them as input to a [VRL][vrl] program.

															[vrl]: https://vector.dev/docs/reference/vrl
															"""
					}
				}
			}
			gelf: {
				description:   "GELF-specific decoding options."
				relevant_when: "codec = \"gelf\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::format::gelf::GelfDeserializerOptions"]
			}
			influxdb: {
				description:   "Influxdb-specific decoding options."
				relevant_when: "codec = \"influxdb\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::format::influxdb::InfluxdbDeserializerOptions"]
			}
			json: {
				description:   "JSON-specific decoding options."
				relevant_when: "codec = \"json\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::format::influxdb::InfluxdbDeserializerOptions"]
			}
			native_json: {
				description:   "Vector's native JSON-specific decoding options."
				relevant_when: "codec = \"native_json\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::format::influxdb::InfluxdbDeserializerOptions"]
			}
			protobuf: {
				description:   "Protobuf-specific decoding options."
				relevant_when: "codec = \"protobuf\""
				required:      false
				type: object: options: {
					desc_file: {
						description: """
																The path to the protobuf descriptor set file.

																This file is the output of `protoc -I <include path> -o <desc output path> <proto>`.

																For more information, see [How Buf images work](https://buf.build/docs/reference/images/#how-buf-images-work).
																"""
						required: false
						type: string: default: ""
					}
					message_type: {
						description: "The name of the message type to use for serializing."
						required:    false
						type: string: {
							default: ""
							examples: ["package.Message"]
						}
					}
					use_json_names: {
						description: """
																Use JSON field names (camelCase) instead of protobuf field names (snake_case).

																When enabled, the deserializer will output fields using their JSON names as defined
																in the `.proto` file (for example, `jobDescription` instead of `job_description`).

																This is useful when working with data that needs to be converted to JSON or
																when interfacing with systems that use JSON naming conventions.
																"""
						required: false
						type: bool: default: false
					}
				}
			}
			signal_types: {
				description: """
					Signal types to attempt parsing, in priority order.

					The deserializer tries to parse signals in the specified order. This allows you to optimize
					performance when you know the expected signal types. For example, if you only receive
					traces, set this to `["traces"]` to avoid attempting to parse as logs or metrics first.

					If not specified, defaults to trying all types in this order: logs, metrics, traces.
					Duplicate signal types are automatically removed while preserving order.
					"""
				relevant_when: "codec = \"otlp\""
				required:      false
				type: array: {
					default: ["logs", "metrics", "traces"]
					items: type: string: enum: {
						logs:    "OTLP logs signal (ExportLogsServiceRequest)"
						metrics: "OTLP metrics signal (ExportMetricsServiceRequest)"
						traces:  "OTLP traces signal (ExportTraceServiceRequest)"
					}
				}
			}
			syslog: {
				description:   "Syslog-specific decoding options."
				relevant_when: "codec = \"syslog\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::format::influxdb::InfluxdbDeserializerOptions"]
			}
			vrl: {
				description:   "VRL-specific decoding options."
				relevant_when: "codec = \"vrl\""
				required:      true
				type:          _schemaDefinitions["codecs::decoding::format::vrl::VrlDeserializerOptions"]
			}
		}
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
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited decoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type:          _schemaDefinitions["codecs::decoding::framing::character_delimited::CharacterDelimitedDecoderOptions"]
			}
			chunked_gelf: {
				description:   "Options for the chunked GELF decoder."
				relevant_when: "method = \"chunked_gelf\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::framing::chunked_gelf::ChunkedGelfDecoderOptions"]
			}
			length_delimited: {
				description:   "Options for the length delimited decoder."
				relevant_when: "method = \"length_delimited\""
				required:      true
				type:          _schemaDefinitions["codecs::common::length_delimited::LengthDelimitedCoderOptions"]
			}
			max_frame_length: {
				description:   "Maximum frame length"
				relevant_when: "method = \"varint_length_delimited\""
				required:      false
				type: uint: default: 8388608
			}
			method: {
				description: "The framing method."
				required:    false
				type: string: {
					default: "bytes"
					enum: {
						bytes:               "Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments)."
						character_delimited: "Byte frames which are delimited by a chosen character."
						chunked_gelf: """
															Byte frames which are chunked GELF messages.

															[chunked_gelf]: https://go2docs.graylog.org/current/getting_in_log_data/gelf.html
															"""
						length_delimited:  "Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length."
						newline_delimited: "Byte frames which are delimited by a newline character."
						octet_counting: """
															Byte frames according to the [octet counting][octet_counting] format.

															[octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
															"""
						varint_length_delimited: """
															Byte frames which are prefixed by a varint indicating the length.
															This is compatible with protobuf's length-delimited encoding.
															"""
					}
				}
			}
			newline_delimited: {
				description:   "Options for the newline delimited decoder."
				relevant_when: "method = \"newline_delimited\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::framing::newline_delimited::NewlineDelimitedDecoderOptions"]
			}
			octet_counting: {
				description:   "Options for the octet counting decoder."
				relevant_when: "method = \"octet_counting\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::framing::octet_counting::OctetCountingDecoderOptions"]
			}
		}
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
