package metadata

generated: components: sources: nats: configuration: {
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
			A [name][nats_connection_name] assigned to the NATS connection.

			[nats_connection_name]: https://docs.nats.io/using-nats/developer/connecting/name
			"""
		required: true
		type: string: examples: [
			"vector",
		]
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
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with a peer. They are prioritized in the order
					that they are defined.
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

					If this is set _and_ is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.crt"]
			}
			enabled: {
				description: """
					Whether to require TLS for incoming or outgoing connections.

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
			server_name: {
				description: """
					Server name to use when using Server Name Indication (SNI).

					Only relevant for outgoing connections.
					"""
				required: false
				type: string: examples: ["www.example.com"]
			}
			verify_certificate: {
				description: """
					Enables certificate verification. For components that create a server, this requires that the
					client connections have a valid client certificate. For components that initiate requests,
					this validates that the upstream has a valid certificate.

					If enabled, certificates must not be expired and must be issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
					certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
					so on, until the verification process reaches a root certificate.

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
			The NATS URL to connect to.

			The URL takes the form of `nats://server:port`.
			If the port is not specified it defaults to 4222.
			"""
		required: true
		type: string: examples: ["nats://demo.nats.io", "nats://127.0.0.1:4242", "nats://localhost:4222,nats://localhost:5222,nats://localhost:6222"]
	}
}

generated: components: sources: nats: configuration: decoding: decodingBase & {
	type: object: options: codec: {
		required: false
		type: string: default: "bytes"
	}
}
generated: components: sources: nats: configuration: framing: framingDecoderBase & {
	type: object: options: method: {
		required: false
		type: string: default: "bytes"
	}
}
