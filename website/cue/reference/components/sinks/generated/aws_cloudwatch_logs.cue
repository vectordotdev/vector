package metadata

generated: components: sinks: aws_cloudwatch_logs: configuration: {
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
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized or compressed.
					"""
				required: false
				type: uint: {
					default: 1048576
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: {
					default: 10000
					unit:    "events"
				}
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 1.0
					unit:    "seconds"
				}
			}
		}
	}
	compression: {
		description: """
			Compression configuration.

			All compression algorithms use the default compression level unless otherwise specified.
			"""
		required: false
		type: string: {
			default: "none"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				snappy: """
					[Snappy][snappy] compression.

					[snappy]: https://github.com/google/snappy/blob/main/docs/README.md
					"""
				zlib: """
					[Zlib][zlib] compression.

					[zlib]: https://zlib.net/
					"""
				zstd: """
					[Zstandard][zstd] compression.

					[zstd]: https://facebook.github.io/zstd/
					"""
			}
		}
	}
	create_missing_group: {
		description: """
			Dynamically create a [log group][log_group] if it does not already exist.

			This ignores `create_missing_stream` directly after creating the group and creates
			the first stream.

			[log_group]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
			"""
		required: false
		type: bool: default: true
	}
	create_missing_stream: {
		description: """
			Dynamically create a [log stream][log_stream] if it does not already exist.

			[log_stream]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
			"""
		required: false
		type: bool: default: true
	}
	dangerously_allow_unconfined_template_resolution: {
		description: """
			Disable all template confinement checks for this sink.

			**DANGEROUS — disables a security control.**

			Bypasses both startup validation and runtime confinement for every
			templated field on this sink. When enabled, a log producer that
			controls any field used in a template can write to arbitrary keys,
			paths, or routing destinations. This flag is a full opt-out: it
			disables confinement even for templates that have a usable static
			prefix.
			"""
		required: false
		type: bool: default: false
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
	group_name: {
		description: """
			The [group name][group_name] of the target CloudWatch Logs stream.

			[group_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
			"""
		required: true
		type: string: {
			examples: ["group-name", "group-{{ file }}"]
			syntax: "template"
		}
	}
	kms_key: {
		description: """
			The [ARN][arn] (Amazon Resource Name) of the [KMS key][kms_key] to use when encrypting log data.

			[arn]: https://docs.aws.amazon.com/IAM/latest/UserGuide/reference-arns.html
			[kms_key]: https://docs.aws.amazon.com/kms/latest/developerguide/overview.html
			"""
		required: false
		type: string: {}
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
		description: "Outbound HTTP request settings."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::http::RequestConfig"]
	}
	retention: {
		description: "Retention policy configuration for AWS CloudWatch Log Group"
		required:    false
		type: object: options: {
			days: {
				description: "If retention is enabled, the number of days to retain logs for."
				required:    false
				type: uint: default: 0
			}
			enabled: {
				description: "Whether or not to set a retention policy when creating a new Log Group."
				required:    false
				type: bool: default: false
			}
		}
	}
	stream_name: {
		description: """
			The [stream name][stream_name] of the target CloudWatch Logs stream.

			There can only be one writer to a log stream at a time. If multiple instances are writing to
			the same log group, the stream name must include an identifier that is guaranteed to be
			unique per instance.

			[stream_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
			"""
		required: true
		type: string: {
			examples: ["stream-{{ host }}", "%Y-%m-%d", "stream-name"]
			syntax: "template"
		}
	}
	tags: {
		description: """
			The Key-value pairs to be applied as [tags][tags] to the log group and stream.

			[tags]: https://docs.aws.amazon.com/whitepapers/latest/tagging-best-practices/what-are-tags.html
			"""
		required: false
		type: object: options: "*": {
			description: "A tag represented as a key-value pair"
			required:    true
			type: string: {}
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
