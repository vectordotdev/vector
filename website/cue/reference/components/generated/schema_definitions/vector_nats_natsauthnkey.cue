package metadata

_schemaDefinitions: "vector::nats::NatsAuthNKey": object: options: {
	nkey: {
		description: """
			User.

			Conceptually, this is equivalent to a public key.
			"""
		required: true
		type: string: {}
	}
	seed: {
		description: """
			Seed.

			Conceptually, this is equivalent to a private key.
			"""
		required: true
		type: string: {}
	}
}
