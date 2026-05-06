package metadata

generated: components: sinks: aws_s3: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Controls whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source that supports end-to-end
				acknowledgements that is connected to that sink waits for events
				to be acknowledged by **all connected sinks** before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
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
		type: object: options: {
			access_key_id: {
				description: "The AWS access key ID."
				required:    true
				type: string: examples: ["AKIAIOSFODNN7EXAMPLE"]
			}
			assume_role: {
				description: """
					The ARN of an [IAM role][iam_role] to assume.

					[iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
					"""
				required: true
				type: string: examples: ["arn:aws:iam::123456789098:role/my_role"]
			}
			credentials_file: {
				description: "Path to the credentials file."
				required:    true
				type: string: examples: ["/my/aws/credentials"]
			}
			external_id: {
				description: """
					The optional unique external ID in conjunction with role to assume.

					[external_id]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_create_for-user_externalid.html
					"""
				required: false
				type: string: examples: ["randomEXAMPLEidString"]
			}
			imds: {
				description: "Configuration for authenticating with AWS through IMDS."
				required:    false
				type: object: options: {
					connect_timeout_seconds: {
						description: "Connect timeout for IMDS."
						required:    false
						type: uint: {
							default: 1
							unit:    "seconds"
						}
					}
					max_attempts: {
						description: "Number of IMDS retries for fetching tokens and metadata."
						required:    false
						type: uint: default: 4
					}
					read_timeout_seconds: {
						description: "Read timeout for IMDS."
						required:    false
						type: uint: {
							default: 1
							unit:    "seconds"
						}
					}
				}
			}
			load_timeout_secs: {
				description: """
					Timeout for successfully loading any credentials, in seconds.

					Relevant when the default credentials chain or `assume_role` is used.
					"""
				required: false
				type: uint: {
					examples: [30]
					unit: "seconds"
				}
			}
			profile: {
				description: """
					The credentials profile to use.

					Used to select AWS credentials from a provided credentials file.
					"""
				required: false
				type: string: {
					default: "default"
					examples: ["develop"]
				}
			}
			region: {
				description: """
					The [AWS region][aws_region] to send STS requests to.

					If not set, this defaults to the configured region
					for the service itself.

					[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
					"""
				required: false
				type: string: examples: ["us-west-2"]
			}
			secret_access_key: {
				description: "The AWS secret access key."
				required:    true
				type: string: examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
			}
			session_name: {
				description: """
					The optional [RoleSessionName][role_session_name] is a unique session identifier for your assumed role.

					Should be unique per principal or reason.
					If not set, the session name is autogenerated like assume-role-provider-1736428351340

					[role_session_name]: https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html
					"""
				required: false
				type: string: examples: ["vector-indexer-role"]
			}
			session_token: {
				description: """
					The AWS session token.
					See [AWS temporary credentials](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_temp_use-resources.html)
					"""
				required: false
				type: string: examples: ["AQoDYXdz...AQoDYXdz..."]
			}
		}
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
					default: 10000000
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: unit: "events"
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 300.0
					unit:    "seconds"
				}
			}
		}
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
			"gzip",
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
			"json",
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
		type: object: options: {
			adaptive_concurrency: {
				description: """
					Configuration of adaptive concurrency parameters.

					These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
					unstable performance and sink behavior. Proceed with caution.
					"""
				required: false
				type: object: options: {
					decrease_ratio: {
						description: """
																The fraction of the current value to set the new concurrency limit when decreasing the limit.

																Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
																when latency increases.

																**Note**: The new limit is rounded down after applying this ratio.
																"""
						required: false
						type: float: default: 0.9
					}
					ewma_alpha: {
						description: """
																The weighting of new measurements compared to older measurements.

																Valid values are greater than `0` and less than `1`.

																ARC uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with
																the current RTT. Smaller values cause this reference to adjust more slowly, which may be useful if a service has
																unusually high response variability.
																"""
						required: false
						type: float: default: 0.4
					}
					initial_concurrency: {
						description: """
																The initial concurrency limit to use. If not specified, the initial limit is 1 (no concurrency).

																Datadog recommends setting this value to your service's average limit if you're seeing that it takes a
																long time to ramp up adaptive concurrency after a restart. You can find this value by looking at the
																`adaptive_concurrency_limit` metric.
																"""
						required: false
						type: uint: default: 1
					}
					max_concurrency_limit: {
						description: """
																The maximum concurrency limit.

																The adaptive request concurrency limit does not go above this bound. This is put in place as a safeguard.
																"""
						required: false
						type: uint: default: 200
					}
					rtt_deviation_scale: {
						description: """
																Scale of RTT deviations which are not considered anomalous.

																Valid values are greater than or equal to `0`, and reasonable values range from `1.0` to `3.0`.

																When calculating the past RTT average, a secondary “deviation” value is also computed that indicates how variable
																those values are. That deviation is used when comparing the past RTT average to the current measurements, so we
																can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
																an appropriate range. Larger values cause the algorithm to ignore larger increases in the RTT.
																"""
						required: false
						type: float: default: 2.5
					}
				}
			}
			concurrency: {
				description: """
					Configuration for outbound request concurrency.

					This can be set either to one of the below enum values or to a positive integer, which denotes
					a fixed concurrency limit.
					"""
				required: false
				type: {
					string: {
						default: "adaptive"
						enum: {
							adaptive: """
															Concurrency is managed by Vector's [Adaptive Request Concurrency][arc] feature.

															[arc]: https://vector.dev/docs/architecture/arc/
															"""
							none: """
															A fixed concurrency of 1.

															Only one request can be outstanding at any given time.
															"""
						}
					}
					uint: {}
				}
			}
			rate_limit_duration_secs: {
				description: "The time window used for the `rate_limit_num` option."
				required:    false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			rate_limit_num: {
				description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
				required:    false
				type: uint: {
					default: 9223372036854775807
					unit:    "requests"
				}
			}
			retry_attempts: {
				description: "The maximum number of retries to make for failed requests."
				required:    false
				type: uint: {
					default: 9223372036854775807
					unit:    "retries"
				}
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the Fibonacci sequence is used to select future backoffs.
					"""
				required: false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			retry_jitter_mode: {
				description: "The jitter mode to use for retry backoff behavior."
				required:    false
				type: string: {
					default: "Full"
					enum: {
						Full: """
															Full jitter.

															The random delay is anywhere from 0 up to the maximum current delay calculated by the backoff
															strategy.

															Incorporating full jitter into your backoff strategy can greatly reduce the likelihood
															of creating accidental denial of service (DoS) conditions against your own systems when
															many clients are recovering from a failure state.
															"""
						None: "No jitter."
					}
				}
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time to wait between retries."
				required:    false
				type: uint: {
					default: 30
					unit:    "seconds"
				}
			}
			timeout_secs: {
				description: """
					The time a request can take before being aborted.

					Datadog highly recommends that you do not lower this value below the service's internal timeout, as this could
					create orphaned requests, pile on retries, and result in duplicate data downstream.
					"""
				required: false
				type: uint: {
					default: 60
					unit:    "seconds"
				}
			}
		}
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
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
					that they are defined.
					"""
				required: false
				type: array: items: type: string: examples: ["h2"]
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/certificate_authority.crt"]
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this server.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.crt"]
			}
			key_file: {
				description: """
					Absolute path to a private key file used to identify this server.

					The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.key"]
			}
			key_pass: {
				description: """
					Passphrase used to unlock the encrypted key file.

					This has no effect unless `key_file` is set.
					"""
				required: false
				type: string: examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
			}
			server_name: {
				description: """
					Server name to use when using Server Name Indication (SNI).

					Only relevant for outgoing connections.
					"""
				required: false
				type: string: examples: ["www.example.com"]
			}
			verify_certificate: {
				description: """
					Enables certificate verification. For components that create a server, this requires that the
					client connections have a valid client certificate. For components that initiate requests,
					this validates that the upstream has a valid certificate.

					If enabled, certificates must not be expired and must be issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
					certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
					so on, until the verification process reaches a root certificate.

					Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
					"""
				required: false
				type: bool: {}
			}
			verify_hostname: {
				description: """
					Enables hostname verification.

					If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
					the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.

					Only relevant for outgoing connections.

					Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
					"""
				required: false
				type: bool: {}
			}
		}
	}
}

generated: components: sinks: aws_s3: configuration: encoding: encodingBase & {
	type: object: options: codec: required: true
}
generated: components: sinks: aws_s3: configuration: framing: framingEncoderBase & {
	type: object: options: method: required: true
}
