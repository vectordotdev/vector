package metadata

base: components: sources: pulsar: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
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
	decoding: {
		description: "Configures how events are decoded from raw bytes."
		required:    false
		type: object: options: {
			codec: {
				description: "The codec to use for decoding events."
				required:    false
				type: string: {
					default: "bytes"
					enum: {
						bytes: "Uses the raw bytes as-is."
						gelf: """
															Decodes the raw bytes as a [GELF][gelf] message.

															[gelf]: https://docs.graylog.org/docs/gelf
															"""
						json: """
															Decodes the raw bytes as [JSON][json].

															[json]: https://www.json.org/
															"""
						native: """
															Decodes the raw bytes as [native Protocol Buffers format][vector_native_protobuf].

															This codec is **[experimental][experimental]**.

															[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
															[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
															"""
						native_json: """
															Decodes the raw bytes as [native JSON format][vector_native_json].

															This codec is **[experimental][experimental]**.

															[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
															[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
															"""
						protobuf: """
															Decodes the raw bytes as [protobuf][protobuf].

															[protobuf]: https://protobuf.dev/
															"""
						syslog: """
															Decodes the raw bytes as a Syslog message.

															Decodes either as the [RFC 3164][rfc3164]-style format ("old" style) or the
															[RFC 5424][rfc5424]-style format ("new" style, includes structured data).

															[rfc3164]: https://www.ietf.org/rfc/rfc3164.txt
															[rfc5424]: https://www.ietf.org/rfc/rfc5424.txt
															"""
					}
				}
			}
			gelf: {
				description:   "GELF-specific decoding options."
				relevant_when: "codec = \"gelf\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
			}
			json: {
				description:   "JSON-specific decoding options."
				relevant_when: "codec = \"json\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
			}
			native_json: {
				description:   "Vector's native JSON-specific decoding options."
				relevant_when: "codec = \"native_json\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
			}
			protobuf: {
				description:   "Protobuf-specific decoding options."
				relevant_when: "codec = \"protobuf\""
				required:      false
				type: object: options: {
					desc_file: {
						description: "Path to desc file"
						required:    false
						type: string: default: ""
					}
					message_type: {
						description: "message type. e.g package.message"
						required:    false
						type: string: default: ""
					}
				}
			}
			syslog: {
				description:   "Syslog-specific decoding options."
				relevant_when: "codec = \"syslog\""
				required:      false
				type: object: options: lossy: {
					description: """
						Determines whether or not to replace invalid UTF-8 sequences instead of failing.

						When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

						[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
						"""
					required: false
					type: bool: default: true
				}
			}
		}
	}
	endpoint: {
		description: "The endpoint to which the Pulsar client should connect to."
		required:    true
		type: string: examples: ["pulsar://127.0.0.1:6650"]
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited decoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type: object: options: {
					delimiter: {
						description: "The character that delimits byte sequences."
						required:    true
						type: uint: {}
					}
					max_length: {
						description: """
																The maximum length of the byte buffer.

																This length does *not* include the trailing delimiter.

																By default, there is no maximum length enforced. If events are malformed, this can lead to
																additional resource usage as events continue to be buffered in memory, and can potentially
																lead to memory exhaustion in extreme cases.

																If there is a risk of processing malformed data, such as logs with user-controlled input,
																consider setting the maximum length to a reasonably large value as a safety net. This
																ensures that processing is not actually unbounded.
																"""
						required: false
						type: uint: {}
					}
				}
			}
			method: {
				description: "The framing method."
				required:    false
				type: string: {
					default: "bytes"
					enum: {
						bytes:               "Byte frames are passed through as-is according to the underlying I/O boundaries (for example, split between messages or stream segments)."
						character_delimited: "Byte frames which are delimited by a chosen character."
						length_delimited:    "Byte frames which are prefixed by an unsigned big-endian 32-bit integer indicating the length."
						newline_delimited:   "Byte frames which are delimited by a newline character."
						octet_counting: """
															Byte frames according to the [octet counting][octet_counting] format.

															[octet_counting]: https://tools.ietf.org/html/rfc6587#section-3.4.1
															"""
					}
				}
			}
			newline_delimited: {
				description:   "Options for the newline delimited decoder."
				relevant_when: "method = \"newline_delimited\""
				required:      false
				type: object: options: max_length: {
					description: """
						The maximum length of the byte buffer.

						This length does *not* include the trailing delimiter.

						By default, there is no maximum length enforced. If events are malformed, this can lead to
						additional resource usage as events continue to be buffered in memory, and can potentially
						lead to memory exhaustion in extreme cases.

						If there is a risk of processing malformed data, such as logs with user-controlled input,
						consider setting the maximum length to a reasonably large value as a safety net. This
						ensures that processing is not actually unbounded.
						"""
					required: false
					type: uint: {}
				}
			}
			octet_counting: {
				description:   "Options for the octet counting decoder."
				relevant_when: "method = \"octet_counting\""
				required:      false
				type: object: options: max_length: {
					description: "The maximum length of the byte buffer."
					required:    false
					type: uint: {}
				}
			}
		}
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
	topics: {
		description: "The Pulsar topic names to read events from."
		required:    true
		type: array: items: type: string: examples: ["[persistent://public/default/my-topic]"]
	}
}
