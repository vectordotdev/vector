package metadata

generated: components: sources: opentelemetry: configuration: {
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
	grpc: {
		description: "Configuration for the `opentelemetry` gRPC server."
		required:    true
		type: object: {
			examples: [{
				address: "0.0.0.0:4317"
				keepalive: {
					max_connection_age_grace_secs: null
					max_connection_age_secs:       null
				}
			}]
			options: {
				address: {
					description: """
						The socket address to listen for connections on.

						It _must_ include a port.
						"""
					required: true
					type: string: examples: ["0.0.0.0:4317", "localhost:4317"]
				}
				keepalive: {
					description: "Configuration of gRPC server keepalive parameters."
					required:    false
					type:        _schemaDefinitions["vector::sources::util::grpc::GrpcKeepaliveConfig"]
				}
				tls: {
					description: "Configures the TLS options for incoming/outgoing connections."
					required:    false
					type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
				}
			}
		}
	}
	http: {
		description: "Configuration for the `opentelemetry` HTTP server."
		required:    true
		type: object: {
			examples: [{
				address: "0.0.0.0:4318"
				headers: []
				keepalive: {
					max_connection_age_jitter_factor: 0.1
					max_connection_age_secs:          300
					tcp_keepalive:                    null
				}
			}]
			options: {
				address: {
					description: """
						The socket address to listen for connections on.

						It _must_ include a port.
						"""
					required: true
					type: string: examples: ["0.0.0.0:4318", "localhost:4318"]
				}
				headers: {
					description: """
						A list of HTTP headers to include in the event.

						Accepts the wildcard (`*`) character for headers matching a specified pattern.

						Specifying "*" results in all headers included in the event.

						For log events in legacy namespace mode, headers are not included if a field with a conflicting name exists.
						For metrics and traces, headers are always added to event metadata.
						"""
					required: false
					type: array: {
						default: []
						items: type: string: examples: ["User-Agent", "X-My-Custom-Header", "X-*", "*"]
					}
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
		}
	}
	use_otlp_decoding: {
		description: """
			Configuration for OTLP decoding behavior.

			This configuration controls how OpenTelemetry Protocol (OTLP) data is decoded for each
			signal type (logs, metrics, traces). When a signal is configured to use OTLP decoding, the raw OTLP format is
			preserved, allowing the data to be forwarded to downstream OTLP collectors without transformation.
			Otherwise, the signal is converted to Vector's native event format.

			Simple boolean form:

			```yaml
			use_otlp_decoding: true  # All signals preserve OTLP format
			# or
			use_otlp_decoding: false # All signals use Vector native format (default)
			```

			Per-signal configuration:

			```yaml
			use_otlp_decoding:
			  logs: false     # Convert to Vector native format
			  metrics: false  # Convert to Vector native format
			  traces: true    # Preserve OTLP format
			```

			**Note:** When OTLP decoding is enabled for metrics:
			- Metrics are parsed as logs while preserving the OTLP format
			- Vector's metric transforms will NOT be compatible with this output
			- The events can be forwarded directly (passthrough) to a downstream OTLP collector
			"""
		required: false
		type: object: options: {
			logs: {
				description: """
					Whether to use OTLP decoding for logs.

					When `true`, logs preserve their OTLP format.
					When `false` (default), logs are converted to Vector's native format.
					"""
				required: false
				type: bool: default: false
			}
			metrics: {
				description: """
					Whether to use OTLP decoding for metrics.

					When `true`, metrics preserve their OTLP format but are processed as logs.
					When `false` (default), metrics are converted to Vector's native metric format.
					"""
				required: false
				type: bool: default: false
			}
			traces: {
				description: """
					Whether to use OTLP decoding for traces.

					When `true`, traces preserve their OTLP format.
					When `false` (default), traces are converted to Vector's native format.
					"""
				required: false
				type: bool: default: false
			}
		}
	}
}
