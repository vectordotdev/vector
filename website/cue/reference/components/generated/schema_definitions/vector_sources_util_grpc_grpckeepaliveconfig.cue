package metadata

_schemaDefinitions: "vector::sources::util::grpc::GrpcKeepaliveConfig": object: options: {
	max_connection_age_grace_secs: {
		description: """
			The grace period added to `max_connection_age_secs` before the server closes the connection.

			This setting only applies when `max_connection_age_secs` is set.
			"""
		required: false
		type: uint: {
			examples: [
				30
			]
			unit: "seconds"
		}
	}
	max_connection_age_secs: {
		description: """
			The maximum amount of time a connection may exist before the server closes it.

			When unset, connections are not closed based on age.
			"""
		required: false
		type: uint: {
			examples: [
				300
			]
			unit: "seconds"
		}
	}
}
