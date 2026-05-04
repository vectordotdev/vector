package metadata

generated: components: sources: socket: configuration: {
	address: {
		description: """
			The socket address to listen for connections on, or `systemd{#N}` to use the Nth socket passed by
			systemd socket activation.

			If a socket address is used, it _must_ include a port.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      true
		type: string: examples: ["0.0.0.0:9000", "systemd", "systemd#3"]
	}
	connection_limit: {
		description:   "The maximum number of TCP connections that are allowed at any given time."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: uint: unit: "connections"
	}
	host_key: {
		description: """
			Overrides the name of the log field used to add the peer host to each event.

			The value will be the peer host's address, including the port i.e. `1.2.3.4:9000`.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			Set to `""` to suppress this key.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: {}
	}
	keepalive: {
		description:   "TCP keepalive settings for socket-based components."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: object: options: time_secs: {
			description: "The time to wait before starting to send TCP keepalive probes on an idle connection."
			required:    false
			type: uint: unit: "seconds"
		}
	}
	max_connection_duration_secs: {
		description: """
			Maximum duration to keep each connection open. Connections open for longer than this duration are closed.

			This is helpful for load balancing long-lived connections.
			"""
		relevant_when: "mode = \"tcp\""
		required:      false
		type: uint: unit: "seconds"
	}
	max_length: {
		description: """
			The maximum buffer size of incoming messages.

			Messages larger than this are truncated.
			"""
		relevant_when: "mode = \"udp\""
		required:      false
		type: uint: {
			default: 102400
			unit:    "bytes"
		}
	}
	mode: {
		description: "The type of socket to use."
		required:    true
		type: string: enum: {
			tcp:           "Listen on TCP."
			udp:           "Listen on UDP."
			unix_datagram: "Listen on a Unix domain socket (UDS), in datagram mode."
			unix_stream:   "Listen on a Unix domain socket (UDS), in stream mode."
		}
	}
	multicast_groups: {
		description: """
			List of IPv4 multicast groups to join on socket's binding process.

			In order to read multicast packets, this source's listening address should be set to `0.0.0.0`.
			If any other address is used (such as `127.0.0.1` or an specific interface address), the
			listening interface will filter out all multicast packets received,
			as their target IP would be the one of the multicast group
			and it will not match the socket's bound IP.

			Note that this setting will only work if the source's address
			is an IPv4 address (IPv6 and systemd file descriptor as source's address are not supported
			with multicast groups).
			"""
		relevant_when: "mode = \"udp\""
		required:      false
		type: array: {
			default: []
			items: type: string: examples: ["['224.0.0.2', '224.0.0.4']"]
		}
	}
	path: {
		description: """
			The Unix socket path.

			This should be an absolute path.
			"""
		relevant_when: "mode = \"unix_datagram\" or mode = \"unix_stream\""
		required:      true
		type: string: examples: ["/path/to/socket"]
	}
	permit_origin: {
		description:   "List of allowed origin IP networks. IP addresses must be in CIDR notation."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: array: items: type: string: examples: ["192.168.0.0/16", "127.0.0.1/32", "::1/128", "9876:9ca3:99ab::23/128"]
	}
	port_key: {
		description: """
			Overrides the name of the log field used to add the peer host's port to each event.

			The value will be the peer host's port i.e. `9000`.

			By default, `"port"` is used.

			Set to `""` to suppress this key.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      false
		type: string: default: "port"
	}
	receive_buffer_bytes: {
		description:   "The size of the receive buffer used for each connection."
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      false
		type: uint: unit: "bytes"
	}
	shutdown_timeout_secs: {
		description:   "The timeout before a connection is forcefully closed during shutdown."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: uint: {
			default: 30
			unit:    "seconds"
		}
	}
	socket_file_mode: {
		description: """
			Unix file mode bits to be applied to the unix socket file as its designated file permissions.

			Note: The file mode value can be specified in any numeric format supported by your configuration
			language, but it is most intuitive to use an octal number.
			"""
		relevant_when: "mode = \"unix_datagram\" or mode = \"unix_stream\""
		required:      false
		type: uint: examples: [511, 384, 508]
	}
	tls: {
		description:   "`TlsEnableableConfig` for `sources`, adding metadata from the client certificate."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
					that they are defined.
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

					If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.crt"]
			}
			enabled: {
				description: """
					Whether to require TLS for incoming or outgoing connections.

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
			server_name: {
				description: """
					Server name to use when using Server Name Indication (SNI).

					Only relevant for outgoing connections.
					"""
				required: false
				type: string: examples: ["www.example.com"]
			}
			verify_certificate: {
				description: """
					Enables certificate verification. For components that create a server, this requires that the
					client connections have a valid client certificate. For components that initiate requests,
					this validates that the upstream has a valid certificate.

					If enabled, certificates must not be expired and must be issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
					certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
					so on, until the verification process reaches a root certificate.

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

generated: components: sources: socket: configuration: decoding: decodingBase & {
	type: object: options: codec: {
		required: false
		type: string: default: "bytes"
	}
}
generated: components: sources: socket: configuration: framing: framingDecoderBase & {
	type: object: options: method: required: true
}
