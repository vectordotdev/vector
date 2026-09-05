package metadata

generated: components: sources: pulsar: configuration: {
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
	auth: {
		description: "Authentication configuration."
		required:    false
		type: object: options: {
			name: {
				description: """
					Basic authentication name/username.

					This can be used either for basic authentication (username/password) or JWT authentication.
					When used for JWT, the value should be `token`.
					"""
				required: true
				type: string: examples: ["${PULSAR_NAME}", "name123"]
			}
			oauth2: {
				description: "OAuth2-specific authentication configuration."
				required:    true
				type: object: options: {
					audience: {
						description: "The OAuth2 audience."
						required:    false
						type: string: examples: ["${OAUTH2_AUDIENCE}", "pulsar"]
					}
					credentials_url: {
						description: """
																The credentials URL.

																A data URL is also supported.
																"""
						required: true
						type: string: examples: ["${OAUTH2_CREDENTIALS_URL}", "file:///oauth2_credentials", "data:application/json;base64,cHVsc2FyCg=="]
					}
					issuer_url: {
						description: "The issuer URL."
						required:    true
						type: string: examples: ["${OAUTH2_ISSUER_URL}", "https://oauth2.issuer"]
					}
					scope: {
						description: "The OAuth2 scope."
						required:    false
						type: string: examples: ["${OAUTH2_SCOPE}", "admin"]
					}
				}
			}
			token: {
				description: """
					Basic authentication password/token.

					This can be used either for basic authentication (username/password) or JWT authentication.
					When used for JWT, the value should be the signed JWT, in the compact representation.
					"""
				required: true
				type: string: examples: ["${PULSAR_TOKEN}", "123456789"]
			}
		}
	}
	batch_size: {
		description: "Max count of messages in a batch."
		required:    false
		type: uint: {}
	}
	consumer_name: {
		description: "The Pulsar consumer name."
		required:    false
		type: string: examples: ["consumer-name"]
	}
	dead_letter_queue_policy: {
		description: "Dead Letter Queue policy configuration."
		required:    false
		type: object: options: {
			dead_letter_topic: {
				description: "Name of the dead letter topic where the failing messages will be sent."
				required:    true
				type: string: {}
			}
			max_redeliver_count: {
				description: "Maximum number of times that a message will be redelivered before being sent to the dead letter queue."
				required:    true
				type: uint: {}
			}
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
	endpoint: {
		description: "The endpoint to which the Pulsar client should connect to."
		required:    true
		type: string: examples: ["pulsar://127.0.0.1:6650"]
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
	priority_level: {
		description: """
			The consumer's priority level.

			The broker follows descending priorities. For example, 0=max-priority, 1, 2,...

			In Shared subscription type, the broker first dispatches messages to the max priority level consumers if they have permits. Otherwise, the broker considers next priority level consumers.
			"""
		required: false
		type: int: {}
	}
	subscription_name: {
		description: "The Pulsar subscription name."
		required:    false
		type: string: examples: ["subscription_name"]
	}
	tls: {
		description: "TLS options configuration for the Pulsar client."
		required:    false
		type: object: options: {
			ca_file: {
				description: "File path containing a list of PEM encoded certificates"
				required:    true
				type: string: examples: ["/etc/certs/chain.pem"]
			}
			verify_certificate: {
				description: """
					Enables certificate verification.

					Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
					"""
				required: false
				type: bool: {}
			}
			verify_hostname: {
				description: """
					Whether hostname verification is enabled when verify_certificate is false

					Set to true if not specified.
					"""
				required: false
				type: bool: {}
			}
		}
	}
	topics: {
		description: "The Pulsar topic names to read events from."
		required:    true
		type: array: items: type: string: examples: ["[persistent://public/default/my-topic]"]
	}
}
