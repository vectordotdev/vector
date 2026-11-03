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
		type:     _schemaDefinitions["bool_or_struct::vector_core::config::SourceAcknowledgementsConfig"]
	}
	decoding: {
		description: """
			Decoding configuration for the signals received by this source.

			Set `codec` to `otlp` to preserve the OTLP representation. When this option is omitted,
			all signals are converted to Vector's native representation. Only the `otlp` codec is
			supported. Do not set this option together with `use_otlp_decoding`.
			"""
		required: false
		type: object: options: {
			codec: {
				description: "Preserves the OTLP representation of the selected signals."
				required:    true
				type: string: enum: otlp: "Preserves the OTLP representation of the selected signals."
			}
			signal_types: {
				description: """
					Signals that retain their OTLP representation.

					Unlisted signals are still accepted and converted to Vector's native representation.
					Omitting this list selects all signals. An empty list uses native conversion for all
					signals. Order has no effect, and duplicate signal names are ignored.
					OTLP metrics are emitted as log events and cannot be used with native metric transforms.
					"""
				required: false
				type: array: {
					default: ["logs", "metrics", "traces"]
					items: type: string: enum: {
						logs:    "OTLP logs signal (ExportLogsServiceRequest)"
						metrics: "OTLP metrics signal (ExportMetricsServiceRequest)"
						traces:  "OTLP traces signal (ExportTraceServiceRequest)"
					}
				}
			}
		}
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
	max_concurrent_requests: {
		description: """
			Maximum number of requests processed concurrently across the HTTP and gRPC servers.

			Requests beyond this limit are rejected. When omitted, no request concurrency limit is enforced.
			"""
		required: false
		type: uint: {}
	}
	request_timeout_secs: {
		description: """
			Maximum time spent processing a request through submission to the source output.

			When omitted, no request processing timeout is enforced. Downstream acknowledgement waiting
			is not subject to this timeout.
			"""
		required: false
		type: uint: unit: "seconds"
	}
	use_otlp_decoding: {
		deprecated: true
		description: """
			Deprecated selection of OTLP decoding behavior.

			Use `decoding.codec: otlp` instead. To select individual signals, set
			`decoding.signal_types`. Remove this option to use native conversion for all signals.
			Existing boolean and object values remain supported during migration.
			See the [migration guide](/deprecations/#opentelemetry-use_otlp_decoding).
			"""
		required: false
		type: {
			bool: {}
			object: options: {
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
}
