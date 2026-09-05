package metadata

generated: components: sinks: databend: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	auth: {
		description: "The username and password to authenticate with. Overrides the username and password in DSN."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::http::Auth>"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
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
			}
		}
	}
	database: {
		description: "The database that contains the table that data is inserted into. Overrides the database in DSN."
		required:    false
		type: string: examples: ["mydatabase"]
	}
	encoding: {
		description: "Configures how events are encoded into raw bytes."
		required:    false
		type: object: options: {
			codec: {
				description: "The codec to use for encoding events."
				required:    false
				type: string: {
					default: "json"
					enum: {
						csv: """
															Encodes an event as a CSV message.

															This codec must be configured with fields to encode.
															"""
						json: """
															Encodes an event as [JSON][json].

															[json]: https://www.json.org/
															"""
					}
				}
			}
			csv: {
				description:   "The CSV Serializer Options."
				relevant_when: "codec = \"csv\""
				required:      true
				type:          _schemaDefinitions["codecs::encoding::format::csv::CsvSerializerOptions"]
			}
			except_fields: {
				description: "List of fields that are excluded from the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			json: {
				description:   "Options for the JsonSerializer."
				relevant_when: "codec = \"json\""
				required:      false
				type:          _schemaDefinitions["codecs::encoding::format::json::JsonSerializerOptions"]
			}
			metric_tag_values: {
				description: """
					Controls how metric tag values are encoded.

					When set to `single`, only the last non-bare value of tags are displayed with the
					metric. When set to `full`, all metric tags are exposed as separate assignments.
					When set to `auto`, tag values are encoded using their underlying shape.
					"""
				relevant_when: "codec = \"json\""
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
	endpoint: {
		description: "The DSN of the Databend server."
		required:    true
		type: string: examples: ["databend://localhost:8000/default?sslmode=disable"]
	}
	missing_field_as: {
		description: """
			Defines how missing fields are handled for NDJson.
			Refer to https://docs.databend.com/sql/sql-reference/file-format-options#null_field_as
			"""
		required: false
		type: string: {
			default: "NULL"
			enum: {
				ERROR:         "Generates an error if a missing field is encountered."
				FIELD_DEFAULT: "Uses the default value of the field for missing fields."
				NULL:          "Interprets missing fields as NULL values. An error will be generated for non-nullable fields."
				TYPE_DEFAULT:  "Uses the default value of the field's data type for missing fields."
			}
		}
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::service::TowerRequestConfig"]
	}
	table: {
		description: "The table that data is inserted into."
		required:    true
		type: string: examples: ["mytable"]
	}
	tls: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use arguments in the DSN instead."
		description:        "The TLS configuration to use when connecting to the Databend server."
		required:           false
		type:               _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
