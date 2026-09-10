package metadata

generated: components: sources: nats: configuration: {
	auth: {
		description: "Configuration of the authentication strategy when interacting with NATS."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::nats::NatsAuthConfig>"]
	}
	connection_name: {
		description: """
			A [name][nats_connection_name] assigned to the NATS connection.

			[nats_connection_name]: https://docs.nats.io/using-nats/developer/connecting/name
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
	jetstream: {
		description: "Configuration for NATS JetStream."
		required:    false
		type: object: options: {
			batch_config: {
				description: """
					Batch settings for a JetStream pull consumer.

					By default, messages are pulled in batches of up to 200.
					Each pull request expires after 30 seconds if not fulfilled.
					There is no explicit maximum byte size per batch unless specified.

					**Note:** These defaults follow the `async-nats` crate’s `StreamBuilder`.
					"""
				required: false
				type: object: options: {
					batch: {
						description: "The maximum number of messages to pull in a single batch."
						required:    false
						type: uint: default: 200
					}
					max_bytes: {
						description: """
																The maximum total byte size for a batch. The pull request will be
																fulfilled when either `size` or `max_bytes` is reached.
																"""
						required: false
						type: uint: default: 0
					}
				}
			}
			consumer: {
				description: "The name of the durable consumer to pull from."
				required:    true
				type: string: {}
			}
			stream: {
				description: "The name of the stream to bind to."
				required:    true
				type: string: {}
			}
		}
	}
	queue: {
		description: "The NATS queue group to join."
		required:    false
		type: string: {}
	}
	subject: {
		description: """
			The NATS [subject][nats_subject] to pull messages from.

			[nats_subject]: https://docs.nats.io/nats-concepts/subjects
			"""
		required: true
		type: string: examples: ["foo", "time.us.east", "time.*.east", "time.>", ">"]
	}
	subject_key_field: {
		description: "The `NATS` subject key."
		required:    false
		type: string: default: "subject"
	}
	subscriber_capacity: {
		description: """
			The buffer capacity of the underlying NATS subscriber.

			This value determines how many messages the NATS subscriber buffers
			before incoming messages are dropped.

			See the [async_nats documentation][async_nats_subscription_capacity] for more information.

			[async_nats_subscription_capacity]: https://docs.rs/async-nats/latest/async_nats/struct.ConnectOptions.html#method.subscription_capacity
			"""
		required: false
		type: uint: default: 65536
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	url: {
		description: """
			The NATS URL to connect to.

			The URL takes the form of `nats://server:port`.
			If the port is not specified it defaults to 4222.
			"""
		required: true
		type: string: examples: ["nats://demo.nats.io", "nats://127.0.0.1:4242", "nats://localhost:4222,nats://localhost:5222,nats://localhost:6222"]
	}
}
