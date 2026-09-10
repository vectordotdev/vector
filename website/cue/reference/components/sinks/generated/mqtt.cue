package metadata

generated: components: sinks: mqtt: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	clean_session: {
		description: "If set to true, the MQTT session is cleaned on login."
		required:    false
		type: bool: default: false
	}
	client_id: {
		description: "MQTT client ID."
		required:    false
		type: string: {}
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
	host: {
		description: "MQTT server address (The broker’s domain name or IP address)."
		required:    true
		type: string: examples: ["mqtt.example.com", "127.0.0.1"]
	}
	keep_alive: {
		description: "Connection keep-alive interval."
		required:    false
		type: uint: default: 60
	}
	max_packet_size: {
		description: "Maximum packet size"
		required:    false
		type: uint: default: 10240
	}
	password: {
		description: "MQTT password."
		required:    false
		type: string: {}
	}
	port: {
		description: "TCP port of the MQTT server to connect to."
		required:    false
		type: uint: default: 1883
	}
	quality_of_service: {
		description: "Supported Quality of Service types for MQTT."
		required:    false
		type: string: {
			default: "atleastonce"
			enum: {
				atleastonce: "AtLeastOnce."
				atmostonce:  "AtMostOnce."
				exactlyonce: "ExactlyOnce."
			}
		}
	}
	retain: {
		description: "Whether the messages should be retained by the server"
		required:    false
		type: bool: default: false
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	topic: {
		description: "MQTT publish topic (templates allowed)"
		required:    true
		type: string: syntax: "template"
	}
	user: {
		description: "MQTT username."
		required:    false
		type: string: {}
	}
}
