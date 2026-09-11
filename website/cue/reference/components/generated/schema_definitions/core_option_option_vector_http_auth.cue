package metadata

_schemaDefinitions: "core::option::Option<vector::http::Auth>": object: options: {
	auth: {
		description:   "The AWS authentication configuration."
		relevant_when: "strategy = \"aws\""
		required:      true
		type:          _schemaDefinitions["vector::aws::auth::AwsAuthentication"]
	}
	password: {
		description:   "The basic authentication password."
		relevant_when: "strategy = \"basic\""
		required:      true
		type: string: examples: ["${PASSWORD}", "password"]
	}
	service: {
		description:   "The AWS service name to use for signing."
		relevant_when: "strategy = \"aws\""
		required:      true
		type: string: {}
	}
	strategy: {
		description: "The authentication strategy to use."
		required:    true
		type: string: enum: {
			aws: "AWS authentication."
			basic: """
				Basic authentication.

				The username and password are concatenated and encoded using [base64][base64].

				[base64]: https://en.wikipedia.org/wiki/Base64
				"""
			bearer: """
				Bearer authentication.

				The bearer token value (OAuth2, JWT, etc.) is passed as-is.
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
