package metadata

generated: components: sources: aws_kinesis_firehose: configuration: {
	access_key: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use `access_keys` instead."
		description: """
			An access key to authenticate requests against.

			AWS Kinesis Firehose can be configured to pass along a user-configurable access key with each request. If
			configured, `access_key` should be set to the same value. Otherwise, all requests are allowed.
			"""
		required: false
		type: string: examples: ["A94A8FE5CCB19BA61C4C08"]
	}
	access_keys: {
		description: """
			A list of access keys to authenticate requests against.

			AWS Kinesis Firehose can be configured to pass along a user-configurable access key with each request. If
			configured, `access_keys` should be set to the same value. Otherwise, all requests are allowed.
			"""
		required: false
		type: array: items: type: string: examples: ["A94A8FE5CCB19BA61C4C08", "B94B8FE5CCB19BA61C4C12"]
	}
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
	address: {
		description: "The socket address to listen for connections on."
		required:    true
		type: string: examples: ["0.0.0.0:443", "localhost:443"]
	}
	common_attributes: {
		description: """
			A list of attributes from X-Amz-Firehose-Common-Attributes header to include in the log event.

			Accepts the wildcard (`*`) character for attributes matching a specified pattern.

			Specifying "*" results in all common attributes included in the log event.

			Legacy namespace: selected attributes are added under the root `common_attributes` object
			Vector namespace: selected attributes are added under the source metadata at `aws_kinesis_firehose.common_attributes`
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["environment", "application_group", "application_*", "*"]
		}
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::2c0db0ef1f05c78303bd4391"]
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
	keepalive: {
		description: "Configuration of HTTP server keepalive parameters."
		required:    false
		type:        _schemaDefinitions["vector::http::KeepaliveConfig"]
	}
	record_compression: {
		description: """
			The compression scheme to use for decompressing records within the Firehose message.

			Some services, like AWS CloudWatch Logs, [compresses the events with gzip][events_with_gzip],
			before sending them AWS Kinesis Firehose. This option can be used to automatically decompress
			them before forwarding them to the next component.

			Note that this is different from [Content encoding option][encoding_option] of the
			Firehose HTTP endpoint destination. That option controls the content encoding of the entire HTTP request.

			[events_with_gzip]: https://docs.aws.amazon.com/firehose/latest/dev/writing-with-cloudwatch-logs.html
			[encoding_option]: https://docs.aws.amazon.com/firehose/latest/dev/create-destination.html#create-destination-http
			"""
		required: false
		type: string: {
			default: "auto"
			enum: {
				auto: """
					Automatically attempt to determine the compression scheme.

					The compression scheme of the object is determined by looking at its file signature, also known
					as [magic bytes][magic_bytes].

					If the record fails to decompress with the discovered format, the record is forwarded as is.
					Thus, if you know the records are always gzip encoded (for example, if they are coming from AWS CloudWatch Logs),
					set `gzip` in this field so that any records that are not-gzipped are rejected.

					[magic_bytes]: https://en.wikipedia.org/wiki/List_of_file_signatures
					"""
				gzip: "GZIP."
				none: "Uncompressed."
			}
		}
	}
	store_access_key: {
		description: """
			Whether or not to store the AWS Firehose Access Key in event secrets.

			If set to `true`, when incoming requests contains an access key sent by AWS Firehose, it is kept in the
			event secrets as "aws_kinesis_firehose_access_key".
			"""
		required: true
		type: bool: {}
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
