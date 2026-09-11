package metadata

_schemaDefinitions: "core::option::Option<vector::common::http::server_auth::HttpServerAuthConfig>": object: options: {
	password: {
		description:   "The basic authentication password."
		relevant_when: "strategy = \"basic\""
		required:      true
		type: string: examples: ["${PASSWORD}", "password"]
	}
	source: {
		description:   "The VRL boolean expression."
		relevant_when: "strategy = \"custom\""
		required:      true
		type: string: {}
	}
	strategy: {
		description: "The authentication strategy to use."
		required:    true
		type: string: enum: {
			basic: """
				Basic authentication.

				The username and password are concatenated and encoded using [base64][base64].

				[base64]: https://en.wikipedia.org/wiki/Base64
				"""
			bearer: """
				Bearer authentication.

				The token is matched against the `Authorization` header using the `Bearer` scheme.
				"""
			custom: """
				Custom authentication using VRL code.

				Takes in request and validates it using VRL code. The VRL program must return a boolean.
				"""
		}
	}
	token: {
		description:   "The bearer token to match against incoming requests."
		relevant_when: "strategy = \"bearer\""
		required:      true
		type: string: examples: ["${TOKEN}", "my-secret-token"]
	}
	username: {
		description:   "The basic authentication username."
		relevant_when: "strategy = \"basic\""
		required:      true
		type: string: examples: ["${USERNAME}", "username"]
	}
}
