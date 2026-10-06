package metadata

generated: components: sinks: aws_s3: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	acl: {
		description: """
			Canned ACL to apply to the created objects.

			For more information, see [Canned ACL][canned_acl].

			[canned_acl]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#canned-acl
			"""
		required: false
		type: string: enum: {
			"authenticated-read": """
				Bucket/object can be read by authenticated users.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
				`AuthenticatedUsers` grantee group is granted the `READ` permission.
				"""
			"aws-exec-read": """
				Bucket/object are private, and readable by EC2.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and the AWS EC2 service is
				granted the `READ` permission for the purpose of reading Amazon Machine Image (AMI) bundles
				from the given bucket.
				"""
			"bucket-owner-full-control": """
				Object is semi-private.

				Both the object owner and bucket owner are granted the `FULL_CONTROL` permission.

				Only relevant when specified for an object: this canned ACL is otherwise ignored when
				specified for a bucket.
				"""
			"bucket-owner-read": """
				Object is private, except to the bucket owner.

				The object owner is granted the `FULL_CONTROL` permission, and the bucket owner is granted the `READ` permission.

				Only relevant when specified for an object: this canned ACL is otherwise ignored when
				specified for a bucket.
				"""
			"log-delivery-write": """
				Bucket can have logs written.

				The `LogDelivery` grantee group is granted `WRITE` and `READ_ACP` permissions.

				Only relevant when specified for a bucket: this canned ACL is otherwise ignored when
				specified for an object.

				For more information about logs, see [Amazon S3 Server Access Logging][serverlogs].

				[serverlogs]: https://docs.aws.amazon.com/AmazonS3/latest/dev/ServerLogs.html
				"""
			private: """
				Bucket/object are private.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and no one else has
				access.

				This is the default.
				"""
			"public-read": """
				Bucket/object can be read publicly.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
				`AllUsers` grantee group is granted the `READ` permission.
				"""
			"public-read-write": """
				Bucket/object can be read and written publicly.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
				`AllUsers` grantee group is granted the `READ` and `WRITE` permissions.

				This is generally not recommended.
				"""
		}
	}
	auth: {
		description: "Configuration of the authentication strategy for interacting with AWS services."
		required:    false
		type:        _schemaDefinitions["vector::aws::auth::AwsAuthentication"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::BulkSizeBasedDefaultBatchSettings>"]
	}
	batch_encoding: {
		description: """
			Batch encoding configuration for columnar formats.

			When set, events are encoded together as a batch in a columnar format (Parquet)
			instead of the standard per-event framing-based encoding. The columnar format handles
			its own internal compression, so the top-level `compression` setting is bypassed.
			"""
		required: false
		type: object: options: {
			codec: {
				description: """
					Encodes events in [Apache Parquet][apache_parquet] columnar format.

					[apache_parquet]: https://parquet.apache.org/
					"""
				required: true
				type: string: enum: parquet: """
					Encodes events in [Apache Parquet][apache_parquet] columnar format.

					[apache_parquet]: https://parquet.apache.org/
					"""
			}
			compression: {
				description: "Compression codec applied per column page inside the Parquet file."
				required:    false
				type: object: options: {
					algorithm: {
						description: "Compression codec applied per column page inside the Parquet file."
						required:    false
						type: string: {
							default: "snappy"
							enum: {
								gzip:   "Gzip compression. Level must be between 1 and 9."
								lz4:    "LZ4 raw compression"
								none:   "No compression"
								snappy: "Snappy compression (no level)."
								zstd:   "Zstd compression. Level must be between 1 and 21."
							}
						}
					}
					level: {
						description:   "Compression level (1–21). This is the range Vector supports; higher values compress more but are slower."
						relevant_when: "algorithm = \"zstd\" or algorithm = \"gzip\""
						required:      true
						type: uint: {}
					}
				}
			}
			schema_file: {
				description: """
					Path to a native Parquet schema file (`.schema`).

					Required unless `schema_mode` is `auto_infer`. The file must contain a valid
					Parquet message type definition.
					"""
				required: false
				type: string: {}
			}
			schema_mode: {
				description: "Controls how events with fields not present in the schema are handled."
				required:    false
				type: string: {
					default: "relaxed"
					enum: {
						auto_infer: "Auto infer schema based on the batch. No schema file needed."
						relaxed:    "Missing fields become null. Extra fields are silently dropped."
						strict:     "Missing fields become null. Extra fields cause an error."
					}
				}
			}
		}
	}
	bucket: {
		description: """
			The S3 bucket name.

			This must not include a leading `s3://` or a trailing `/`.
			"""
		required: true
		type: string: examples: ["my-bucket"]
	}
	compression: {
		description: """
			Compression configuration.

			All compression algorithms use the default compression level unless otherwise specified.

			Some cloud storage API clients and browsers handle decompression transparently, so
			depending on how they are accessed, files may not always appear to be compressed.
			"""
		required: false
		type: string: {
			default: "gzip"
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
	content_encoding: {
		description: """
			Overrides what content encoding has been applied to the object.

			Directly comparable to the `Content-Encoding` HTTP header.

			If not specified, the compression scheme used dictates this value.
			"""
		required: false
		type: string: examples: [
			"gzip"
		]
	}
	content_type: {
		description: """
			Overrides the MIME type of the object.

			Directly comparable to the `Content-Type` HTTP header.

			If not specified, the compression scheme used dictates this value.
			When `compression` is set to `none`, the value `text/x-log` is used.
			"""
		required: false
		type: string: examples: ["application/gzip"]
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
	filename_append_uuid: {
		description: """
			Whether or not to append a UUID v4 token to the end of the object key.

			The UUID is appended to the timestamp portion of the object key, such that if the object key
			generated is `date=2022-07-18/1658176486`, setting this field to `true` results
			in an object key that looks like `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.

			This ensures there are no name collisions, and can be useful in high-volume workloads where
			object keys must be unique.
			"""
		required: false
		type: bool: default: true
	}
	filename_extension: {
		description: """
			The filename extension to use in the object key.

			This overrides setting the extension based on the configured `compression`.
			"""
		required: false
		type: string: examples: [
			"json"
		]
	}
	filename_time_format: {
		description: """
			The timestamp format for the time component of the object key.

			By default, object keys are appended with a timestamp that reflects when the objects are
			sent to S3, such that the resulting object key is functionally equivalent to joining the key
			prefix with the formatted timestamp, such as `date=2022-07-18/1658176486`.

			This would represent a `key_prefix` set to `date=%F/` and the timestamp of Mon Jul 18 2022
			20:34:44 GMT+0000, with the `filename_time_format` being set to `%s`, which renders
			timestamps in seconds since the Unix epoch.

			Supports the common [`strftime`][chrono_strftime_specifiers] specifiers found in most
			languages.

			When set to an empty string, no timestamp is appended to the key prefix.

			[chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
			"""
		required: false
		type: string: default: "%s"
	}
	force_path_style: {
		description: """
			Specifies which addressing style to use.

			This controls if the bucket name is in the hostname or part of the URL.
			"""
		required: false
		type: bool: default: true
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
	}
	grant_full_control: {
		description: """
			Grants `READ`, `READ_ACP`, and `WRITE_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to read the created objects and their metadata, as well as read and
			modify the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
	}
	grant_read: {
		description: """
			Grants `READ` permissions on the created objects to the named [grantee].

			This allows the grantee to read the created objects and their metadata.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
	}
	grant_read_acp: {
		description: """
			Grants `READ_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to read the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
	}
	grant_write_acp: {
		description: """
			Grants `WRITE_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to modify the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
	}
	key_prefix: {
		description: """
			A prefix to apply to all object keys.

			Prefixes are useful for partitioning objects, such as by creating an object key that
			stores objects under a particular directory. If using a prefix for this purpose, it must end
			in `/` to act as a directory path. A trailing `/` is **not** automatically added.
			"""
		required: false
		type: string: {
			default: "date=%F"
			examples: ["date=%F/hour=%H", "year=%Y/month=%m/day=%d", "application_id={{ application_id }}/date=%F"]
			syntax: "template"
		}
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
	retry_strategy: {
		description: """
			Specifies retry strategy for failed requests.

			By default, the sink only retries attempts it deems possible to retry.
			These settings extend the default behavior.
			"""
		required: false
		type: object: options: {
			status_codes: {
				description:   "Retry on these specific HTTP status codes"
				relevant_when: "type = \"custom\""
				required:      true
				type: array: items: type: uint: {}
			}
			type: {
				description: "The retry strategy enum."
				required:    false
				type: string: {
					default: "default"
					enum: {
						all:    "Retry on *all* errors"
						custom: "Custom retry strategy"
						default: """
															Default strategy. The following error types will be retried:
															- `TimeoutError`
															- `DispatchFailure`
															- `ResponseError` or `ServiceError` when:
															  - HTTP status is 5xx
															  - Status is 429 (Too Many Requests)
															  - `x-amz-retry-after` header is present
															  - HTTP status is 4xx and response body contains one of:
															    - `"RequestTimeout"`
															    - `"RequestExpired"`
															    - `"ThrottlingException"`
															- Fallback: Any unknown error variant
															"""
						none: "Don't retry any errors"
					}
				}
			}
		}
	}
	server_side_encryption: {
		description: """
			AWS S3 Server-Side Encryption algorithms.

			The Server-side Encryption algorithm used when storing these objects.
			"""
		required: false
		type: string: enum: {
			AES256: """
				Each object is encrypted with AES-256 using a unique key.

				This corresponds to the `SSE-S3` option.
				"""
			"aws:kms": """
				Each object is encrypted with AES-256 using keys managed by AWS KMS.

				Depending on whether or not a KMS key ID is specified, this corresponds either to the
				`SSE-KMS` option (keys generated/managed by KMS) or the `SSE-C` option (keys generated by
				the customer, managed by KMS).
				"""
		}
	}
	ssekms_key_id: {
		description: """
			Specifies the ID of the AWS Key Management Service (AWS KMS) symmetrical customer managed
			customer master key (CMK) that is used for the created objects.

			Only applies when `server_side_encryption` is configured to use KMS.

			If not specified, Amazon S3 uses the AWS managed CMK in AWS to protect the data.
			"""
		required: false
		type: string: {
			examples: ["abcd1234"]
			syntax: "template"
		}
	}
	storage_class: {
		description: """
			The storage class for the created objects.

			See the [S3 Storage Classes][s3_storage_classes] for more details.

			[s3_storage_classes]: https://docs.aws.amazon.com/AmazonS3/latest/dev/storage-class-intro.html
			"""
		required: false
		type: string: {
			default: "STANDARD"
			enum: {
				DEEP_ARCHIVE:        "Glacier Deep Archive."
				EXPRESS_ONEZONE:     "High Performance (single Availability zone)."
				GLACIER:             "Glacier Flexible Retrieval."
				GLACIER_IR:          "Glacier Instant Retrieval."
				INTELLIGENT_TIERING: "Intelligent Tiering."
				ONEZONE_IA:          "Infrequently Accessed (single Availability zone)."
				REDUCED_REDUNDANCY:  "Reduced Redundancy."
				STANDARD:            "Standard Redundancy."
				STANDARD_IA:         "Infrequently Accessed."
			}
		}
	}
	tags: {
		description: "The tag-set for the object."
		required:    false
		type: object: {
			examples: [{
				Classification: "confidential"
				PHI:            "True"
				Project:        "Blue"
			}]
			options: "*": {
				description: "A single tag."
				required:    true
				type: string: {}
			}
		}
	}
	timezone: {
		description: """
			Timezone to use for any date specifiers in template strings.

			This can refer to any valid timezone as defined in the [TZ database][tzdb], or "local" which refers to the system local timezone. It will default to the [globally configured timezone](https://vector.dev/docs/reference/configuration/global-options/#timezone).

			[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
