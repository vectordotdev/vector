package metadata

generated: components: sources: mqtt: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled for this source.

			Prefer enabling `acknowledgements` at the [global][global_acks] or
			sink level instead of here: this setting takes precedence over both
			when explicitly set, which can silently disable acknowledgements a
			connected sink otherwise requires.

			When enabled (through this setting, the global setting, or because a
			connected sink requires it), the QoS 1/2 acknowledgement for an
			incoming publish is deferred until the resulting events have been
			delivered to all connected sinks, giving at-least-once delivery. An
			event that a sink rejects outright is acknowledged and dropped, with
			an error logged, because redelivering it cannot succeed.

			Acknowledgements require a stable `client_id`, so the MQTT session
			(and its unacknowledged messages) can be resumed after a restart, and
			a non-zero `keep_alive`. If either is missing, enabling
			acknowledgements here is a configuration error; if they were enabled
			only by the global setting or a connected sink, the source logs a
			warning and runs without them.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			"""
		required: false
		type:     _schemaDefinitions["bool_or_struct::vector_core::config::SourceAcknowledgementsConfig"]
	}
	client_id: {
		description: "MQTT client ID."
		required:    false
		type: string: {}
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::185fc3d66caef78f8b28ff32"]
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
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	topic: {
		description: "MQTT topic or topics from which messages are to be read."
		required:    false
		type: string: default: "vector"
	}
	topic_key: {
		description: """
			Overrides the name of the log field used to add the topic to each event.

			The value is the topic from which the MQTT message was published to.

			By default, `"topic"` is used.
			"""
		required: false
		type: string: {
			default: "topic"
			examples: [
				"topic"
			]
		}
	}
	user: {
		description: "MQTT username."
		required:    false
		type: string: {}
	}
}
