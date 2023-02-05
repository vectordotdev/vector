package metadata

base: components: sinks: aws_s3: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source connected to that sink, where the source supports
				end-to-end acknowledgements as well, will wait for events to be acknowledged by the sink
				before acknowledging them at the source.

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
				"""
			private: """
				Bucket/object are private.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and no one else has
				access.

				This is the default.
				"""
			"public-read": """
				Bucket/object can be read publically.

				The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
				`AllUsers` grantee group is granted the `READ` permission.
				"""
			"public-read-write": """
				Bucket/object can be read and written publically.

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

					Relevant when the default credentials chain is used or `assume_role`.
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
				type: string: examples: ["develop"]
			}
			region: {
				description: """
					The [AWS region][aws_region] to send STS requests to.

					If not set, this will default to the configured region
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
		}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that will be processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized / compressed.
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
	bucket: {
		description: """
			The S3 bucket name.

			This must not include a leading `s3://` or a trailing `/`.
			"""
		required: true
		type: string: {}
	}
	compression: {
		description: """
			Compression configuration.

			All compression algorithms use the default compression level unless otherwise specified.
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
				zlib: """
					[Zlib][zlib] compression.

					[zlib]: https://zlib.net/
					"""
			}
		}
	}
	content_encoding: {
		description: """
			Specifies what content encoding has been applied to the object.

			Directly comparable to the `Content-Encoding` HTTP header.

			By default, the compression scheme used dictates this value.
			"""
		required: false
		type: string: {}
	}
	content_type: {
		description: """
			Specifies the MIME type of the object.

			Directly comparable to the `Content-Type` HTTP header.

			By default, `text/x-log` is used.
			"""
		required: false
		type: string: {}
	}
	encoding: {
		description: "Configures how events are encoded into raw bytes."
		required:    true
		type: object: options: {
			avro: {
				description:   "Apache Avro-specific encoder options."
				relevant_when: "codec = \"avro\""
				required:      true
				type: object: options: schema: {
					description: "The Avro schema."
					required:    true
					type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
				}
			}
			codec: {
				description: "The codec to use for encoding events."
				required:    true
				type: string: enum: {
					avro: """
						Encodes an event as an [Apache Avro][apache_avro] message.

						[apache_avro]: https://avro.apache.org/
						"""
					gelf: """
						Encodes an event as a [GELF][gelf] message.

						[gelf]: https://docs.graylog.org/docs/gelf
						"""
					json: """
						Encodes an event as [JSON][json].

						[json]: https://www.json.org/
						"""
					logfmt: """
						Encodes an event as a [logfmt][logfmt] message.

						[logfmt]: https://brandur.org/logfmt
						"""
					native: """
						Encodes an event in Vector’s [native Protocol Buffers format][vector_native_protobuf].

						This codec is **[experimental][experimental]**.

						[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					native_json: """
						Encodes an event in Vector’s [native JSON format][vector_native_json].

						This codec is **[experimental][experimental]**.

						[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					raw_message: """
						No encoding.

						This "encoding" simply uses the `message` field of a log event.

						Users should take care if they're modifying their log events (such as by using a `remap`
						transform, etc) and removing the message field while doing additional parsing on it, as this
						could lead to the encoding emitting empty strings for the given event.
						"""
					text: """
						Plain text encoding.

						This "encoding" simply uses the `message` field of a log event. For metrics, it uses an
						encoding that resembles the Prometheus export format.

						Users should take care if they're modifying their log events (such as by using a `remap`
						transform, etc) and removing the message field while doing additional parsing on it, as this
						could lead to the encoding emitting empty strings for the given event.
						"""
				}
			}
			except_fields: {
				description: "List of fields that will be excluded from the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			metric_tag_values: {
				description: """
					Controls how metric tag values are encoded.

					When set to `single`, only the last non-bare value of tags will be displayed with the
					metric.  When set to `full`, all metric tags will be exposed as separate assignments.
					"""
				relevant_when: "codec = \"json\" or codec = \"text\""
				required:      false
				type: string: {
					default: "single"
					enum: {
						full: "All tags will be exposed as arrays of either string or null values."
						single: """
															Tag values will be exposed as single strings, the same as they were before this config
															option. Tags with multiple values will show the last assigned value, and null values will be
															ignored.
															"""
					}
				}
			}
			only_fields: {
				description: "List of fields that will be included in the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			timestamp_format: {
				description: "Format used for timestamp fields."
				required:    false
				type: string: enum: {
					rfc3339: "Represent the timestamp as a RFC 3339 timestamp."
					unix:    "Represent the timestamp as a Unix timestamp."
				}
			}
		}
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
			being generated was `date=2022-07-18/1658176486`, setting this field to `true` would result
			in an object key that looked like `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.

			This ensures there are no name collisions, and can be useful in high-volume workloads where
			object keys must be unique.
			"""
		required: false
		type: bool: {}
	}
	filename_extension: {
		description: "The filename extension to use in the object key."
		required:    false
		type: string: {}
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

			When set to an empty string, no timestamp will be appended to the key prefix.

			[chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
			"""
		required: false
		type: string: {}
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited encoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type: object: options: delimiter: {
					description: "The ASCII (7-bit) character that delimits byte sequences."
					required:    true
					type: uint: {}
				}
			}
			method: {
				description: "The framing method."
				required:    true
				type: string: enum: {
					bytes:               "Event data is not delimited at all."
					character_delimited: "Event data is delimited by a single ASCII (7-bit) character."
					length_delimited: """
						Event data is prefixed with its length in bytes.

						The prefix is a 32-bit unsigned integer, little endian.
						"""
					newline_delimited: "Event data is delimited by a newline (LF) character."
				}
			}
		}
	}
	grant_full_control: {
		description: """
			Grants `READ`, `READ_ACP`, and `WRITE_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to read the created objects and their metadata, as well as read and
			modify the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: {}
	}
	grant_read: {
		description: """
			Grants `READ` permissions on the created objects to the named [grantee].

			This allows the grantee to read the created objects and their metadata.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: {}
	}
	grant_read_acp: {
		description: """
			Grants `READ_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to read the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: {}
	}
	grant_write_acp: {
		description: """
			Grants `WRITE_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to modify the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: {}
	}
	key_prefix: {
		description: """
			A prefix to apply to all object keys.

			Prefixes are useful for partitioning objects, such as by creating an object key that
			stores objects under a particular "directory". If using a prefix for this purpose, it must end
			in `/` to act as a directory path. A trailing `/` is **not** automatically added.
			"""
		required: false
		type: string: syntax: "template"
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

			Various settings can be configured, such as concurrency and rate limits, timeouts, etc.
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

																Note that the new limit is rounded down after applying this ratio.
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
					rtt_deviation_scale: {
						description: """
																Scale of RTT deviations which are not considered anomalous.

																Valid values are greater than or equal to `0`, and we expect reasonable values to range from `1.0` to `3.0`.

																When calculating the past RTT average, we also compute a secondary “deviation” value that indicates how variable
																those values are. We use that deviation when comparing the past RTT average to the current measurements, so we
																can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
																an appropriate range.  Larger values cause the algorithm to ignore larger increases in the RTT.
																"""
						required: false
						type: float: default: 2.5
					}
				}
			}
			concurrency: {
				description: "Configuration for outbound request concurrency."
				required:    false
				type: {
					string: {
						default: "none"
						enum: {
							adaptive: """
															Concurrency will be managed by Vector's [Adaptive Request Concurrency][arc] feature.

															[arc]: https://vector.dev/docs/about/under-the-hood/networking/arc/
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
				description: """
					The maximum number of retries to make for failed requests.

					The default, for all intents and purposes, represents an infinite number of retries.
					"""
				required: false
				type: uint: {
					default: 9223372036854775807
					unit:    "retries"
				}
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the fibonacci sequence will be used to select future backoffs.
					"""
				required: false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time to wait between retries."
				required:    false
				type: uint: {
					default: 3600
					unit:    "seconds"
				}
			}
			timeout_secs: {
				description: """
					The time a request can take before being aborted.

					It is highly recommended that you do not lower this value below the service’s internal timeout, as this could
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
	server_side_encryption: {
		description: "The Server-side Encryption algorithm used when storing these objects."
		required:    false
		type: string: enum: {
			AES256: """
				Each object is encrypted with AES-256 using a unique key.

				This corresponds to the `SSE-S3` option.
				"""
			"aws:kms": """
				Each object is encrypted with AES-256 using keys managed by AWS KMS.

				Depending on whether or not a KMS key ID is specified, this will correspond either to the
				`SSE-KMS` option (keys generated/managed by KMS) or the `SSE-C` option (keys generated by
				the customer, managed by KMS).
				"""
		}
	}
	ssekms_key_id: {
		description: """
			Specifies the ID of the AWS Key Management Service (AWS KMS) symmetrical customer managed
			customer master key (CMK) that will used for the created objects.

			Only applies when `server_side_encryption` is configured to use KMS.

			If not specified, Amazon S3 uses the AWS managed CMK in AWS to protect the data.
			"""
		required: false
		type: string: syntax: "template"
	}
	storage_class: {
		description: """
			The storage class for the created objects.

			See the [S3 Storage Classes][s3_storage_classes] for more details.

			[s3_storage_classes]: https://docs.aws.amazon.com/AmazonS3/latest/dev/storage-class-intro.html
			"""
		required: false
		type: string: enum: {
			DEEP_ARCHIVE:        "Glacier Deep Archive."
			GLACIER:             "Glacier Flexible Retrieval."
			INTELLIGENT_TIERING: "Intelligent Tiering."
			ONEZONE_IA:          "Infrequently Accessed (single Availability zone)."
			REDUCED_REDUNDANCY:  "Reduced Redundancy."
			STANDARD: """
				Standard Redundancy.

				This is the default.
				"""
			STANDARD_IA: "Infrequently Accessed."
		}
	}
	tags: {
		description: "The tag-set for the object."
		required:    false
		type: object: options: "*": {
			description: "A single tag."
			required:    true
			type: string: {}
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
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

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
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
			verify_certificate: {
				description: """
					Enables certificate verification.

					If enabled, certificates must be valid in terms of not being expired, as well as being issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that not only the leaf certificate (the
					certificate presented by the client/server) is valid, but also that the issuer of that certificate is valid, and
					so on until reaching a root certificate.

					Relevant for both incoming and outgoing connections.

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
