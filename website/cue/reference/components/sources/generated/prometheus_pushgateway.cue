package metadata

generated: components: sources: prometheus_pushgateway: configuration: {
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

			The address _must_ include a port.
			"""
		required: true
		type: string: examples: ["0.0.0.0:9091"]
	}
	aggregate_metrics: {
		description: """
			Whether to aggregate values across pushes.

			Only applies to counters and histograms as gauges and summaries can't be
			meaningfully aggregated.
			"""
		required: false
		type: bool: default: false
	}
	auth: {
		description: """
			Configuration of the authentication strategy for server mode sinks and sources.

			Use the HTTP authentication with HTTPS only. The authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::common::http::server_auth::HttpServerAuthConfig>"]
	}
	keepalive: {
		description: "Configuration of HTTP server keepalive parameters."
		required:    false
		type:        _schemaDefinitions["vector::http::KeepaliveConfig"]
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
