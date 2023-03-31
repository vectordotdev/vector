package metadata

base: components: sinks: http: configuration: {
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
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type: object: options: {
			password: {
				description:   "The basic authentication password."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: examples: ["${PASSWORD}", "password"]
			}
			strategy: {
				description: "The authentication strategy to use."
				required:    true
				type: string: enum: {
					basic: """
						Basic authentication.

						The username and password are concatenated and encoded via [base64][base64].

						[base64]: https://en.wikipedia.org/wiki/Base64
						"""
					bearer: """
						Bearer authentication.

						The bearer token value (OAuth2, JWT, etc.) is passed as-is.
						"""
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
		}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized/compressed.
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
	}
	compression: {
		description: """
			Compression configuration.

			All compression algorithms use the default compression level unless otherwise specified.
			"""
		required: false
		type: string: {
			default: "none"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				zlib: """
					[Zlib][zlib] compression.

					[zlib]: https://zlib.net/
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
	headers: {
		deprecated:  true
		description: "A list of custom headers to add to each request."
		required:    false
		type: object: options: "*": {
			description: "An HTTP request header and it's value."
			required:    true
			type: string: {}
		}
	}
	method: {
		description: """
			HTTP method.

			The HTTP method to use when making the request.
			"""
		required: false
		type: string: {
			default: "post"
			enum: {
				delete:  "DELETE."
				get:     "GET."
				head:    "HEAD."
				options: "OPTIONS."
				patch:   "PATCH."
				post:    "POST."
				put:     "PUT."
				trace:   "TRACE."
			}
		}
	}
	payload_prefix: {
		description: """
			A string to prefix the payload with.

			This option is ignored if the encoding is not character delimited JSON.

			If specified, the `payload_suffix` must also be specified and together they must produce a valid JSON object.
			"""
		required: false
		type: string: {
			default: ""
			examples: ["{\"data\":"]
		}
	}
	payload_suffix: {
		description: """
			A string to suffix the payload with.

			This option is ignored if the encoding is not character delimited JSON.

			If specified, the `payload_prefix` must also be specified and together they must produce a valid JSON object.
			"""
		required: false
		type: string: {
			default: ""
			examples: [
				"}",
			]
		}
	}
	request: {
		description: "Outbound HTTP request settings."
		required:    false
		type: object: options: {
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
			concurrency: {
				description: "Configuration for outbound request concurrency."
				required:    false
				type: {
					string: {
						default: "none"
						enum: {
							adaptive: """
															Concurrency will be managed by Vector's [Adaptive Request Concurrency][arc] feature.

															[arc]: https://vector.dev/docs/about/under-the-hood/networking/arc/
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
						"X-My-Custom-Header": "A-Value"
					}]
					options: "*": {
						description: "An HTTP request header and it's value."
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
				description: """
					The maximum number of retries to make for failed requests.

					The default, for all intents and purposes, represents an infinite number of retries.
					"""
				required: false
				type: uint: {
					default: 9223372036854775807
					unit:    "retries"
				}
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the fibonacci sequence is used to select future backoffs.
					"""
				required: false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time to wait between retries."
				required:    false
				type: uint: {
					default: 3600
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
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. They are prioritized in the order
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

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
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
			verify_certificate: {
				description: """
					Enables certificate verification.

					If enabled, certificates must not be expired and must be issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
					certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
					so on until the verification process reaches a root certificate.

					Relevant for both incoming and outgoing connections.

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
	}
	uri: {
		description: """
			The full URI to make HTTP requests to.

			This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
			"""
		required: true
		type: string: examples: ["https://10.22.212.22:9000/endpoint"]
	}
}
