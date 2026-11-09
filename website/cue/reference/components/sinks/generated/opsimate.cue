package metadata

generated: components: sinks: opsimate: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["bool_or_struct::vector_core::config::AcknowledgementsConfig"]
	}
	api_token: {
		description: """
			The API token that is used to authenticate against OpsiMate.

			It is sent in the `x-api-token` header of every request.
			"""
		required: true
		type: string: examples: ["${OPSIMATE_API_TOKEN}"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["derived::6f45f8b8572be16d3c7f3049"]
	}
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	endpoint: {
		description: """
			The base URL of the OpsiMate server.

			Alerts are sent to `{endpoint}/api/v1/alerts/custom` and the health check
			queries `{endpoint}/health`.
			"""
		required: true
		type: string: examples: ["http://opsimate.example.com:3001"]
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::service::TowerRequestConfig"]
	}
	retry_strategy: {
		description: """
			Configurable retry strategy for `http` based sinks.

			For more information about error responses, see [Client Error Responses][error_responses].

			[error_responses]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status#client_error_responses
			"""
		required: false
		type:     _schemaDefinitions["derived::vector::sinks::util::http::RetryStrategy::64fe77681a1c274eed24cca6"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
