package metadata

generated: components: sources: iggy: configuration: {
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
		type: object: options: enabled: {
			description: "Whether or not end-to-end acknowledgements are enabled for this source."
			required:    false
			type: bool: {}
		}
	}
	batch_length: {
		description: "The maximum number of messages pulled per poll. Defaults to 1000."
		required:    false
		type: uint: default: 1000
	}
	commit_interval_secs: {
		description: """
			The interval, in seconds, at which consumer offsets are committed to the
			Iggy server. Only used when end-to-end acknowledgements are enabled.
			"""
		required: false
		type: uint: default: 5
	}
	consumer_name: {
		description: """
			The consumer name. Used as the durable consumer identifier (and as the
			consumer group name when `partition` is unset).
			"""
		required: true
		type: string: examples: [
			"vector"
		]
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::185fc3d66caef78f8b28ff32"]
	}
	drain_timeout_secs: {
		description: """
			The maximum time, in seconds, to wait for in-flight events to be
			acknowledged downstream during shutdown before the final consumer
			offsets are committed. Only used when end-to-end acknowledgements are
			enabled.
			"""
		required: false
		type: uint: default: 5
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
	offset_key_field: {
		description: """
			The key under which the source message offset is recorded on each
			event (Legacy log namespace only).
			"""
		required: false
		type: string: default: "offset"
	}
	partition: {
		description: """
			Pin the consumer to a single partition. When unset, a consumer group
			named after `consumer_name` is used and the broker assigns partitions
			across members.
			"""
		required: false
		type: uint: {}
	}
	partition_id_key_field: {
		description: """
			The key under which the source partition ID is recorded on each event
			(Legacy log namespace only).
			"""
		required: false
		type: string: default: "partition_id"
	}
	poll_interval_ms: {
		description: """
			The maximum time, in milliseconds, the broker may hold a poll while
			no messages are available.
			"""
		required: false
		type: uint: default: 100
	}
	stream: {
		description: "The Iggy stream name to consume from."
		required:    true
		type: string: examples: [
			"vector"
		]
	}
	stream_key_field: {
		description: """
			The Iggy stream key under which the source stream name is recorded on
			each event (Legacy log namespace only).
			"""
		required: false
		type: string: default: "stream"
	}
	topic: {
		description: "The Iggy topic name within the stream to consume from."
		required:    true
		type: string: examples: [
			"logs"
		]
	}
	topic_key_field: {
		description: """
			The Iggy topic key under which the source topic name is recorded on
			each event (Legacy log namespace only).
			"""
		required: false
		type: string: default: "topic"
	}
	url: {
		description: """
			The Iggy [connection string][iggy_conn] of the server to consume from.

			The connection string takes the form
			`iggy+<protocol>://<credentials>@<host>:<port>` where `<protocol>` is one
			of `tcp`, `quic`, `http`, or `ws`, and `<credentials>` is either
			`username:password` or a personal access token.

			[iggy_conn]: https://iggy.apache.org/docs/connection-string
			"""
		required: true
		type: string: examples: ["iggy+tcp://iggy:iggy@127.0.0.1:8090"]
	}
}
