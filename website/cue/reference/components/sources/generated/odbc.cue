package metadata

generated: components: sources: odbc: configuration: {
	connection_string: {
		description: """
			The connection string to use for ODBC.
			If the `connection_string_filepath` is set, this value is ignored.
			"""
		required: true
		type: string: examples: ["driver={MariaDB Unicode};server=<ip or host>;port=<port number>;database=<database name>;uid=<user>;pwd=<password>"]
	}
	connection_string_filepath: {
		description: """
			The path to the file that contains the connection string.
			If this is not set or the file at that path does not exist, the `connection_string` field is used instead.
			"""
		required: false
		type: string: examples: ["driver={MariaDB Unicode};server=<ip or host>;port=<port number>;database=<database name>;uid=<user>;pwd=<password>"]
	}
	decoding: {
		description: "Decoder to use for query results."
		required:    false
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
						description: """
																For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to `true` to strip the schema ID prefix.
																According to [Confluent Kafka's document](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format).
																"""
						required: true
						type: bool: {}
					}
				}
			}
			codec: {
				description: "The codec to use for decoding events."
				required:    false
				type: string: {
					default: "bytes"
					enum: {
						avro: """
															Decodes the raw bytes as as an [Apache Avro][apache_avro] message.

															[apache_avro]: https://avro.apache.org/
															"""
						bytes: "Uses the raw bytes as-is."
						gelf: """
															Decodes the raw bytes as a [GELF][gelf] message.

															This codec is experimental for the following reason:

															The GELF specification is more strict than the actual Graylog receiver.
															Vector's decoder adheres more strictly to the GELF spec, with
															the exception that some characters such as `@`  are allowed in field names.

															Other GELF codecs such as Loki's, use a [Go SDK][implementation] that is maintained
															by Graylog, and is much more relaxed than the GELF spec.

															Going forward, Vector will use that [Go SDK][implementation] as the reference implementation, which means
															the codec may continue to relax the enforcement of specification.

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

															This decoder can output all types of events (logs, metrics, traces).

															This codec is **[experimental][experimental]**.

															[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
															[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
															"""
						native_json: """
															Decodes the raw bytes as [native JSON format][vector_native_json].

															This decoder can output all types of events (logs, metrics, traces).

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

																You can read more [here](https://buf.build/docs/reference/images/#how-buf-images-work).
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
																in the `.proto` file (e.g., `jobDescription` instead of `job_description`).

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

					The deserializer tries to parse signal types in the order specified. This allows you to optimize
					performance when you know the expected signal types. For example, if you only receive
					traces, set this to `["traces"]` to avoid attempting to parse as logs or metrics first.

					If not specified, defaults to trying all types in order: logs, metrics, traces.
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
																Note that the final contents of the `.` target is used as the decoding result.
																Compilation error or use of 'abort' in a program results in a decoding error.

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
	last_run_metadata_path: {
		description: """
			The path to the file where the last row of the result set is saved.
			The last row of the result set is saved in JSON format.
			This file provides parameters for the SQL query in the next scheduled run.
			If the file does not exist or the path is not specified, the initial value from `statement_init_params` is used.

			# Examples

			If `tracking_columns = ["id", "name"]`, it is saved as the following JSON data.

			```json
			{"id":1, "name": "vector"}
			```
			"""
		required: false
		type: string: examples: ["/path/to/tracking.json"]
	}
	odbc_batch_size: {
		description: """
			Number of rows to fetch per batch from the ODBC driver.
			The default is 100.
			"""
		required: false
		type: uint: {
			default: 100
			examples: [
				100,
			]
		}
	}
	odbc_default_timezone: {
		description: """
			Timezone applied to database date/time columns that lack timezone information.
			The default is UTC.
			"""
		required: false
		type: string: {
			default: "UTC"
			examples: [
				"UTC",
			]
		}
	}
	odbc_max_str_limit: {
		description: """
			Maximum string length for ODBC driver operations.
			The default is 4096.
			"""
		required: false
		type: uint: {
			default: 100
			examples: [
				4096,
			]
		}
	}
	schedule: {
		description: """
			Cron expression used to schedule database queries.
			When omitted, the statement runs only once by default.
			"""
		required: false
		type: string: {}
	}
	schedule_timezone: {
		description: """
			The timezone to use for the `schedule`.
			Typically the timezone used when evaluating the cron expression.
			The default is UTC.

			[Wikipedia]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: {
			default: "UTC"
			examples: [
				"UTC",
			]
		}
	}
	statement: {
		description: """
			The SQL statement to execute.
			This SQL statement is executed periodically according to the `schedule`.
			Defaults to `None`. If no SQL statement is provided, the source returns an error.
			If the `statement_filepath` is set, this value is ignored.
			"""
		required: false
		type: string: examples: ["SELECT * FROM users WHERE id = ?"]
	}
	statement_filepath: {
		description: """
			The path to the file that contains the SQL statement.
			If this is unset or the file cannot be read, the value from `statement` is used instead.
			"""
		required: false
		type: string: {}
	}
	statement_init_params: {
		description: """
			Initial parameters for the first execution of the statement.
			Used if `last_run_metadata_path` does not exist.
			Values must be strings and follow the parameter order defined in the query.

			# Examples

			When the source runs for the first time, the file at `last_run_metadata_path` does not exist.
			In that case, declare the initial values in `statement_init_params`.

			```toml
			[sources.odbc]
			statement = "SELECT * FROM users WHERE id = ?"
			statement_init_params = { "id": "0" }
			tracking_columns = ["id"]
			last_run_metadata_path = "/path/to/tracking.json"
			# The rest of the fields are omitted
			```
			"""
		required: false
		type: object: options: "*": {
			description: "Initial value for the SQL statement parameters. The value is always a string."
			required:    true
			type: "*": {}
		}
	}
	statement_timeout: {
		description: """
			Maximum time to allow the SQL statement to run.
			If the query does not finish within this window, it is canceled and retried at the next scheduled run.
			The default is 3 seconds.
			"""
		required: false
		type: uint: {
			default: 3
			examples: [
				3,
			]
			unit: "seconds"
		}
	}
	tracking_columns: {
		description: """
			Specifies the columns to track from the last row of the statement result set.
			Their values are passed as parameters to the SQL statement in the next scheduled run.

			# Examples

			```toml
			[sources.odbc]
			statement = "SELECT * FROM users WHERE id = ?"
			tracking_columns = ["id"]
			# The rest of the fields are omitted
			```
			"""
		required: false
		type: array: items: type: string: examples: ["id"]
	}
}
