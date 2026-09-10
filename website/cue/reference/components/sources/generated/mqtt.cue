package metadata

generated: components: sources: mqtt: configuration: {
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
		type:     _schemaDefinitions["derived::2c0db0ef1f05c78303bd4391"]
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["derived::2a4f9b813a8f495175f80ccf"]
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
