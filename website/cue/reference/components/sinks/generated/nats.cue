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
		type: object: options: {
			adaptive_concurrency: {
				description: """
					Configuration of adaptive concurrency parameters.

					These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
					unstable performance and sink behavior. Proceed with caution.
					"""
				required: false
				type:     _schemaDefinitions["vector::sinks::util::adaptive_concurrency::AdaptiveConcurrencySettings"]
			}
			concurrency: {
				description: """
					Configuration for outbound request concurrency.

					This can be set either to one of the below enum values or to a positive integer, which denotes
					a fixed concurrency limit.
					"""
				required: false
				type: {
					string: {
						default: "none"
						enum: {
							adaptive: """
															Concurrency is managed by Vector's [Adaptive Request Concurrency][arc] feature.

															[arc]: https://vector.dev/docs/architecture/arc/
															"""
							none: """
															A fixed concurrency of 1.

															Only one request can be outstanding at any given time.
															"""
						}
					}
					uint: {}
				}
			}
			rate_limit_duration_secs: {
				description: "The time window used for the `rate_limit_num` option."
				required:    false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			rate_limit_num: {
				description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
				required:    false
				type: uint: {
					default: 9223372036854775807
					unit:    "requests"
				}
			}
			retry_attempts: {
				description: "The maximum number of retries to make for failed requests."
				required:    false
				type: uint: {
					default: 9223372036854775807
					unit:    "retries"
				}
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the Fibonacci sequence is used to select future backoffs.
					"""
				required: false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			retry_jitter_mode: {
				description: "The jitter mode to use for retry backoff behavior."
				required:    false
				type: string: {
					default: "Full"
					enum: {
						Full: """
															Full jitter.

															The random delay is anywhere from 0 up to the maximum current delay calculated by the backoff
															strategy.

															Incorporating full jitter into your backoff strategy can greatly reduce the likelihood
															of creating accidental denial of service (DoS) conditions against your own systems when
															many clients are recovering from a failure state.
															"""
						None: "No jitter."
					}
				}
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time to wait between retries."
				required:    false
				type: uint: {
					default: 30
					unit:    "seconds"
				}
			}
			timeout_secs: {
				description: """
					The time a request can take before being aborted.

					Datadog highly recommends that you do not lower this value below the service's internal timeout, as this could
					create orphaned requests, pile on retries, and result in duplicate data downstream.
					"""
				required: false
				type: uint: {
					default: 60
					unit:    "seconds"
				}
			}
		}
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
