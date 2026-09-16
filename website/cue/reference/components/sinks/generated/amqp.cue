package metadata

generated: components: sinks: amqp: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
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
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type:     _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
	}
	exchange: {
		description: "The exchange to publish messages to."
		required:    true
		type: string: syntax: "template"
	}
	max_channels: {
		description: "Maximum number of AMQP channels to keep active (channels are created as needed)."
		required:    false
		type: uint: default: 4
	}
	properties: {
		description: """
			Configure the AMQP message properties.

			AMQP message properties.
			"""
		required: false
		type: object: options: {
			content_encoding: {
				description: "Content-Encoding for the AMQP messages."
				required:    false
				type: string: {}
			}
			content_type: {
				description: "Content-Type for the AMQP messages."
				required:    false
				type: string: {}
			}
			expiration_ms: {
				description: "Expiration for AMQP messages (in milliseconds)."
				required:    false
				type: uint: {}
			}
			priority: {
				description: "Priority for AMQP messages. It can be templated to an integer between 0 and 255 inclusive."
				required:    false
				type: {
					string: syntax: "template"
					uint: {}
				}
			}
		}
	}
	routing_key: {
		description: "Template used to generate a routing key which corresponds to a queue binding."
		required:    false
		type: string: syntax: "template"
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
