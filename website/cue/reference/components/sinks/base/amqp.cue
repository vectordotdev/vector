package metadata

base: components: sinks: amqp: configuration: {
	acknowledgements: {
		description: "Configuration of acknowledgement behavior."
		required:    false
		type: object: options: enabled: {
			description: "Enables end-to-end acknowledgements."
			required:    false
			type: bool: {}
		}
	}
	connection: {
		description: "Connection options for the `amqp` sink."
		required:    true
		type: object: options: {
			connection_string: {
				description: """
					URI for the `AMQP` server.

					Format: amqp://<user>:<password>@<host>:<port>/<vhost>?timeout=<seconds>
					"""
				required: true
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
	exchange: {
		description: "The exchange to publish messages to."
		required:    true
		type: string: syntax: "template"
	}
	routing_key: {
		description: "Template used to generate a routing key which corresponds to a queue binding."
		required:    false
		type: string: syntax: "template"
	}
}
