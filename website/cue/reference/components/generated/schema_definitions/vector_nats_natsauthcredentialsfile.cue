package metadata

_schemaDefinitions: "vector::nats::NatsAuthCredentialsFile": object: options: path: {
	description: "Path to credentials file."
	required:    true
	type: string: examples: ["/etc/nats/nats.creds"]
}
