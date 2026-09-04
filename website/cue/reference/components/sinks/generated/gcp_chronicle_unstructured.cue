package metadata

generated: components: sinks: gcp_chronicle_unstructured: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
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
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized or compressed.
					"""
				required: false
				type: uint: {
					default: 1000000
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
					default: 15.0
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
			}
		}
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
	customer_id: {
		description: "The Unique identifier (UUID) corresponding to the Chronicle instance."
		required:    true
		type: string: examples: ["c8c65bfa-5f2c-42d4-9189-64bb7b939f2c"]
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
		description: """
			The endpoint to send data to.

			Exactly one of `endpoint` or `region` must be set.
			"""
		required: false
		required_one_of: ["endpoint", "region"]
		required_one_of_group: "region_or_endpoint"
		type: string: examples: ["http://127.0.0.1:8080", "http://example.com:12345"]
	}
	fallback_log_type: {
		description: "The default `log_type` to attach to events if the template in `log_type` cannot be resolved."
		required:    false
		type: string: examples: ["VECTOR_DEV"]
	}
	labels: {
		description: "A set of labels that are attached to each batch of events."
		required:    false
		type: object: {
			examples: [{
				source: "vector"
				tenant: "marketing"
			}]
			options: "*": {
				description: "A Chronicle label."
				required:    true
				type: string: {}
			}
		}
	}
	log_type: {
		description: """
			The type of log entries in a request.

			This must be one of the [supported log types][unstructured_log_types_doc], otherwise
			Chronicle rejects the entry with an error.

			[unstructured_log_types_doc]: https://cloud.google.com/chronicle/docs/ingestion/parser-list/supported-default-parsers
			"""
		required: true
		type: string: {
			examples: ["WINDOWS_DNS", "{{ log_type }}"]
			syntax: "template"
		}
	}
	namespace: {
		description: "User-configured environment namespace to identify the data domain the logs originated from."
		required:    false
		type: string: {
			examples: ["production", "production-{{ namespace }}"]
			syntax: "template"
		}
	}
	region: {
		description: """
			The GCP region to use.

			Exactly one of `endpoint` or `region` must be set.
			"""
		required: false
		required_one_of: ["endpoint", "region"]
		required_one_of_group: "region_or_endpoint"
		type: string: enum: {
			asia:      "APAC region (this is the same as the Singapore region endpoint retained for backwards compatibility)"
			canada:    "Canada Region"
			dammam:    "Dammam Region"
			doha:      "Doha Region"
			eu:        "European Multi region"
			frankfurt: "Frankfurt Region"
			london:    "London Region"
			mumbai:    "Mumbai Region"
			paris:     "Paris Region"
			singapore: "Singapore Region"
			sydney:    "Sydney Region"
			são_paulo: "SãoPaulo Region"
			tel_aviv:  "TelAviv Region"
			tokyo:     "Tokyo Region"
			turin:     "Turin Region"
			us:        "US Multi region"
			zurich:    "Zurich Region"
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
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
