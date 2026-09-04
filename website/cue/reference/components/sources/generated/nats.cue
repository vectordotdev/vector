package metadata

generated: components: sources: nats: configuration: {
	auth: {
		description: "Configuration of the authentication strategy when interacting with NATS."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::nats::NatsAuthConfig>"]
	}
	connection_name: {
		description: """
			A [name][nats_connection_name] assigned to the NATS connection.

			[nats_connection_name]: https://docs.nats.io/using-nats/developer/connecting/name
			"""
		required: true
		type: string: examples: [
			"vector"
		]
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
	jetstream: {
		description: "Configuration for NATS JetStream."
		required:    false
		type: object: options: {
			batch_config: {
				description: """
					Batch settings for a JetStream pull consumer.

					By default, messages are pulled in batches of up to 200.
					Each pull request expires after 30 seconds if not fulfilled.
					There is no explicit maximum byte size per batch unless specified.

					**Note:** These defaults follow the `async-nats` crate’s `StreamBuilder`.
					"""
				required: false
				type: object: options: {
					batch: {
						description: "The maximum number of messages to pull in a single batch."
						required:    false
						type: uint: default: 200
					}
					max_bytes: {
						description: """
																The maximum total byte size for a batch. The pull request will be
																fulfilled when either `size` or `max_bytes` is reached.
																"""
						required: false
						type: uint: default: 0
					}
				}
			}
			consumer: {
				description: "The name of the durable consumer to pull from."
				required:    true
				type: string: {}
			}
			stream: {
				description: "The name of the stream to bind to."
				required:    true
				type: string: {}
			}
		}
	}
	queue: {
		description: "The NATS queue group to join."
		required:    false
		type: string: {}
	}
	subject: {
		description: """
			The NATS [subject][nats_subject] to pull messages from.

			[nats_subject]: https://docs.nats.io/nats-concepts/subjects
			"""
		required: true
		type: string: examples: ["foo", "time.us.east", "time.*.east", "time.>", ">"]
	}
	subject_key_field: {
		description: "The `NATS` subject key."
		required:    false
		type: string: default: "subject"
	}
	subscriber_capacity: {
		description: """
			The buffer capacity of the underlying NATS subscriber.

			This value determines how many messages the NATS subscriber buffers
			before incoming messages are dropped.

			See the [async_nats documentation][async_nats_subscription_capacity] for more information.

			[async_nats_subscription_capacity]: https://docs.rs/async-nats/latest/async_nats/struct.ConnectOptions.html#method.subscription_capacity
			"""
		required: false
		type: uint: default: 65536
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	url: {
		description: """
			The NATS URL to connect to.

			The URL takes the form of `nats://server:port`.
			If the port is not specified it defaults to 4222.
			"""
		required: true
		type: string: examples: ["nats://demo.nats.io", "nats://127.0.0.1:4242", "nats://localhost:4222,nats://localhost:5222,nats://localhost:6222"]
	}
}
