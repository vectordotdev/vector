package metadata

generated: components: sources: amqp: configuration: {
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
	connection_string: {
		description: """
			URI for the AMQP server.

			The URI has the format of
			`amqp://<user>:<password>@<host>:<port>/<vhost>?timeout=<seconds>`.

			The default vhost can be specified by using a value of `%2f`.

			To connect over TLS, a scheme of `amqps` can be specified instead. For example,
			`amqps://...`. Additional TLS settings, such as client certificate verification, can be
			configured under the `tls` section.
			"""
		required: true
		type: string: examples: ["amqp://user:password@127.0.0.1:5672/%2f?timeout=10"]
	}
	consumer: {
		description: "The identifier for the consumer."
		required:    false
		type: string: {
			default: "vector"
			examples: ["consumer-group-name"]
		}
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::2c0db0ef1f05c78303bd4391"]
	}
	exchange_key: {
		description: "The `AMQP` exchange key."
		required:    false
		type: string: default: "exchange"
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
	offset_key: {
		description: "The `AMQP` offset key."
		required:    false
		type: string: default: "offset"
	}
	prefetch_count: {
		description: """
			Maximum number of unacknowledged messages the broker will deliver to this consumer.

			This controls flow control via AMQP QoS prefetch. Lower values limit memory usage and
			prevent overwhelming slow consumers, but may reduce throughput. Higher values increase
			throughput but consume more memory.

			If not set, the broker/client default applies (often unlimited).
			"""
		required: false
		type: uint: examples: [
			100
		]
	}
	queue: {
		description: "The name of the queue to consume."
		required:    false
		type: string: default: "vector"
	}
	routing_key_field: {
		description: "The `AMQP` routing key."
		required:    false
		type: string: default: "routing"
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
