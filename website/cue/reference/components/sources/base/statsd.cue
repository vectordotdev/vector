package metadata

base: components: sources: statsd: configuration: {
	address: {
		description:   "The address to listen for connections on."
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      true
		type: {
			number: {}
			string: {}
		}
	}
	connection_limit: {
		description:   "The maximum number of TCP connections that will be allowed at any given time."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: uint: {}
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
	mode: {
		description: "The type of socket to use."
		required:    true
		type: string: enum: {
			tcp:  "Listen on TCP."
			udp:  "Listen on UDP."
			unix: "Listen on UDS. (Unix domain socket)"
		}
	}
	path: {
		description: """
			The Unix socket path.

			This should be an absolute path.
			"""
		relevant_when: "mode = \"unix\""
		required:      true
		type: string: {}
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
		description:   "The timeout before a connection is forcefully closed during shutdown."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: uint: default: 30
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
			client_metadata_key: {
				description: "Event field for client certificate metadata."
				required:    false
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
}
