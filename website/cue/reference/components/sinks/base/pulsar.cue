package metadata

base: components: sinks: pulsar: configuration: {
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
				required: false
				type: string: examples: ["${PULSAR_NAME}", "name123"]
			}
			oauth2: {
				description: "OAuth2-specific authentication configuration."
				required:    false
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
						type: string: examples: ["{OAUTH2_CREDENTIALS_URL}", "file:///oauth2_credentials", "data:application/json;base64,cHVsc2FyCg=="]
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
				required: false
				type: string: examples: ["${PULSAR_TOKEN}", "123456789"]
			}
		}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: max_events: {
			description: "The maximum size of a batch before it is flushed."
			required:    false
			type: uint: {
				examples: [1000]
				unit: "events"
			}
		}
	}
	compression: {
		description: "Supported compression types for Pulsar."
		required:    false
		type: string: {
			default: "none"
			enum: {
				lz4:    "LZ4."
				none:   "No compression."
				snappy: "Snappy."
				zlib:   "Zlib."
				zstd:   "Zstandard."
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

						[gelf]: https://docs.graylog.org/docs/gelf
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
				type: object: options: fields: {
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
			timestamp_format: {
				description: "Format used for timestamp fields."
				required:    false
				type: string: enum: {
					rfc3339: "Represent the timestamp as a RFC 3339 timestamp."
					unix:    "Represent the timestamp as a Unix timestamp."
				}
			}
		}
	}
	endpoint: {
		description: """
			The endpoint to which the Pulsar client should connect to.

			The endpoint should specify the pulsar protocol and port.
			"""
		required: true
		type: string: examples: ["pulsar://127.0.0.1:6650"]
	}
	partition_key_field: {
		description: "Log field to use as Pulsar message key."
		required:    false
		type: string: examples: ["message", "my_field"]
	}
	producer_name: {
		description: "The name of the producer. If not specified, the default name assigned by Pulsar is used."
		required:    false
		type: string: examples: ["producer-name"]
	}
	topic: {
		description: "The Pulsar topic name to write events to."
		required:    true
		type: string: examples: ["topic-1234"]
	}
}
