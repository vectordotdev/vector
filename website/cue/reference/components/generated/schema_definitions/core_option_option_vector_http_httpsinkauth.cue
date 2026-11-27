package metadata

_schemaDefinitions: "core::option::Option<vector::http::HttpSinkAuth>": object: options: {
	audience: {
		description: """
			The audience to request from the authorization server.

			Vector sends this value as the `audience` parameter of the token request. Some
			authorization servers reject a token request without it.
			"""
		relevant_when: "strategy = \"oauth2\""
		required:      false
		type: string: examples: ["${AUDIENCE}"]
	}
	auth: {
		description:   "The AWS authentication configuration."
		relevant_when: "strategy = \"aws\""
		required:      true
		type:          _schemaDefinitions["vector::aws::auth::AwsAuthentication"]
	}
	client_id: {
		description:   "The client identifier from the authorization server."
		relevant_when: "strategy = \"oauth2\""
		required:      true
		type: string: examples: ["${CLIENT_ID}"]
	}
	client_secret: {
		description: """
			The client secret.

			When this field is set, Vector authenticates to the token endpoint with HTTP Basic
			authentication (RFC 6749, section 2.3.1). When this field is unset, Vector authenticates to
			the token endpoint with a client certificate.
			"""
		relevant_when: "strategy = \"oauth2\""
		required:      false
		type: string: examples: ["${CLIENT_SECRET}"]
	}
	expiry_buffer_secs: {
		description: """
			The number of seconds to refresh the access token before it expires.

			When this value is equal to or larger than the access token lifetime, Vector requests a new
			access token for each request.
			"""
		relevant_when: "strategy = \"oauth2\""
		required:      false
		type: uint: {
			default: 60
			unit:    "seconds"
		}
	}
	password: {
		description:   "The basic authentication password."
		relevant_when: "strategy = \"basic\""
		required:      true
		type: string: examples: ["${PASSWORD}"]
	}
	resource: {
		description: """
			The resource to request from the authorization server.

			Vector sends this value as the `resource` parameter of the token request
			([RFC 8707][rfc8707]).

			[rfc8707]: https://datatracker.ietf.org/doc/html/rfc8707
			"""
		relevant_when: "strategy = \"oauth2\""
		required:      false
		type: string: examples: ["${RESOURCE}"]
	}
	scope: {
		description: """
			The scope to request from the authorization server.

			Some authorization servers require a scope. Vector sends this value as the `scope`
			parameter of the token request.
			"""
		relevant_when: "strategy = \"oauth2\""
		required:      false
		type: string: examples: [
			"${SCOPE}"
		]
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

				The bearer token value (OAuth 2.0, JWT, etc.) is passed as-is.
				"""
			custom: "Custom Authorization Header Value, will be inserted into the headers as `Authorization: < value >`"
			oauth2: """
				OAuth 2.0 client credentials.

				Vector requests an access token from `token_endpoint`. Vector caches the access token and
				refreshes it before it expires. Vector sends the access token as a bearer token. Set
				`client_secret` to use the standard grant. Set `tls` with a client certificate to use mutual
				TLS ([RFC 8705][rfc8705]).

				[rfc8705]: https://datatracker.ietf.org/doc/html/rfc8705
				"""
		}
	}
	timeout_secs: {
		description: """
			The time limit for a complete token request.

			A stalled authorization server cannot hold the refresh lock and block the requests of the
			sink for longer than this value. A value of `0` is rejected.
			"""
		relevant_when: "strategy = \"oauth2\""
		required:      false
		type: uint: {
			default: 10
			unit:    "seconds"
		}
	}
	tls: {
		description: """
			The TLS settings for the connection to the token endpoint.

			Set this field when the token endpoint needs different TLS settings than the sink `tls`
			field. A client certificate is an example. When this field is unset, the sink `tls` field
			applies.
			"""
		relevant_when: "strategy = \"oauth2\""
		required:      false
		type:          _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	token: {
		description:   "The bearer authentication token."
		relevant_when: "strategy = \"bearer\""
		required:      true
		type: string: {}
	}
	token_endpoint: {
		description: """
			The token endpoint that issues the access token.

			Vector sends the client credential to this endpoint. Use a URI with the `https` scheme
			unless the endpoint is on a trusted network.
			"""
		relevant_when: "strategy = \"oauth2\""
		required:      true
		type: string: examples: ["https://auth.example.com/oauth/token"]
	}
	user: {
		description:   "The basic authentication username."
		relevant_when: "strategy = \"basic\""
		required:      true
		type: string: examples: ["${USERNAME}"]
	}
	value: {
		description:   "Custom string value of the Authorization header."
		relevant_when: "strategy = \"custom\""
		required:      true
		type: string: examples: ["${AUTH_HEADER_VALUE}"]
	}
}
