package metadata

base: components: sinks: humio_metrics: configuration: {
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
					default: 1.0
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
	endpoint: {
		description: """
			The base URL of the Humio instance.

			The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
			by the [`Splunk`][splunk] API are used.

			[splunk]: https://docs.splunk.com/Documentation/Splunk/8.0.0/Data/HECRESTendpoints
			"""
		required: false
		type: string: {
			default: "https://cloud.humio.com"
			examples: ["http://127.0.0.1", "https://example.com"]
		}
	}
	event_type: {
		description: """
			The type of events sent to this sink. Humio uses this as the name of the parser to use to ingest the data.

			If unset, Humio defaults it to none.
			"""
		required: false
		type: string: {
			examples: ["json", "none", "{{ event_type }}"]
			syntax: "template"
		}
	}
	host_key: {
		description: """
			Overrides the name of the log field used to retrieve the hostname to send to Humio.

			By default, the [global `log_schema.host_key` option][global_host_key] is used if log
			events are Legacy namespaced, or the semantic meaning of "host" is used, if defined.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: default: "host"
	}
	host_tag: {
		description: """
			Name of the tag in the metric to use for the source host.

			If present, the value of the tag is set on the generated log event in the `host` field,
			where the field key uses the [global `host_key` option][global_log_schema_host_key].

			[global_log_schema_host_key]: https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key
			"""
		required: false
		type: string: examples: ["host", "hostname"]
	}
	index: {
		description: """
			Optional name of the repository to ingest into.

			In public-facing APIs, this must (if present) be equal to the repository used to create the ingest token used for authentication.

			In private cluster setups, Humio can be configured to allow these to be different.

			For more information, see [Humio’s Format of Data][humio_data_format].

			[humio_data_format]: https://docs.humio.com/integrations/data-shippers/hec/#format-of-data
			"""
		required: false
		type: string: {
			examples: ["{{ host }}", "custom_index"]
			syntax: "template"
		}
	}
	indexed_fields: {
		description: """
			Event fields to be added to Humio’s extra fields.

			Can be used to tag events by specifying fields starting with `#`.

			For more information, see [Humio’s Format of Data][humio_data_format].

			[humio_data_format]: https://docs.humio.com/integrations/data-shippers/hec/#format-of-data
			"""
		required: false
		type: array: {
			default: []
			items: type: string: {}
		}
	}
	metric_tag_values: {
		description: """
			Controls how metric tag values are encoded.

			When set to `single`, only the last non-bare value of tags is displayed with the
			metric.  When set to `full`, all metric tags are exposed as separate assignments as
			described by [the `native_json` codec][vector_native_json].

			[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
			"""
		required: false
		type: string: {
			default: "single"
			enum: {
				full: "All tags are exposed as arrays of either string or null values."
				single: """
					Tag values are exposed as single strings, the same as they were before this config
					option. Tags with multiple values show the last assigned value, and null values
					are ignored.
					"""
			}
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
															Concurrency is managed by Vector's [Adaptive Request Concurrency][arc] feature.

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
	source: {
		description: """
			The source of events sent to this sink.

			Typically the filename the metrics originated from. Maps to `@source` in Humio.
			"""
		required: false
		type: string: syntax: "template"
	}
	timezone: {
		description: """
			The name of the time zone to apply to timestamp conversions that do not contain an explicit
			time zone.

			This overrides the [global `timezone`][global_timezone] option. The time zone name may be
			any name in the [TZ database][tz_database] or `local` to indicate system local time.

			[global_timezone]: https://vector.dev/docs/reference/configuration//global-options#timezone
			[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
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
	token: {
		description: "The Humio ingestion token."
		required:    true
		type: string: examples: ["${HUMIO_TOKEN}", "A94A8FE5CCB19BA61C4C08"]
	}
}
