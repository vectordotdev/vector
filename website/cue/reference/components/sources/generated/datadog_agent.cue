package metadata

generated: components: sources: datadog_agent: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::SourceAcknowledgementsConfig"]
	}
	address: {
		description: """
			The socket address to accept connections on.

			It _must_ include a port.
			"""
		required: true
		type: string: examples: ["0.0.0.0:80", "localhost:80"]
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::2c0db0ef1f05c78303bd4391"]
	}
	disable_llmobs: {
		description: "If this is set to `true`, LLM Observability events are not accepted by the component."
		required:    false
		type: bool: default: false
	}
	disable_logs: {
		description: "If this is set to `true`, logs are not accepted by the component."
		required:    false
		type: bool: default: false
	}
	disable_metrics: {
		description: "If this is set to `true`, metrics (beta) are not accepted by the component."
		required:    false
		type: bool: default: false
	}
	disable_traces: {
		description: "If this is set to `true`, traces (alpha) are not accepted by the component."
		required:    false
		type: bool: default: false
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::FramingConfig::2a4f9b813a8f495175f80ccf"]
	}
	keepalive: {
		description: "Configuration of HTTP server keepalive parameters."
		required:    false
		type:        _schemaDefinitions["vector::http::KeepaliveConfig"]
	}
	multiple_outputs: {
		description: """
			If this is set to `true`, logs, metrics (beta), and traces (alpha) are sent to different outputs.

			For a source component named `agent`, the received logs, metrics (beta), and traces (alpha) can then be
			configured as input to other components by specifying `agent.logs`, `agent.metrics`, and
			`agent.traces`, respectively.
			"""
		required: false
		type: bool: default: false
	}
	parse_ddtags: {
		description: """
			If this is set to `true`, when log events contain the field `ddtags`, the string value that
			contains a list of key:value pairs set by the Agent is parsed and expanded into an array.
			"""
		required: false
		type: bool: default: false
	}
	send_timeout_secs: {
		description: """
			The timeout before responding to requests with a HTTP 503 Service Unavailable error.

			If not set, responses to completed requests will block indefinitely until connected
			transforms or sinks are ready to receive the events. When this happens, the sending Datadog
			Agent will eventually time out the request and drop the connection, resulting Vector
			generating an "Events dropped." error and incrementing the `component_discarded_events_total`
			internal metric. By setting this option to a value less than the Agent's timeout, Vector
			will instead respond to the Agent with a HTTP 503 Service Unavailable error, emit a warning,
			and increment the `component_timed_out_events_total` internal metric instead.
			"""
		required: false
		type: float: {}
	}
	split_metric_namespace: {
		description: """
			If this is set to `true`, metric names are split at the first '.' into a namespace and name.
			For example, `system.cpu.usage` would be split into namespace `system` and name `cpu.usage`.
			If `false`, the full metric name is used without splitting. This may be useful if you are using a
			default namespace for metrics in sinks connected to this source.
			"""
		required: false
		type: bool: default: true
	}
	store_api_key: {
		description: """
			If this is set to `true`, when incoming events contain a Datadog API key, it is
			stored in the event metadata and used if the event is sent to a Datadog sink.
			"""
		required: false
		type: bool: default: true
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
