package metadata

generated: components: sinks: azure_blob: configuration: {
	account_name: {
		description: """
			The Azure Blob Storage Account name.

			If provided, this will be used instead of the `connection_string`.
			This is useful for authenticating with an Azure credential.
			"""
		required: false
		type: string: examples: ["mylogstorage"]
	}
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
	auth: {
		description: "Azure service principal authentication."
		required:    false
		type: object: options: {
			azure_client_id: {
				description: """
					The [Azure Client ID][azure_client_id].

					[azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"client_certificate_credential\" or azure_credential_kind = \"client_secret_credential\""
				required:      true
				type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_CLIENT_ID:?err}"]
			}
			azure_client_secret: {
				description: """
					The [Azure Client Secret][azure_client_secret].

					[azure_client_secret]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"client_secret_credential\""
				required:      true
				type: string: examples: ["00-00~000000-0000000~0000000000000000000", "${AZURE_CLIENT_SECRET:?err}"]
			}
			azure_credential_kind: {
				description: "The kind of Azure credential to use."
				required:    true
				type: string: enum: {
					azure_cli:                         "Use Azure CLI credentials"
					client_certificate_credential:     "Use certificate credentials"
					client_secret_credential:          "Use client ID/secret credentials"
					managed_identity:                  "Use Managed Identity credentials"
					managed_identity_client_assertion: "Use Managed Identity with Client Assertion credentials"
					workload_identity:                 "Use Workload Identity credentials"
				}
			}
			azure_tenant_id: {
				description: """
					The [Azure Tenant ID][azure_tenant_id].

					[azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"client_certificate_credential\" or azure_credential_kind = \"client_secret_credential\""
				required:      true
				type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_TENANT_ID:?err}"]
			}
			certificate_password: {
				description:   "The password for the client certificate, if applicable."
				relevant_when: "azure_credential_kind = \"client_certificate_credential\""
				required:      false
				type: string: examples: ["${AZURE_CLIENT_CERTIFICATE_PASSWORD}"]
			}
			certificate_path: {
				description:   "PKCS12 certificate with RSA private key."
				relevant_when: "azure_credential_kind = \"client_certificate_credential\""
				required:      true
				type: string: examples: ["path/to/certificate.pfx", "${AZURE_CLIENT_CERTIFICATE_PATH:?err}"]
			}
			client_assertion_client_id: {
				description:   "The target Client ID to use."
				relevant_when: "azure_credential_kind = \"managed_identity_client_assertion\""
				required:      true
				type: string: examples: ["00000000-0000-0000-0000-000000000000"]
			}
			client_assertion_tenant_id: {
				description:   "The target Tenant ID to use."
				relevant_when: "azure_credential_kind = \"managed_identity_client_assertion\""
				required:      true
				type: string: examples: ["00000000-0000-0000-0000-000000000000"]
			}
			client_id: {
				description: """
					The [Azure Client ID][azure_client_id]. Defaults to the value of the environment variable `AZURE_CLIENT_ID`.

					[azure_client_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"workload_identity\""
				required:      false
				type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_CLIENT_ID}"]
			}
			tenant_id: {
				description: """
					The [Azure Tenant ID][azure_tenant_id]. Defaults to the value of the environment variable `AZURE_TENANT_ID`.

					[azure_tenant_id]: https://learn.microsoft.com/entra/identity-platform/howto-create-service-principal-portal
					"""
				relevant_when: "azure_credential_kind = \"workload_identity\""
				required:      false
				type: string: examples: ["00000000-0000-0000-0000-000000000000", "${AZURE_TENANT_ID}"]
			}
			token_file_path: {
				description:   "Path of a file containing a Kubernetes service account token. Defaults to the value of the environment variable `AZURE_FEDERATED_TOKEN_FILE`."
				relevant_when: "azure_credential_kind = \"workload_identity\""
				required:      false
				type: string: examples: ["/var/run/secrets/azure/tokens/azure-identity-token", "${AZURE_FEDERATED_TOKEN_FILE}"]
			}
			user_assigned_managed_identity_id: {
				description:   "The User Assigned Managed Identity to use."
				relevant_when: "azure_credential_kind = \"managed_identity\" or azure_credential_kind = \"managed_identity_client_assertion\""
				required:      false
				type: string: examples: ["00000000-0000-0000-0000-000000000000"]
			}
			user_assigned_managed_identity_id_type: {
				description: """
					The type of the User Assigned Managed Identity ID provided (Client ID, Object ID,
					or Resource ID). Defaults to Client ID.
					"""
				relevant_when: "azure_credential_kind = \"managed_identity\" or azure_credential_kind = \"managed_identity_client_assertion\""
				required:      false
				type: string: enum: {
					client_id:   "Client ID"
					object_id:   "Object ID"
					resource_id: "Resource ID"
				}
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
	blob_append_uuid: {
		description: """
			Whether or not to append a UUID v4 token to the end of the blob key.

			The UUID is appended to the timestamp portion of the object key, such that if the blob key
			generated is `date=2022-07-18/1658176486`, setting this field to `true` results
			in an blob key that looks like
			`date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.

			This ensures there are no name collisions, and can be useful in high-volume workloads where
			blob keys must be unique.
			"""
		required: false
		type: bool: {}
	}
	blob_endpoint: {
		description: """
			The Azure Blob Storage endpoint.

			If provided, this will be used instead of the `connection_string`.
			This is useful for authenticating with an Azure credential.
			"""
		required: false
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

			By default, blob keys are appended with a timestamp that reflects when the blob are sent to
			Azure Blob Storage, such that the resulting blob key is functionally equivalent to joining
			the blob prefix with the formatted timestamp, such as `date=2022-07-18/1658176486`.

			This would represent a `blob_prefix` set to `date=%F/` and the timestamp of Mon Jul 18 2022
			20:34:44 GMT+0000, with the `filename_time_format` being set to `%s`, which renders
			timestamps in seconds since the Unix epoch.

			Supports the common [`strftime`][chrono_strftime_specifiers] specifiers found in most
			languages.

			When set to an empty string, no timestamp is appended to the blob prefix.

			[chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
			"""
		required: false
		type: string: syntax: "strftime"
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
			"""
		required: false
		type: string: examples: ["DefaultEndpointsProtocol=https;AccountName=mylogstorage;AccountKey=storageaccountkeybase64encoded;EndpointSuffix=core.windows.net", "BlobEndpoint=https://mylogstorage.blob.core.windows.net/;SharedAccessSignature=generatedsastoken", "AccountName=mylogstorage"]
		warnings: ["Access keys and SAS tokens can be used to gain unauthorized access to Azure Blob Storage resources. Numerous security breaches have occurred due to leaked connection strings. It is important to keep connection strings secure and not expose them in logs, error messages, or version control systems."]
	}
	container_name: {
		description: "The Azure Blob Storage Account container name."
		required:    true
		type: string: examples: ["my-logs"]
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

generated: components: sinks: azure_blob: configuration: encoding: encodingBase & {
	type: object: options: codec: required: true
}
generated: components: sinks: azure_blob: configuration: framing: framingEncoderBase & {
	type: object: options: method: required: true
}
