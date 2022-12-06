package metadata

base: components: sources: http: configuration: {
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
		description: "The address to listen for connections on."
		required:    true
		type: string: syntax: "literal"
	}
	auth: {
		description: "HTTP Basic authentication configuration."
		required:    false
		type: object: options: {
			password: {
				description: "The password for basic authentication."
				required:    true
				type: string: syntax: "literal"
			}
			username: {
				description: "The username for basic authentication."
				required:    true
				type: string: syntax: "literal"
			}
		}
	}
	decoding: {
		description: "Configuration for building a `Deserializer`."
		required:    false
		type: object: options: codec: {
			required: true
			type: string: enum: {
				bytes:       "Configures the `BytesDeserializer`."
				gelf:        "Configures the `GelfDeserializer`."
				json:        "Configures the `JsonDeserializer`."
				native:      "Configures the `NativeDeserializer`."
				native_json: "Configures the `NativeJsonDeserializer`."
				syslog:      "Configures the `SyslogDeserializer`."
			}
		}
	}
	encoding: {
		description: """
			The expected encoding of received data.

			Note that for `json` and `ndjson` encodings, the fields of the JSON objects are output as separate fields.
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
				required: true
				type: string: enum: {
					bytes:               "Configures the `BytesDecoder`."
					character_delimited: "Configures the `CharacterDelimitedDecoder`."
					length_delimited:    "Configures the `LengthDelimitedDecoder`."
					newline_delimited:   "Configures the `NewlineDelimitedDecoder`."
					octet_counting:      "Configures the `OctetCountingDecoder`."
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
	headers: {
		description: """
			A list of HTTP headers to include in the log event.

			These will override any values included in the JSON payload with conflicting names.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: syntax: "literal"
		}
	}
	method: {
		description: "Specifies the action of the HTTP request."
		required:    false
		type: string: {
			default: "POST"
			enum: {
				DELETE: "HTTP DELETE method."
				GET:    "HTTP GET method."
				HEAD:   "HTTP HEAD method."
				PATCH:  "HTTP PATCH method."
				POST:   "HTTP POST method."
				PUT:    "HTTP Put method."
			}
		}
	}
	path: {
		description: "The URL path on which log event POST requests shall be sent."
		required:    false
		type: string: {
			default: "/"
			syntax:  "literal"
		}
	}
	path_key: {
		description: "The event key in which the requested URL path used to send the request will be stored."
		required:    false
		type: string: {
			default: "path"
			syntax:  "literal"
		}
	}
	query_parameters: {
		description: """
			A list of URL query parameters to include in the log event.

			These will override any values included in the body with conflicting names.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: syntax: "literal"
		}
	}
	strict_path: {
		description: """
			Whether or not to treat the configured `path` as an absolute path.

			If set to `true`, only requests using the exact URL path specified in `path` will be accepted. Otherwise,
			requests sent to a URL path that starts with the value of `path` will be accepted.

			With `strict_path` set to `false` and `path` set to `""`, the configured HTTP source will accept requests from
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

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
					"""
				required: false
				type: array: items: type: string: syntax: "literal"
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
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
}
