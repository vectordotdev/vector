package metadata

base: components: sources: gcp_pubsub: configuration: {
	ack_deadline_seconds: {
		description: "Deprecated, old name of `ack_deadline_secs`."
		required:    false
		type: int: {}
	}
	ack_deadline_secs: {
		description: """
			The acknowledgement deadline, in seconds, to use for this stream.

			Messages that are not acknowledged when this deadline expires may be retransmitted.
			"""
		required: false
		type: int: {}
	}
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
	api_key: {
		description: """
			An API key. ([documentation](https://cloud.google.com/docs/authentication/api-keys))

			Either an API key, or a path to a service account credentials JSON file can be specified.

			If both are unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename. If no
			filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is
			running on. If Vector is not running on a GCE instance, then you must define eith an API key or service account
			credentials JSON file.
			"""
		required: false
		type: string: syntax: "literal"
	}
	credentials_path: {
		description: """
			Path to a service account credentials JSON file. ([documentation](https://cloud.google.com/docs/authentication/production#manually))

			Either an API key, or a path to a service account credentials JSON file can be specified.

			If both are unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename. If no
			filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is
			running on. If Vector is not running on a GCE instance, then you must define eith an API key or service account
			credentials JSON file.
			"""
		required: false
		type: string: syntax: "literal"
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
	endpoint: {
		description: "The endpoint from which to pull data."
		required:    false
		type: string: syntax: "literal"
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
	full_response_size: {
		description: """
			The number of messages in a response to mark a stream as
			"busy". This is used to determine if more streams should be
			started.
			"""
		required: false
		type: uint: default: 100
	}
	keepalive_secs: {
		description: """
			The amount of time, in seconds, with no received activity
			before sending a keepalive request. If this is set larger than
			`60`, you may see periodic errors sent from the server.
			"""
		required: false
		type: float: default: 60.0
	}
	max_concurrency: {
		description: "The maximum number of concurrent stream connections to open at once."
		required:    false
		type: uint: default: 10
	}
	poll_time_seconds: {
		description: """
			How often to poll the currently active streams to see if they
			are all busy and so open a new stream.
			"""
		required: false
		type: float: default: 2.0
	}
	project: {
		description: "The project name from which to pull logs."
		required:    true
		type: string: syntax: "literal"
	}
	retry_delay_seconds: {
		description: "Deprecated, old name of `retry_delay_secs`."
		required:    false
		type: float: {}
	}
	retry_delay_secs: {
		description: "The amount of time, in seconds, to wait between retry attempts after an error."
		required:    false
		type: float: {}
	}
	skip_authentication: {
		description: "Skip all authentication handling. For use with integration tests only."
		required:    false
		type: bool: default: false
	}
	subscription: {
		description: "The subscription within the project which is configured to receive logs."
		required:    true
		type: string: syntax: "literal"
	}
	tls: {
		description: "Standard TLS options."
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
}
