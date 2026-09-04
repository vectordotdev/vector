package metadata

generated: components: sinks: mezmo: configuration: {
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
		description: "The Ingestion API key."
		required:    true
		type: string: examples: ["${LOGDNA_API_KEY}", "ef8d5de700e7989468166c40fc8a0ccd"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
	}
	default_app: {
		description: "The default app that is set for events that do not contain a `file` or `app` field."
		required:    false
		type: string: {
			default: "vector"
			examples: [
				"my-app"
			]
		}
	}
	default_env: {
		description: "The default environment that is set for events that do not contain an `env` field."
		required:    false
		type: string: {
			default: "production"
			examples: ["staging"]
		}
	}
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	endpoint: {
		description: """
			The HTTP endpoint to send logs to.

			Both IP address and hostname are accepted formats.
			"""
		required: false
		type: string: {
			default: "https://logs.mezmo.com/"
			examples: ["http://127.0.0.1", "http://example.com"]
		}
	}
	hostname: {
		description: "The hostname that is attached to each batch of events."
		required:    true
		type: string: {
			examples: ["${HOSTNAME}", "my-local-machine"]
			syntax: "template"
		}
	}
	ip: {
		description: "The IP address that is attached to each batch of events."
		required:    false
		type: string: examples: ["0.0.0.0"]
	}
	mac: {
		description: "The MAC address that is attached to each batch of events."
		required:    false
		type: string: examples: ["my-mac-address"]
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
	tags: {
		description: "The tags that are attached to each batch of events."
		required:    false
		type: array: items: type: string: {
			examples: ["tag1", "tag2"]
			syntax: "template"
		}
	}
}
