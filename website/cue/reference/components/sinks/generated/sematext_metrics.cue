package metadata

generated: components: sinks: sematext_metrics: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::greptimedb::GreptimeDBDefaultBatchSettings>"]
	}
	default_namespace: {
		description: """
			Sets the default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with a period (`.`).
			"""
		required: true
		type: string: examples: ["service"]
	}
	endpoint: {
		description: """
			The endpoint to send data to.

			Setting this option overrides the `region` option.
			"""
		required: false
		type: string: examples: ["http://127.0.0.1", "https://example.com"]
	}
	region: {
		description: "The Sematext region to send data to."
		required:    false
		type: string: {
			default: "us"
			enum: {
				eu: "Europe"
				us: "United States"
			}
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
	token: {
		description: "The token that is used to write to Sematext."
		required:    true
		type: string: examples: ["${SEMATEXT_TOKEN}", "some-sematext-token"]
	}
}
