package metadata

base: components: sources: kafka: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level. Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how Vector handles event acknowledgement.

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
	auto_offset_reset: {
		description: """
			If offsets for consumer group do not exist, set them using this strategy.

			See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for the `auto.offset.reset` option for further clarification.
			"""
		required: false
		type: string: {
			default: "largest"
			syntax:  "literal"
		}
	}
	bootstrap_servers: {
		description: """
			A comma-separated list of Kafka bootstrap servers.

			These are the servers in a Kafka cluster that a client should use to "bootstrap" its connection to the cluster,
			allowing discovering all other hosts in the cluster.

			Must be in the form of `host:port`, and comma-separated.
			"""
		required: true
		type: string: syntax: "literal"
	}
	commit_interval_ms: {
		description: "The frequency that the consumer offsets are committed (written) to offset storage, in milliseconds."
		required:    false
		type: uint: default: 5000
	}
	decoding: {
		description: "Configuration for building a `Deserializer`."
		required:    false
		type: object: options: codec: {
			required: false
			type: string: {
				default: "bytes"
				enum: {
					bytes:       "Configures the `BytesDeserializer`."
					gelf:        "Configures the `GelfDeserializer`."
					json:        "Configures the `JsonDeserializer`."
					native:      "Configures the `NativeDeserializer`."
					native_json: "Configures the `NativeJsonDeserializer`."
					syslog:      "Configures the `SyslogDeserializer`."
				}
			}
		}
	}
	fetch_wait_max_ms: {
		description: "Maximum time the broker may wait to fill the response, in milliseconds."
		required:    false
		type: uint: default: 100
	}
	framing: {
		description: "Configuration for building a `Framer`."
		required:    false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited decoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type: object: options: {
					delimiter: {
						description: "The character that delimits byte sequences."
						required:    true
						type: uint: {}
					}
					max_length: {
						description: """
																The maximum length of the byte buffer.

																This length does *not* include the trailing delimiter.
																"""
						required: false
						type: uint: {}
					}
				}
			}
			method: {
				required: false
				type: string: {
					default: "bytes"
					enum: {
						bytes:               "Configures the `BytesDecoder`."
						character_delimited: "Configures the `CharacterDelimitedDecoder`."
						length_delimited:    "Configures the `LengthDelimitedDecoder`."
						newline_delimited:   "Configures the `NewlineDelimitedDecoder`."
						octet_counting:      "Configures the `OctetCountingDecoder`."
					}
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
	group_id: {
		description: "The consumer group name to be used to consume events from Kafka."
		required:    true
		type: string: syntax: "literal"
	}
	headers_key: {
		description: """
			Overrides the name of the log field used to add the headers to each event.

			The value will be the headers of the Kafka message itself.

			By default, `"headers"` is used.
			"""
		required: false
		type: string: {
			default: "headers"
			syntax:  "literal"
		}
	}
	key_field: {
		description: """
			Overrides the name of the log field used to add the message key to each event.

			The value will be the message key of the Kafka message itself.

			By default, `"message_key"` is used.
			"""
		required: false
		type: string: {
			default: "message_key"
			syntax:  "literal"
		}
	}
	librdkafka_options: {
		description: """
			Advanced options set directly on the underlying `librdkafka` client.

			See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.
			"""
		required: false
		type: object: options: "*": {
			description: """
				Advanced options set directly on the underlying `librdkafka` client.

				See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.
				"""
			required: true
			type: string: syntax: "literal"
		}
	}
	offset_key: {
		description: """
			Overrides the name of the log field used to add the offset to each event.

			The value will be the offset of the Kafka message itself.

			By default, `"offset"` is used.
			"""
		required: false
		type: string: {
			default: "offset"
			syntax:  "literal"
		}
	}
	partition_key: {
		description: """
			Overrides the name of the log field used to add the partition to each event.

			The value will be the partition from which the Kafka message was consumed from.

			By default, `"partition"` is used.
			"""
		required: false
		type: string: {
			default: "partition"
			syntax:  "literal"
		}
	}
	sasl: {
		description: "Configuration for SASL authentication when interacting with Kafka."
		required:    false
		type: object: options: {
			enabled: {
				description: """
					Enables SASL authentication.

					Only `PLAIN` and `SCRAM`-based mechanisms are supported when configuring SASL authentication via `sasl.*`. For
					other mechanisms, `librdkafka_options.*` must be used directly to configure other `librdkafka`-specific values
					i.e. `sasl.kerberos.*` and so on.

					See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.

					SASL authentication is not supported on Windows.
					"""
				required: false
				type: bool: {}
			}
			mechanism: {
				description: "The SASL mechanism to use."
				required:    false
				type: string: syntax: "literal"
			}
			password: {
				description: "The SASL password."
				required:    false
				type: string: syntax: "literal"
			}
			username: {
				description: "The SASL username."
				required:    false
				type: string: syntax: "literal"
			}
		}
	}
	session_timeout_ms: {
		description: "The Kafka session timeout, in milliseconds."
		required:    false
		type: uint: default: 10000
	}
	socket_timeout_ms: {
		description: "Timeout for network requests, in milliseconds."
		required:    false
		type: uint: default: 60000
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
					"""
				required: false
				type: array: items: type: string: syntax: "literal"
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certficate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: syntax: "literal"
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this server.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: syntax: "literal"
			}
			enabled: {
				description: """
					Whether or not to require TLS for incoming/outgoing connections.

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
				type: string: syntax: "literal"
			}
			key_pass: {
				description: """
					Passphrase used to unlock the encrypted key file.

					This has no effect unless `key_file` is set.
					"""
				required: false
				type: string: syntax: "literal"
			}
			verify_certificate: {
				description: """
					Enables certificate verification.

					If enabled, certificates must be valid in terms of not being expired, as well as being issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that not only the leaf certificate (the
					certificate presented by the client/server) is valid, but also that the issuer of that certificate is valid, and
					so on until reaching a root certificate.

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
	topic_key: {
		description: """
			Overrides the name of the log field used to add the topic to each event.

			The value will be the topic from which the Kafka message was consumed from.

			By default, `"topic"` is used.
			"""
		required: false
		type: string: {
			default: "topic"
			syntax:  "literal"
		}
	}
	topics: {
		description: """
			The Kafka topics names to read events from.

			Regular expression syntax is supported if the topic begins with `^`.
			"""
		required: true
		type: array: items: type: string: syntax: "literal"
	}
}
