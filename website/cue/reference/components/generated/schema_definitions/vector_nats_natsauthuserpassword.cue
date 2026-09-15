package metadata

_schemaDefinitions: "vector::nats::NatsAuthUserPassword": object: options: {
	password: {
		description: "Password."
		required:    true
		type: string: {}
	}
	user: {
		description: "Username."
		required:    true
		type: string: {}
	}
}
