package metadata

_schemaDefinitions: "vector::http::KeepaliveConfig": object: options: {
	max_connection_age_jitter_factor: {
		description: """
			The factor by which to jitter the `max_connection_age_secs` value.

			A value of 0.1 means that the actual duration will be between 90% and 110% of the
			specified maximum duration.
			"""
		required: false
		type: float: default: 0.1
	}
	max_connection_age_secs: {
		description: """
			The maximum amount of time a connection may exist before it is closed by sending
			a `Connection: close` header on the HTTP response. Set this to a large value like
			`100000000` to "disable" this feature

			Only applies to HTTP/0.9, HTTP/1.0, and HTTP/1.1 requests.

			A random jitter configured by `max_connection_age_jitter_factor` is added
			to the specified duration to spread out connection storms.
			"""
		required: false
		type: uint: {
			default: 300
			examples: [
				600
			]
			unit: "seconds"
		}
	}
	tcp_keepalive: {
		description: """
			TCP keepalive settings for accepted connections.

			Configures OS-level TCP keepalive probes on accepted connections. When set, the OS
			will send keepalive probes after the specified idle time has elapsed, detecting and
			closing connections where the remote peer has disappeared without sending a FIN or
			RST packet (for example, due to an abrupt machine failure or network partition).
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector_core::tcp::TcpKeepaliveConfig>"]
	}
}
