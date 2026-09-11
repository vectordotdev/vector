package metadata

generated: components: sources: vector: configuration: {
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
			The socket address to listen for connections on.

			It _must_ include a port.
			"""
		required: true
		type: string: {}
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
	version: {
		description: "Version of the configuration."
		required:    false
		type: string: enum: "2": "Marker value for version two."
	}
}
