package metadata

base: components: sources: socket: configuration: {
	address: {
		description:   "The address to listen for connections on."
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      true
		type: {
			number: {}
			string: syntax: "literal"
		}
	}
	connection_limit: {
		description:   "The maximum number of TCP connections that will be allowed at any given time."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: uint: {}
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
	host_key: {
		description: """
			Overrides the name of the log field used to add the peer host to each event.

			The value will be the peer host's address, including the port i.e. `1.2.3.4:9000`.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: syntax: "literal"
	}
	keepalive: {
		description:   "TCP keepalive settings for socket-based components."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: object: options: time_secs: {
			description: "The time to wait, in seconds, before starting to send TCP keepalive probes on an idle connection."
			required:    false
			type: uint: {}
		}
	}
	log_namespace: {
		description: "The namespace to use for logs. This overrides the global setting."
		required:    false
		type: bool: {}
	}
	max_length: {
		description: """
			The maximum buffer size, in bytes, of incoming messages.

			Messages larger than this are truncated.
			"""
		required: false
		type: uint: {}
	}
	mode: {
		required: true
		type: string: enum: {
			tcp:           "Listen on TCP."
			udp:           "Listen on UDP."
			unix_datagram: "Listen on UDS, in datagram mode. (Unix domain socket)"
			unix_stream:   "Listen on UDS, in stream mode. (Unix domain socket)"
		}
	}
	path: {
		description: """
			The Unix socket path.

			This should be an absolute path.
			"""
		relevant_when: "mode = \"unix_datagram\" or mode = \"unix_stream\""
		required:      true
		type: string: syntax: "literal"
	}
	port_key: {
		description: """
			Overrides the name of the log field used to add the peer host's port to each event.

			The value will be the peer host's port i.e. `9000`.

			By default, `"port"` is used.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      false
		type: string: syntax: "literal"
	}
	receive_buffer_bytes: {
		description: """
			The size, in bytes, of the receive buffer used for each connection.

			This should not typically needed to be changed.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      false
		type: uint: {}
	}
	shutdown_timeout_secs: {
		description:   "The timeout, in seconds, before a connection is forcefully closed during shutdown."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: uint: default: 30
	}
	socket_file_mode: {
		description: """
			Unix file mode bits to be applied to the unix socket file as its designated file permissions.

			Note that the file mode value can be specified in any numeric format supported by your configuration
			language, but it is most intuitive to use an octal number.
			"""
		relevant_when: "mode = \"unix_datagram\" or mode = \"unix_stream\""
		required:      false
		type: uint: {}
	}
	tls: {
		description:   "TlsEnableableConfig for `sources`, adding metadata from the client certificate"
		relevant_when: "mode = \"tcp\""
		required:      false
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
			client_metadata_key: {
				description: "Event field for client certificate metadata."
				required:    false
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
