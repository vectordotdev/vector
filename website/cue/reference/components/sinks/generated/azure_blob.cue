package metadata

generated: components: sinks: azure_blob: configuration: {
	account_name: {
		description: """
			The Azure Blob Storage Account name.

			If provided, this will be used instead of the `connection_string`.
			This is useful for authenticating with an Azure credential.

			Exactly one of `connection_string`, `account_name`, or `blob_endpoint` must be set.
			"""
		required: false
		required_one_of: ["connection_string", "account_name", "blob_endpoint"]
		required_one_of_group: "azure_blob_credentials"
		type: string: examples: ["mylogstorage"]
	}
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
		description: "Azure service principal authentication."
		required:    false
		type:        _schemaDefinitions["vector::sinks::azure_common::config::AzureAuthentication"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::BulkSizeBasedDefaultBatchSettings>"]
	}
	blob_append_uuid: {
		description: """
			Whether or not to append a UUID v4 token to the end of the blob key.

			The UUID is appended to the timestamp portion of the object key, such that if the blob key
			generated is `date=2022-07-18/1658176486`, setting this field to `true` results
			in a blob key that looks like
			`date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.

			The default value depends on `blob_type`:
			- `block`: `true` — guarantees unique blob names across concurrent writers.
			- `append`: `false` — multiple batches must share the same blob name to append to it.
			  Set to `true` only if you intentionally want each flush to target a distinct append blob.
			"""
		required: false
		type: bool: {}
	}
	blob_endpoint: {
		description: """
			The Azure Blob Storage endpoint.

			If provided, this will be used instead of the `connection_string`.
			This is useful for authenticating with an Azure credential.

			Exactly one of `connection_string`, `account_name`, or `blob_endpoint` must be set.
			"""
		required: false
		required_one_of: ["connection_string", "account_name", "blob_endpoint"]
		required_one_of_group: "azure_blob_credentials"
		type: string: examples: ["https://mylogstorage.blob.core.windows.net/"]
	}
	blob_prefix: {
		description: """
			A prefix to apply to all blob keys.

			Prefixes are useful for partitioning objects, such as by creating a blob key that
			stores blobs under a particular directory. If using a prefix for this purpose, it must end
			in `/` to act as a directory path. A trailing `/` is **not** automatically added.
			"""
		required: false
		type: string: {
			default: "blob/%F/"
			examples: ["date/%F/hour/%H/", "year=%Y/month=%m/day=%d/", "kubernetes/{{ metadata.cluster }}/{{ metadata.application_name }}/"]
			syntax: "template"
		}
	}
	blob_time_format: {
		description: """
			The timestamp format for the time component of the blob key.

			Blob keys are appended with a timestamp that reflects when the blob is sent to
			Azure Blob Storage. The resulting blob key is functionally equivalent to joining
			the blob prefix with the formatted timestamp, such as `date=2022-07-18/1658176486`.

			This would represent a `blob_prefix` set to `date=%F/` and the timestamp of Mon Jul 18 2022
			20:34:44 GMT+0000, with the `blob_time_format` set to `%s`, which renders timestamps in
			seconds since the Unix epoch.

			Supports the common [`strftime`][chrono_strftime_specifiers] specifiers found in most
			languages.

			When set to an empty string, no timestamp is appended to the blob prefix.

			The default value depends on `blob_type`:
			- `block`: `%s` (Unix epoch seconds) — each batch gets a unique timestamp.
			- `append`: `%Y-%m-%dT%H` (ISO 8601 date and hour) — batches within the same hour share
			  the same blob.

			[chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
			"""
		required: false
		type: string: syntax: "strftime"
	}
	blob_type: {
		description: """
			The type of blob to use when writing to Azure Blob Storage.

			- `block` (default): each batch creates a new uniquely-named blob.
			  `blob_append_uuid` defaults to `true`; `blob_time_format` defaults to `%s`.
			- `append`: each batch appends to the same blob, keyed off `blob_prefix` and
			  `blob_time_format`. `blob_append_uuid` defaults to `false`; `blob_time_format`
			  defaults to `%Y-%m-%dT%H` (hourly rotation).

			Azure limits each `append_block` call to 4 MiB (4,194,304 bytes), so `batch.max_bytes`
			defaults to that limit in `append` mode and any explicit value above it is rejected at
			startup. `batch.max_bytes` measures the pre-encoding event size, while Azure enforces the
			limit on the encoded (and, if enabled, compressed) request body — with the default `gzip`
			compression the encoded body is smaller than the batched events, so 4 MiB leaves
			headroom. If you disable compression, encoding overhead (for example JSON escaping) can
			push a near-limit batch over the limit and Azure rejects the request; lower
			`batch.max_bytes` to leave headroom in that case.

			Azure caps an append blob at 50,000 blocks and each flush consumes one, so
			`blob_time_format` must rotate to a new blob before that cap is hit. The hourly default
			allows 50,000 flushes per hour, or about 56 MiB/s at the 4 MiB batch limit; daily rotation
			would cap the same partition near 2.3 MiB/s, after which Azure rejects appends with
			`BlockCountExceedsLimit` until the name rolls over.

			Appended blocks are persisted in the order Azure receives the requests, so `append` mode
			defaults `request.concurrency` to `1` to keep flushes to the same blob in order. As with
			all Vector sinks, delivery is at-least-once: a flush retried after Azure already committed
			the block is appended twice. Setting `request.retry_attempts` to `0` disables sink-level
			retries, but it does not give at-most-once delivery — upstream retries and resending
			sources can still produce duplicates.
			"""
		required: false
		type: string: {
			default: "block"
			enum: {
				append: """
					Stores data as append blobs.

					Each flush appends to a stable-named blob instead of creating a new one, which suits
					continuous log streaming: one growing file per time window.

					Batches land verbatim, one after the other, so `compression` must be concatenation-safe
					(`gzip`, `zstd`, or `none`; read such a blob with a multi-stream decompressor like `gunzip`)
					and `framing` must terminate every record — `codec = "json"` therefore defaults to
					newline-delimited JSON rather than the one array per blob that `block` emits. Settings that
					cannot be appended safely are rejected at startup.

					Changing `encoding` mixes formats inside a blob whose `Content-Type` is already set. Change
					`blob_prefix` or `blob_time_format`, or wait for the next time window, to start a new blob.
					"""
				block: """
					Stores data as block blobs.

					Each batch creates a new uniquely-named blob. Recommended for high-throughput
					scenarios where blobs are written once and read many times.
					"""
			}
		}
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
	connection_string: {
		description: """
			The Azure Blob Storage Account connection string.

			Authentication with an access key or shared access signature (SAS)
			are supported authentication methods. If using a non-account SAS,
			healthchecks will fail and will need to be disabled by setting
			`healthcheck.enabled` to `false` for this sink

			When generating an account SAS, the following are the minimum required option
			settings for Vector to access blob storage and pass a health check.
			| Option                 | Value              |
			| ---------------------- | ------------------ |
			| Allowed services       | Blob               |
			| Allowed resource types | Container & Object |
			| Allowed permissions    | Read & Create      |

			If you also configure the `tags` option, the SAS must include the
			`Tags` permission. Azure applies the *Set Blob Tags* authorization requirement to
			the `Put Blob` request that carries the `x-ms-tags` header, so without it tagged
			uploads fail with an authorization error even when the health check still passes.

			When `blob_type` is `append`, the SAS token additionally needs the `Add` (or `Write`)
			permission. `Read & Create` is sufficient to pass the health check and create the blob,
			but every `Append Block` call fails with `403 Forbidden` without `Add`/`Write`.

			Exactly one of `connection_string`, `account_name`, or `blob_endpoint` must be set.
			"""
		required: false
		required_one_of: ["connection_string", "account_name", "blob_endpoint"]
		required_one_of_group: "azure_blob_credentials"
		type: string: examples: ["DefaultEndpointsProtocol=https;AccountName=mylogstorage;AccountKey=MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=;EndpointSuffix=core.windows.net", "BlobEndpoint=https://mylogstorage.blob.core.windows.net/;SharedAccessSignature=generatedsastoken", "AccountName=mylogstorage"]
		warnings: ["Access keys and SAS tokens can be used to gain unauthorized access to Azure Blob Storage resources. Numerous security breaches have occurred due to leaked connection strings. It is important to keep connection strings secure and not expose them in logs, error messages, or version control systems."]
	}
	container_name: {
		description: "The Azure Blob Storage Account container name."
		required:    true
		type: string: examples: ["my-logs"]
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
	framing: {
		description: "Framing configuration."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
	}
	metadata: {
		description: """
			The set of [custom metadata][blob_metadata] `key:value` pairs to apply to created blobs.

			Each entry becomes an `x-ms-meta-{key}` header. Azure limits the total size of all
			metadata and restricts key names to ASCII alphanumeric characters and underscores,
			starting with a letter. Non-ASCII values must be Base64-encoded before being set.
			The service rejects invalid configurations. See the [Azure documentation][blob_metadata]
			for current limits.

			[blob_metadata]: https://learn.microsoft.com/rest/api/storageservices/set-blob-metadata
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
					default: 250
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
	tags: {
		description: """
			The set of [blob index tags][blob_index_tags] to apply to created blobs.

			Each entry becomes a tag in the `x-ms-tags` header. Azure limits blobs to 10 tags,
			with restricted character sets for keys and values; the service rejects invalid
			configurations.

			When authenticating with a shared access signature (SAS), the token must include the
			`Tags` permission in addition to `Read` and `Create`. Azure applies the *Set Blob Tags*
			authorization requirement to the `Put Blob` request that carries these tags, so without
			it tagged uploads fail with an authorization error even when the health check still passes.

			When authenticating with an Azure credential (managed identity, workload identity, and so
			on), the identity needs the
			`Microsoft.Storage/storageAccounts/blobServices/containers/blobs/tags/write` RBAC action.
			The least-privileged built-in role that grants it is *Storage Blob Data Owner*; the
			*Storage Blob Data Contributor* role commonly sufficient for uploads does not include it.

			[blob_index_tags]: https://learn.microsoft.com/azure/storage/blobs/storage-blob-index-how-to
			"""
		required: false
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
	tls: {
		description: "TLS configuration."
		required:    false
		type: object: options: ca_file: {
			description: """
				Absolute path to an additional CA certificate file.

				The certificate must be in PEM (X.509) format.
				"""
			required: false
			type: string: examples: ["/path/to/certificate_authority.crt"]
		}
	}
}
