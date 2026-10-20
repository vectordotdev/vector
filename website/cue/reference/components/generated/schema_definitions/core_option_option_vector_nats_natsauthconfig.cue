package metadata

_schemaDefinitions: "core::option::Option<vector::nats::NatsAuthConfig>": object: options: {
	credentials_file: {
		description:   "Credentials file configuration."
		relevant_when: "strategy = \"credentials_file\""
		required:      true
		type:          _schemaDefinitions["vector::nats::NatsAuthCredentialsFile"]
	}
	nkey: {
		description:   "NKeys configuration."
		relevant_when: "strategy = \"nkey\""
		required:      true
		type:          _schemaDefinitions["vector::nats::NatsAuthNKey"]
	}
	strategy: {
		description: """
			The strategy used to authenticate with the NATS server.

			More information on NATS authentication, and the various authentication strategies, can be found in the
			NATS [documentation][nats_auth_docs]. For TLS client certificate authentication specifically, see the
			`tls` settings.

			[nats_auth_docs]: https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro
			"""
		required: true
		type: string: enum: {
			credentials_file: "Credentials file authentication. (JWT-based)"
			nkey:             "NKey authentication."
			token:            "Token authentication."
			user_password:    "Username/password authentication."
		}
	}
	token: {
		description:   "Token configuration."
		relevant_when: "strategy = \"token\""
		required:      true
		type:          _schemaDefinitions["vector::nats::NatsAuthToken"]
	}
	user_password: {
		description:   "Username and password configuration."
		relevant_when: "strategy = \"user_password\""
		required:      true
		type:          _schemaDefinitions["vector::nats::NatsAuthUserPassword"]
	}
}
