package metadata

generated: components: sinks: datadog_events: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
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
	retry_strategy: {
		description: """
			Configurable retry strategy for `http` based sinks.

			For more information about error responses, see [Client Error Responses][error_responses].

			[error_responses]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status#client_error_responses
			"""
		required: false
		type: object: options: {
			status_codes: {
				description:   "Retry on these specific HTTP status codes"
				relevant_when: "type = \"custom\""
				required:      true
				type: array: items: type: uint: {}
			}
			type: {
				description: "The retry strategy enum."
				required:    false
				type: string: {
					default: "default"
					enum: {
						all:     "Retry on *all* HTTP status codes except for success codes (2xx)"
						custom:  "Custom retry strategy"
						default: "Default strategy. See [`RetryStrategy::retry_action`] for more details."
						none:    "Don't retry any errors, including request timeouts."
					}
				}
			}
		}
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
