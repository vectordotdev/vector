package metadata

_schemaDefinitions: "core::option::Option<vector_core::tcp::TcpKeepaliveConfig>": object: options: time_secs: {
	description: "The time to wait before starting to send TCP keepalive probes on an idle connection."
	required:    false
	type: uint: unit: "seconds"
}
