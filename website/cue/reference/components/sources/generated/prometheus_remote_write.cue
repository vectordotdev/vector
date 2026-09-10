package metadata

generated: components: sources: prometheus_remote_write: configuration: {
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
		type: string: examples: ["0.0.0.0:9090"]
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
	metadata_conflict_strategy: {
		description: "Defines the behavior for handling conflicting metric metadata."
		required:    false
		type: string: {
			default: "reject"
			enum: {
				ignore: "Silently ignore metadata conflicts, keeping the first metadata entry. This aligns with Prometheus/Thanos behavior."
				reject: "Reject requests with conflicting metadata by returning an HTTP 400 error. This is the default to preserve backwards compatibility."
			}
		}
	}
	path: {
		description: "The URL path on which metric POST requests are accepted."
		required:    false
		type: string: {
			default: "/"
			examples: ["/api/v1/write", "/remote-write"]
		}
	}
	skip_nan_values: {
		description: """
			Whether to skip/discard received samples with NaN values.

			When enabled, any metric sample with a NaN value will be filtered out
			during parsing, preventing downstream processing of invalid metrics.
			"""
		required: false
		type: bool: default: false
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
