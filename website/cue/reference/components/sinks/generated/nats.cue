package metadata

generated: components: sinks: nats: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	auth: {
		description: "Configuration of the authentication strategy when interacting with NATS."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::nats::NatsAuthConfig>"]
	}
	connection_name: {
		description: """
			A NATS [name][nats_connection_name] assigned to the NATS connection.

			[nats_connection_name]: https://docs.nats.io/using-nats/developer/connecting/name
			"""
		required: false
		type: string: {
			default: "vector"
			examples: [
				"foo"
			]
		}
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
	jetstream: {
		description: """
			Send messages using [Jetstream][jetstream].

			If set, the `subject` must belong to an existing JetStream stream.

			[jetstream]: https://docs.nats.io/nats-concepts/jetstream
			"""
		required: false
		type: object: options: {
			enabled: {
				description: "Whether to enable Jetstream."
				required:    false
				type: bool: default: false
			}
			headers: {
				description: "A map of NATS headers to be included in each message."
				required:    false
				type: object: options: message_id: {
					description: """
						A unique identifier for the message. Useful for deduplication.

						Can be a template that references fields in the event, e.g., `{{ event_id }}`.
						"""
					required: false
					type: string: {
						examples: ["event-{{ event_id }}"]
						syntax: "template"
					}
				}
			}
		}
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["derived::vector::sinks::util::service::TowerRequestConfig::bb2440a04988b7e322be398c"]
	}
	subject: {
		description: """
			The NATS [subject][nats_subject] to publish messages to.

			[nats_subject]: https://docs.nats.io/nats-concepts/subjects
			"""
		required: true
		type: string: {
			examples: ["events-{{ host }}", "foo", "time.us.east", "time.*.east", "time.>", ">"]
			syntax: "template"
		}
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	url: {
		description: """
			The NATS [URL][nats_url] to connect to.

			The URL must take the form of `nats://server:port`.
			If the port is not specified it defaults to 4222.

			[nats_url]: https://docs.nats.io/using-nats/developer/connecting#nats-url
			"""
		required: true
		type: string: examples: ["nats://demo.nats.io", "nats://127.0.0.1:4242", "nats://localhost:4222,nats://localhost:5222,nats://localhost:6222"]
	}
}
