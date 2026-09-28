package metadata

generated: components: sinks: aws_sqs: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	auth: {
		description: "Configuration of the authentication strategy for interacting with AWS services."
		required:    false
		type:        _schemaDefinitions["vector::aws::auth::AwsAuthentication"]
	}
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type:     _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
	}
	endpoint: {
		description: "Custom endpoint for use with AWS-compatible services."
		required:    false
		type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
	}
	message_deduplication_id: {
		description: """
			The message deduplication ID value to allow AWS to identify duplicate messages.

			This value is a template which should result in a unique string for each event. See the [AWS
			documentation][deduplication_id_docs] for more about how AWS does message deduplication.

			[deduplication_id_docs]: https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/using-messagededuplicationid-property.html
			"""
		required: false
		type: string: {}
	}
	message_group_id: {
		description: """
			The tag that specifies that a message belongs to a specific message group.

			Can be applied only to FIFO queues.
			"""
		required: false
		type: string: {}
	}
	queue_url: {
		description: "The URL of the Amazon SQS queue to which messages are sent."
		required:    true
		type: string: examples: ["https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"]
	}
	region: {
		description: """
			The [AWS region][aws_region] of the target service.

			[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
			"""
		required: false
		type: string: examples: ["us-east-1"]
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
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
