package metadata

base: components: sinks: aws_s3: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how Vector handles event acknowledgement.

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
				type: string: syntax: "literal"
			}
			assume_role: {
				description: "The ARN of the role to assume."
				required:    true
				type: string: syntax: "literal"
			}
			credentials_file: {
				description: "Path to the credentials file."
				required:    true
				type: string: syntax: "literal"
			}
			load_timeout_secs: {
				description: "Timeout for successfully loading any credentials, in seconds."
				required:    false
				type: uint: {}
			}
			profile: {
				description: "The credentials profile to use."
				required:    false
				type: string: syntax: "literal"
			}
			region: {
				description: """
					The AWS region to send STS requests to.

					If not set, this will default to the configured region
					for the service itself.
					"""
				required: false
				type: string: syntax: "literal"
			}
			secret_access_key: {
				description: "The AWS secret access key."
				required:    true
				type: string: syntax: "literal"
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
				type: uint: {}
			}
			max_events: {
				description: "The maximum size of a batch, in events, before it is flushed."
				required:    false
				type: uint: {}
			}
			timeout_secs: {
				description: "The maximum age of a batch, in seconds, before it is flushed."
				required:    false
				type: float: {}
			}
		}
	}
	bucket: {
		description: """
			The S3 bucket name.

			This must not include a leading `s3://` or a trailing `/`.
			"""
		required: true
		type: string: syntax: "literal"
	}
	compression: {
		description: "Compression configuration."
		required:    false
		type: {
			object: options: {
				algorithm: {
					required: false
					type: string: {
						const:   "zlib"
						default: "gzip"
					}
				}
				level: {
					description: "Compression level."
					required:    false
					type: {
						string: enum: ["none", "fast", "best", "default"]
						uint: enum: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
					}
				}
			}
			string: enum: ["none", "gzip", "zlib"]
		}
	}
	content_encoding: {
		description: """
			Specifies what content encoding has been applied to the object.

			Directly comparable to the `Content-Encoding` HTTP header.

			By default, the compression scheme used dictates this value.
			"""
		required: false
		type: string: syntax: "literal"
	}
	content_type: {
		description: """
			Specifies the MIME type of the object.

			Directly comparable to the `Content-Type` HTTP header.

			By default, `text/x-log` is used.
			"""
		required: false
		type: string: syntax: "literal"
	}
	encoding: {
		description: "Encoding configuration."
		required:    true
		type: object: options: {
			avro: {
				description:   "Apache Avro serializer options."
				relevant_when: "codec = \"avro\""
				required:      true
				type: object: options: schema: {
					description: "The Avro schema."
					required:    true
					type: string: syntax: "literal"
				}
			}
			codec: {
				required: true
				type: string: enum: {
					avro:        "Apache Avro serialization."
					gelf:        "GELF serialization."
					json:        "JSON serialization."
					logfmt:      "Logfmt serialization."
					native:      "Native Vector serialization based on Protocol Buffers."
					native_json: "Native Vector serialization based on JSON."
					raw_message: """
						No serialization.

						This encoding, specifically, will only encode the `message` field of a log event. Users should take care if
						they're modifying their log events (such as by using a `remap` transform, etc) and removing the message field
						while doing additional parsing on it, as this could lead to the encoding emitting empty strings for the given
						event.
						"""
					text: """
						Plaintext serialization.

						This encoding, specifically, will only encode the `message` field of a log event. Users should take care if
						they're modifying their log events (such as by using a `remap` transform, etc) and removing the message field
						while doing additional parsing on it, as this could lead to the encoding emitting empty strings for the given
						event.
						"""
				}
			}
			except_fields: {
				description: "List of fields that will be excluded from the encoded event."
				required:    false
				type: array: items: type: string: syntax: "literal"
			}
			only_fields: {
				description: "List of fields that will be included in the encoded event."
				required:    false
				type: array: items: type: string: syntax: "literal"
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
		description: "The API endpoint of the service."
		required:    false
		type: string: syntax: "literal"
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
		type: string: syntax: "literal"
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
		type: string: syntax: "literal"
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
				required: true
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
		type: string: syntax: "literal"
	}
	grant_read: {
		description: """
			Grants `READ` permissions on the created objects to the named [grantee].

			This allows the grantee to read the created objects and their metadata.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: syntax: "literal"
	}
	grant_read_acp: {
		description: """
			Grants `READ_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to read the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: syntax: "literal"
	}
	grant_write_acp: {
		description: """
			Grants `WRITE_ACP` permissions on the created objects to the named [grantee].

			This allows the grantee to modify the ACL on the created objects.

			[grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
			"""
		required: false
		type: string: syntax: "literal"
	}
	key_prefix: {
		description: """
			A prefix to apply to all object keys.

			Prefixes are useful for partitioning objects, such as by creating an object key that
			stores objects under a particular "directory". If using a prefix for this purpose, it must end
			in `/` in order to act as a directory path: Vector will **not** add a trailing `/` automatically.
			"""
		required: false
		type: string: syntax: "template"
	}
	region: {
		description: "The AWS region to use."
		required:    false
		type: string: syntax: "literal"
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
				type: object: {
					default: {
						decrease_ratio:      0.9
						ewma_alpha:          0.4
						rtt_deviation_scale: 2.5
					}
					options: {
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
			}
			concurrency: {
				description: "Configuration for outbound request concurrency."
				required:    false
				type: {
					number: {}
					string: {
						const:   "adaptive"
						default: "none"
					}
				}
			}
			rate_limit_duration_secs: {
				description: "The time window, in seconds, used for the `rate_limit_num` option."
				required:    false
				type: uint: default: 1
			}
			rate_limit_num: {
				description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
				required:    false
				type: uint: default: 9223372036854775807
			}
			retry_attempts: {
				description: """
					The maximum number of retries to make for failed requests.

					The default, for all intents and purposes, represents an infinite number of retries.
					"""
				required: false
				type: uint: default: 9223372036854775807
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the fibonacci sequence will be used to select future backoffs.
					"""
				required: false
				type: uint: default: 1
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time, in seconds, to wait between retries."
				required:    false
				type: uint: default: 3600
			}
			timeout_secs: {
				description: """
					The maximum time a request can take before being aborted.

					It is highly recommended that you do not lower this value below the service’s internal timeout, as this could
					create orphaned requests, pile on retries, and result in duplicate data downstream.
					"""
				required: false
				type: uint: default: 60
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
			description: "The tag-set for the object."
			required:    true
			type: string: syntax: "literal"
		}
	}
	tls: {
		description: "Standard TLS options."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
					"""
				required: false
				type: array: items: type: string: syntax: "literal"
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certficate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: syntax: "literal"
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this server.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: syntax: "literal"
			}
			key_file: {
				description: """
					Absolute path to a private key file used to identify this server.

					The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: syntax: "literal"
			}
			key_pass: {
				description: """
					Passphrase used to unlock the encrypted key file.

					This has no effect unless `key_file` is set.
					"""
				required: false
				type: string: syntax: "literal"
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
