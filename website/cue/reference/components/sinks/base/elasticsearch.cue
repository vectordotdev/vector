package metadata

base: components: sinks: elasticsearch: configuration: {
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
	api_version: {
		description: """
			The API version of Elasticsearch.

			Amazon OpenSearch Serverless requires this option to be set to `auto` (the default).
			"""
		required: false
		type: string: {
			default: "auto"
			enum: {
				auto: """
					Auto-detect the API version.

					If the [cluster state version endpoint][es_version] isn't reachable, a warning is logged to
					stdout, and the version is assumed to be V6 if the `suppress_type_name` option is set to
					`true`. Otherwise, the version is assumed to be V8. In the future, the sink instead
					returns an error during configuration parsing, since a wrongly assumed version could lead to
					incorrect API calls.

					[es_version]: https://www.elastic.co/guide/en/elasticsearch/reference/current/cluster-state.html#cluster-state-api-path-params
					"""
				v6: "Use the Elasticsearch 6.x API."
				v7: "Use the Elasticsearch 7.x API."
				v8: "Use the Elasticsearch 8.x API."
			}
		}
	}
	auth: {
		description: "Elasticsearch Authentication strategies."
		required:    false
		type: object: options: {
			access_key_id: {
				description:   "The AWS access key ID."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: examples: ["AKIAIOSFODNN7EXAMPLE"]
			}
			assume_role: {
				description: """
					The ARN of an [IAM role][iam_role] to assume.

					[iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
					"""
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: examples: ["arn:aws:iam::123456789098:role/my_role"]
			}
			credentials_file: {
				description:   "Path to the credentials file."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: examples: ["/my/aws/credentials"]
			}
			external_id: {
				description: """
					The optional unique external ID in conjunction with role to assume.

					[external_id]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_create_for-user_externalid.html
					"""
				relevant_when: "strategy = \"aws\""
				required:      false
				type: string: examples: ["randomEXAMPLEidString"]
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
				description: """
					Timeout for successfully loading any credentials, in seconds.

					Relevant when the default credentials chain or `assume_role` is used.
					"""
				relevant_when: "strategy = \"aws\""
				required:      false
				type: uint: {
					examples: [30]
					unit: "seconds"
				}
			}
			password: {
				description:   "Basic authentication password."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: examples: ["${ELASTICSEARCH_PASSWORD}", "password"]
			}
			profile: {
				description: """
					The credentials profile to use.

					Used to select AWS credentials from a provided credentials file.
					"""
				relevant_when: "strategy = \"aws\""
				required:      false
				type: string: {
					default: "default"
					examples: ["develop"]
				}
			}
			region: {
				description: """
					The [AWS region][aws_region] to send STS requests to.

					If not set, this defaults to the configured region
					for the service itself.

					[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
					"""
				relevant_when: "strategy = \"aws\""
				required:      false
				type: string: examples: ["us-west-2"]
			}
			secret_access_key: {
				description:   "The AWS secret access key."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
			}
			session_name: {
				description: """
					The optional [RoleSessionName][role_session_name] is a unique session identifier for your assumed role.

					Should be unique per principal or reason.
					If not set, session name will be autogenerated like assume-role-provider-1736428351340

					[role_session_name]: https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRole.html
					"""
				relevant_when: "strategy = \"aws\""
				required:      false
				type: string: examples: ["vector-indexer-role"]
			}
			strategy: {
				description: """
					The authentication strategy to use.

					Amazon OpenSearch Serverless requires this option to be set to `aws`.
					"""
				required: true
				type: string: enum: {
					aws:   "Amazon OpenSearch Service-specific authentication."
					basic: "HTTP Basic Authentication."
				}
			}
			user: {
				description:   "Basic authentication username."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: examples: ["${ELASTICSEARCH_USERNAME}", "username"]
			}
		}
	}
	aws: {
		description: "Configuration of the region/endpoint to use when interacting with an AWS service."
		required:    false
		type: object: options: {
			endpoint: {
				description: "Custom endpoint for use with AWS-compatible services."
				required:    false
				type: string: examples: ["http://127.0.0.0:5000/path/to/service"]
			}
			region: {
				description: """
					The [AWS region][aws_region] of the target service.

					[aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
					"""
				required: false
				type: string: examples: ["us-east-1"]
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
	bulk: {
		description: "Elasticsearch bulk mode configuration."
		required:    false
		type: object: options: {
			action: {
				description: """
					Action to use when making requests to the [Elasticsearch Bulk API][es_bulk].

					Only `index`, `create` and `update` actions are supported.

					[es_bulk]: https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html
					"""
				required: false
				type: string: {
					default: "index"
					examples: ["create", "{{ action }}"]
					syntax: "template"
				}
			}
			index: {
				description: "The name of the index to write events to."
				required:    false
				type: string: {
					default: "vector-%Y.%m.%d"
					examples: ["application-{{ application_id }}-%Y-%m-%d", "{{ index }}"]
					syntax: "template"
				}
			}
			template_fallback_index: {
				description: "The default index to write events to if the template in `bulk.index` cannot be resolved"
				required:    false
				type: string: examples: ["test-index"]
			}
			version: {
				description: "Version field value."
				required:    false
				type: string: {
					examples: ["{{ obj_version }}-%Y-%m-%d", "123"]
					syntax: "template"
				}
			}
			version_type: {
				description: """
					Version type.

					Possible values are `internal`, `external` or `external_gt` and `external_gte`.

					[es_index_versioning]: https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-index_.html#index-versioning
					"""
				required: false
				type: string: {
					default: "internal"
					enum: {
						external:     "The `external` or `external_gt` type."
						external_gte: "The `external_gte` type."
						internal:     "The `internal` type."
					}
					examples: ["internal", "external"]
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
	data_stream: {
		description: "Elasticsearch data stream mode configuration."
		required:    false
		type: object: options: {
			auto_routing: {
				description: """
					Automatically routes events by deriving the data stream name using specific event fields.

					The format of the data stream name is `<type>-<dataset>-<namespace>`, where each value comes
					from the `data_stream` configuration field of the same name.

					If enabled, the value of the `data_stream.type`, `data_stream.dataset`, and
					`data_stream.namespace` event fields are used if they are present. Otherwise, the values
					set in this configuration are used.
					"""
				required: false
				type: bool: default: true
			}
			dataset: {
				description: "The data stream dataset used to construct the data stream at index time."
				required:    false
				type: string: {
					default: "generic"
					examples: ["generic", "nginx", "{{ service }}"]
					syntax: "template"
				}
			}
			namespace: {
				description: "The data stream namespace used to construct the data stream at index time."
				required:    false
				type: string: {
					default: "default"
					examples: ["{{ environment }}"]
					syntax: "template"
				}
			}
			sync_fields: {
				description: """
					Automatically adds and syncs the `data_stream.*` event fields if they are missing from the event.

					This ensures that fields match the name of the data stream that is receiving events.
					"""
				required: false
				type: bool: default: true
			}
			type: {
				description: "The data stream type used to construct the data stream at index time."
				required:    false
				type: string: {
					default: "logs"
					examples: ["metrics", "synthetics", "{{ type }}"]
					syntax: "template"
				}
			}
		}
	}
	distribution: {
		description: "Options for determining the health of an endpoint."
		required:    false
		type: object: options: {
			retry_initial_backoff_secs: {
				description: "Initial delay between attempts to reactivate endpoints once they become unhealthy."
				required:    false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			retry_max_duration_secs: {
				description: "Maximum delay between attempts to reactivate endpoints once they become unhealthy."
				required:    false
				type: uint: {
					default: 3600
					unit:    "seconds"
				}
			}
		}
	}
	doc_type: {
		description: """
			The [`doc_type`][doc_type] for your index data.

			This is only relevant for Elasticsearch <= 6.X. If you are using >= 7.0 you do not need to
			set this option since Elasticsearch has removed it.

			[doc_type]: https://www.elastic.co/guide/en/elasticsearch/reference/6.8/actions-index.html
			"""
		required: false
		type: string: default: "_doc"
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
	endpoint: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, the `endpoints` option should be used instead."
		description: """
			The Elasticsearch endpoint to send logs to.

			The endpoint must contain an HTTP scheme, and may specify a
			hostname or IP address and port.
			"""
		required: false
		type: string: {}
	}
	endpoints: {
		description: """
			A list of Elasticsearch endpoints to send logs to.

			The endpoint must contain an HTTP scheme, and may specify a
			hostname or IP address and port.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["http://10.24.32.122:9000", "https://example.com", "https://user:password@example.com"]
		}
	}
	id_key: {
		description: """
			The name of the event key that should map to Elasticsearch’s [`_id` field][es_id].

			By default, the `_id` field is not set, which allows Elasticsearch to set this
			automatically. Setting your own Elasticsearch IDs can [hinder performance][perf_doc].

			[es_id]: https://www.elastic.co/guide/en/elasticsearch/reference/current/mapping-id-field.html
			[perf_doc]: https://www.elastic.co/guide/en/elasticsearch/reference/master/tune-for-indexing-speed.html#_use_auto_generated_ids
			"""
		required: false
		type: string: examples: ["id", "_id"]
	}
	metrics: {
		description: "Configuration for the `metric_to_log` transform."
		required:    false
		type: object: options: {
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
		}
	}
	mode: {
		description: "Elasticsearch Indexing mode."
		required:    false
		type: string: {
			default: "bulk"
			enum: {
				bulk: "Ingests documents in bulk, using the bulk API `index` action."
				data_stream: """
					Ingests documents in bulk, using the bulk API `create` action.

					Elasticsearch Data Streams only support the `create` action.

					If the mode is set to `data_stream` and a `timestamp` field is present in a message,
					Vector renames this field to the expected `@timestamp` to comply with the Elastic Common Schema.
					"""
			}
		}
	}
	opensearch_service_type: {
		description: "Amazon OpenSearch service type"
		required:    false
		type: string: {
			default: "managed"
			enum: {
				managed:    "Elasticsearch or OpenSearch Managed domain"
				serverless: "OpenSearch Serverless collection"
			}
		}
	}
	pipeline: {
		description: "The name of the pipeline to apply."
		required:    false
		type: string: examples: ["pipeline-name"]
	}
	query: {
		description: "Custom parameters to add to the query string for each HTTP request sent to Elasticsearch."
		required:    false
		type: object: {
			examples: [{
				"X-Powered-By": "Vector"
			}]
			options: "*": {
				description: "A query string parameter."
				required:    true
				type: string: {}
			}
		}
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
			headers: {
				description: "Additional HTTP headers to add to every HTTP request."
				required:    false
				type: object: {
					examples: [{
						Accept:               "text/plain"
						"X-My-Custom-Header": "A-Value"
					}]
					options: "*": {
						description: "An HTTP request header and it's value."
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
	request_retry_partial: {
		description: """
			Whether or not to retry successful requests containing partial failures.

			To avoid duplicates in Elasticsearch, please use option `id_key`.
			"""
		required: false
		type: bool: default: false
	}
	suppress_type_name: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, the `api_version` option should be used instead."
		description: """
			Whether or not to send the `type` field to Elasticsearch.

			The `type` field was deprecated in Elasticsearch 7.x and removed in Elasticsearch 8.x.

			If enabled, the `doc_type` option is ignored.
			"""
		required: false
		type: bool: default: false
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
}
