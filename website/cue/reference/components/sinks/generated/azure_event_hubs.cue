package metadata

generated: components: sinks: azure_event_hubs: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
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
	}
	batch_enabled: {
		description: """
			Whether to batch events before sending.

			When enabled, events are accumulated per partition and sent as an `EventDataBatch`,
			preserving per-partition ordering. When disabled, each event is sent individually.
			"""
		required: false
		type: bool: default: true
	}
	batch_max_events: {
		description: """
			Maximum number of events to accumulate before flushing a batch.

			Only used when `batch_enabled` is `true`.
			"""
		required: false
		type: uint: {
			default: 100
			examples: [
				100,
			]
		}
	}
	batch_timeout_secs: {
		description: """
			Maximum time to wait before flushing a batch, in seconds.

			Only used when `batch_enabled` is `true`.
			"""
		required: false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	connection_string: {
		description: """
			The connection string for the Event Hubs namespace.

			If not set, authentication falls back to `azure_identity` (e.g., Managed Identity).
			In that case, `namespace` and `event_hub_name` must be provided.
			"""
		required: false
		type: string: examples: ["Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=mykeyname;SharedAccessKey=mykey;EntityPath=my-event-hub"]
	}
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type: object: options: {
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
						required: false
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
					"""
				relevant_when: "codec = \"json\" or codec = \"text\""
				required:      false
				type: string: {
					default: "single"
					enum: {
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
	}
	event_hub_name: {
		description: "The name of the Event Hub to send events to."
		required:    false
		type: string: examples: ["my-event-hub"]
	}
	namespace: {
		description: """
			The fully qualified Event Hubs namespace host.

			Required when not using a connection string.
			"""
		required: false
		type: string: examples: ["mynamespace.servicebus.windows.net"]
	}
	partition_id_field: {
		description: """
			The log field to use as the Event Hubs partition ID.

			If set, events are routed to the specified partition. If not set,
			Event Hubs automatically selects a partition (round-robin).
			"""
		required: false
		type: string: {}
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
	retry_initial_delay_ms: {
		description: "Initial delay before the first retry, in milliseconds."
		required:    false
		type: uint: {
			default: 200
			unit:    "milliseconds"
		}
	}
	retry_max_elapsed_secs: {
		description: "Maximum total time for all retry attempts, in seconds."
		required:    false
		type: uint: {
			default: 60
			unit:    "seconds"
		}
	}
	retry_max_retries: {
		description: """
			Maximum number of retry attempts for failed sends.

			The SDK uses exponential backoff between retries.
			"""
		required: false
		type: uint: {
			default: 8
			examples: [
				8,
			]
		}
	}
}
