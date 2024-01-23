package metadata

base: components: sinks: file: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source connected to that sink, where the source supports
				end-to-end acknowledgements as well, waits for events to be acknowledged by the sink
				before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
	}
	compression: {
		description: "Compression configuration."
		required:    false
		type: string: {
			default: "none"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				zstd: """
					[Zstandard][zstd] compression.

					[zstd]: https://facebook.github.io/zstd/
					"""
			}
		}
	}
	encoding: {
		description: "Configures how events are encoded into raw bytes."
		required:    true
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
			codec: {
				description: "The codec to use for encoding events."
				required:    true
				type: string: enum: {
					avro: """
						Encodes an event as an [Apache Avro][apache_avro] message.

						[apache_avro]: https://avro.apache.org/
						"""
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

						Other GELF codecs such as Loki's, use a [Go SDK][implementation] that is maintained
						by Graylog, and is much more relaxed than the GELF spec.

						Going forward, Vector will use that [Go SDK][implementation] as the reference implementation, which means
						the codec may continue to relax the enforcement of specification.

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
																Set the capacity (in bytes) of the internal buffer used in the CSV writer.
																This defaults to a reasonable setting.
																"""
						required: false
						type: uint: default: 8192
					}
					delimiter: {
						description: "The field delimiter to use when writing CSV."
						required:    false
						type: uint: default: 44
					}
					double_quote: {
						description: """
																Enable double quote escapes.

																This is enabled by default, but it may be disabled. When disabled, quotes in
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

																To use this, `double_quotes` needs to be disabled as well otherwise it is ignored.
																"""
						required: false
						type: uint: default: 34
					}
					fields: {
						description: """
																Configures the fields that will be encoded, as well as the order in which they
																appear in the output.

																If a field is not present in the event, the output will be an empty string.

																Values of type `Array`, `Object`, and `Regex` are not supported and the
																output will be an empty string.
																"""
						required: true
						type: array: items: type: string: {}
					}
					quote: {
						description: "The quote character to use when writing CSV."
						required:    false
						type: uint: default: 34
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
																			Namely, when writing a field that does not parse as a valid float or integer,
																			then quotes are used even if they aren't strictly necessary.
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
			metric_tag_values: {
				description: """
					Controls how metric tag values are encoded.

					When set to `single`, only the last non-bare value of tags are displayed with the
					metric.  When set to `full`, all metric tags are exposed as separate assignments.
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

																This file is the output of `protoc -o <path> ...`
																"""
						required: true
						type: string: examples: ["/etc/vector/protobuf_descriptor_set.desc"]
					}
					message_type: {
						description: "The name of the message type to use for serializing."
						required:    true
						type: string: examples: ["package.Message"]
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
					unix_us:    "Represent the timestamp as a Unix timestamp in microseconds"
				}
			}
		}
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited encoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type: object: options: delimiter: {
					description: "The ASCII (7-bit) character that delimits byte sequences."
					required:    true
					type: uint: {}
				}
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
				}
			}
		}
	}
	idle_timeout_secs: {
		description: """
			The amount of time that a file can be idle and stay open.

			After not receiving any events in this amount of time, the file is flushed and closed.
			"""
		required: false
		type: uint: {
			default: 30
			examples: [
				600,
			]
			unit: "seconds"
		}
	}
	internal_metrics: {
		description: "Configuration of internal metrics for file-based components."
		required:    false
		type: object: options: include_file_tag: {
			description: """
				Whether or not to include the "file" tag on the component's corresponding internal metrics.

				This is useful for distinguishing between different files while monitoring. However, the tag's
				cardinality is unbounded.
				"""
			required: false
			type: bool: default: false
		}
	}
	path: {
		description: """
			File path to write events to.

			Compression format extension must be explicit.
			"""
		required: true
		type: string: {
			examples: ["/tmp/vector-%Y-%m-%d.log", "/tmp/application-{{ application_id }}-%Y-%m-%d.log", "/tmp/vector-%Y-%m-%d.log.zst"]
			syntax: "template"
		}
	}
	timezone: {
		description: """
			Timezone to use for any date specifiers in template strings.

			This can refer to any valid timezone as defined in the [TZ database][tzdb], or "local" which refers to the system local timezone. It will default to the [globally configured timezone](https://vector.dev/docs/reference/configuration/global-options/#timezone).

			[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
}
