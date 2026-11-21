package metadata

_schemaDefinitions: "core::option::Option<vector::sources::util::http::cors::CorsConfig>": object: options: {
	allowed_headers: {
		description: """
			Additional HTTP headers permitted in cross-origin requests.

			`Content-Type` and `Content-Encoding` are always permitted.
			Add `Authorization` here when using HTTP authentication.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["Authorization"]
		}
	}
	allowed_origins: {
		description: """
			Origins permitted to send cross-origin requests.

			Each origin must be an HTTP or HTTPS origin, including its port when non-default,
			without a path or trailing slash. Use `["*"]` to allow any origin.
			Subdomain wildcards and credentialed browser requests are not supported.
			"""
		required: true
		type: array: items: type: string: examples: ["https://app.example.com"]
	}
	max_age_secs: {
		description: "Maximum time a browser may cache a successful preflight response."
		required:    false
		type: uint: unit: "seconds"
	}
}
