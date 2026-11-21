package metadata

_schemaDefinitions: "core::option::Option<vector::http::VectorAuthConfig>": object: options: {
	password: {
		description:   "The basic authentication password."
		relevant_when: "strategy = \"basic\""
		required:      true
		type: string: examples: ["${PASSWORD}", "password"]
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

				The bearer token value (OAuth 2.0, JWT, etc.) is passed as-is.
				"""
			custom: "Custom Authorization Header Value, will be inserted into the headers as `Authorization: < value >`"
		}
	}
	token: {
		description:   "The bearer authentication token."
		relevant_when: "strategy = \"bearer\""
		required:      true
		type: string: {}
	}
	user: {
		description:   "The basic authentication username."
		relevant_when: "strategy = \"basic\""
		required:      true
		type: string: examples: ["${USERNAME}", "username"]
	}
	value: {
		description:   "Custom string value of the Authorization header"
		relevant_when: "strategy = \"custom\""
		required:      true
		type: string: examples: ["${AUTH_HEADER_VALUE}", "CUSTOM_PREFIX ${TOKEN}"]
	}
}
