package metadata

generated: components: sources: statsd: configuration: {
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
	convert_to: {
		description: "Specifies the target unit for converting incoming StatsD timing values. When set to \"seconds\" (the default), timing values in milliseconds (`ms`) are converted to seconds (`s`). When set to \"milliseconds\", the original timing values are preserved."
		required:    false
		type: string: {
			default: "seconds"
			enum: {
				milliseconds: "Convert to milliseconds."
				seconds:      "Convert to seconds."
			}
		}
	}
	keepalive: {
		description:   "TCP keepalive settings for socket-based components."
		relevant_when: "mode = \"tcp\""
		required:      false
		type:          _schemaDefinitions["core::option::Option<vector_core::tcp::TcpKeepaliveConfig>"]
	}
	mode: {
		description: "The type of socket to use."
		required:    true
		type: string: enum: {
			tcp:  "Listen on TCP."
			udp:  "Listen on UDP."
			unix: "Listen on a Unix domain Socket (UDS)."
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
		description:   "The size of the receive buffer used for each connection."
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      false
		type: uint: unit: "bytes"
	}
	sanitize: {
		description: """
			Whether or not to sanitize incoming statsd key names. When "true", keys are sanitized by:
			- "/" is replaced with "-"
			- All whitespace is replaced with "_"
			- All non alphanumeric characters (A-Z, a-z, 0-9, _, or -) are removed.
			"""
		required: false
		type: bool: default: true
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
