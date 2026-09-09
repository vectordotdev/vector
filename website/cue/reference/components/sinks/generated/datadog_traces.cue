package metadata

generated: components: sinks: datadog_traces: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
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
					default: 3000000
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: {
					default: 1000
					unit:    "events"
				}
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 10.0
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
		type: string: enum: {
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
	default_api_key: {
		description: """
			The default Datadog [API key][api_key] to use in authentication of HTTP requests.

			If an event has a Datadog [API key][api_key] set explicitly in its metadata, it takes
			precedence over this setting.

			This value can also be set by specifying the `DD_API_KEY` environment variable.
			The value specified here takes precedence over the environment variable.

			[api_key]: https://docs.datadoghq.com/api/?lang=bash#authentication
			[global_options]: /docs/reference/configuration/global-options/#datadog
			"""
		required: false
		type: string: examples: ["${DATADOG_API_KEY_ENV_VAR}", "ef8d5de700e7989468166c40fc8a0ccd"]
	}
	endpoint: {
		description: """
			The endpoint to send observability data to.

			The endpoint must be an absolute HTTP(S) URL. A missing scheme defaults
			to `https`. The API path should NOT be specified as this is handled by
			the sink.

			If set, overrides the `site` option.
			"""
		required: false
		type: string: examples: ["http://127.0.0.1:8080", "http://example.com:12345"]
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::service::TowerRequestConfig"]
	}
	site: {
		description: """
			The Datadog [site][dd_site] to send observability data to.

			This value can also be set by specifying the `DD_SITE` environment variable.
			The value specified here takes precedence over the environment variable.

			If not specified by the environment variable, a default value of
			`datadoghq.com` is taken.

			[dd_site]: https://docs.datadoghq.com/getting_started/site
			"""
		required: false
		type: string: examples: ["us3.datadoghq.com", "datadoghq.eu"]
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
