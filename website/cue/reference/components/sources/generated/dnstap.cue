package metadata

generated: components: sources: dnstap: configuration: {
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
		type:          _schemaDefinitions["core::option::Option<vector_core::tcp::TcpKeepaliveConfig>"]
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
		description:   "`TlsEnableableConfig` for `sources`, adding metadata from the client certificate."
		relevant_when: "mode = \"tcp\""
		required:      false
		type:          _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsSourceConfig>"]
	}
}
