package metadata

generated: components: sources: aws_s3: configuration: {
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
	compression: {
		description: "The compression scheme used for decompressing objects retrieved from S3."
		required:    false
		type: string: {
			default: "auto"
			enum: {
				auto: """
					Automatically attempt to determine the compression scheme.

					The compression scheme of the object is determined from its `Content-Encoding` and
					`Content-Type` metadata, as well as the key suffix (for example, `.gz`).

					It is set to `none` if the compression scheme cannot be determined.
					"""
				gzip: "GZIP."
				none: "Uncompressed."
				zstd: "ZSTD."
			}
		}
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::2c0db0ef1f05c78303bd4391"]
	}
	endpoint: {
		description: "Custom endpoint for use with AWS-compatible services."
		required:    false
		type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
	}
	force_path_style: {
		description: """
			Specifies which addressing style to use.

			This controls whether the bucket name is in the hostname, or part of the URL.
			"""
		required: false
		type: bool: default: true
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited decoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type:          _schemaDefinitions["codecs::decoding::framing::character_delimited::CharacterDelimitedDecoderOptions"]
			}
			chunked_gelf: {
				description:   "Options for the chunked GELF decoder."
				relevant_when: "method = \"chunked_gelf\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::framing::chunked_gelf::ChunkedGelfDecoderOptions"]
			}
			length_delimited: {
				description:   "Options for the length delimited decoder."
				relevant_when: "method = \"length_delimited\""
				required:      true
				type:          _schemaDefinitions["codecs::common::length_delimited::LengthDelimitedCoderOptions"]
			}
			max_frame_length: {
				description:   "Maximum frame length"
				relevant_when: "method = \"varint_length_delimited\""
				required:      false
				type: uint: default: 8388608
			}
			method: {
				description: "The framing method."
				required:    false
				type: string: {
					default: "newline_delimited"
					enum: {
						bytes:               "Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments)."
						character_delimited: "Byte frames which are delimited by a chosen character."
						chunked_gelf: """
															Byte frames which are chunked GELF messages.

															[chunked_gelf]: https://go2docs.graylog.org/current/getting_in_log_data/gelf.html
															"""
						length_delimited:  "Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length."
						newline_delimited: "Byte frames which are delimited by a newline character."
						octet_counting: """
															Byte frames according to the [octet counting][octet_counting] format.

															[octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
															"""
						varint_length_delimited: """
															Byte frames which are prefixed by a varint indicating the length.
															This is compatible with protobuf's length-delimited encoding.
															"""
					}
				}
			}
			newline_delimited: {
				description:   "Options for the newline delimited decoder."
				relevant_when: "method = \"newline_delimited\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::framing::newline_delimited::NewlineDelimitedDecoderOptions"]
			}
			octet_counting: {
				description:   "Options for the octet counting decoder."
				relevant_when: "method = \"octet_counting\""
				required:      false
				type:          _schemaDefinitions["codecs::decoding::framing::octet_counting::OctetCountingDecoderOptions"]
			}
		}
	}
	multiline: {
		description: """
			Multiline aggregation configuration.

			If not specified, multiline aggregation is disabled.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::sources::util::multiline_config::MultilineConfig>"]
	}
	region: {
		description: """
			The [AWS region][aws_region] of the target service.

			[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
			"""
		required: false
		type: string: examples: ["us-east-1"]
	}
	request_payer: {
		description: """
			Enables retrieving objects from [S3 Requester Pays buckets][requester_pays].

			Set this to `requester` to acknowledge that the AWS account associated with Vector's
			configured credentials accepts the request and data transfer charges.

			When unset, Vector does not specify a request payer.

			[requester_pays]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/RequesterPaysBuckets.html
			"""
		required: false
		type: string: enum: requester: """
			Enables retrieving objects from [S3 Requester Pays buckets][requester_pays].

			The requester accepts the S3 request and data transfer charges.
			"""
	}
	sqs: {
		description: "Configuration options for SQS."
		required:    false
		type: object: options: {
			client_concurrency: {
				description: """
					Number of concurrent tasks to create for polling the queue for messages.

					Defaults to the number of available CPUs on the system.

					Should not typically need to be changed, but it can sometimes be beneficial to raise this
					value when there is a high rate of messages being pushed into the queue and the objects
					being fetched are small. In these cases, system resources may not be fully utilized without
					fetching more messages per second, as the SQS message consumption rate affects the S3 object
					retrieval rate.
					"""
				required: false
				type: uint: {
					examples: [5]
					unit: "tasks"
				}
			}
			connect_timeout_seconds: {
				description: """
					The connection timeout for AWS requests

					Limits the amount of time allowed to initiate a socket connection.
					"""
				required: false
				type: uint: {
					examples: [20]
					unit: "seconds"
				}
			}
			deferred: {
				description: "Configuration for deferring events to another queue based on their age."
				required:    false
				type: object: options: {
					max_age_secs: {
						description: """
																Event must have been emitted within the last `max_age_secs` seconds to be processed.

																If the event is older, it is forwarded to the `queue_url` for later processing.
																"""
						required: true
						type: uint: {
							examples: [3600]
							unit: "seconds"
						}
					}
					queue_url: {
						description: "The URL of the queue to forward events to when they are older than `max_age_secs`."
						required:    true
						type: string: examples: ["https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"]
					}
				}
			}
			delete_failed_message: {
				description: """
					Whether to delete non-retryable messages.

					If a message is rejected by the sink and not retryable, it is deleted from the queue.
					"""
				required: false
				type: bool: default: true
			}
			delete_message: {
				description: """
					Whether to delete the message once it is processed.

					It can be useful to set this to `false` for debugging or during the initial setup.
					"""
				required: false
				type: bool: default: true
			}
			max_number_of_messages: {
				description: """
					Maximum number of messages to poll from SQS in a batch

					Defaults to 10

					Should be set to a smaller value when the files are large to help prevent the ingestion of
					one file from causing the other files to exceed the visibility_timeout. Valid values are 1 - 10
					"""
				required: false
				type: uint: {
					default: 10
					examples: [1]
				}
			}
			operation_timeout_seconds: {
				description: """
					The operation timeout for AWS requests

					Limits the amount of time allowed for an operation to be fully serviced; an
					operation represents the full request/response lifecycle of a call to a service.
					Take care when configuring this settings to allow enough time for the polling
					interval configured in `poll_secs`
					"""
				required: false
				type: uint: {
					examples: [20]
					unit: "seconds"
				}
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
				description: "The URL of the SQS queue to poll for bucket notifications."
				required:    true
				type: string: examples: ["https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"]
			}
			read_timeout_seconds: {
				description: """
					The read timeout for AWS requests

					Limits the amount of time allowed to read the first byte of a response from the
					time the request is initiated. Take care when configuring this settings to allow
					enough time for the polling interval configured in `poll_secs`
					"""
				required: false
				type: uint: {
					examples: [20]
					unit: "seconds"
				}
			}
			tls_options: {
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
	}
	tls_options: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
