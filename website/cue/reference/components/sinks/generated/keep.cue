package metadata

generated: components: sinks: keep: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	api_key: {
		description: "The API key that is used to authenticate against Keep."
		required:    true
		type: string: examples: ["${KEEP_API_KEY}", "keepappkey"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["derived::eeb6320bec5ff6539edbeb63"]
	}
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	endpoint: {
		description: "Keeps endpoint to send logs to"
		required:    false
		type: string: {
			default: "http://localhost:8080/alerts/event/vectordev?provider_id=test"
			examples: ["https://backend.keep.com:8081/alerts/event/vectordev?provider_id=test"]
		}
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
}
