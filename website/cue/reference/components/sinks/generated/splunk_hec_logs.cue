package metadata

generated: components: sinks: splunk_hec_logs: configuration: {
	acknowledgements: {
		description: "Splunk HEC acknowledgement configuration."
		required:    false
		type:        _schemaDefinitions["vector::sinks::splunk_hec::common::acknowledgements::HecClientAcknowledgementsConfig"]
	}
	auto_extract_timestamp: {
		description: """
			Passes the `auto_extract_timestamp` option to Splunk.

			This option is only relevant to Splunk v8.x and above, and is only applied when
			`endpoint_target` is set to `event`.

			Setting this to `true` causes Splunk to extract the timestamp from the message text
			rather than use the timestamp embedded in the event. The timestamp must be in the format
			`yyyy-mm-dd hh:mm:ss`.
			"""
		required: false
		type: bool: {}
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
	default_token: {
		description: """
			Default Splunk HEC token.

			If an event has a token set in its secrets (`splunk_hec_token`), it prevails over the one set here.
			"""
		required: true
		type: string: {}
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
			The base URL of the Splunk instance.

			The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
			by the [`Splunk`][splunk] API are used.

			[splunk]: https://docs.splunk.com/Documentation/Splunk/8.0.0/Data/HECRESTendpoints
			"""
		required: true
		type: string: examples: ["https://http-inputs-hec.splunkcloud.com", "https://hec.splunk.com:8088", "http://example.com"]
	}
	endpoint_target: {
		description: "Splunk HEC endpoint configuration."
		required:    false
		type: string: {
			default: "event"
			enum: {
				event: """
					Events are sent to the [event endpoint][event_endpoint_docs].

					When the event endpoint is used, configured [event metadata][event_metadata_docs] is sent
					directly with each event.

					[event_endpoint_docs]: https://docs.splunk.com/Documentation/Splunk/8.0.0/RESTREF/RESTinput#services.2Fcollector.2Fevent
					[event_metadata_docs]: https://docs.splunk.com/Documentation/Splunk/latest/Data/FormateventsforHTTPEventCollector#Event_metadata
					"""
				raw: """
					Events are sent to the [raw endpoint][raw_endpoint_docs].

					When the raw endpoint is used, configured [event metadata][event_metadata_docs] is sent as
					query parameters on the request, except for the `timestamp` field.

					[raw_endpoint_docs]: https://docs.splunk.com/Documentation/Splunk/8.0.0/RESTREF/RESTinput#services.2Fcollector.2Fraw
					[event_metadata_docs]: https://docs.splunk.com/Documentation/Splunk/latest/Data/FormateventsforHTTPEventCollector#Event_metadata
					"""
			}
		}
	}
	host_key: {
		description: """
			Overrides the name of the log field used to retrieve the hostname to send to Splunk HEC.

			By default, the [global `log_schema.host_key` option][global_host_key] is used if log
			events are Legacy namespaced, or the semantic meaning of "host" is used, if defined.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: {}
	}
	index: {
		description: """
			The name of the index to send events to.

			If not specified, the default index defined within Splunk is used.
			"""
		required: false
		type: string: {
			examples: ["index-{{ host }}", "custom_index"]
			syntax: "template"
		}
	}
	indexed_fields: {
		description: """
			Fields to be [added to Splunk index][splunk_field_index_docs].

			[splunk_field_index_docs]: https://docs.splunk.com/Documentation/Splunk/8.0.0/Data/IFXandHEC
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["field1", "field2"]
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
	timestamp_key: {
		description: """
			Overrides the name of the log field used to retrieve the timestamp to send to Splunk HEC.
			When set to `“”`, a timestamp is not set in the events sent to Splunk HEC.

			By default, either the [global `log_schema.timestamp_key` option][global_timestamp_key] is used
			if log events are Legacy namespaced, or the semantic meaning of "timestamp" is used, if defined.

			[global_timestamp_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.timestamp_key
			"""
		required: false
		type: string: examples: ["timestamp", ""]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
