package metadata

base: components: sources: aws_sqs: configuration: {
	acknowledgements: {
		description: "Configuration of acknowledgement behavior."
		required:    false
		type: object: options: enabled: {
			description: "Enables end-to-end acknowledgements."
			required:    false
			type: bool: {}
		}
	}
	auth: {
		description: "Configuration of the authentication strategy for interacting with AWS services."
		required:    false
		type: object: options: {
			access_key_id: {
				description: "The AWS access key ID."
				required:    true
				type: string: syntax: "literal"
			}
			assume_role: {
				description: "The ARN of the role to assume."
				required:    true
				type: string: syntax: "literal"
			}
			credentials_file: {
				description: "Path to the credentials file."
				required:    true
				type: string: syntax: "literal"
			}
			load_timeout_secs: {
				description: "Timeout for successfully loading any credentials, in seconds."
				required:    false
				type: uint: {}
			}
			profile: {
				description: "The credentials profile to use."
				required:    false
				type: string: syntax: "literal"
			}
			region: {
				description: """
					The AWS region to send STS requests to.

					If not set, this will default to the configured region
					for the service itself.
					"""
				required: false
				type: string: syntax: "literal"
			}
			secret_access_key: {
				description: "The AWS secret access key."
				required:    true
				type: string: syntax: "literal"
			}
		}
	}
	client_concurrency: {
		description: """
			Number of concurrent tasks to create for polling the queue for messages.

			Defaults to the number of available CPUs on the system.

			Should not typically need to be changed, but it can sometimes be beneficial to raise this value when there is a
			high rate of messages being pushed into the queue and the messages being fetched are small. In these cases,
			Vector may not fully utilize system resources without fetching more messages per second, as it spends more time
			fetching the messages than processing them.
			"""
		required: false
		type: uint: default: 24
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
	delete_message: {
		description: """
			Whether to delete the message once Vector processes it.

			It can be useful to set this to `false` to debug or during initial Vector setup.
			"""
		required: false
		type: bool: default: true
	}
	endpoint: {
		description: "The API endpoint of the service."
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
	poll_secs: {
		description: """
			How long to wait while polling the queue for new messages, in seconds.

			Generally should not be changed unless instructed to do so, as if messages are available, they will always be
			consumed, regardless of the value of `poll_secs`.
			"""
		required: false
		type: uint: default: 15
	}
	queue_url: {
		description: "The URL of the SQS queue to poll for messages."
		required:    true
		type: string: syntax: "literal"
	}
	region: {
		description: "The AWS region to use."
		required:    false
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
	visibility_timeout_secs: {
		description: """
			The visibility timeout to use for messages, in secords.

			This controls how long a message is left unavailable after Vector receives it. If Vector receives a message, and
			takes longer than `visibility_timeout_secs` to process and delete the message from the queue, it will be made reavailable for another consumer.

			This can happen if, for example, if Vector crashes between consuming a message and deleting it.
			"""
		required: false
		type: uint: default: 300
	}
}
