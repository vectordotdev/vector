package metadata

base: components: sinks: websocket: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how Vector handles event acknowledgement.

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
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should almost always be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type: object: options: {
			password: {
				description:   "The password to send."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: syntax: "literal"
			}
			strategy: {
				required: true
				type: string: enum: {
					basic: """
						Basic authentication.

						The username and password are concatenated and encoded via base64.
						"""
					bearer: """
						Bearer authentication.

						A bearer token (OAuth2, JWT, etc) is passed as-is.
						"""
				}
			}
			token: {
				description:   "The bearer token to send."
				relevant_when: "strategy = \"bearer\""
				required:      true
				type: string: syntax: "literal"
			}
			user: {
				description:   "The username to send."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: syntax: "literal"
			}
		}
	}
	encoding: {
		description: "Encoding configuration."
		required:    true
		type: object: options: {
			avro: {
				description:   "Apache Avro serializer options."
				relevant_when: "codec = \"avro\""
				required:      true
				type: object: options: schema: {
					description: "The Avro schema."
					required:    true
					type: string: syntax: "literal"
				}
			}
			codec: {
				required: true
				type: string: enum: {
					avro:        "Apache Avro serialization."
					gelf:        "GELF serialization."
					json:        "JSON serialization."
					logfmt:      "Logfmt serialization."
					native:      "Native Vector serialization based on Protocol Buffers."
					native_json: "Native Vector serialization based on JSON."
					raw_message: """
						No serialization.

						This encoding, specifically, will only encode the `message` field of a log event. Users should take care if
						they're modifying their log events (such as by using a `remap` transform, etc) and removing the message field
						while doing additional parsing on it, as this could lead to the encoding emitting empty strings for the given
						event.
						"""
					text: """
						Plaintext serialization.

						This encoding, specifically, will only encode the `message` field of a log event. Users should take care if
						they're modifying their log events (such as by using a `remap` transform, etc) and removing the message field
						while doing additional parsing on it, as this could lead to the encoding emitting empty strings for the given
						event.
						"""
				}
			}
			except_fields: {
				description: "List of fields that will be excluded from the encoded event."
				required:    false
				type: array: items: type: string: syntax: "literal"
			}
			only_fields: {
				description: "List of fields that will be included in the encoded event."
				required:    false
				type: array: items: type: string: syntax: "literal"
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
	ping_interval: {
		description: "The interval, in seconds, between sending PINGs to the remote peer."
		required:    false
		type: uint: {}
	}
	ping_timeout: {
		description: """
			The timeout, in seconds, while waiting for a PONG response from the remote peer.

			If a response is not received in this time, the connection is reestablished.
			"""
		required: false
		type: uint: {}
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
	uri: {
		description: """
			The WebSocket URI to connect to.

			This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
			"""
		required: true
		type: string: syntax: "literal"
	}
}
