package metadata

generated: components: sinks: splunk_hec_metrics: configuration: {
	acknowledgements: {
		description: "Splunk HEC acknowledgement configuration."
		required:    false
		type:        _schemaDefinitions["vector::sinks::splunk_hec::common::acknowledgements::HecClientAcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::splunk_hec::common::util::SplunkHecDefaultBatchSettings>"]
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
	default_namespace: {
		description: """
			Sets the default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with a period (`.`).
			"""
		required: false
		type: string: examples: ["service"]
	}
	default_token: {
		description: """
			Default Splunk HEC token.

			If an event has a token set in its metadata, it prevails over the one set here.
			"""
		required: true
		type: string: examples: ["${SPLUNK_HEC_TOKEN}", "A94A8FE5CCB19BA61C4C08"]
	}
	endpoint: {
		description: """
			The base URL of the Splunk instance.

			The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
			by the [`Splunk`][splunk] API are used.

			[splunk]: https://docs.splunk.com/Documentation/Splunk/8.0.0/Data/HECRESTendpoints
			"""
		required: true
		type: string: examples: ["https://http-inputs-hec.splunkcloud.com", "https://hec.splunk.com:8088", "http://example.com"]
	}
	host_key: {
		description: """
			Overrides the name of the log field used to retrieve the hostname to send to Splunk HEC.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: default: "host"
	}
	index: {
		description: """
			The name of the index where to send the events to.

			If not specified, the default index defined within Splunk is used.
			"""
		required: false
		type: string: {
			examples: ["index-{{ host }}", "custom_index"]
			syntax: "template"
		}
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
	source: {
		description: """
			The source of events sent to this sink.

			This is typically the filename the logs originated from.

			If unset, the Splunk collector sets it.
			"""
		required: false
		type: string: {
			examples: ["source-{{ file }}", "/var/log/syslog", "UDP:514"]
			syntax: "template"
		}
	}
	sourcetype: {
		description: """
			The sourcetype of events sent to this sink.

			If unset, Splunk defaults to `httpevent`.
			"""
		required: false
		type: string: {
			examples: ["sourcetype-{{ sourcetype }}", "_json"]
			syntax: "template"
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
