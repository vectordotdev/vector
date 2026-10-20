package metadata

generated: components: sinks: opensearch: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["bool_or_struct::vector_core::config::AcknowledgementsConfig"]
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
					Automatically select the API version.

					The `opensearch` sink selects an OpenSearch-compatible API version without querying the server.
					The `elasticsearch` sink detects the version from the server.

					If the [cluster state version endpoint][es_version] isn't reachable, a warning is logged to
					stdout, and the version is assumed to be V6 if the `suppress_type_name` option is set to
					`true`. Otherwise, the version is assumed to be V8. In the future, the sink instead
					returns an error during configuration parsing, since a wrongly assumed version could lead to
					incorrect API calls.

					[es_version]: https://www.elastic.co/guide/en/elasticsearch/reference/current/cluster-state.html#cluster-state-api-path-params
					"""
				v6: "Use the Elasticsearch 6.x API."
				v7: "Use the Elasticsearch 7.x-compatible API, including OpenSearch."
				v8: "Use the Elasticsearch 8.x API."
			}
		}
	}
	auth: {
		description: "Elasticsearch Authentication strategies."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::sinks::elasticsearch::ElasticsearchAuthConfig>"]
	}
	aws: {
		description: "Configuration of the region/endpoint to use when interacting with an AWS service."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::aws::region::RegionOrEndpoint>"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
	}
	bulk: {
		description: "Elasticsearch bulk mode configuration."
		required:    false
		type:        _schemaDefinitions["vector::sinks::elasticsearch::config::BulkConfig"]
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
	data_stream: {
		description: "Elasticsearch data stream mode configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::sinks::elasticsearch::config::DataStreamConfig>"]
	}
	distribution: {
		description: "Options for determining the health of an endpoint."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::sinks::util::service::health::HealthConfig>"]
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
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	endpoint: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, the `endpoints` option should be used instead."
		description: """
			The Elasticsearch endpoint to send logs to.

			The endpoint must contain an HTTP scheme, and may specify a
			hostname or IP address and port.

			Exactly one of `endpoint` or `endpoints` must be set.
			"""
		required: false
		required_one_of: ["endpoint", "endpoints"]
		required_one_of_group: "endpoint"
		type: string: {}
	}
	endpoints: {
		description: """
			A list of Elasticsearch endpoints to send logs to.

			The endpoint must contain an HTTP scheme, and may specify a
			hostname or IP address and port.
			The endpoint may include basic authentication credentials,
			e.g., `https://user:password@example.com`. If credentials are provided in the endpoint,
			they will be used to authenticate against Elasticsearch.

			If `auth` is specified and the endpoint contains credentials,
			a configuration error will be raised.

			Exactly one of `endpoint` or `endpoints` must be set.
			"""
		required: false
		required_one_of: ["endpoint", "endpoints"]
		required_one_of_group: "endpoint"
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
		type:        _schemaDefinitions["core::option::Option<vector::transforms::metric_to_log::MetricToLogConfig>"]
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
		type:        _schemaDefinitions["derived::9fc260a4ee16eb0cc035ad87"]
	}
	request: {
		description: "Outbound HTTP request settings."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::http::RequestConfig"]
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
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
