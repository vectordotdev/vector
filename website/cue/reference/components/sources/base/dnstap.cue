package metadata

base: components: sources: dnstap: configuration: {
	address: {
		description: """
			The socket address to listen for connections on, or `systemd{#N}` to use the Nth socket passed by
			systemd socket activation.

			If a socket address is used, it _must_ include a port.
			"""
		relevant_when: "mode = \"tcp\""
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
			Overrides the name of the log field used to add the source path to each event.

			The value is the socket path itself.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

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
	lowercase_hostnames: {
		description: "Whether to downcase all DNSTAP hostnames received for consistency"
		required:    false
		type: bool: default: false
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
	max_frame_handling_tasks: {
		description: "Maximum number of frames that can be processed concurrently."
		required:    false
		type: uint: {}
	}
	max_frame_length: {
		description: """
			Maximum DNSTAP frame length that the source accepts.

			If any frame is longer than this, it is discarded.
			"""
		required: false
		type: uint: {
			default: 102400
			unit:    "bytes"
		}
	}
	mode: {
		description: "The type of dnstap socket to use."
		required:    true
		type: string: enum: {
			tcp:  "Listen on TCP."
			unix: "Listen on a Unix domain socket"
		}
	}
	multithreaded: {
		description: "Whether or not to concurrently process DNSTAP frames."
		required:    false
		type: bool: {}
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
		relevant_when: "mode = \"tcp\""
		required:      false
		type: string: default: "port"
	}
	raw_data_only: {
		description: """
			Whether or not to skip parsing or decoding of DNSTAP frames.

			If set to `true`, frames are not parsed or decoded. The raw frame data is set as a field on the event
			(called `rawData`) and encoded as a base64 string.
			"""
		required: false
		type: bool: {}
	}
	receive_buffer_bytes: {
		description:   "The size of the receive buffer used for each connection."
		relevant_when: "mode = \"tcp\""
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
		relevant_when: "mode = \"unix\""
		required:      false
		type: uint: {}
	}
	socket_path: {
		description: """
			Absolute path to the socket file to read DNSTAP data from.

			The DNS server must be configured to send its DNSTAP data to this socket file. The socket file is created
			if it doesn't already exist when the source first starts.
			"""
		relevant_when: "mode = \"unix\""
		required:      true
		type: string: {}
	}
	socket_receive_buffer_size: {
		description: """
			The size, in bytes, of the receive buffer used for the socket.

			This should not typically needed to be changed.
			"""
		relevant_when: "mode = \"unix\""
		required:      false
		type: uint: unit: "bytes"
	}
	socket_send_buffer_size: {
		description: """
			The size, in bytes, of the send buffer used for the socket.

			This should not typically needed to be changed.
			"""
		relevant_when: "mode = \"unix\""
		required:      false
		type: uint: unit: "bytes"
	}
	tls: {
		description:   "TlsEnableableConfig for `sources`, adding metadata from the client certificate."
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
					Whether or not to require TLS for incoming or outgoing connections.

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
