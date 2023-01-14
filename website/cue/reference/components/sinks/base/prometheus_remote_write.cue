package metadata

base: components: sinks: prometheus_remote_write: configuration: {
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
	auth: {
		description: "Authentication strategies."
		required:    false
		type: object: options: {
			access_key_id: {
				description:   "The AWS access key ID."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: {}
			}
			assume_role: {
				description:   "The ARN of the role to assume."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: {}
			}
			credentials_file: {
				description:   "Path to the credentials file."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: {}
			}
			imds: {
				description:   "Configuration for authenticating with AWS through IMDS."
				relevant_when: "strategy = \"aws\""
				required:      false
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
				description:   "Timeout for successfully loading any credentials, in seconds."
				relevant_when: "strategy = \"aws\""
				required:      false
				type: uint: {}
			}
			password: {
				description:   "Basic authentication password."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: {}
			}
			profile: {
				description:   "The credentials profile to use."
				relevant_when: "strategy = \"aws\""
				required:      false
				type: string: {}
			}
			region: {
				description: """
					The AWS region to send STS requests to.

					If not set, this will default to the configured region
					for the service itself.
					"""
				relevant_when: "strategy = \"aws\""
				required:      false
				type: string: {}
			}
			secret_access_key: {
				description:   "The AWS secret access key."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: {}
			}
			strategy: {
				description: "The authentication strategy to use."
				required:    true
				type: string: enum: {
					aws:   "Amazon Prometheus Service-specific authentication."
					basic: "HTTP Basic Authentication."
					bearer: """
						Bearer authentication.

						A bearer token (OAuth2, JWT, etc) is passed as-is.
						"""
				}
			}
			token: {
				description:   "The bearer token to send."
				relevant_when: "strategy = \"bearer\""
				required:      true
				type: string: {}
			}
			user: {
				description:   "Basic authentication username."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: {}
			}
		}
	}
	aws: {
		description: "Configuration of the region/endpoint to use when interacting with an AWS service."
		required:    false
		type: object: options: {
			endpoint: {
				description: "The API endpoint of the service."
				required:    false
				type: string: {}
			}
			region: {
				description: "The AWS region to use."
				required:    false
				type: string: {}
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
	buckets: {
		description: """
			Default buckets to use for aggregating [distribution][dist_metric_docs] metrics into histograms.

			[dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
			"""
		required: false
		type: array: {
			default: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
			items: type: number: {}
		}
	}
	default_namespace: {
		description: """
			The default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with an underscore (`_`).

			It should follow the Prometheus [naming conventions][prom_naming_docs].

			[prom_naming_docs]: https://prometheus.io/docs/practices/naming/#metric-names
			"""
		required: false
		type: string: {}
	}
	endpoint: {
		description: "The endpoint to send data to."
		required:    true
		type: string: {}
	}
	quantiles: {
		description: """
			Quantiles to use for aggregating [distribution][dist_metric_docs] metrics into a summary.

			[dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
			"""
		required: false
		type: array: {
			default: [0.5, 0.75, 0.9, 0.95, 0.99]
			items: type: number: {}
		}
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
	tenant_id: {
		description: """
			The tenant ID to send.

			If set, a header named `X-Scope-OrgID` will be added to outgoing requests with the value of this setting.

			This may be used by Cortex or other remote services to identify the tenant making the request.
			"""
		required: false
		type: string: syntax: "template"
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
