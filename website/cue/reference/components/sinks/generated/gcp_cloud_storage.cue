package metadata

generated: components: sinks: gcp_cloud_storage: configuration: {
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
			The Predefined ACL to apply to created objects.

			For more information, see [Predefined ACLs][predefined_acls].

			[predefined_acls]: https://cloud.google.com/storage/docs/access-control/lists#predefined-acl
			"""
		required: false
		type: string: enum: {
			"authenticated-read": """
				Bucket/object can be read by authenticated users.

				The bucket/object owner is granted the `OWNER` permission, and anyone authenticated Google
				account holder is granted the `READER` permission.
				"""
			"bucket-owner-full-control": """
				Object is semi-private.

				Both the object owner and bucket owner are granted the `OWNER` permission.

				Only relevant when specified for an object: this predefined ACL is otherwise ignored when
				specified for a bucket.
				"""
			"bucket-owner-read": """
				Object is private, except to the bucket owner.

				The object owner is granted the `OWNER` permission, and the bucket owner is granted the
				`READER` permission.

				Only relevant when specified for an object: this predefined ACL is otherwise ignored when
				specified for a bucket.
				"""
			private: """
				Bucket/object are private.

				The bucket/object owner is granted the `OWNER` permission, and no one else has
				access.
				"""
			"project-private": """
				Bucket/object are private within the project.

				Project owners and project editors are granted the `OWNER` permission, and anyone who is
				part of the project team is granted the `READER` permission.

				This is the default.
				"""
			"public-read": """
				Bucket/object can be read publicly.

				The bucket/object owner is granted the `OWNER` permission, and all other users, whether
				authenticated or anonymous, are granted the `READER` permission.
				"""
		}
	}
	api_key: {
		description: """
			An [API key][gcp_api_key].

			Either an API key or a path to a service account credentials JSON file can be specified.

			If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
			filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
			running on. If this is not on a GCE instance, then you must define it with an API key or service account
			credentials JSON file.

			[gcp_api_key]: https://cloud.google.com/docs/authentication/api-keys
			"""
		required: false
		type: string: {}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::BulkSizeBasedDefaultBatchSettings>"]
	}
	bucket: {
		description: "The GCS bucket name."
		required:    true
		type: string: examples: ["my-bucket"]
	}
	cache_control: {
		description: """
			Sets the `Cache-Control` header for the created objects.

			Directly comparable to the `Cache-Control` HTTP header.
			"""
		required: false
		type: string: examples: ["no-transform"]
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
	content_encoding: {
		description: """
			Overrides what content encoding has been applied to the object.

			Directly comparable to the `Content-Encoding` HTTP header.

			If not specified, the compression scheme used dictates this value.
			"""
		required: false
		type: string: examples: ["gzip", "zstd"]
	}
	content_type: {
		description: """
			Overrides the MIME type of the created objects.

			Directly comparable to the `Content-Type` HTTP header.

			If not specified, defaults to the encoder's content type.
			"""
		required: false
		type: string: examples: ["text/plain; charset=utf-8", "application/gzip"]
	}
	credentials_path: {
		description: """
			Path to a [service account][gcp_service_account_credentials] credentials JSON file.

			Either an API key or a path to a service account credentials JSON file can be specified.

			If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
			filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
			running on. If this is not on a GCE instance, then you must define it with an API key or service account
			credentials JSON file.

			[gcp_service_account_credentials]: https://cloud.google.com/docs/authentication/production#manually
			"""
		required: false
		type: string: {}
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
		description: "API endpoint for Google Cloud Storage"
		required:    false
		type: string: {
			default: "https://storage.googleapis.com/"
			examples: ["http://localhost:9000"]
		}
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

			If not specified, the extension is determined by the compression scheme used.
			"""
		required: false
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

			When set to an empty string, no timestamp is appended to the key prefix.

			[chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
			"""
		required: false
		type: string: default: "%s"
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
	}
	key_prefix: {
		description: """
			A prefix to apply to all object keys.

			Prefixes are useful for partitioning objects, such as by creating an object key that
			stores objects under a particular directory. If using a prefix for this purpose, it must end
			in `/` in order to act as a directory path. A trailing `/` is **not** automatically added.
			"""
		required: false
		type: string: {
			examples: ["date=%F/", "date=%F/hour=%H/", "year=%Y/month=%m/day=%d/", "application_id={{ application_id }}/date=%F/"]
			syntax: "template"
		}
	}
	metadata: {
		description: """
			The set of metadata `key:value` pairs for the created objects.

			For more information, see the [custom metadata][custom_metadata] documentation.

			[custom_metadata]: https://cloud.google.com/storage/docs/metadata#custom-metadata
			"""
		required: false
		type: object: options: "*": {
			description: "A key/value pair."
			required:    true
			type: string: {}
		}
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
				type:     _schemaDefinitions["vector::sinks::util::adaptive_concurrency::AdaptiveConcurrencySettings"]
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
					default: 1000
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
	storage_class: {
		description: """
			The storage class for created objects.

			For more information, see the [storage classes][storage_classes] documentation.

			[storage_classes]: https://cloud.google.com/storage/docs/storage-classes
			"""
		required: false
		type: string: enum: {
			ARCHIVE:  "Archive storage."
			COLDLINE: "Coldline storage."
			NEARLINE: "Nearline storage."
			STANDARD: """
				Standard storage.

				This is the default.
				"""
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
