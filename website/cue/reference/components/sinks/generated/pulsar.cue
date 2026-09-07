package metadata

generated: components: sinks: pulsar: configuration: {
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
		description: "Authentication configuration."
		required:    false
		type: object: options: {
			name: {
				description: """
					Basic authentication name/username.

					This can be used either for basic authentication (username/password) or JWT authentication.
					When used for JWT, the value should be `token`.
					"""
				required: false
				type: string: examples: ["${PULSAR_NAME}", "name123"]
			}
			oauth2: {
				description: "OAuth2-specific authentication configuration."
				required:    false
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
						type: string: examples: ["{OAUTH2_CREDENTIALS_URL}", "file:///oauth2_credentials", "data:application/json;base64,cHVsc2FyCg=="]
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
				required: false
				type: string: examples: ["${PULSAR_TOKEN}", "123456789"]
			}
		}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: unit: "bytes"
			}
			max_events: {
				description: """
					The maximum amount of events in a batch before it is flushed.

					Note this is an unsigned 32 bit integer which is a smaller capacity than
					many of the other sink batch settings.
					"""
				required: false
				type: uint: {
					examples: [1000]
					unit: "events"
				}
			}
		}
	}
	compression: {
		description: "Supported compression types for Pulsar."
		required:    false
		type: string: {
			default: "none"
			enum: {
				lz4:    "LZ4."
				none:   "No compression."
				snappy: "Snappy."
				zlib:   "Zlib."
				zstd:   "Zstandard."
			}
		}
	}
	connection_retry_options: {
		description: "Custom connection retry options configuration for the Pulsar client."
		required:    false
		type: object: options: {
			connection_timeout_secs: {
				description: "Time limit to establish a connection."
				required:    false
				type: uint: {
					examples: [10]
					unit: "seconds"
				}
			}
			keep_alive_secs: {
				description: "Keep-alive interval for each broker connection."
				required:    false
				type: uint: {
					examples: [60]
					unit: "seconds"
				}
			}
			max_backoff_secs: {
				description: "Maximum delay between reconnection retries."
				required:    false
				type: uint: {
					examples: [30]
					unit: "seconds"
				}
			}
			max_retries: {
				description: "Maximum number of connection retries."
				required:    false
				type: uint: examples: [12]
			}
			min_backoff_ms: {
				description: "Minimum delay between connection retries."
				required:    false
				type: uint: unit: "milliseconds"
			}
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
	endpoint: {
		description: """
			The endpoint to which the Pulsar client should connect to.

			The endpoint should specify the pulsar protocol and port.
			"""
		required: true
		type: string: examples: ["pulsar://127.0.0.1:6650"]
	}
	partition_key_field: {
		description: """
			The log field name or tags key to use for the partition key.

			If the field does not exist in the log event or metric tags, a blank value will be used.

			If omitted, the key is not sent.

			Pulsar uses a hash of the key to choose the topic-partition or uses round-robin if the record has no key.
			"""
		required: false
		type: string: examples: ["message", "my_field"]
	}
	producer_name: {
		description: "The name of the producer. If not specified, the default name assigned by Pulsar is used."
		required:    false
		type: string: examples: ["producer-name"]
	}
	properties_key: {
		description: """
			The log field name to use for the Pulsar properties key.

			If omitted, no properties will be written.
			"""
		required: false
		type: string: {}
	}
	tls: {
		description: "TLS options configuration for the Pulsar client."
		required:    false
		type: object: options: {
			ca_file: {
				description: "File path containing a list of PEM encoded certificates."
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
					Whether hostname verification is enabled when verify_certificate is false.

					Set to true if not specified.
					"""
				required: false
				type: bool: {}
			}
		}
	}
	topic: {
		description: "The Pulsar topic name to write events to."
		required:    true
		type: string: {
			examples: ["topic-1234"]
			syntax: "template"
		}
	}
}
