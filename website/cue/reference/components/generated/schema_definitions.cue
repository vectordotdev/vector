package metadata

_schemaDefinitions: {
	"codecs::common::length_delimited::LengthDelimitedCoderOptions": object: options: {
		length_field_is_big_endian: {
			description: "Length field byte order (little or big endian)"
			required:    false
			type: bool: default: true
		}
		length_field_length: {
			description: "Number of bytes representing the field length"
			required:    false
			type: uint: default: 4
		}
		length_field_offset: {
			description: "Number of bytes in the header before the length field"
			required:    false
			type: uint: default: 0
		}
		max_frame_length: {
			description: "Maximum frame length"
			required:    false
			type: uint: default: 8388608
		}
	}
	"codecs::decoding::DeserializerConfig": object: options: {
		avro: {
			description:   "Apache Avro-specific encoder options."
			relevant_when: "codec = \"avro\""
			required:      true
			type: object: options: {
				schema: {
					description: """
						The Avro schema definition.
						**Note**: The following [`apache_avro::types::Value`] variants are *not* supported:
						* `Date`
						* `Decimal`
						* `Duration`
						* `Fixed`
						* `TimeMillis`
						"""
					required: true
					type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
				}
				strip_schema_id_prefix: {
					description: "For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to `true` to strip the schema ID prefix, as described in [Confluent Kafka's documentation](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format)."
					required:    true
					type: bool: {}
				}
			}
		}
		codec: {
			description: "The codec to use for decoding events."
			required:    true
			type: string: enum: {
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
		gelf: {
			description:   "GELF-specific decoding options."
			relevant_when: "codec = \"gelf\""
			required:      false
			type: object: options: {
				lossy: {
					description: """
						Determines whether to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
				validation: {
					description: "Configures the decoding validation mode."
					required:    false
					type: string: {
						default: "strict"
						enum: {
							relaxed: """
														Uses more relaxed validation that skips strict GELF specification checks.

														This mode does not treat specification violations as errors, allowing the decoder
														to accept messages from sources that don't strictly follow the GELF spec.
														"""
							strict: "Uses strict validation that closely follows the GELF spec."
						}
					}
				}
			}
		}
		influxdb: {
			description:   "Influxdb-specific decoding options."
			relevant_when: "codec = \"influxdb\""
			required:      false
			type: object: options: lossy: {
				description: """
					Determines whether to replace invalid UTF-8 sequences instead of failing.

					When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

					[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
					"""
				required: false
				type: bool: default: true
			}
		}
		json: {
			description:   "JSON-specific decoding options."
			relevant_when: "codec = \"json\""
			required:      false
			type: object: options: lossy: {
				description: """
					Determines whether to replace invalid UTF-8 sequences instead of failing.

					When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

					[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
					"""
				required: false
				type: bool: default: true
			}
		}
		native_json: {
			description:   "Vector's native JSON-specific decoding options."
			relevant_when: "codec = \"native_json\""
			required:      false
			type: object: options: lossy: {
				description: """
					Determines whether to replace invalid UTF-8 sequences instead of failing.

					When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

					[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
					"""
				required: false
				type: bool: default: true
			}
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
			type: object: options: lossy: {
				description: """
					Determines whether to replace invalid UTF-8 sequences instead of failing.

					When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

					[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
					"""
				required: false
				type: bool: default: true
			}
		}
		vrl: {
			description:   "VRL-specific decoding options."
			relevant_when: "codec = \"vrl\""
			required:      true
			type: object: options: {
				source: {
					description: """
						The [Vector Remap Language][vrl] (VRL) program to execute for each event.
						The final contents of the `.` target are used as the decoding result.
						Compilation errors or use of `abort` in the program result in a decoding error.

						[vrl]: https://vector.dev/docs/reference/vrl
						"""
					required: true
					type: string: {}
				}
				timezone: {
					description: """
						The name of the timezone to apply to timestamp conversions that do not contain an explicit
						time zone. The time zone name may be any name in the [TZ database][tz_database], or `local`
						to indicate system local time.

						If not set, `local` is used.

						[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
						"""
					required: false
					type: string: examples: ["local", "America/New_York", "EST5EDT"]
				}
			}
		}
	}
	"codecs::decoding::FramingConfig": object: options: {
		character_delimited: {
			description:   "Options for the character delimited decoder."
			relevant_when: "method = \"character_delimited\""
			required:      true
			type: object: options: {
				delimiter: {
					description: "The character that delimits byte sequences."
					required:    true
					type: ascii_char: {}
				}
				max_length: {
					description: """
						The maximum length of the byte buffer.

						This length does *not* include the trailing delimiter.

						By default, no maximum length is enforced. If events are malformed, this can lead to
						additional resource usage as events continue to be buffered in memory, and can potentially
						lead to memory exhaustion in extreme cases.

						If there is a risk of processing malformed data, such as logs with user-controlled input,
						consider setting the maximum length to a reasonably large value as a safety net. This
						prevents processing from being unbounded.
						"""
					required: false
					type: uint: {}
				}
				oversized_action: {
					description: """
						The behavior when a frame exceeds `max_length`.

						When set to `drop` (the default), the entire oversized frame is discarded.
						When set to `truncate`, the frame is truncated to `max_length` bytes and the
						remainder is discarded up to the next delimiter.

						This option has no effect if `max_length` is not set.
						"""
					required: false
					type: string: {
						default: "drop"
						enum: {
							drop: "Drop the entire oversized frame."
							truncate: """
														Truncate the frame to the maximum allowed size and emit the partial content.

														The remainder of the oversized frame is discarded up to the next delimiter.
														"""
						}
					}
				}
			}
		}
		chunked_gelf: {
			description:   "Options for the chunked GELF decoder."
			relevant_when: "method = \"chunked_gelf\""
			required:      false
			type: object: options: {
				decompression: {
					description: "Decompression configuration for GELF messages."
					required:    false
					type: string: {
						default: "Auto"
						enum: {
							Auto: "Automatically detect the decompression method based on the magic bytes of the message."
							Gzip: "Use Gzip decompression."
							None: "Do not decompress the message."
							Zlib: "Use Zlib decompression."
						}
					}
				}
				max_length: {
					description: """
						The maximum length of a single GELF message, in bytes. Messages longer than this length are
						dropped. If this option is not set, the decoder does not limit the length of messages and
						the per-message memory is unbounded.

						**Note**: A message can be composed of multiple chunks, and this limit applies to the whole
						message, not to individual chunks.

						This limit takes into account only the message payload. GELF header bytes are excluded from the calculation.
						The message payload is the concatenation of all chunk payloads.
						"""
					required: false
					type: uint: {}
				}
				pending_messages_limit: {
					description: """
						The maximum number of pending incomplete messages. If this limit is reached, the decoder starts
						dropping chunks of new messages, ensuring the memory usage of the decoder's state is bounded.
						If this option is not set, the decoder does not limit the number of pending messages and the memory usage
						of its messages buffer can grow unbounded. This matches Graylog Server's behavior.
						"""
					required: false
					type: uint: {}
				}
				timeout_secs: {
					description: """
						The timeout, in seconds, for a message to be fully received. If the timeout is reached, the
						decoder drops all received chunks for the timed-out message.
						"""
					required: false
					type: float: default: 5.0
				}
			}
		}
		length_delimited: {
			description:   "Options for the length delimited decoder."
			relevant_when: "method = \"length_delimited\""
			required:      true
			type: object: options: {
				length_field_is_big_endian: {
					description: "Length field byte order (little or big endian)"
					required:    false
					type: bool: default: true
				}
				length_field_length: {
					description: "Number of bytes representing the field length"
					required:    false
					type: uint: default: 4
				}
				length_field_offset: {
					description: "Number of bytes in the header before the length field"
					required:    false
					type: uint: default: 0
				}
				max_frame_length: {
					description: "Maximum frame length"
					required:    false
					type: uint: default: 8388608
				}
			}
		}
		max_frame_length: {
			description:   "Maximum frame length"
			relevant_when: "method = \"varint_length_delimited\""
			required:      false
			type: uint: default: 8388608
		}
		method: {
			description: "The framing method."
			required:    true
			type: string: enum: {
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
		newline_delimited: {
			description:   "Options for the newline delimited decoder."
			relevant_when: "method = \"newline_delimited\""
			required:      false
			type: object: options: {
				max_length: {
					description: """
						The maximum length of the byte buffer.

						This length does *not* include the trailing delimiter.

						By default, no maximum length is enforced. If events are malformed, this can lead to
						additional resource usage as events continue to be buffered in memory, and can potentially
						lead to memory exhaustion in extreme cases.

						If there is a risk of processing malformed data, such as logs with user-controlled input,
						consider setting the maximum length to a reasonably large value as a safety net. This
						prevents processing from being unbounded.
						"""
					required: false
					type: uint: {}
				}
				oversized_action: {
					description: """
						The behavior when a line exceeds `max_length`.

						When set to `drop` (the default), the entire oversized line is discarded.
						When set to `truncate`, the line is truncated to `max_length` bytes and the
						remainder is discarded up to the next newline.

						This option has no effect if `max_length` is not set.
						"""
					required: false
					type: string: {
						default: "drop"
						enum: {
							drop: "Drop the entire oversized frame."
							truncate: """
														Truncate the frame to the maximum allowed size and emit the partial content.

														The remainder of the oversized frame is discarded up to the next delimiter.
														"""
						}
					}
				}
			}
		}
		octet_counting: {
			description:   "Options for the octet counting decoder."
			relevant_when: "method = \"octet_counting\""
			required:      false
			type: object: options: max_length: {
				description: "The maximum length of the byte buffer."
				required:    false
				type: uint: {}
			}
		}
	}
	"codecs::decoding::format::avro::AvroDeserializerOptions": object: options: {
		schema: {
			description: """
				The Avro schema definition.
				**Note**: The following [`apache_avro::types::Value`] variants are *not* supported:
				* `Date`
				* `Decimal`
				* `Duration`
				* `Fixed`
				* `TimeMillis`
				"""
			required: true
			type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
		}
		strip_schema_id_prefix: {
			description: "For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to `true` to strip the schema ID prefix, as described in [Confluent Kafka's documentation](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format)."
			required:    true
			type: bool: {}
		}
	}
	"codecs::decoding::format::gelf::GelfDeserializerOptions": object: options: {
		lossy: {
			description: """
				Determines whether to replace invalid UTF-8 sequences instead of failing.

				When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

				[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
				"""
			required: false
			type: bool: default: true
		}
		validation: {
			description: "Configures the decoding validation mode."
			required:    false
			type: string: {
				default: "strict"
				enum: {
					relaxed: """
						Uses more relaxed validation that skips strict GELF specification checks.

						This mode does not treat specification violations as errors, allowing the decoder
						to accept messages from sources that don't strictly follow the GELF spec.
						"""
					strict: "Uses strict validation that closely follows the GELF spec."
				}
			}
		}
	}
	"codecs::decoding::format::influxdb::InfluxdbDeserializerOptions": object: options: lossy: {
		description: """
			Determines whether to replace invalid UTF-8 sequences instead of failing.

			When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

			[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
			"""
		required: false
		type: bool: default: true
	}
	"codecs::decoding::format::vrl::VrlDeserializerOptions": object: options: {
		source: {
			description: """
				The [Vector Remap Language][vrl] (VRL) program to execute for each event.
				The final contents of the `.` target are used as the decoding result.
				Compilation errors or use of `abort` in the program result in a decoding error.

				[vrl]: https://vector.dev/docs/reference/vrl
				"""
			required: true
			type: string: {}
		}
		timezone: {
			description: """
				The name of the timezone to apply to timestamp conversions that do not contain an explicit
				time zone. The time zone name may be any name in the [TZ database][tz_database], or `local`
				to indicate system local time.

				If not set, `local` is used.

				[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
				"""
			required: false
			type: string: examples: ["local", "America/New_York", "EST5EDT"]
		}
	}
	"codecs::decoding::framing::character_delimited::CharacterDelimitedDecoderOptions": object: options: {
		delimiter: {
			description: "The character that delimits byte sequences."
			required:    true
			type: ascii_char: {}
		}
		max_length: {
			description: """
				The maximum length of the byte buffer.

				This length does *not* include the trailing delimiter.

				By default, no maximum length is enforced. If events are malformed, this can lead to
				additional resource usage as events continue to be buffered in memory, and can potentially
				lead to memory exhaustion in extreme cases.

				If there is a risk of processing malformed data, such as logs with user-controlled input,
				consider setting the maximum length to a reasonably large value as a safety net. This
				prevents processing from being unbounded.
				"""
			required: false
			type: uint: {}
		}
		oversized_action: {
			description: """
				The behavior when a frame exceeds `max_length`.

				When set to `drop` (the default), the entire oversized frame is discarded.
				When set to `truncate`, the frame is truncated to `max_length` bytes and the
				remainder is discarded up to the next delimiter.

				This option has no effect if `max_length` is not set.
				"""
			required: false
			type: string: {
				default: "drop"
				enum: {
					drop: "Drop the entire oversized frame."
					truncate: """
						Truncate the frame to the maximum allowed size and emit the partial content.

						The remainder of the oversized frame is discarded up to the next delimiter.
						"""
				}
			}
		}
	}
	"codecs::decoding::framing::chunked_gelf::ChunkedGelfDecoderOptions": object: options: {
		decompression: {
			description: "Decompression configuration for GELF messages."
			required:    false
			type: string: {
				default: "Auto"
				enum: {
					Auto: "Automatically detect the decompression method based on the magic bytes of the message."
					Gzip: "Use Gzip decompression."
					None: "Do not decompress the message."
					Zlib: "Use Zlib decompression."
				}
			}
		}
		max_length: {
			description: """
				The maximum length of a single GELF message, in bytes. Messages longer than this length are
				dropped. If this option is not set, the decoder does not limit the length of messages and
				the per-message memory is unbounded.

				**Note**: A message can be composed of multiple chunks, and this limit applies to the whole
				message, not to individual chunks.

				This limit takes into account only the message payload. GELF header bytes are excluded from the calculation.
				The message payload is the concatenation of all chunk payloads.
				"""
			required: false
			type: uint: {}
		}
		pending_messages_limit: {
			description: """
				The maximum number of pending incomplete messages. If this limit is reached, the decoder starts
				dropping chunks of new messages, ensuring the memory usage of the decoder's state is bounded.
				If this option is not set, the decoder does not limit the number of pending messages and the memory usage
				of its messages buffer can grow unbounded. This matches Graylog Server's behavior.
				"""
			required: false
			type: uint: {}
		}
		timeout_secs: {
			description: """
				The timeout, in seconds, for a message to be fully received. If the timeout is reached, the
				decoder drops all received chunks for the timed-out message.
				"""
			required: false
			type: float: default: 5.0
		}
	}
	"codecs::decoding::framing::newline_delimited::NewlineDelimitedDecoderOptions": object: options: {
		max_length: {
			description: """
				The maximum length of the byte buffer.

				This length does *not* include the trailing delimiter.

				By default, no maximum length is enforced. If events are malformed, this can lead to
				additional resource usage as events continue to be buffered in memory, and can potentially
				lead to memory exhaustion in extreme cases.

				If there is a risk of processing malformed data, such as logs with user-controlled input,
				consider setting the maximum length to a reasonably large value as a safety net. This
				prevents processing from being unbounded.
				"""
			required: false
			type: uint: {}
		}
		oversized_action: {
			description: """
				The behavior when a line exceeds `max_length`.

				When set to `drop` (the default), the entire oversized line is discarded.
				When set to `truncate`, the line is truncated to `max_length` bytes and the
				remainder is discarded up to the next newline.

				This option has no effect if `max_length` is not set.
				"""
			required: false
			type: string: {
				default: "drop"
				enum: {
					drop: "Drop the entire oversized frame."
					truncate: """
						Truncate the frame to the maximum allowed size and emit the partial content.

						The remainder of the oversized frame is discarded up to the next delimiter.
						"""
				}
			}
		}
	}
	"codecs::decoding::framing::octet_counting::OctetCountingDecoderOptions": object: options: max_length: {
		description: "The maximum length of the byte buffer."
		required:    false
		type: uint: {}
	}
	"codecs::encoding::config::EncodingConfig": object: options: {
		avro: {
			description:   "Apache Avro-specific encoder options."
			relevant_when: "codec = \"avro\""
			required:      true
			type: object: options: schema: {
				description: "The Avro schema."
				required:    true
				type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
			}
		}
		cef: {
			description:   "The CEF Serializer Options."
			relevant_when: "codec = \"cef\""
			required:      true
			type: object: options: {
				device_event_class_id: {
					description: """
						Unique identifier for each event type. Identifies the type of event reported.
						The value length must be less than or equal to 1023.
						"""
					required: true
					type: string: {}
				}
				device_product: {
					description: """
						Identifies the product of a vendor.
						The part of a unique device identifier. No two products can use the same combination of device vendor and device product.
						The value length must be less than or equal to 63.
						"""
					required: true
					type: string: {}
				}
				device_vendor: {
					description: """
						Identifies the vendor of the product.
						The part of a unique device identifier. No two products can use the same combination of device vendor and device product.
						The value length must be less than or equal to 63.
						"""
					required: true
					type: string: {}
				}
				device_version: {
					description: """
						Identifies the version of the problem. The combination of the device product, vendor, and this value make up the unique id of the device that sends messages.
						The value length must be less than or equal to 31.
						"""
					required: true
					type: string: {}
				}
				extensions: {
					description: """
						The collection of key-value pairs. Keys are the keys of the extensions, and values are paths that point to the extension values of a log event.
						The event can have any number of key-value pairs in any order.
						"""
					required: true
					type: object: options: "*": {
						description: "This is a path that points to the extension value of a log event."
						required:    true
						type: string: {}
					}
				}
				name: {
					description: """
						This is a path that points to the human-readable description of a log event.
						The value length must be less than or equal to 512.
						Equals "cef.name" by default.
						"""
					required: true
					type: string: {}
				}
				severity: {
					description: """
						This is a path that points to the field of a log event that reflects importance of the event.

						It must point to a number from 0 to 10.
						0 = lowest_importance, 10 = highest_importance.
						Set to "cef.severity" by default.
						"""
					required: true
					type: string: {}
				}
				version: {
					description: """
						CEF Version. Can be either 0 or 1.
						Set to "0" by default.
						"""
					required: true
					type: string: enum: {
						V0: "CEF specification version 0.1."
						V1: "CEF specification version 1.x."
					}
				}
			}
		}
		codec: {
			description: "The codec to use for encoding events."
			required:    true
			type: string: enum: {
				avro: """
					Encodes an event as an [Apache Avro][apache_avro] message.

					[apache_avro]: https://avro.apache.org/
					"""
				cef: "Encodes an event as a CEF (Common Event Format) formatted message."
				csv: """
					Encodes an event as a CSV message.

					This codec must be configured with fields to encode.
					"""
				gelf: """
					Encodes an event as a [GELF][gelf] message.

					This codec is experimental for the following reason:

					The GELF specification is more strict than the actual Graylog receiver.
					Vector's encoder currently adheres more strictly to the GELF spec, with
					the exception that some characters such as `@`  are allowed in field names.

					Other GELF codecs, such as Loki's, use a [Go SDK][implementation] that is maintained
					by Graylog and is much more relaxed than the GELF spec.

					Going forward, Vector will use that [Go SDK][implementation] as the reference implementation, which means
					the codec might continue to relax the enforcement of the specification.

					[gelf]: https://docs.graylog.org/docs/gelf
					[implementation]: https://github.com/Graylog2/go-gelf/blob/v2/gelf/reader.go
					"""
				json: """
					Encodes an event as [JSON][json].

					[json]: https://www.json.org/
					"""
				logfmt: """
					Encodes an event as a [logfmt][logfmt] message.

					[logfmt]: https://brandur.org/logfmt
					"""
				native: """
					Encodes an event in the [native Protocol Buffers format][vector_native_protobuf].

					This codec is **[experimental][experimental]**.

					[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
					[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
					"""
				native_json: """
					Encodes an event in the [native JSON format][vector_native_json].

					This codec is **[experimental][experimental]**.

					[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
					[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
					"""
				otlp: """
					Encodes an event in the [OTLP (OpenTelemetry Protocol)][otlp] format.

					This codec uses protobuf encoding, which is the recommended format for OTLP.
					The output is suitable for sending to OTLP-compatible endpoints with
					`content-type: application/x-protobuf`.

					[otlp]: https://opentelemetry.io/docs/specs/otlp/
					"""
				protobuf: """
					Encodes an event as a [Protobuf][protobuf] message.

					[protobuf]: https://protobuf.dev/
					"""
				raw_message: """
					No encoding.

					This encoding uses the `message` field of a log event.

					Be careful if you are modifying your log events (for example, by using a `remap`
					transform) and removing the message field while doing additional parsing on it, as this
					could lead to the encoding emitting empty strings for the given event.
					"""
				syslog: """
					Syslog encoding
					RFC 3164 and 5424 are supported
					"""
				text: """
					Plain text encoding.

					This encoding uses the `message` field of a log event. For metrics, it uses an
					encoding that resembles the Prometheus export format.

					Be careful if you are modifying your log events (for example, by using a `remap`
					transform) and removing the message field while doing additional parsing on it, as this
					could lead to the encoding emitting empty strings for the given event.
					"""
			}
		}
		csv: {
			description:   "The CSV Serializer Options."
			relevant_when: "codec = \"csv\""
			required:      true
			type: object: options: {
				capacity: {
					description: """
						Sets the capacity (in bytes) of the internal buffer used in the CSV writer.
						This defaults to 8192 bytes (8KB).
						"""
					required: false
					type: uint: default: 8192
				}
				delimiter: {
					description: "The field delimiter to use when writing CSV."
					required:    false
					type: ascii_char: default: ","
				}
				double_quote: {
					description: """
						Enables double quote escapes.

						This is enabled by default, but you can disable it. When disabled, quotes in
						field data are escaped instead of doubled.
						"""
					required: false
					type: bool: default: true
				}
				escape: {
					description: """
						The escape character to use when writing CSV.

						In some variants of CSV, quotes are escaped using a special escape character
						like \\ (instead of escaping quotes by doubling them).

						To use this, `double_quotes` needs to be disabled as well; otherwise, this setting is ignored.
						"""
					required: false
					type: ascii_char: default: "\""
				}
				fields: {
					description: """
						Configures the fields that are encoded, as well as the order in which they
						appear in the output.

						If a field is not present in the event, the output for that field is an empty string.

						Values of type `Array`, `Object`, and `Regex` are not supported, and the
						output for any of these types is an empty string.
						"""
					required: true
					type: array: items: type: string: {}
				}
				quote: {
					description: "The quote character to use when writing CSV."
					required:    false
					type: ascii_char: default: "\""
				}
				quote_style: {
					description: "The quoting style to use when writing CSV data."
					required:    false
					type: string: {
						default: "necessary"
						enum: {
							always: "Always puts quotes around every field."
							necessary: """
														Puts quotes around fields only when necessary.
														They are necessary when fields contain a quote, delimiter, or record terminator.
														Quotes are also necessary when writing an empty record
														(which is indistinguishable from a record with one empty field).
														"""
							never: "Never writes quotes, even if it produces invalid CSV data."
							non_numeric: """
														Puts quotes around all fields that are non-numeric.
														This means that when writing a field that does not parse as a valid float or integer,
														quotes are used even if they aren't strictly necessary.
														"""
						}
					}
				}
			}
		}
		except_fields: {
			description: "List of fields that are excluded from the encoded event."
			required:    false
			type: array: items: type: string: {}
		}
		gelf: {
			description:   "The GELF Serializer Options."
			relevant_when: "codec = \"gelf\""
			required:      false
			type: object: options: max_chunk_size: {
				description: """
					Maximum size for each GELF chunked datagram (including 12-byte header).
					Chunking starts when datagrams exceed this size.
					For Graylog target, keep at or below 8192 bytes; for Vector target (`gelf` decoding with `chunked_gelf` framing), up to 65,500 bytes is recommended.
					"""
				required: false
				type: uint: default: 8192
			}
		}
		json: {
			description:   "Options for the JsonSerializer."
			relevant_when: "codec = \"json\""
			required:      false
			type: object: options: pretty: {
				description: "Whether to use pretty JSON formatting."
				required:    false
				type: bool: default: false
			}
		}
		metric_tag_values: {
			description: """
				Controls how metric tag values are encoded.

				When set to `single`, only the last non-bare value of tags are displayed with the
				metric. When set to `full`, all metric tags are exposed as separate assignments.
				When set to `auto`, tag values are encoded using their underlying shape.
				"""
			relevant_when: "codec = \"json\" or codec = \"text\""
			required:      false
			type: string: {
				default: "single"
				enum: {
					auto: """
						Tag values are exposed using their underlying shape: single-value tags as strings,
						multi-value tags as arrays. A length-1 array round-trips as a scalar; use `Full` to
						force array shape.
						"""
					full: "All tags are exposed as arrays of either string or null values."
					single: """
						Tag values are exposed as single strings, the same as they were before this config
						option. Tags with multiple values show the last assigned value, and null values
						are ignored.
						"""
				}
			}
		}
		only_fields: {
			description: "List of fields that are included in the encoded event."
			required:    false
			type: array: items: type: string: {}
		}
		protobuf: {
			description:   "Options for the Protobuf serializer."
			relevant_when: "codec = \"protobuf\""
			required:      true
			type: object: options: {
				desc_file: {
					description: """
						The path to the protobuf descriptor set file.

						This file is the output of `protoc -I <include path> -o <desc output path> <proto>`

						You can read more [here](https://buf.build/docs/reference/images/#how-buf-images-work).
						"""
					required: true
					type: string: examples: ["/etc/vector/protobuf_descriptor_set.desc"]
				}
				message_type: {
					description: "The name of the message type to use for serializing."
					required:    true
					type: string: examples: ["package.Message"]
				}
				use_json_names: {
					description: """
						Use JSON field names (camelCase) instead of protobuf field names (snake_case).

						When enabled, the serializer looks for fields using their JSON names as defined
						in the `.proto` file (for example `jobDescription` instead of `job_description`).

						This is useful when working with data that has already been converted from JSON or
						when interfacing with systems that use JSON naming conventions.
						"""
					required: false
					type: bool: default: false
				}
			}
		}
		syslog: {
			description:   "Options for the Syslog serializer."
			relevant_when: "codec = \"syslog\""
			required:      false
			type: object: options: {
				app_name: {
					description: """
						Path to a field in the event to use for the app name.

						If not provided, the encoder checks for a semantic "service" field.
						If that is also missing, it defaults to "vector".
						"""
					required: false
					type: string: {}
				}
				facility: {
					description: "Path to a field in the event to use for the facility. Defaults to \"user\"."
					required:    false
					type: string: {}
				}
				msg_id: {
					description: "Path to a field in the event to use for the msg ID."
					required:    false
					type: string: {}
				}
				proc_id: {
					description: "Path to a field in the event to use for the proc ID."
					required:    false
					type: string: {}
				}
				rfc: {
					description: "RFC to use for formatting."
					required:    false
					type: string: {
						default: "rfc5424"
						enum: {
							rfc3164: "The legacy RFC3164 syslog format."
							rfc5424: "The modern RFC5424 syslog format."
						}
					}
				}
				severity: {
					description: "Path to a field in the event to use for the severity. Defaults to \"informational\"."
					required:    false
					type: string: {}
				}
			}
		}
		timestamp_format: {
			description: "Format used for timestamp fields."
			required:    false
			type: string: enum: {
				rfc3339:    "Represent the timestamp as a RFC 3339 timestamp."
				unix:       "Represent the timestamp as a Unix timestamp."
				unix_float: "Represent the timestamp as a Unix timestamp in floating point."
				unix_ms:    "Represent the timestamp as a Unix timestamp in milliseconds."
				unix_ns:    "Represent the timestamp as a Unix timestamp in nanoseconds."
				unix_us:    "Represent the timestamp as a Unix timestamp in microseconds."
			}
		}
	}
	"codecs::encoding::format::csv::CsvSerializerOptions": object: options: {
		capacity: {
			description: """
				Sets the capacity (in bytes) of the internal buffer used in the CSV writer.
				This defaults to 8192 bytes (8KB).
				"""
			required: false
			type: uint: default: 8192
		}
		delimiter: {
			description: "The field delimiter to use when writing CSV."
			required:    false
			type: ascii_char: default: ","
		}
		double_quote: {
			description: """
				Enables double quote escapes.

				This is enabled by default, but you can disable it. When disabled, quotes in
				field data are escaped instead of doubled.
				"""
			required: false
			type: bool: default: true
		}
		escape: {
			description: """
				The escape character to use when writing CSV.

				In some variants of CSV, quotes are escaped using a special escape character
				like \\ (instead of escaping quotes by doubling them).

				To use this, `double_quotes` needs to be disabled as well; otherwise, this setting is ignored.
				"""
			required: false
			type: ascii_char: default: "\""
		}
		fields: {
			description: """
				Configures the fields that are encoded, as well as the order in which they
				appear in the output.

				If a field is not present in the event, the output for that field is an empty string.

				Values of type `Array`, `Object`, and `Regex` are not supported, and the
				output for any of these types is an empty string.
				"""
			required: true
			type: array: items: type: string: {}
		}
		quote: {
			description: "The quote character to use when writing CSV."
			required:    false
			type: ascii_char: default: "\""
		}
		quote_style: {
			description: "The quoting style to use when writing CSV data."
			required:    false
			type: string: {
				default: "necessary"
				enum: {
					always: "Always puts quotes around every field."
					necessary: """
						Puts quotes around fields only when necessary.
						They are necessary when fields contain a quote, delimiter, or record terminator.
						Quotes are also necessary when writing an empty record
						(which is indistinguishable from a record with one empty field).
						"""
					never: "Never writes quotes, even if it produces invalid CSV data."
					non_numeric: """
						Puts quotes around all fields that are non-numeric.
						This means that when writing a field that does not parse as a valid float or integer,
						quotes are used even if they aren't strictly necessary.
						"""
				}
			}
		}
	}
	"codecs::encoding::format::json::JsonSerializerOptions": object: options: pretty: {
		description: "Whether to use pretty JSON formatting."
		required:    false
		type: bool: default: false
	}
	"codecs::encoding::framing::framer::FramingConfig": object: options: {
		character_delimited: {
			description:   "Options for the character delimited encoder."
			relevant_when: "method = \"character_delimited\""
			required:      true
			type: object: options: delimiter: {
				description: "The ASCII (7-bit) character that delimits byte sequences."
				required:    true
				type: ascii_char: {}
			}
		}
		length_delimited: {
			description:   "Options for the length delimited decoder."
			relevant_when: "method = \"length_delimited\""
			required:      true
			type: object: options: {
				length_field_is_big_endian: {
					description: "Length field byte order (little or big endian)"
					required:    false
					type: bool: default: true
				}
				length_field_length: {
					description: "Number of bytes representing the field length"
					required:    false
					type: uint: default: 4
				}
				length_field_offset: {
					description: "Number of bytes in the header before the length field"
					required:    false
					type: uint: default: 0
				}
				max_frame_length: {
					description: "Maximum frame length"
					required:    false
					type: uint: default: 8388608
				}
			}
		}
		max_frame_length: {
			description:   "Maximum frame length"
			relevant_when: "method = \"varint_length_delimited\""
			required:      false
			type: uint: default: 8388608
		}
		method: {
			description: "The framing method."
			required:    true
			type: string: enum: {
				bytes:               "Event data is not delimited at all."
				character_delimited: "Event data is delimited by a single ASCII (7-bit) character."
				length_delimited: """
					Event data is prefixed with its length in bytes.

					The prefix is a 32-bit unsigned integer, little endian.
					"""
				newline_delimited: "Event data is delimited by a newline (LF) character."
				varint_length_delimited: """
					Event data is prefixed with its length in bytes as a varint.

					This is compatible with protobuf's length-delimited encoding.
					"""
			}
		}
	}
	"codecs::encoding::transformer::Transformer": object: options: {
		except_fields: {
			description: "List of fields that are excluded from the encoded event."
			required:    false
			type: array: items: type: string: {}
		}
		only_fields: {
			description: "List of fields that are included in the encoded event."
			required:    false
			type: array: items: type: string: {}
		}
		timestamp_format: {
			description: "Format used for timestamp fields."
			required:    false
			type: string: enum: {
				rfc3339:    "Represent the timestamp as a RFC 3339 timestamp."
				unix:       "Represent the timestamp as a Unix timestamp."
				unix_float: "Represent the timestamp as a Unix timestamp in floating point."
				unix_ms:    "Represent the timestamp as a Unix timestamp in milliseconds."
				unix_ns:    "Represent the timestamp as a Unix timestamp in nanoseconds."
				unix_us:    "Represent the timestamp as a Unix timestamp in microseconds."
			}
		}
	}
	"core::option::Option<vector::aws::region::RegionOrEndpoint>": object: options: {
		endpoint: {
			description: "Custom endpoint for use with AWS-compatible services."
			required:    false
			type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
		}
		region: {
			description: """
				The [AWS region][aws_region] of the target service.

				[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
				"""
			required: false
			type: string: examples: [
				"us-east-1"
			]
		}
	}
	"core::option::Option<vector::common::http::server_auth::HttpServerAuthConfig>": object: options: {
		password: {
			description:   "The basic authentication password."
			relevant_when: "strategy = \"basic\""
			required:      true
			type: string: examples: ["${PASSWORD}", "password"]
		}
		source: {
			description:   "The VRL boolean expression."
			relevant_when: "strategy = \"custom\""
			required:      true
			type: string: {}
		}
		strategy: {
			description: "The authentication strategy to use."
			required:    true
			type: string: enum: {
				basic: """
					Basic authentication.

					The username and password are concatenated and encoded using [base64][base64].

					[base64]: https://en.wikipedia.org/wiki/Base64
					"""
				custom: """
					Custom authentication using VRL code.

					Takes in request and validates it using VRL code. The VRL program must return a boolean.
					"""
			}
		}
		username: {
			description:   "The basic authentication username."
			relevant_when: "strategy = \"basic\""
			required:      true
			type: string: examples: ["${USERNAME}", "username"]
		}
	}
	"core::option::Option<vector::http::Auth>": object: options: {
		auth: {
			description:   "The AWS authentication configuration."
			relevant_when: "strategy = \"aws\""
			required:      true
			type: object: options: {
				access_key_id: {
					description: "The AWS access key ID."
					required:    true
					type: string: examples: ["AKIAIOSFODNN7EXAMPLE"]
				}
				assume_role: {
					description: """
						The ARN of an [IAM role][iam_role] to assume.

						[iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
						"""
					required: true
					type: string: examples: ["arn:aws:iam::123456789098:role/my_role"]
				}
				credentials_file: {
					description: "Path to the credentials file."
					required:    true
					type: string: examples: ["/my/aws/credentials"]
				}
				external_id: {
					description: """
						The optional unique external ID in conjunction with role to assume.

						[external_id]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_create_for-user_externalid.html
						"""
					required: false
					type: string: examples: ["randomEXAMPLEidString"]
				}
				imds: {
					description: "Configuration for authenticating with AWS through IMDS."
					required:    false
					type: object: options: {
						connect_timeout_seconds: {
							description: "Connect timeout for IMDS."
							required:    false
							type: uint: {
								default: 1
								unit:    "seconds"
							}
						}
						max_attempts: {
							description: "Number of IMDS retries for fetching tokens and metadata."
							required:    false
							type: uint: default: 4
						}
						read_timeout_seconds: {
							description: "Read timeout for IMDS."
							required:    false
							type: uint: {
								default: 1
								unit:    "seconds"
							}
						}
					}
				}
				load_timeout_secs: {
					description: """
						Timeout for successfully loading any credentials, in seconds.

						Relevant when the default credentials chain or `assume_role` is used.
						"""
					required: false
					type: uint: {
						examples: [30]
						unit: "seconds"
					}
				}
				profile: {
					description: """
						The credentials profile to use.

						Used to select AWS credentials from a provided credentials file.
						"""
					required: false
					type: string: {
						default: "default"
						examples: ["develop"]
					}
				}
				region: {
					description: """
						The [AWS region][aws_region] to send STS requests to.

						If not set, this defaults to the configured region
						for the service itself.

						[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
						"""
					required: false
					type: string: examples: ["us-west-2"]
				}
				secret_access_key: {
					description: "The AWS secret access key."
					required:    true
					type: string: examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
				}
				session_name: {
					description: """
						The optional [RoleSessionName][role_session_name] is a unique session identifier for your assumed role.

						Should be unique per principal or reason.
						If not set, the session name is autogenerated like assume-role-provider-1736428351340

						[role_session_name]: https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html
						"""
					required: false
					type: string: examples: ["vector-indexer-role"]
				}
				session_token: {
					description: """
						The AWS session token.
						See [AWS temporary credentials](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_temp_use-resources.html)
						"""
					required: false
					type: string: examples: ["AQoDYXdz...AQoDYXdz..."]
				}
			}
		}
		password: {
			description:   "The basic authentication password."
			relevant_when: "strategy = \"basic\""
			required:      true
			type: string: examples: ["${PASSWORD}", "password"]
		}
		service: {
			description:   "The AWS service name to use for signing."
			relevant_when: "strategy = \"aws\""
			required:      true
			type: string: {}
		}
		strategy: {
			description: "The authentication strategy to use."
			required:    true
			type: string: enum: {
				aws: "AWS authentication."
				basic: """
					Basic authentication.

					The username and password are concatenated and encoded using [base64][base64].

					[base64]: https://en.wikipedia.org/wiki/Base64
					"""
				bearer: """
					Bearer authentication.

					The bearer token value (OAuth2, JWT, etc.) is passed as-is.
					"""
				custom: "Custom Authorization Header Value, will be inserted into the headers as `Authorization: < value >`"
			}
		}
		token: {
			description:   "The bearer authentication token."
			relevant_when: "strategy = \"bearer\""
			required:      true
			type: string: {}
		}
		user: {
			description:   "The basic authentication username."
			relevant_when: "strategy = \"basic\""
			required:      true
			type: string: examples: ["${USERNAME}", "username"]
		}
		value: {
			description:   "Custom string value of the Authorization header"
			relevant_when: "strategy = \"custom\""
			required:      true
			type: string: examples: ["${AUTH_HEADER_VALUE}", "CUSTOM_PREFIX ${TOKEN}"]
		}
	}
	"core::option::Option<vector::kafka::KafkaSaslConfig>": object: options: {
		enabled: {
			description: """
				Enables SASL authentication.

				Only `PLAIN`- and `SCRAM`-based mechanisms are supported when configuring SASL authentication using `sasl.*`. For
				other mechanisms, `librdkafka_options.*` must be used directly to configure other `librdkafka`-specific values.
				If using `sasl.kerberos.*` as an example, where `*` is `service.name`, `principal`, `kinit.md`, etc., then
				`librdkafka_options.*` as a result becomes `librdkafka_options.sasl.kerberos.service.name`,
				`librdkafka_options.sasl.kerberos.principal`, etc.

				See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.

				SASL authentication is not supported on Windows.
				"""
			required: false
			type: bool: {}
		}
		mechanism: {
			description: "The SASL mechanism to use."
			required:    false
			type: string: examples: ["SCRAM-SHA-256", "SCRAM-SHA-512"]
		}
		password: {
			description: "The SASL password."
			required:    false
			type: string: examples: [
				"password"
			]
		}
		username: {
			description: "The SASL username."
			required:    false
			type: string: examples: [
				"username"
			]
		}
	}
	"core::option::Option<vector::nats::NatsAuthConfig>": object: options: {
		credentials_file: {
			description:   "Credentials file configuration."
			relevant_when: "strategy = \"credentials_file\""
			required:      true
			type: object: options: path: {
				description: "Path to credentials file."
				required:    true
				type: string: examples: ["/etc/nats/nats.creds"]
			}
		}
		nkey: {
			description:   "NKeys configuration."
			relevant_when: "strategy = \"nkey\""
			required:      true
			type: object: options: {
				nkey: {
					description: """
						User.

						Conceptually, this is equivalent to a public key.
						"""
					required: true
					type: string: {}
				}
				seed: {
					description: """
						Seed.

						Conceptually, this is equivalent to a private key.
						"""
					required: true
					type: string: {}
				}
			}
		}
		strategy: {
			description: """
				The strategy used to authenticate with the NATS server.

				More information on NATS authentication, and the various authentication strategies, can be found in the
				NATS [documentation][nats_auth_docs]. For TLS client certificate authentication specifically, see the
				`tls` settings.

				[nats_auth_docs]: https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro
				"""
			required: true
			type: string: enum: {
				credentials_file: "Credentials file authentication. (JWT-based)"
				nkey:             "NKey authentication."
				token:            "Token authentication."
				user_password:    "Username/password authentication."
			}
		}
		token: {
			description:   "Token configuration."
			relevant_when: "strategy = \"token\""
			required:      true
			type: object: options: value: {
				description: "Token."
				required:    true
				type: string: {}
			}
		}
		user_password: {
			description:   "Username and password configuration."
			relevant_when: "strategy = \"user_password\""
			required:      true
			type: object: options: {
				password: {
					description: "Password."
					required:    true
					type: string: {}
				}
				user: {
					description: "Username."
					required:    true
					type: string: {}
				}
			}
		}
	}
	"core::option::Option<vector::sinks::util::service::health::HealthConfig>": object: options: {
		retry_initial_backoff_secs: {
			description: "Initial delay between attempts to reactivate endpoints once they become unhealthy."
			required:    false
			type: uint: {
				default: 1
				unit:    "seconds"
			}
		}
		retry_max_duration_secs: {
			description: "Maximum delay between attempts to reactivate endpoints once they become unhealthy."
			required:    false
			type: uint: {
				default: 3600
				unit:    "seconds"
			}
		}
	}
	"core::option::Option<vector::sources::util::multiline_config::MultilineConfig>": object: options: {
		condition_pattern: {
			description: """
				Regular expression pattern that is used to determine whether or not more lines should be read.

				This setting must be configured in conjunction with `mode`.
				"""
			required: true
			type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
		}
		mode: {
			description: """
				Aggregation mode.

				This setting must be configured in conjunction with `condition_pattern`.
				"""
			required: true
			type: string: enum: {
				continue_past: """
					All consecutive lines matching this pattern, plus one additional line, are included in the group.

					This is useful in cases where a log message ends with a continuation marker, such as a backslash, indicating
					that the following line is part of the same message.
					"""
				continue_through: """
					All consecutive lines matching this pattern are included in the group.

					The first line (the line that matched the start pattern) does not need to match the `ContinueThrough` pattern.

					This is useful in cases such as a Java stack trace, where some indicator in the line (such as a leading
					whitespace) indicates that it is an extension of the proceeding line.
					"""
				halt_before: """
					All consecutive lines not matching this pattern are included in the group.

					This is useful where a log line contains a marker indicating that it begins a new message.
					"""
				halt_with: """
					All consecutive lines, up to and including the first line matching this pattern, are included in the group.

					This is useful where a log line ends with a termination marker, such as a semicolon.
					"""
			}
		}
		start_pattern: {
			description: "Regular expression pattern that is used to match the start of a new message."
			required:    true
			type: string: examples: ["^[\\s]+", "\\\\$", "^(INFO|ERROR) ", ";$"]
		}
		timeout_ms: {
			description: """
				The maximum amount of time to wait for the next additional line, in milliseconds.

				Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete.
				"""
			required: true
			type: uint: {
				examples: [1000, 600000]
				unit: "milliseconds"
			}
		}
	}
	"core::option::Option<vector_core::tcp::TcpKeepaliveConfig>": object: options: time_secs: {
		description: "The time to wait before starting to send TCP keepalive probes on an idle connection."
		required:    false
		type: uint: unit: "seconds"
	}
	"core::option::Option<vector_core::tls::settings::TlsConfig>": object: options: {
		alpn_protocols: {
			description: """
				Sets the list of supported ALPN protocols.

				Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
				that they are defined.
				"""
			required: false
			type: array: items: type: string: examples: ["h2"]
		}
		ca_file: {
			description: """
				Absolute path to an additional CA certificate file.

				The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
				"""
			required: false
			type: string: examples: ["/path/to/certificate_authority.crt"]
		}
		crt_file: {
			description: """
				Absolute path to a certificate file used to identify this server.

				The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
				an inline string in PEM format.

				If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
				"""
			required: false
			type: string: examples: ["/path/to/host_certificate.crt"]
		}
		key_file: {
			description: """
				Absolute path to a private key file used to identify this server.

				The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
				"""
			required: false
			type: string: examples: ["/path/to/host_certificate.key"]
		}
		key_pass: {
			description: """
				Passphrase used to unlock the encrypted key file.

				This has no effect unless `key_file` is set.
				"""
			required: false
			type: string: examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
		}
		server_name: {
			description: """
				Server name to use when using Server Name Indication (SNI).

				Only relevant for outgoing connections.
				"""
			required: false
			type: string: examples: ["www.example.com"]
		}
		verify_certificate: {
			description: """
				Enables certificate verification. For components that create a server, this requires that the
				client connections have a valid client certificate. For components that initiate requests,
				this validates that the upstream has a valid certificate.

				If enabled, certificates must not be expired and must be issued by a trusted
				issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
				certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
				so on, until the verification process reaches a root certificate.

				Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
				"""
			required: false
			type: bool: {}
		}
		verify_hostname: {
			description: """
				Enables hostname verification.

				If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
				the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.

				Only relevant for outgoing connections.

				Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
				"""
			required: false
			type: bool: {}
		}
	}
	"core::option::Option<vector_core::tls::settings::TlsEnableableConfig>": object: options: {
		alpn_protocols: {
			description: """
				Sets the list of supported ALPN protocols.

				Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
				that they are defined.
				"""
			required: false
			type: array: items: type: string: examples: ["h2"]
		}
		ca_file: {
			description: """
				Absolute path to an additional CA certificate file.

				The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
				"""
			required: false
			type: string: examples: ["/path/to/certificate_authority.crt"]
		}
		crt_file: {
			description: """
				Absolute path to a certificate file used to identify this server.

				The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
				an inline string in PEM format.

				If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
				"""
			required: false
			type: string: examples: ["/path/to/host_certificate.crt"]
		}
		enabled: {
			description: """
				Whether to require TLS for incoming or outgoing connections.

				When enabled and used for incoming connections, an identity certificate is also required. See `tls.crt_file` for
				more information.
				"""
			required: false
			type: bool: {}
		}
		key_file: {
			description: """
				Absolute path to a private key file used to identify this server.

				The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
				"""
			required: false
			type: string: examples: ["/path/to/host_certificate.key"]
		}
		key_pass: {
			description: """
				Passphrase used to unlock the encrypted key file.

				This has no effect unless `key_file` is set.
				"""
			required: false
			type: string: examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
		}
		server_name: {
			description: """
				Server name to use when using Server Name Indication (SNI).

				Only relevant for outgoing connections.
				"""
			required: false
			type: string: examples: ["www.example.com"]
		}
		verify_certificate: {
			description: """
				Enables certificate verification. For components that create a server, this requires that the
				client connections have a valid client certificate. For components that initiate requests,
				this validates that the upstream has a valid certificate.

				If enabled, certificates must not be expired and must be issued by a trusted
				issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
				certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
				so on, until the verification process reaches a root certificate.

				Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
				"""
			required: false
			type: bool: {}
		}
		verify_hostname: {
			description: """
				Enables hostname verification.

				If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
				the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.

				Only relevant for outgoing connections.

				Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
				"""
			required: false
			type: bool: {}
		}
	}
	"core::option::Option<vector_core::tls::settings::TlsSourceConfig>": object: options: {
		alpn_protocols: {
			description: """
				Sets the list of supported ALPN protocols.

				Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
				that they are defined.
				"""
			required: false
			type: array: items: type: string: examples: ["h2"]
		}
		ca_file: {
			description: """
				Absolute path to an additional CA certificate file.

				The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
				"""
			required: false
			type: string: examples: ["/path/to/certificate_authority.crt"]
		}
		client_metadata_key: {
			description: "Event field for client certificate metadata."
			required:    false
			type: string: {}
		}
		crt_file: {
			description: """
				Absolute path to a certificate file used to identify this server.

				The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
				an inline string in PEM format.

				If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
				"""
			required: false
			type: string: examples: ["/path/to/host_certificate.crt"]
		}
		enabled: {
			description: """
				Whether to require TLS for incoming or outgoing connections.

				When enabled and used for incoming connections, an identity certificate is also required. See `tls.crt_file` for
				more information.
				"""
			required: false
			type: bool: {}
		}
		key_file: {
			description: """
				Absolute path to a private key file used to identify this server.

				The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
				"""
			required: false
			type: string: examples: ["/path/to/host_certificate.key"]
		}
		key_pass: {
			description: """
				Passphrase used to unlock the encrypted key file.

				This has no effect unless `key_file` is set.
				"""
			required: false
			type: string: examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
		}
		server_name: {
			description: """
				Server name to use when using Server Name Indication (SNI).

				Only relevant for outgoing connections.
				"""
			required: false
			type: string: examples: ["www.example.com"]
		}
		verify_certificate: {
			description: """
				Enables certificate verification. For components that create a server, this requires that the
				client connections have a valid client certificate. For components that initiate requests,
				this validates that the upstream has a valid certificate.

				If enabled, certificates must not be expired and must be issued by a trusted
				issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
				certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
				so on, until the verification process reaches a root certificate.

				Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
				"""
			required: false
			type: bool: {}
		}
		verify_hostname: {
			description: """
				Enables hostname verification.

				If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
				the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.

				Only relevant for outgoing connections.

				Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
				"""
			required: false
			type: bool: {}
		}
	}
	"vector::aws::auth::AwsAuthentication": object: options: {
		access_key_id: {
			description: "The AWS access key ID."
			required:    true
			type: string: examples: ["AKIAIOSFODNN7EXAMPLE"]
		}
		assume_role: {
			description: """
				The ARN of an [IAM role][iam_role] to assume.

				[iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
				"""
			required: true
			type: string: examples: ["arn:aws:iam::123456789098:role/my_role"]
		}
		credentials_file: {
			description: "Path to the credentials file."
			required:    true
			type: string: examples: ["/my/aws/credentials"]
		}
		external_id: {
			description: """
				The optional unique external ID in conjunction with role to assume.

				[external_id]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_create_for-user_externalid.html
				"""
			required: false
			type: string: examples: ["randomEXAMPLEidString"]
		}
		imds: {
			description: "Configuration for authenticating with AWS through IMDS."
			required:    false
			type: object: options: {
				connect_timeout_seconds: {
					description: "Connect timeout for IMDS."
					required:    false
					type: uint: {
						default: 1
						unit:    "seconds"
					}
				}
				max_attempts: {
					description: "Number of IMDS retries for fetching tokens and metadata."
					required:    false
					type: uint: default: 4
				}
				read_timeout_seconds: {
					description: "Read timeout for IMDS."
					required:    false
					type: uint: {
						default: 1
						unit:    "seconds"
					}
				}
			}
		}
		load_timeout_secs: {
			description: """
				Timeout for successfully loading any credentials, in seconds.

				Relevant when the default credentials chain or `assume_role` is used.
				"""
			required: false
			type: uint: {
				examples: [
					30
				]
				unit: "seconds"
			}
		}
		profile: {
			description: """
				The credentials profile to use.

				Used to select AWS credentials from a provided credentials file.
				"""
			required: false
			type: string: {
				default: "default"
				examples: [
					"develop"
				]
			}
		}
		region: {
			description: """
				The [AWS region][aws_region] to send STS requests to.

				If not set, this defaults to the configured region
				for the service itself.

				[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
				"""
			required: false
			type: string: examples: [
				"us-west-2"
			]
		}
		secret_access_key: {
			description: "The AWS secret access key."
			required:    true
			type: string: examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
		}
		session_name: {
			description: """
				The optional [RoleSessionName][role_session_name] is a unique session identifier for your assumed role.

				Should be unique per principal or reason.
				If not set, the session name is autogenerated like assume-role-provider-1736428351340

				[role_session_name]: https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html
				"""
			required: false
			type: string: examples: ["vector-indexer-role"]
		}
		session_token: {
			description: """
				The AWS session token.
				See [AWS temporary credentials](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_temp_use-resources.html)
				"""
			required: false
			type: string: examples: ["AQoDYXdz...AQoDYXdz..."]
		}
	}
	"vector::aws::auth::ImdsAuthentication": object: options: {
		connect_timeout_seconds: {
			description: "Connect timeout for IMDS."
			required:    false
			type: uint: {
				default: 1
				unit:    "seconds"
			}
		}
		max_attempts: {
			description: "Number of IMDS retries for fetching tokens and metadata."
			required:    false
			type: uint: default: 4
		}
		read_timeout_seconds: {
			description: "Read timeout for IMDS."
			required:    false
			type: uint: {
				default: 1
				unit:    "seconds"
			}
		}
	}
	"vector::config::dot_graph::GraphConfig": object: options: {
		edge_attributes: {
			description: """
				Edge attributes to add to the edges linked to this component's node in resulting graph

				They are added to the edge as provided
				"""
			required: false
			type: object: {
				examples: [{
					example_input: {
						color: "red"
						label: "Example Edge"
						width: "5.0"
					}
				}]
				options: "*": {
					description: "A collection of graph edge attributes in graphviz DOT language, related to a single input component."
					required:    true
					type: object: {
						examples: [{
							color: "red"
							label: "Example Edge"
							width: "5.0"
						}]
						options: "*": {
							description: "A single graph edge attribute in graphviz DOT language."
							required:    true
							type: string: {}
						}
					}
				}
			}
		}
		node_attributes: {
			description: """
				Node attributes to add to this component's node in resulting graph

				They are added to the node as provided
				"""
			required: false
			type: object: {
				examples: [{
					color: "red"
					name:  "Example Node"
					width: "5.0"
				}]
				options: "*": {
					description: "A single graph node attribute in graphviz DOT language."
					required:    true
					type: string: {}
				}
			}
		}
	}
	"vector::http::KeepaliveConfig": object: options: {
		max_connection_age_jitter_factor: {
			description: """
				The factor by which to jitter the `max_connection_age_secs` value.

				A value of 0.1 means that the actual duration will be between 90% and 110% of the
				specified maximum duration.
				"""
			required: false
			type: float: default: 0.1
		}
		max_connection_age_secs: {
			description: """
				The maximum amount of time a connection may exist before it is closed by sending
				a `Connection: close` header on the HTTP response. Set this to a large value like
				`100000000` to "disable" this feature

				Only applies to HTTP/0.9, HTTP/1.0, and HTTP/1.1 requests.

				A random jitter configured by `max_connection_age_jitter_factor` is added
				to the specified duration to spread out connection storms.
				"""
			required: false
			type: uint: {
				default: 300
				examples: [
					600
				]
				unit: "seconds"
			}
		}
		tcp_keepalive: {
			description: """
				TCP keepalive settings for accepted connections.

				Configures OS-level TCP keepalive probes on accepted connections. When set, the OS
				will send keepalive probes after the specified idle time has elapsed, detecting and
				closing connections where the remote peer has disappeared without sending a FIN or
				RST packet (for example, due to an abrupt machine failure or network partition).
				"""
			required: false
			type: object: options: time_secs: {
				description: "The time to wait before starting to send TCP keepalive probes on an idle connection."
				required:    false
				type: uint: unit: "seconds"
			}
		}
	}
	"vector::http::ParameterValue": {
		object: options: {
			type: {
				description: "The parameter type, indicating how the `value` should be treated."
				required:    false
				type: string: {
					default: "string"
					enum: {
						string: "The parameter value is a plain string."
						vrl:    "The parameter value is a VRL expression that is evaluated before each request."
					}
				}
			}
			value: {
				description: "The raw value of the parameter."
				required:    true
				type: string: {}
			}
		}
		string: {}
	}
	"vector::internal_events::file::FileInternalMetricsConfig": object: options: include_file_tag: {
		description: """
			Whether or not to include the "file" tag on the component's corresponding internal metrics.

			This is useful for distinguishing between different files while monitoring. However, the tag's
			cardinality is unbounded.
			"""
		required: false
		type: bool: default: false
	}
	"vector::sinks::azure_common::config::AzureAuthentication": object: options: {
		azure_client_id: {
			description: """
				The [Azure Client ID][azure_client_id].

				[azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
				"""
			relevant_when: "azure_credential_kind = \"client_certificate_credential\" or azure_credential_kind = \"client_secret_credential\""
			required:      true
			type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_CLIENT_ID:?err}"]
		}
		azure_client_secret: {
			description: """
				The [Azure Client Secret][azure_client_secret].

				[azure_client_secret]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
				"""
			relevant_when: "azure_credential_kind = \"client_secret_credential\""
			required:      true
			type: string: examples: ["00-00~000000-0000000~0000000000000000000", "${AZURE_CLIENT_SECRET:?err}"]
		}
		azure_credential_kind: {
			description: "The kind of Azure credential to use."
			required:    true
			type: string: enum: {
				azure_cli:                         "Use Azure CLI credentials"
				client_certificate_credential:     "Use certificate credentials"
				client_secret_credential:          "Use client ID/secret credentials"
				managed_identity:                  "Use Managed Identity credentials"
				managed_identity_client_assertion: "Use Managed Identity with Client Assertion credentials"
				workload_identity:                 "Use Workload Identity credentials"
			}
		}
		azure_tenant_id: {
			description: """
				The [Azure Tenant ID][azure_tenant_id].

				[azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
				"""
			relevant_when: "azure_credential_kind = \"client_certificate_credential\" or azure_credential_kind = \"client_secret_credential\""
			required:      true
			type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_TENANT_ID:?err}"]
		}
		certificate_password: {
			description:   "The password for the client certificate, if applicable."
			relevant_when: "azure_credential_kind = \"client_certificate_credential\""
			required:      false
			type: string: examples: ["${AZURE_CLIENT_CERTIFICATE_PASSWORD}"]
		}
		certificate_path: {
			description:   "PKCS12 certificate with RSA private key."
			relevant_when: "azure_credential_kind = \"client_certificate_credential\""
			required:      true
			type: string: examples: ["path/to/certificate.pfx", "${AZURE_CLIENT_CERTIFICATE_PATH:?err}"]
		}
		client_assertion_client_id: {
			description:   "The target Client ID to use."
			relevant_when: "azure_credential_kind = \"managed_identity_client_assertion\""
			required:      true
			type: string: examples: ["00000000-0000-0000-0000-000000000000"]
		}
		client_assertion_tenant_id: {
			description:   "The target Tenant ID to use."
			relevant_when: "azure_credential_kind = \"managed_identity_client_assertion\""
			required:      true
			type: string: examples: ["00000000-0000-0000-0000-000000000000"]
		}
		client_id: {
			description: """
				The [Azure Client ID][azure_client_id]. Defaults to the value of the environment variable `AZURE_CLIENT_ID`.

				[azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
				"""
			relevant_when: "azure_credential_kind = \"workload_identity\""
			required:      false
			type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_CLIENT_ID}"]
		}
		tenant_id: {
			description: """
				The [Azure Tenant ID][azure_tenant_id]. Defaults to the value of the environment variable `AZURE_TENANT_ID`.

				[azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
				"""
			relevant_when: "azure_credential_kind = \"workload_identity\""
			required:      false
			type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_TENANT_ID}"]
		}
		token_file_path: {
			description:   "Path of a file containing a Kubernetes service account token. Defaults to the value of the environment variable `AZURE_FEDERATED_TOKEN_FILE`."
			relevant_when: "azure_credential_kind = \"workload_identity\""
			required:      false
			type: string: examples: ["/var/run/secrets/azure/tokens/azure-identity-token", "${AZURE_FEDERATED_TOKEN_FILE}"]
		}
		user_assigned_managed_identity_id: {
			description:   "The User Assigned Managed Identity to use."
			relevant_when: "azure_credential_kind = \"managed_identity\" or azure_credential_kind = \"managed_identity_client_assertion\""
			required:      false
			type: string: examples: ["00000000-0000-0000-0000-000000000000"]
		}
		user_assigned_managed_identity_id_type: {
			description: """
				The type of the User Assigned Managed Identity ID provided (Client ID, Object ID,
				or Resource ID). Defaults to Client ID.
				"""
			relevant_when: "azure_credential_kind = \"managed_identity\" or azure_credential_kind = \"managed_identity_client_assertion\""
			required:      false
			type: string: enum: {
				client_id:   "Client ID"
				object_id:   "Object ID"
				resource_id: "Resource ID"
			}
		}
	}
	"vector::sinks::splunk_hec::common::acknowledgements::HecClientAcknowledgementsConfig": object: options: {
		enabled: {
			description: """
				Controls whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source that supports end-to-end
				acknowledgements that is connected to that sink waits for events
				to be acknowledged by **all connected sinks** before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
		indexer_acknowledgements_enabled: {
			description: """
				Controls if the sink integrates with [Splunk HEC indexer acknowledgements][splunk_indexer_ack_docs] for end-to-end acknowledgements.

				[splunk_indexer_ack_docs]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck
				"""
			required: false
			type: bool: default: true
		}
		max_pending_acks: {
			description: """
				The maximum number of pending acknowledgements from events sent to the Splunk HEC collector.

				Once reached, the sink begins applying backpressure.
				"""
			required: false
			type: uint: default: 1000000
		}
		query_interval: {
			description: "The amount of time to wait between queries to the Splunk HEC indexer acknowledgement endpoint."
			required:    false
			type: uint: {
				default: 10
				unit:    "seconds"
			}
		}
		retry_limit: {
			description: "The maximum number of times an acknowledgement ID is queried for its status."
			required:    false
			type: uint: default: 30
		}
	}
	"vector::sinks::util::adaptive_concurrency::AdaptiveConcurrencySettings": object: options: {
		decrease_ratio: {
			description: """
				The fraction of the current value to set the new concurrency limit when decreasing the limit.

				Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
				when latency increases.

				**Note**: The new limit is rounded down after applying this ratio.
				"""
			required: false
			type: float: default: 0.9
		}
		ewma_alpha: {
			description: """
				The weighting of new measurements compared to older measurements.

				Valid values are greater than `0` and less than `1`.

				ARC uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with
				the current RTT. Smaller values cause this reference to adjust more slowly, which may be useful if a service has
				unusually high response variability.
				"""
			required: false
			type: float: default: 0.4
		}
		initial_concurrency: {
			description: """
				The initial concurrency limit to use. If not specified, the initial limit is 1 (no concurrency).

				Datadog recommends setting this value to your service's average limit if you're seeing that it takes a
				long time to ramp up adaptive concurrency after a restart. You can find this value by looking at the
				`adaptive_concurrency_limit` metric.
				"""
			required: false
			type: uint: default: 1
		}
		max_concurrency_limit: {
			description: """
				The maximum concurrency limit.

				The adaptive request concurrency limit does not go above this bound. This is put in place as a safeguard.
				"""
			required: false
			type: uint: default: 200
		}
		rtt_deviation_scale: {
			description: """
				Scale of RTT deviations which are not considered anomalous.

				Valid values are greater than or equal to `0`, and reasonable values range from `1.0` to `3.0`.

				When calculating the past RTT average, a secondary “deviation” value is also computed that indicates how variable
				those values are. That deviation is used when comparing the past RTT average to the current measurements, so we
				can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
				an appropriate range. Larger values cause the algorithm to ignore larger increases in the RTT.
				"""
			required: false
			type: float: default: 2.5
		}
	}
	"vector::sinks::util::batch::BatchConfig<vector::sinks::greptimedb::GreptimeDBDefaultBatchSettings>": object: options: {
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
			type: uint: {
				default: 20
				unit:    "events"
			}
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
	"vector::sinks::util::batch::BatchConfig<vector::sinks::splunk_hec::common::util::SplunkHecDefaultBatchSettings>": object: options: {
		max_bytes: {
			description: """
				The maximum size of a batch that is processed by a sink.

				This is based on the uncompressed size of the batched events, before they are
				serialized or compressed.
				"""
			required: false
			type: uint: {
				default: 1000000
				unit:    "bytes"
			}
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
	"vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::BulkSizeBasedDefaultBatchSettings>": object: options: {
		max_bytes: {
			description: """
				The maximum size of a batch that is processed by a sink.

				This is based on the uncompressed size of the batched events, before they are
				serialized or compressed.
				"""
			required: false
			type: uint: {
				default: 10000000
				unit:    "bytes"
			}
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
				default: 300.0
				unit:    "seconds"
			}
		}
	}
	"vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>": object: options: {
		max_bytes: {
			description: """
				The maximum size of a batch that is processed by a sink.

				This is based on the uncompressed size of the batched events, before they are
				serialized or compressed.
				"""
			required: false
			type: uint: {
				default: 10000000
				unit:    "bytes"
			}
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
	"vector::sinks::util::http::RequestConfig": object: options: {
		adaptive_concurrency: {
			description: """
				Configuration of adaptive concurrency parameters.

				These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
				unstable performance and sink behavior. Proceed with caution.
				"""
			required: false
			type: object: options: {
				decrease_ratio: {
					description: """
						The fraction of the current value to set the new concurrency limit when decreasing the limit.

						Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
						when latency increases.

						**Note**: The new limit is rounded down after applying this ratio.
						"""
					required: false
					type: float: default: 0.9
				}
				ewma_alpha: {
					description: """
						The weighting of new measurements compared to older measurements.

						Valid values are greater than `0` and less than `1`.

						ARC uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with
						the current RTT. Smaller values cause this reference to adjust more slowly, which may be useful if a service has
						unusually high response variability.
						"""
					required: false
					type: float: default: 0.4
				}
				initial_concurrency: {
					description: """
						The initial concurrency limit to use. If not specified, the initial limit is 1 (no concurrency).

						Datadog recommends setting this value to your service's average limit if you're seeing that it takes a
						long time to ramp up adaptive concurrency after a restart. You can find this value by looking at the
						`adaptive_concurrency_limit` metric.
						"""
					required: false
					type: uint: default: 1
				}
				max_concurrency_limit: {
					description: """
						The maximum concurrency limit.

						The adaptive request concurrency limit does not go above this bound. This is put in place as a safeguard.
						"""
					required: false
					type: uint: default: 200
				}
				rtt_deviation_scale: {
					description: """
						Scale of RTT deviations which are not considered anomalous.

						Valid values are greater than or equal to `0`, and reasonable values range from `1.0` to `3.0`.

						When calculating the past RTT average, a secondary “deviation” value is also computed that indicates how variable
						those values are. That deviation is used when comparing the past RTT average to the current measurements, so we
						can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
						an appropriate range. Larger values cause the algorithm to ignore larger increases in the RTT.
						"""
					required: false
					type: float: default: 2.5
				}
			}
		}
		concurrency: {
			description: """
				Configuration for outbound request concurrency.

				This can be set either to one of the below enum values or to a positive integer, which denotes
				a fixed concurrency limit.
				"""
			required: false
			type: {
				string: {
					default: "adaptive"
					enum: {
						adaptive: """
										Concurrency is managed by Vector's [Adaptive Request Concurrency][arc] feature.

										[arc]: https://vector.dev/docs/architecture/arc/
										"""
						none: """
										A fixed concurrency of 1.

										Only one request can be outstanding at any given time.
										"""
					}
				}
				uint: {}
			}
		}
		headers: {
			description: "Additional HTTP headers to add to every HTTP request."
			required:    false
			type: object: {
				examples: [{
					Accept:               "text/plain"
					"X-Event-Level":      "{{level}}"
					"X-Event-Timestamp":  "{{timestamp}}"
					"X-My-Custom-Header": "A-Value"
				}]
				options: "*": {
					description: "An HTTP request header and its value. Both header names and values support templating with event data."
					required:    true
					type: string: {}
				}
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
		retry_attempts: {
			description: "The maximum number of retries to make for failed requests."
			required:    false
			type: uint: {
				default: 9223372036854775807
				unit:    "retries"
			}
		}
		retry_initial_backoff_secs: {
			description: """
				The amount of time to wait before attempting the first retry for a failed request.

				After the first retry has failed, the Fibonacci sequence is used to select future backoffs.
				"""
			required: false
			type: uint: {
				default: 1
				unit:    "seconds"
			}
		}
		retry_jitter_mode: {
			description: "The jitter mode to use for retry backoff behavior."
			required:    false
			type: string: {
				default: "Full"
				enum: {
					Full: """
						Full jitter.

						The random delay is anywhere from 0 up to the maximum current delay calculated by the backoff
						strategy.

						Incorporating full jitter into your backoff strategy can greatly reduce the likelihood
						of creating accidental denial of service (DoS) conditions against your own systems when
						many clients are recovering from a failure state.
						"""
					None: "No jitter."
				}
			}
		}
		retry_max_duration_secs: {
			description: "The maximum amount of time to wait between retries."
			required:    false
			type: uint: {
				default: 30
				unit:    "seconds"
			}
		}
		timeout_secs: {
			description: """
				The time a request can take before being aborted.

				Datadog highly recommends that you do not lower this value below the service's internal timeout, as this could
				create orphaned requests, pile on retries, and result in duplicate data downstream.
				"""
			required: false
			type: uint: {
				default: 60
				unit:    "seconds"
			}
		}
	}
	"vector::sinks::util::service::TowerRequestConfig": object: options: {
		adaptive_concurrency: {
			description: """
				Configuration of adaptive concurrency parameters.

				These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
				unstable performance and sink behavior. Proceed with caution.
				"""
			required: false
			type: object: options: {
				decrease_ratio: {
					description: """
						The fraction of the current value to set the new concurrency limit when decreasing the limit.

						Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
						when latency increases.

						**Note**: The new limit is rounded down after applying this ratio.
						"""
					required: false
					type: float: default: 0.9
				}
				ewma_alpha: {
					description: """
						The weighting of new measurements compared to older measurements.

						Valid values are greater than `0` and less than `1`.

						ARC uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with
						the current RTT. Smaller values cause this reference to adjust more slowly, which may be useful if a service has
						unusually high response variability.
						"""
					required: false
					type: float: default: 0.4
				}
				initial_concurrency: {
					description: """
						The initial concurrency limit to use. If not specified, the initial limit is 1 (no concurrency).

						Datadog recommends setting this value to your service's average limit if you're seeing that it takes a
						long time to ramp up adaptive concurrency after a restart. You can find this value by looking at the
						`adaptive_concurrency_limit` metric.
						"""
					required: false
					type: uint: default: 1
				}
				max_concurrency_limit: {
					description: """
						The maximum concurrency limit.

						The adaptive request concurrency limit does not go above this bound. This is put in place as a safeguard.
						"""
					required: false
					type: uint: default: 200
				}
				rtt_deviation_scale: {
					description: """
						Scale of RTT deviations which are not considered anomalous.

						Valid values are greater than or equal to `0`, and reasonable values range from `1.0` to `3.0`.

						When calculating the past RTT average, a secondary “deviation” value is also computed that indicates how variable
						those values are. That deviation is used when comparing the past RTT average to the current measurements, so we
						can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
						an appropriate range. Larger values cause the algorithm to ignore larger increases in the RTT.
						"""
					required: false
					type: float: default: 2.5
				}
			}
		}
		concurrency: {
			description: """
				Configuration for outbound request concurrency.

				This can be set either to one of the below enum values or to a positive integer, which denotes
				a fixed concurrency limit.
				"""
			required: false
			type: {
				string: {
					default: "adaptive"
					enum: {
						adaptive: """
										Concurrency is managed by Vector's [Adaptive Request Concurrency][arc] feature.

										[arc]: https://vector.dev/docs/architecture/arc/
										"""
						none: """
										A fixed concurrency of 1.

										Only one request can be outstanding at any given time.
										"""
					}
				}
				uint: {}
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
		retry_attempts: {
			description: "The maximum number of retries to make for failed requests."
			required:    false
			type: uint: {
				default: 9223372036854775807
				unit:    "retries"
			}
		}
		retry_initial_backoff_secs: {
			description: """
				The amount of time to wait before attempting the first retry for a failed request.

				After the first retry has failed, the Fibonacci sequence is used to select future backoffs.
				"""
			required: false
			type: uint: {
				default: 1
				unit:    "seconds"
			}
		}
		retry_jitter_mode: {
			description: "The jitter mode to use for retry backoff behavior."
			required:    false
			type: string: {
				default: "Full"
				enum: {
					Full: """
						Full jitter.

						The random delay is anywhere from 0 up to the maximum current delay calculated by the backoff
						strategy.

						Incorporating full jitter into your backoff strategy can greatly reduce the likelihood
						of creating accidental denial of service (DoS) conditions against your own systems when
						many clients are recovering from a failure state.
						"""
					None: "No jitter."
				}
			}
		}
		retry_max_duration_secs: {
			description: "The maximum amount of time to wait between retries."
			required:    false
			type: uint: {
				default: 30
				unit:    "seconds"
			}
		}
		timeout_secs: {
			description: """
				The time a request can take before being aborted.

				Datadog highly recommends that you do not lower this value below the service's internal timeout, as this could
				create orphaned requests, pile on retries, and result in duplicate data downstream.
				"""
			required: false
			type: uint: {
				default: 60
				unit:    "seconds"
			}
		}
	}
	"vector::sources::splunk_hec::CodecConfig": object: options: {
		decoding: {
			description: """
				Decoding configuration applied to the payload.

				When unset, the endpoint preserves its existing per-endpoint default
				behavior. When set, the endpoint-selected payload is processed through
				`framing` and `decoding`, and a single payload can fan out to multiple
				events.
				"""
			required: false
			type: object: options: {
				avro: {
					description:   "Apache Avro-specific encoder options."
					relevant_when: "codec = \"avro\""
					required:      true
					type: object: options: {
						schema: {
							description: """
															The Avro schema definition.
															**Note**: The following [`apache_avro::types::Value`] variants are *not* supported:
															* `Date`
															* `Decimal`
															* `Duration`
															* `Fixed`
															* `TimeMillis`
															"""
							required: true
							type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
						}
						strip_schema_id_prefix: {
							description: "For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to `true` to strip the schema ID prefix, as described in [Confluent Kafka's documentation](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format)."
							required:    true
							type: bool: {}
						}
					}
				}
				codec: {
					description: "The codec to use for decoding events."
					required:    true
					type: string: enum: {
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
				gelf: {
					description:   "GELF-specific decoding options."
					relevant_when: "codec = \"gelf\""
					required:      false
					type: object: options: {
						lossy: {
							description: """
															Determines whether to replace invalid UTF-8 sequences instead of failing.

															When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

															[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
															"""
							required: false
							type: bool: default: true
						}
						validation: {
							description: "Configures the decoding validation mode."
							required:    false
							type: string: {
								default: "strict"
								enum: {
									relaxed: """
																		Uses more relaxed validation that skips strict GELF specification checks.

																		This mode does not treat specification violations as errors, allowing the decoder
																		to accept messages from sources that don't strictly follow the GELF spec.
																		"""
									strict: "Uses strict validation that closely follows the GELF spec."
								}
							}
						}
					}
				}
				influxdb: {
					description:   "Influxdb-specific decoding options."
					relevant_when: "codec = \"influxdb\""
					required:      false
					type: object: options: lossy: {
						description: """
															Determines whether to replace invalid UTF-8 sequences instead of failing.

															When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

															[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
															"""
						required: false
						type: bool: default: true
					}
				}
				json: {
					description:   "JSON-specific decoding options."
					relevant_when: "codec = \"json\""
					required:      false
					type: object: options: lossy: {
						description: """
															Determines whether to replace invalid UTF-8 sequences instead of failing.

															When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

															[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
															"""
						required: false
						type: bool: default: true
					}
				}
				native_json: {
					description:   "Vector's native JSON-specific decoding options."
					relevant_when: "codec = \"native_json\""
					required:      false
					type: object: options: lossy: {
						description: """
															Determines whether to replace invalid UTF-8 sequences instead of failing.

															When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

															[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
															"""
						required: false
						type: bool: default: true
					}
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
					type: object: options: lossy: {
						description: """
															Determines whether to replace invalid UTF-8 sequences instead of failing.

															When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

															[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
															"""
						required: false
						type: bool: default: true
					}
				}
				vrl: {
					description:   "VRL-specific decoding options."
					relevant_when: "codec = \"vrl\""
					required:      true
					type: object: options: {
						source: {
							description: """
															The [Vector Remap Language][vrl] (VRL) program to execute for each event.
															The final contents of the `.` target are used as the decoding result.
															Compilation errors or use of `abort` in the program result in a decoding error.

															[vrl]: https://vector.dev/docs/reference/vrl
															"""
							required: true
							type: string: {}
						}
						timezone: {
							description: """
															The name of the timezone to apply to timestamp conversions that do not contain an explicit
															time zone. The time zone name may be any name in the [TZ database][tz_database], or `local`
															to indicate system local time.

															If not set, `local` is used.

															[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
															"""
							required: false
							type: string: examples: ["local", "America/New_York", "EST5EDT"]
						}
					}
				}
			}
		}
		framing: {
			description: """
				Framing configuration applied to the payload.

				Only used when `decoding` is also set. Defaults to a per-codec choice
				(typically `bytes`) that produces one event per payload.
				"""
			required: false
			type: object: options: {
				character_delimited: {
					description:   "Options for the character delimited decoder."
					relevant_when: "method = \"character_delimited\""
					required:      true
					type: object: options: {
						delimiter: {
							description: "The character that delimits byte sequences."
							required:    true
							type: ascii_char: {}
						}
						max_length: {
							description: """
															The maximum length of the byte buffer.

															This length does *not* include the trailing delimiter.

															By default, no maximum length is enforced. If events are malformed, this can lead to
															additional resource usage as events continue to be buffered in memory, and can potentially
															lead to memory exhaustion in extreme cases.

															If there is a risk of processing malformed data, such as logs with user-controlled input,
															consider setting the maximum length to a reasonably large value as a safety net. This
															prevents processing from being unbounded.
															"""
							required: false
							type: uint: {}
						}
						oversized_action: {
							description: """
															The behavior when a frame exceeds `max_length`.

															When set to `drop` (the default), the entire oversized frame is discarded.
															When set to `truncate`, the frame is truncated to `max_length` bytes and the
															remainder is discarded up to the next delimiter.

															This option has no effect if `max_length` is not set.
															"""
							required: false
							type: string: {
								default: "drop"
								enum: {
									drop: "Drop the entire oversized frame."
									truncate: """
																		Truncate the frame to the maximum allowed size and emit the partial content.

																		The remainder of the oversized frame is discarded up to the next delimiter.
																		"""
								}
							}
						}
					}
				}
				chunked_gelf: {
					description:   "Options for the chunked GELF decoder."
					relevant_when: "method = \"chunked_gelf\""
					required:      false
					type: object: options: {
						decompression: {
							description: "Decompression configuration for GELF messages."
							required:    false
							type: string: {
								default: "Auto"
								enum: {
									Auto: "Automatically detect the decompression method based on the magic bytes of the message."
									Gzip: "Use Gzip decompression."
									None: "Do not decompress the message."
									Zlib: "Use Zlib decompression."
								}
							}
						}
						max_length: {
							description: """
															The maximum length of a single GELF message, in bytes. Messages longer than this length are
															dropped. If this option is not set, the decoder does not limit the length of messages and
															the per-message memory is unbounded.

															**Note**: A message can be composed of multiple chunks, and this limit applies to the whole
															message, not to individual chunks.

															This limit takes into account only the message payload. GELF header bytes are excluded from the calculation.
															The message payload is the concatenation of all chunk payloads.
															"""
							required: false
							type: uint: {}
						}
						pending_messages_limit: {
							description: """
															The maximum number of pending incomplete messages. If this limit is reached, the decoder starts
															dropping chunks of new messages, ensuring the memory usage of the decoder's state is bounded.
															If this option is not set, the decoder does not limit the number of pending messages and the memory usage
															of its messages buffer can grow unbounded. This matches Graylog Server's behavior.
															"""
							required: false
							type: uint: {}
						}
						timeout_secs: {
							description: """
															The timeout, in seconds, for a message to be fully received. If the timeout is reached, the
															decoder drops all received chunks for the timed-out message.
															"""
							required: false
							type: float: default: 5.0
						}
					}
				}
				length_delimited: {
					description:   "Options for the length delimited decoder."
					relevant_when: "method = \"length_delimited\""
					required:      true
					type: object: options: {
						length_field_is_big_endian: {
							description: "Length field byte order (little or big endian)"
							required:    false
							type: bool: default: true
						}
						length_field_length: {
							description: "Number of bytes representing the field length"
							required:    false
							type: uint: default: 4
						}
						length_field_offset: {
							description: "Number of bytes in the header before the length field"
							required:    false
							type: uint: default: 0
						}
						max_frame_length: {
							description: "Maximum frame length"
							required:    false
							type: uint: default: 8388608
						}
					}
				}
				max_frame_length: {
					description:   "Maximum frame length"
					relevant_when: "method = \"varint_length_delimited\""
					required:      false
					type: uint: default: 8388608
				}
				method: {
					description: "The framing method."
					required:    true
					type: string: enum: {
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
				newline_delimited: {
					description:   "Options for the newline delimited decoder."
					relevant_when: "method = \"newline_delimited\""
					required:      false
					type: object: options: {
						max_length: {
							description: """
															The maximum length of the byte buffer.

															This length does *not* include the trailing delimiter.

															By default, no maximum length is enforced. If events are malformed, this can lead to
															additional resource usage as events continue to be buffered in memory, and can potentially
															lead to memory exhaustion in extreme cases.

															If there is a risk of processing malformed data, such as logs with user-controlled input,
															consider setting the maximum length to a reasonably large value as a safety net. This
															prevents processing from being unbounded.
															"""
							required: false
							type: uint: {}
						}
						oversized_action: {
							description: """
															The behavior when a line exceeds `max_length`.

															When set to `drop` (the default), the entire oversized line is discarded.
															When set to `truncate`, the line is truncated to `max_length` bytes and the
															remainder is discarded up to the next newline.

															This option has no effect if `max_length` is not set.
															"""
							required: false
							type: string: {
								default: "drop"
								enum: {
									drop: "Drop the entire oversized frame."
									truncate: """
																		Truncate the frame to the maximum allowed size and emit the partial content.

																		The remainder of the oversized frame is discarded up to the next delimiter.
																		"""
								}
							}
						}
					}
				}
				octet_counting: {
					description:   "Options for the octet counting decoder."
					relevant_when: "method = \"octet_counting\""
					required:      false
					type: object: options: max_length: {
						description: "The maximum length of the byte buffer."
						required:    false
						type: uint: {}
					}
				}
			}
		}
	}
	"vector::sources::util::grpc::GrpcKeepaliveConfig": object: options: {
		max_connection_age_grace_secs: {
			description: """
				The grace period added to `max_connection_age_secs` before the server closes the connection.

				This setting only applies when `max_connection_age_secs` is set.
				"""
			required: false
			type: uint: {
				examples: [
					30
				]
				unit: "seconds"
			}
		}
		max_connection_age_secs: {
			description: """
				The maximum amount of time a connection may exist before the server closes it.

				When unset, connections are not closed based on age.
				"""
			required: false
			type: uint: {
				examples: [
					300
				]
				unit: "seconds"
			}
		}
	}
	"vector::transforms::tag_cardinality_limit::config::InternalMetricsConfig": object: options: include_extended_tags: {
		description: """
			Whether to include extended tags (metric_name, tag_key) in the `tag_value_limit_exceeded_total` metric.

			This helps identify which metrics and tag keys are hitting cardinality limits, but can significantly
			increase metric cardinality. Defaults to `false` because these tags have potentially unbounded cardinality.
			"""
		required: false
		type: bool: default: false
	}
	"vector::transforms::tag_cardinality_limit::config::PerTagConfig": object: options: {
		cache_size_per_key: {
			description: """
				Override the bloom filter cache size for this specific tag key.
				Only valid in `probabilistic` mode; setting this in `exact` mode is a configuration error.
				Inherits from the enclosing config when unset.
				"""
			relevant_when: "mode = \"limit_override\""
			required:      false
			type: uint: {}
		}
		mode: {
			description: "Controls how this tag key is handled."
			required:    true
			type: string: enum: {
				excluded: """
					Opt this tag out of cardinality tracking entirely. All values pass through
					without being recorded or checked against any `value_limit`.
					"""
				limit_override: """
					Track this tag with a per-tag value limit. All other settings are inherited from
					the enclosing config.
					"""
			}
		}
		value_limit: {
			description:   "Maximum number of distinct values to accept for this tag key."
			relevant_when: "mode = \"limit_override\""
			required:      true
			type: uint: {}
		}
	}
	"vector_core::config::AcknowledgementsConfig": object: options: enabled: {
		description: """
			Controls whether or not end-to-end acknowledgements are enabled.

			When enabled for a sink, any source that supports end-to-end
			acknowledgements that is connected to that sink waits for events
			to be acknowledged by **all connected sinks** before acknowledging them at the source.

			Enabling or disabling acknowledgements at the sink level takes precedence over any global
			[`acknowledgements`][global_acks] configuration.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			"""
		required: false
		type: bool: {}
	}
	"vector_core::config::SourceAcknowledgementsConfig": object: options: enabled: {
		description: "Whether or not end-to-end acknowledgements are enabled for this source."
		required:    false
		type: bool: {}
	}
	"vector_core::config::proxy::ProxyConfig": object: options: {
		enabled: {
			description: "Enables proxying support."
			required:    false
			type: bool: default: true
		}
		http: {
			description: """
				Proxy endpoint to use when proxying HTTP traffic.

				Must be a valid URI string.
				"""
			required: false
			type: string: examples: ["http://foo.bar:3128"]
		}
		https: {
			description: """
				Proxy endpoint to use when proxying HTTPS traffic.

				Must be a valid URI string.
				"""
			required: false
			type: string: examples: ["http://foo.bar:3128"]
		}
		no_proxy: {
			description: """
				A list of hosts to avoid proxying.

				Multiple patterns are allowed:

				| Pattern             | Example match                                                               |
				| ------------------- | --------------------------------------------------------------------------- |
				| Domain names        | `example.com` matches requests to `example.com`                     |
				| Wildcard domains    | `.example.com` matches requests to `example.com` and its subdomains |
				| IP addresses        | `127.0.0.1` matches requests to `127.0.0.1`                         |
				| [CIDR][cidr] blocks | `192.168.0.0/16` matches requests to any IP addresses in this range     |
				| Splat               | `*` matches all hosts                                                   |

				[cidr]: https://en.wikipedia.org/wiki/Classless_Inter-Domain_Routing
				"""
			required: false
			type: array: {
				default: []
				items: type: string: examples: ["localhost", ".foo.bar", "*"]
			}
		}
	}
}
