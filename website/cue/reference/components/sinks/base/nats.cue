package metadata

base: components: sinks: nats: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source connected to that sink, where the source supports
				end-to-end acknowledgements as well, will wait for events to be acknowledged by the sink
				before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
	}
	auth: {
		description: "Configuration of the authentication strategy when interacting with NATS."
		required:    false
		type: object: options: {
			credentials_file: {
				description:   "Credentials file configuration."
				relevant_when: "strategy = \"credentials_file\""
				required:      true
				type: object: options: path: {
					description: "Path to credentials file."
					required:    true
					type: string: examples: ["/etc/nats/nats.creds"]
				}
			}
			nkey: {
				description:   "NKeys configuration."
				relevant_when: "strategy = \"nkey\""
				required:      true
				type: object: options: {
					nkey: {
						description: """
																User.

																Conceptually, this is equivalent to a public key.
																"""
						required: true
						type: string: {}
					}
					seed: {
						description: """
																Seed.

																Conceptually, this is equivalent to a private key.
																"""
						required: true
						type: string: {}
					}
				}
			}
			strategy: {
				description: """
					The strategy used to authenticate with the NATS server.

					More information on NATS authentication, and the various authentication strategies, can be found in the
					NATS [documentation][nats_auth_docs]. For TLS client certificate authentication specifically, see the
					`tls` settings.

					[nats_auth_docs]: https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro
					"""
				required: true
				type: string: enum: {
					credentials_file: "Credentials file authentication. (JWT-based)"
					nkey:             "NKey authentication."
					token:            "Token authentication."
					user_password:    "Username/password authentication."
				}
			}
			token: {
				description:   "Token configuration."
				relevant_when: "strategy = \"token\""
				required:      true
				type: object: options: value: {
					description: "Token."
					required:    true
					type: string: {}
				}
			}
			user_password: {
				description:   "Username and password configuration."
				relevant_when: "strategy = \"user_password\""
				required:      true
				type: object: options: {
					password: {
						description: "Password."
						required:    true
						type: string: {}
					}
					user: {
						description: "Username."
						required:    true
						type: string: {}
					}
				}
			}
		}
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
				"foo",
			]
		}
	}
	encoding: {
		description: "Configures how events are encoded into raw bytes."
		required:    true
		type: object: options: {
			avro: {
				description:   "Apache Avro-specific encoder options."
				relevant_when: "codec = \"avro\""
				required:      true
				type: object: options: schema: {
					description: "The Avro schema."
					required:    true
					type: string: examples: ["{ \"type\": \"record\", \"name\": \"log\", \"fields\": [{ \"name\": \"message\", \"type\": \"string\" }] }"]
				}
			}
			codec: {
				description: "The codec to use for encoding events."
				required:    true
				type: string: enum: {
					avro: """
						Encodes an event as an [Apache Avro][apache_avro] message.

						[apache_avro]: https://avro.apache.org/
						"""
					gelf: """
						Encodes an event as a [GELF][gelf] message.

						[gelf]: https://docs.graylog.org/docs/gelf
						"""
					json: """
						Encodes an event as [JSON][json].

						[json]: https://www.json.org/
						"""
					logfmt: """
						Encodes an event as a [logfmt][logfmt] message.

						[logfmt]: https://brandur.org/logfmt
						"""
					native: """
						Encodes an event in Vector’s [native Protocol Buffers format][vector_native_protobuf].

						This codec is **[experimental][experimental]**.

						[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					native_json: """
						Encodes an event in Vector’s [native JSON format][vector_native_json].

						This codec is **[experimental][experimental]**.

						[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
						[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
						"""
					raw_message: """
						No encoding.

						This "encoding" simply uses the `message` field of a log event.

						Users should take care if they're modifying their log events (such as by using a `remap`
						transform, etc) and removing the message field while doing additional parsing on it, as this
						could lead to the encoding emitting empty strings for the given event.
						"""
					text: """
						Plain text encoding.

						This "encoding" simply uses the `message` field of a log event. For metrics, it uses an
						encoding that resembles the Prometheus export format.

						Users should take care if they're modifying their log events (such as by using a `remap`
						transform, etc) and removing the message field while doing additional parsing on it, as this
						could lead to the encoding emitting empty strings for the given event.
						"""
				}
			}
			except_fields: {
				description: "List of fields that will be excluded from the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			metric_tag_values: {
				description: """
					Controls how metric tag values are encoded.

					When set to `single`, only the last non-bare value of tags will be displayed with the
					metric.  When set to `full`, all metric tags will be exposed as separate assignments.
					"""
				relevant_when: "codec = \"json\" or codec = \"text\""
				required:      false
				type: string: {
					default: "single"
					enum: {
						full: "All tags will be exposed as arrays of either string or null values."
						single: """
															Tag values will be exposed as single strings, the same as they were before this config
															option. Tags with multiple values will show the last assigned value, and null values will be
															ignored.
															"""
					}
				}
			}
			only_fields: {
				description: "List of fields that will be included in the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			timestamp_format: {
				description: "Format used for timestamp fields."
				required:    false
				type: string: enum: {
					rfc3339: "Represent the timestamp as a RFC 3339 timestamp."
					unix:    "Represent the timestamp as a Unix timestamp."
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
			examples: ["{{ host }}", "foo", "time.us.east", "time.*.east", "time.>", ">"]
			syntax: "template"
		}
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
					"""
				required: false
				type: array: items: type: string: examples: ["h2"]
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/certificate_authority.crt"]
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this server.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.crt"]
			}
			enabled: {
				description: """
					Whether or not to require TLS for incoming/outgoing connections.

					When enabled and used for incoming connections, an identity certificate is also required. See `tls.crt_file` for
					more information.
					"""
				required: false
				type: bool: {}
			}
			key_file: {
				description: """
					Absolute path to a private key file used to identify this server.

					The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.key"]
			}
			key_pass: {
				description: """
					Passphrase used to unlock the encrypted key file.

					This has no effect unless `key_file` is set.
					"""
				required: false
				type: string: examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
			}
			verify_certificate: {
				description: """
					Enables certificate verification.

					If enabled, certificates must be valid in terms of not being expired, as well as being issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that not only the leaf certificate (the
					certificate presented by the client/server) is valid, but also that the issuer of that certificate is valid, and
					so on until reaching a root certificate.

					Relevant for both incoming and outgoing connections.

					Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
					"""
				required: false
				type: bool: {}
			}
			verify_hostname: {
				description: """
					Enables hostname verification.

					If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
					the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.

					Only relevant for outgoing connections.

					Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
					"""
				required: false
				type: bool: {}
			}
		}
	}
	url: {
		description: """
			The NATS [URL][nats_url] to connect to.

			The URL must take the form of `nats://server:port`.
			If the port is not specified it defaults to 4222.

			[nats_url]: https://docs.nats.io/using-nats/developer/connecting#nats-url
			"""
		required: true
		type: string: examples: ["nats://demo.nats.io", "nats://127.0.0.1:4242"]
	}
}
