package metadata

generated: components: sources: aws_sqs: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::SourceAcknowledgementsConfig"]
	}
	auth: {
		description: "Configuration of the authentication strategy for interacting with AWS services."
		required:    false
		type:        _schemaDefinitions["vector::aws::auth::AwsAuthentication"]
	}
	client_concurrency: {
		description: """
			Number of concurrent tasks to create for polling the queue for messages.

			Defaults to the number of available CPUs on the system.

			Should not typically need to be changed, but it can sometimes be beneficial to raise this
			value when there is a high rate of messages being pushed into the queue and the messages
			being fetched are small. In these cases, system resources may not be fully utilized without
			fetching more messages per second, as it spends more time fetching the messages than
			processing them.
			"""
		required: false
		type: uint: {}
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::2c0db0ef1f05c78303bd4391"]
	}
	delete_message: {
		description: """
			Whether to delete the message once it is processed.

			It can be useful to set this to `false` for debugging or during the initial setup.
			"""
		required: false
		type: bool: default: true
	}
	endpoint: {
		description: "Custom endpoint for use with AWS-compatible services."
		required:    false
		type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::FramingConfig::2a4f9b813a8f495175f80ccf"]
	}
	poll_secs: {
		description: """
			How long to wait while polling the queue for new messages, in seconds.

			Generally, this should not be changed unless instructed to do so, as if messages are available,
			they are always consumed, regardless of the value of `poll_secs`.
			"""
		required: false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
	queue_url: {
		description: "The URL of the SQS queue to poll for messages."
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
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	visibility_timeout_secs: {
		description: """
			The visibility timeout to use for messages, in seconds.

			This controls how long a message is left unavailable after it is received. If a message is received, and
			takes longer than `visibility_timeout_secs` to process and delete the message from the queue, it is made available again for another consumer.

			This can happen if there is an issue between consuming a message and deleting it.
			"""
		required: false
		type: uint: {
			default: 300
			unit:    "seconds"
		}
	}
}
