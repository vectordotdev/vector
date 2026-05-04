package metadata

generated: components: sources: pulsar: configuration: {
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
	auth: {
		description: "Authentication configuration."
		required:    false
		type: object: options: {
			name: {
				description: """
					Basic authentication name/username.

					This can be used either for basic authentication (username/password) or JWT authentication.
					When used for JWT, the value should be `token`.
					"""
				required: true
				type: string: examples: ["${PULSAR_NAME}", "name123"]
			}
			oauth2: {
				description: "OAuth2-specific authentication configuration."
				required:    true
				type: object: options: {
					audience: {
						description: "The OAuth2 audience."
						required:    false
						type: string: examples: ["${OAUTH2_AUDIENCE}", "pulsar"]
					}
					credentials_url: {
						description: """
																The credentials URL.

																A data URL is also supported.
																"""
						required: true
						type: string: examples: ["${OAUTH2_CREDENTIALS_URL}", "file:///oauth2_credentials", "data:application/json;base64,cHVsc2FyCg=="]
					}
					issuer_url: {
						description: "The issuer URL."
						required:    true
						type: string: examples: ["${OAUTH2_ISSUER_URL}", "https://oauth2.issuer"]
					}
					scope: {
						description: "The OAuth2 scope."
						required:    false
						type: string: examples: ["${OAUTH2_SCOPE}", "admin"]
					}
				}
			}
			token: {
				description: """
					Basic authentication password/token.

					This can be used either for basic authentication (username/password) or JWT authentication.
					When used for JWT, the value should be the signed JWT, in the compact representation.
					"""
				required: true
				type: string: examples: ["${PULSAR_TOKEN}", "123456789"]
			}
		}
	}
	batch_size: {
		description: "Max count of messages in a batch."
		required:    false
		type: uint: {}
	}
	consumer_name: {
		description: "The Pulsar consumer name."
		required:    false
		type: string: examples: ["consumer-name"]
	}
	dead_letter_queue_policy: {
		description: "Dead Letter Queue policy configuration."
		required:    false
		type: object: options: {
			dead_letter_topic: {
				description: "Name of the dead letter topic where the failing messages will be sent."
				required:    true
				type: string: {}
			}
			max_redeliver_count: {
				description: "Maximum number of times that a message will be redelivered before being sent to the dead letter queue."
				required:    true
				type: uint: {}
			}
		}
	}
	endpoint: {
		description: "The endpoint to which the Pulsar client should connect to."
		required:    true
		type: string: examples: ["pulsar://127.0.0.1:6650"]
	}
	priority_level: {
		description: """
			The consumer's priority level.

			The broker follows descending priorities. For example, 0=max-priority, 1, 2,...

			In Shared subscription type, the broker first dispatches messages to the max priority level consumers if they have permits. Otherwise, the broker considers next priority level consumers.
			"""
		required: false
		type: int: {}
	}
	subscription_name: {
		description: "The Pulsar subscription name."
		required:    false
		type: string: examples: ["subscription_name"]
	}
	tls: {
		description: "TLS options configuration for the Pulsar client."
		required:    false
		type: object: options: {
			ca_file: {
				description: "File path containing a list of PEM encoded certificates"
				required:    true
				type: string: examples: ["/etc/certs/chain.pem"]
			}
			verify_certificate: {
				description: """
					Enables certificate verification.

					Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
					"""
				required: false
				type: bool: {}
			}
			verify_hostname: {
				description: """
					Whether hostname verification is enabled when verify_certificate is false

					Set to true if not specified.
					"""
				required: false
				type: bool: {}
			}
		}
	}
	topics: {
		description: "The Pulsar topic names to read events from."
		required:    true
		type: array: items: type: string: examples: ["[persistent://public/default/my-topic]"]
	}
}

generated: components: sources: pulsar: configuration: decoding: decodingBase & {
	type: object: options: codec: {
		required: false
		type: string: default: "bytes"
	}
}
generated: components: sources: pulsar: configuration: framing: framingDecoderBase & {
	type: object: options: method: {
		required: false
		type: string: default: "bytes"
	}
}
