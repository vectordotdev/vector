package metadata

generated: components: sources: syslog: configuration: {
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
		type: uint: {}
	}
	host_key: {
		description: """
			Overrides the name of the log field used to add the peer host to each event.

			If using TCP or UDP, the value is the peer host's address, including the port. For example, `1.2.3.4:9000`. If using
			UDS, the value is the socket path itself.

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
		type:          _schemaDefinitions["core::option::Option<vector_core::tcp::TcpKeepaliveConfig>"]
	}
	max_length: {
		description: """
			The maximum buffer size of incoming messages, in bytes.

			Messages larger than this are truncated.
			"""
		required: false
		type: uint: {
			default: 102400
			unit:    "bytes"
		}
	}
	mode: {
		description: "The type of socket to use."
		required:    true
		type: string: enum: {
			tcp: "Listen on TCP."
			udp: "Listen on UDP."
			unix: """
				Listen on UDS (Unix domain socket). This only supports Unix stream sockets.

				For Unix datagram sockets, use the `socket` source instead.
				"""
		}
	}
	path: {
		description: """
			The Unix socket path.

			This should be an absolute path.
			"""
		relevant_when: "mode = \"unix\""
		required:      true
		type: string: examples: ["/path/to/socket"]
	}
	permit_origin: {
		description:   "List of allowed origin IP networks. IP addresses must be in CIDR notation."
		relevant_when: "mode = \"tcp\""
		required:      false
		type: array: items: type: string: examples: ["192.168.0.0/16", "127.0.0.1/32", "::1/128", "9876:9ca3:99ab::23/128"]
	}
	receive_buffer_bytes: {
		description: """
			The size of the receive buffer used for each connection.

			This should not typically needed to be changed.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      false
		type: uint: unit: "bytes"
	}
	socket_file_mode: {
		description: """
			Unix file mode bits to be applied to the unix socket file as its designated file permissions.

			The file mode value can be specified in any numeric format supported by your configuration
			language, but it is most intuitive to use an octal number.
			"""
		relevant_when: "mode = \"unix\""
		required:      false
		type: uint: {}
	}
	tls: {
		description:   "`TlsEnableableConfig` for `sources`, adding metadata from the client certificate."
		relevant_when: "mode = \"tcp\""
		required:      false
		type:          _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsSourceConfig>"]
	}
	tls_handshake_timeout_secs: {
		description: """
			The timeout, in seconds, before a TLS handshake is aborted if it has not completed.

			This bounds how long a connection can hold its slot against `connection_limit`
			before the TLS handshake finishes, protecting against clients that open a
			connection but never complete (or never start) a handshake.
			"""
		relevant_when: "mode = \"tcp\""
		required:      false
		type: uint: unit: "seconds"
	}
}
