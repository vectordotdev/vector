package metadata

base: components: sources: datadog_agent: configuration: {
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
	address: {
		description: """
			The address to accept connections on.

			The address _must_ include a port.
			"""
		required: true
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
						Decodes the raw bytes as Vector’s [native Protocol Buffers format][vector_native_protobuf] ([EXPERIMENTAL][experimental]).

						[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					native_json: """
						Decodes the raw bytes as Vector’s [native JSON format][vector_native_json] ([EXPERIMENTAL][experimental]).

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
	disable_logs: {
		description: "If this settings is set to `true`, logs won't be accepted by the component."
		required:    false
		type: bool: default: false
	}
	disable_metrics: {
		description: "If this settings is set to `true`, metrics won't be accepted by the component."
		required:    false
		type: bool: default: false
	}
	disable_traces: {
		description: "If this settings is set to `true`, traces won't be accepted by the component."
		required:    false
		type: bool: default: false
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
	multiple_outputs: {
		description: """
			If this setting is set to `true` logs, metrics and traces will be sent to different outputs.

			For a source component named `agent` the received logs, metrics, and traces can then be accessed by specifying
			`agent.logs`, `agent.metrics`, and `agent.traces`, respectively, as the input to another component.
			"""
		required: false
		type: bool: default: false
	}
	store_api_key: {
		description: """
			When incoming events contain a Datadog API key, if this setting is set to `true` the key will kept in the event
			metadata and will be used if the event is sent to a Datadog sink.
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

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
					"""
				required: false
				type: array: items: type: string: {}
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: {}
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this server.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: {}
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
				type: string: {}
			}
			key_pass: {
				description: """
					Passphrase used to unlock the encrypted key file.

					This has no effect unless `key_file` is set.
					"""
				required: false
				type: string: {}
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
}
