package metadata

generated: components: sinks: victoriametrics: configuration: {
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
		description: """
			HTTP authentication.

			An event secret named `victoriametrics_token` overrides this setting for that event: the
			secret is sent as a bearer token. Use `set_secret` in VRL to choose a `vmauth` credential
			per event.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::http::Auth>"]
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
					default: 8388608
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
	compression_level: {
		description: """
			The zstd compression level.

			Higher levels reduce network traffic at the cost of CPU usage. Negative levels reduce CPU
			usage at the cost of network traffic, like `-remoteWrite.vmProtoCompressLevel` in `vmagent`.
			If unset, the zstd default level (3) is used.
			"""
		required: false
		type: int: examples: [3, -3]
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
	default_namespace: {
		description: """
			The default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with an underscore (`_`).
			"""
		required: false
		type: string: examples: ["service"]
	}
	endpoint: {
		description: """
			The base URL of VictoriaMetrics.

			This is the URL of single-node VictoriaMetrics, `vminsert`, or `vmauth`. The sink appends
			the write path, which depends on `tenant.mode`.
			"""
		required: true
		type: string: examples: ["http://localhost:8428", "http://vminsert:8480", "http://vmauth:8427"]
	}
	expire_metrics_secs: {
		description: """
			The amount of time, in seconds, that incremental metrics persist in the internal metrics
			cache after having not been updated before they expire and are removed.

			If unset, sending unique incremental metrics to this sink causes indefinite memory growth.
			"""
		required: false
		type: float: examples: [
			300.0
		]
	}
	request: {
		description: "Outbound HTTP request settings."
		required:    false
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
			headers: {
				description: """
					Additional HTTP headers to add to every HTTP request.

					Values are applied verbatim; template expansion is not supported.
					"""
				required: false
				type: object: {
					examples: [{
						"X-My-Custom-Header": "A-Value"
					}]
					options: "*": {
						description: "An HTTP request header and its static value."
						required:    true
						type: string: {}
					}
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
			The retry strategy for failed requests.

			For this sink, `default` matches `vmagent`: requests rejected with 400, 409, or 415 are
			dropped, and every other failed request is retried, including 401 and 403, which `vmauth`
			returns while credentials are rotated.
			"""
		required: false
		type:     _schemaDefinitions["derived::vector::sinks::util::http::RetryStrategy::64fe77681a1c274eed24cca6"]
	}
	send_metadata: {
		description: """
			Whether to send metric type metadata.

			VictoriaMetrics stores metadata when `-enableMetadata` is set, which is the default. As in
			`vmagent`, metadata is sent by default.
			"""
		required: false
		type: bool: default: true
	}
	tenant: {
		description: "How the VictoriaMetrics tenant is chosen."
		required:    false
		type: object: options: {
			id: {
				description: """
					The tenant, as `accountID` or `accountID:projectID`.

					Required when `mode` is `path` or `labels`. Events whose rendered tenant is not a valid
					VictoriaMetrics tenant are rejected.
					"""
				required: false
				type: string: {
					examples: ["42", "42:{{ tags.project_id }}"]
					syntax: "template"
				}
			}
			mode: {
				description: "Where the tenant is sent."
				required:    false
				type: string: {
					default: "none"
					enum: {
						labels: """
															The tenant is sent in the `vm_account_id` and `vm_project_id` labels. Writes to
															`/insert/multitenant/prometheus/api/v1/write` on `vminsert`.
															"""
						none: """
															No tenant. Writes to `/api/v1/write`.

															Use this for single-node VictoriaMetrics, and for `vmauth`, which chooses the tenant from
															the credential.
															"""
						path: """
															The tenant is part of the URL path. Writes to
															`/insert/<tenant>/prometheus/api/v1/write` on `vminsert`.
															"""
					}
				}
			}
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
