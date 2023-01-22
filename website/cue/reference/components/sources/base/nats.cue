package metadata

base: components: sources: nats: configuration: {
	auth: {
		description: "Configuration of the authentication strategy when interacting with NATS."
		required:    false
		type: object: options: {
			credentials_file: {
				description:   "Credentials file configuration."
				relevant_when: "strategy = \"credentials_file\""
				required:      true
				type: object: options: path: {
					description: "Path to credentials file."
					required:    true
					type: string: {}
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
	}
	connection_name: {
		description: "A name assigned to the NATS connection."
		required:    true
		type: string: {}
	}
	decoding: {
		description: "Configures how events are decoded from raw bytes."
		required:    false
		type: object: options: codec: {
			description: "The codec to use for decoding events."
			required:    false
			type: string: {
				default: "bytes"
				enum: {
					bytes: "Uses the raw bytes as-is."
					gelf: """
						Decodes the raw bytes as a [GELF][gelf] message.

						[gelf]: https://docs.graylog.org/docs/gelf
						"""
					json: """
						Decodes the raw bytes as [JSON][json].

						[json]: https://www.json.org/
						"""
					native: """
						Decodes the raw bytes as Vector’s [native Protocol Buffers format][vector_native_protobuf].

						This codec is **[experimental][experimental]**.

						[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					native_json: """
						Decodes the raw bytes as Vector’s [native JSON format][vector_native_json].

						This codec is **[experimental][experimental]**.

						[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					syslog: """
						Decodes the raw bytes as a Syslog message.

						Will decode either as the [RFC 3164][rfc3164]-style format ("old" style) or the more modern
						[RFC 5424][rfc5424]-style format ("new" style, includes structured data).

						[rfc3164]: https://www.ietf.org/rfc/rfc3164.txt
						[rfc5424]: https://www.ietf.org/rfc/rfc5424.txt
						"""
				}
			}
		}
	}
	framing: {
		description: """
			Framing configuration.

			Framing deals with how events are separated when encoded in a raw byte form, where each event is
			a "frame" that must be prefixed, or delimited, in a way that marks where an event begins and
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
				description: "The framing method."
				required:    false
				type: string: {
					default: "bytes"
					enum: {
						bytes:               "Byte frames are passed through as-is according to the underlying I/O boundaries (e.g. split between messages or stream segments)."
						character_delimited: "Byte frames which are delimited by a chosen character."
						length_delimited:    "Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length."
						newline_delimited:   "Byte frames which are delimited by a newline character."
						octet_counting: """
															Byte frames according to the [octet counting][octet_counting] format.

															[octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
															"""
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
	queue: {
		description: "NATS Queue Group to join."
		required:    false
		type: string: {}
	}
	subject: {
		description: "The NATS subject to pull messages from."
		required:    true
		type: string: {}
	}
	subject_key_field: {
		description: "The `NATS` subject key."
		required:    false
		type: string: default: "subject"
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
	url: {
		description: """
			The NATS URL to connect to.

			The URL must take the form of `nats://server:port`.
			"""
		required: true
		type: string: {}
	}
}
