package metadata

base: components: sinks: gcp_stackdriver_logs: configuration: {
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
				end-to-end acknowledgements as well, waits for events to be acknowledged by the sink
				before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
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
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized/compressed.
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
					default: 1.0
					unit:    "seconds"
				}
			}
		}
	}
	billing_account_id: {
		description: """
			The billing account ID to which to publish logs.

			Exactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set.
			"""
		required: true
		type: string: {}
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
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type: object: options: {
			except_fields: {
				description: "List of fields that are excluded from the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			only_fields: {
				description: "List of fields that are included in the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			timestamp_format: {
				description: "Format used for timestamp fields."
				required:    false
				type: string: enum: {
					rfc3339:    "Represent the timestamp as a RFC 3339 timestamp."
					unix:       "Represent the timestamp as a Unix timestamp."
					unix_float: "Represent the timestamp as a Unix timestamp in floating point."
					unix_ms:    "Represent the timestamp as a Unix timestamp in milliseconds."
					unix_ns:    "Represent the timestamp as a Unix timestamp in nanoseconds."
					unix_us:    "Represent the timestamp as a Unix timestamp in microseconds"
				}
			}
		}
	}
	folder_id: {
		description: """
			The folder ID to which to publish logs.

			See the [Google Cloud Platform folder documentation][folder_docs] for more details.

			Exactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set.

			[folder_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-folders
			"""
		required: true
		type: string: {}
	}
	log_id: {
		description: """
			The log ID to which to publish logs.

			This is a name you create to identify this log stream.
			"""
		required: true
		type: string: syntax: "template"
	}
	organization_id: {
		description: """
			The organization ID to which to publish logs.

			This would be the identifier assigned to your organization on Google Cloud Platform.

			Exactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set.
			"""
		required: true
		type: string: {}
	}
	project_id: {
		description: """
			The project ID to which to publish logs.

			See the [Google Cloud Platform project management documentation][project_docs] for more details.

			Exactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set.

			[project_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-projects
			"""
		required: true
		type: string: {}
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, retry behavior, etc.

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
					initial_concurrency: {
						description: """
																The initial concurrency limit to use. If not specified, the initial limit will be 1 (no concurrency).

																It is recommended to set this value to your service's average limit if you're seeing that it takes a
																long time to ramp up adaptive concurrency after a restart. You can find this value by looking at the
																`adaptive_concurrency_limit` metric.
																"""
						required: false
						type: uint: default: 1
					}
					max_concurrency_limit: {
						description: """
																The maximum concurrency limit.

																The adaptive request concurrency limit will not go above this bound. This is put in place as a safeguard.
																"""
						required: false
						type: uint: default: 200
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

					After the first retry has failed, the fibonacci sequence is used to select future backoffs.
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
	resource: {
		description: """
			A monitored resource.

			The monitored resource to associate the logs with.
			"""
		required: true
		type: object: {
			examples: [{
				instanceId: "Twilight"
				zone:       "{{ zone }}"
			}]
			options: {
				"*": {
					description: "A type-specific label."
					required:    true
					type: string: syntax: "template"
				}
				type: {
					description: """
						The monitored resource type.

						For example, the type of a Compute Engine VM instance is `gce_instance`.
						See the [Google Cloud Platform monitored resource documentation][gcp_resources] for
						more details.

						[gcp_resources]: https://cloud.google.com/monitoring/api/resources
						"""
					required: true
					type: string: {}
				}
			}
		}
	}
	severity_key: {
		description: """
			The field of the log event from which to take the outgoing log’s `severity` field.

			The named field is removed from the log event if present, and must be either an integer
			between 0 and 800 or a string containing one of the [severity level names][sev_names] (case
			is ignored) or a common prefix such as `err`.

			If no severity key is specified, the severity of outgoing records is set to 0 (`DEFAULT`).

			See the [GCP Stackdriver Logging LogSeverity description][logsev_docs] for more details on
			the value of the `severity` field.

			[sev_names]: https://cloud.google.com/logging/docs/reference/v2/rest/v2/LogEntry#logseverity
			[logsev_docs]: https://cloud.google.com/logging/docs/reference/v2/rest/v2/LogEntry#logseverity
			"""
		required: false
		type: string: examples: ["severity"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. They are prioritized in the order
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

					If enabled, certificates must not be expired and must be issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
					certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
					so on until the verification process reaches a root certificate.

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
