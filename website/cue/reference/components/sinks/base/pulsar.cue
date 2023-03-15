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
				end-to-end acknowledgements as well, will wait for events to be acknowledged by the sink
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
	batch_size: {
		description: """
			Determines the batch size.

			Defaults to 1000.
			"""
		required: false
		type: uint: {}
	}
	compression: {
		description: "Identifies the compression options that are available within Pulsar."
		required:    false
		type: string: enum: {
			Lz4: """
				[LZ4][lz4] compression.

				[lz4]: https://lz4.github.io/lz4/
				"""
			None: "No compression."
			Snappy: """
				[Snappy][snappy] compression.

				[snappy]: https://google.github.io/snappy/
				"""
			Zlib: """
				[Zlib][zlib] compression.

				[zlib]: https://www.zlib.net
				"""
			Zstd: """
				[Zstd][zstd] compression.

				[zstd]: https://zstd.net
				"""
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
						Encodes an event in Vector’s [native Protocol Buffers format][vector_native_protobuf].

						This codec is **[experimental][experimental]**.

						[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					native_json: """
						Encodes an event in Vector’s [native JSON format][vector_native_json].

						This codec is **[experimental][experimental]**.

						[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					raw_message: """
						No encoding.

						This "encoding" simply uses the `message` field of a log event.

						Users should take care if they're modifying their log events (such as by using a `remap`
						transform, etc) and removing the message field while doing additional parsing on it, as this
						could lead to the encoding emitting empty strings for the given event.
						"""
					text: """
						Plain text encoding.

						This "encoding" simply uses the `message` field of a log event. For metrics, it uses an
						encoding that resembles the Prometheus export format.

						Users should take care if they're modifying their log events (such as by using a `remap`
						transform, etc) and removing the message field while doing additional parsing on it, as this
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
				description: "List of fields that will be excluded from the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			metric_tag_values: {
				description: """
					Controls how metric tag values are encoded.

					When set to `single`, only the last non-bare value of tags will be displayed with the
					metric.  When set to `full`, all metric tags will be exposed as separate assignments.
					"""
				relevant_when: "codec = \"json\" or codec = \"text\""
				required:      false
				type: string: {
					default: "single"
					enum: {
						full: "All tags will be exposed as arrays of either string or null values."
						single: """
															Tag values will be exposed as single strings, the same as they were before this config
															option. Tags with multiple values will show the last assigned value, and null values will be
															ignored.
															"""
					}
				}
			}
			only_fields: {
				description: "List of fields that will be included in the encoded event."
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
<<<<<<< HEAD
	key_field: {
		description: """
			The log field name or tags key to use for the topic key.

			If the field does not exist in the log or in tags, a blank value will be used. If unspecified, the key is not sent.

			Pulsar uses a hash of the key to choose the topic-partition or uses round-robin if the record has no key.
			"""
		required: false
		type: string: syntax: "literal"
||||||| 7e6727983
	partition_key_field: {
		description: "Log field to use as Pulsar message key"
		required:    false
		type: string: syntax: "literal"
=======
	partition_key_field: {
		description: "Log field to use as Pulsar message key."
		required:    false
		type: string: examples: ["message", "my_field"]
	}
	producer_name: {
		description: "The name of the producer. If not specified, the default name assigned by Pulsar will be used."
		required:    false
		type: string: examples: ["producer-name"]
>>>>>>> master
	}
	properties_key: {
		description: """
			The log field name to use for the Pulsar properties.

			If omitted, no properties will be written.
			"""
		required: false
		type: string: syntax: "literal"
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, etc.
			"""
		required: false
		type: object: options: {
			adaptive_concurrency: {
				description: """
					Configuration of adaptive concurrency parameters.

					These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
					unstable performance and sink behavior. Proceed with caution.
					"""
				required: false
				type: object: {
					default: {
						decrease_ratio:      0.9
						ewma_alpha:          0.4
						rtt_deviation_scale: 2.5
					}
					options: {
						decrease_ratio: {
							description: """
																The fraction of the current value to set the new concurrency limit when decreasing the limit.

																Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
																when latency increases.

																Note that the new limit is rounded down after applying this ratio.
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
						rtt_deviation_scale: {
							description: """
																Scale of RTT deviations which are not considered anomalous.

																Valid values are greater than or equal to `0`, and we expect reasonable values to range from `1.0` to `3.0`.

																When calculating the past RTT average, we also compute a secondary “deviation” value that indicates how variable
																those values are. We use that deviation when comparing the past RTT average to the current measurements, so we
																can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
																an appropriate range.  Larger values cause the algorithm to ignore larger increases in the RTT.
																"""
							required: false
							type: float: default: 2.5
						}
					}
				}
			}
			concurrency: {
				description: "Configuration for outbound request concurrency."
				required:    false
				type: {
					number: {}
					string: {
						const:   "adaptive"
						default: "none"
					}
				}
			}
			rate_limit_duration_secs: {
				description: "The time window, in seconds, used for the `rate_limit_num` option."
				required:    false
				type: uint: default: 1
			}
			rate_limit_num: {
				description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
				required:    false
				type: uint: default: 9223372036854775807
			}
			retry_attempts: {
				description: """
					The maximum number of retries to make for failed requests.

					The default, for all intents and purposes, represents an infinite number of retries.
					"""
				required: false
				type: uint: default: 9223372036854775807
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the fibonacci sequence will be used to select future backoffs.
					"""
				required: false
				type: uint: default: 1
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time, in seconds, to wait between retries."
				required:    false
				type: uint: default: 3600
			}
			timeout_secs: {
				description: """
					The maximum time a request can take before being aborted.

					It is highly recommended that you do not lower this value below the service’s internal timeout, as this could
					create orphaned requests, pile on retries, and result in duplicate data downstream.
					"""
				required: false
				type: uint: default: 60
			}
		}
	}
	topic: {
		description: "The Pulsar topic name to write events to."
		required:    true
		type: string: examples: ["topic-1234"]
	}
}
