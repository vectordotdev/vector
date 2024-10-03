package metadata

base: components: sources: http_server: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: "Whether or not end-to-end acknowledgements are enabled for this source."
			required:    false
			type: bool: {}
		}
	}
	address: {
		description: """
			The socket address to listen for connections on.

			It _must_ include a port.
			"""
		required: true
		type: string: examples: ["0.0.0.0:80", "localhost:80"]
	}
	auth: {
		description: "HTTP Basic authentication configuration."
		required:    false
		type: object: options: {
			password: {
				description: "The password for basic authentication."
				required:    true
				type: string: examples: ["hunter2", "${PASSWORD}"]
			}
			username: {
				description: "The username for basic authentication."
				required:    true
				type: string: examples: ["AzureDiamond", "admin"]
			}
		}
	}
	decoding: {
		description: "Configures how events are decoded from raw bytes."
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
																Please note that the following [`apache_avro::types::Value`] variants are currently *not* supported:
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
																For Avro datum encoded in Kafka messages, the bytes are prefixed with the schema ID.  Set this to true to strip the schema ID prefix.
																According to [Confluent Kafka's document](https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html#wire-format).
																"""
						required: true
						type: bool: {}
					}
				}
			}
			codec: {
				description: "The codec to use for decoding events."
				required:    true
				type: string: enum: {
					avro: """
						Decodes the raw bytes as as an [Apache Avro][apache_avro] message.

						[apache_avro]: https://avro.apache.org/
						"""
					bytes: "Uses the raw bytes as-is."
					gelf: """
						Decodes the raw bytes as a [GELF][gelf] message.

						This codec is experimental for the following reason:

						The GELF specification is more strict than the actual Graylog receiver.
						Vector's decoder currently adheres more strictly to the GELF spec, with
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

						This codec is **[experimental][experimental]**.

						[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					native_json: """
						Decodes the raw bytes as [native JSON format][vector_native_json].

						This codec is **[experimental][experimental]**.

						[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
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
				type: object: options: lossy: {
					description: """
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

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
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

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
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

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
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

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
						description: "Path to desc file"
						required:    false
						type: string: default: ""
					}
					message_type: {
						description: "message type. e.g package.message"
						required:    false
						type: string: default: ""
					}
				}
			}
			syslog: {
				description:   "Syslog-specific decoding options."
				relevant_when: "codec = \"syslog\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

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
																Note that the final contents of the `.` target will be used as the decoding result.
																Compilation error or use of 'abort' in a program will result in a decoding error.

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

																If not set, `local` will be used.

																[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
																"""
						required: false
						type: string: examples: ["local", "America/New_York", "EST5EDT"]
					}
				}
			}
		}
	}
	encoding: {
		description: """
			The expected encoding of received data.

			For `json` and `ndjson` encodings, the fields of the JSON objects are output as separate fields.
			"""
		required: false
		type: string: enum: {
			binary: "Binary."
			json:   "JSON."
			ndjson: "Newline-delimited JSON."
			text:   "Plaintext."
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

																By default, there is no maximum length enforced. If events are malformed, this can lead to
																additional resource usage as events continue to be buffered in memory, and can potentially
																lead to memory exhaustion in extreme cases.

																If there is a risk of processing malformed data, such as logs with user-controlled input,
																consider setting the maximum length to a reasonably large value as a safety net. This
																ensures that processing is not actually unbounded.
																"""
						required: false
						type: uint: {}
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
			method: {
				description: "The framing method."
				required:    true
				type: string: enum: {
					bytes:               "Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments)."
					character_delimited: "Byte frames which are delimited by a chosen character."
					length_delimited:    "Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length."
					newline_delimited:   "Byte frames which are delimited by a newline character."
					octet_counting: """
						Byte frames according to the [octet counting][octet_counting] format.

						[octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
						"""
				}
			}
			newline_delimited: {
				description:   "Options for the newline delimited decoder."
				relevant_when: "method = \"newline_delimited\""
				required:      false
				type: object: options: max_length: {
					description: """
						The maximum length of the byte buffer.

						This length does *not* include the trailing delimiter.

						By default, there is no maximum length enforced. If events are malformed, this can lead to
						additional resource usage as events continue to be buffered in memory, and can potentially
						lead to memory exhaustion in extreme cases.

						If there is a risk of processing malformed data, such as logs with user-controlled input,
						consider setting the maximum length to a reasonably large value as a safety net. This
						ensures that processing is not actually unbounded.
						"""
					required: false
					type: uint: {}
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
	headers: {
		description: """
			A list of HTTP headers to include in the log event.

			Accepts the wildcard (`*`) character for headers matching a specified pattern.

			Specifying "*" results in all headers included in the log event.

			These override any values included in the JSON payload with conflicting names.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["User-Agent", "X-My-Custom-Header", "X-*", "*"]
		}
	}
	host_key: {
		description: "If set, the name of the log field used to add the remote IP to each event"
		required:    false
		type: string: {
			default: ""
			examples: ["hostname"]
		}
	}
	keepalive: {
		description: "Configuration of HTTP server keepalive parameters."
		required:    false
		type: object: options: {
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
					examples: [600]
					unit: "seconds"
				}
			}
		}
	}
	method: {
		description: "Specifies the action of the HTTP request."
		required:    false
		type: string: {
			default: "POST"
			enum: {
				DELETE:  "HTTP DELETE method."
				GET:     "HTTP GET method."
				HEAD:    "HTTP HEAD method."
				OPTIONS: "HTTP OPTIONS method."
				PATCH:   "HTTP PATCH method."
				POST:    "HTTP POST method."
				PUT:     "HTTP Put method."
			}
		}
	}
	path: {
		description: "The URL path on which log event POST requests are sent."
		required:    false
		type: string: {
			default: "/"
			examples: ["/event/path", "/logs"]
		}
	}
	path_key: {
		description: "The event key in which the requested URL path used to send the request is stored."
		required:    false
		type: string: {
			default: "path"
			examples: ["vector_http_path"]
		}
	}
	query_parameters: {
		description: """
			A list of URL query parameters to include in the log event.

			These override any values included in the body with conflicting names.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["application", "source"]
		}
	}
	response_code: {
		description: "Specifies the HTTP response status code that will be returned on successful requests."
		required:    false
		type: uint: {
			default: 200
			examples: [
				202,
			]
		}
	}
	strict_path: {
		description: """
			Whether or not to treat the configured `path` as an absolute path.

			If set to `true`, only requests using the exact URL path specified in `path` are accepted. Otherwise,
			requests sent to a URL path that starts with the value of `path` are accepted.

			With `strict_path` set to `false` and `path` set to `""`, the configured HTTP source accepts requests from
			any URL path.
			"""
		required: false
		type: bool: default: true
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
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
			enabled: {
				description: """
					Whether or not to require TLS for incoming or outgoing connections.

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
					so on until the verification process reaches a root certificate.

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
}
