package metadata

base: components: sources: opentelemetry: configuration: {
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
	grpc: {
		description: "Configuration for the `opentelemetry` gRPC server."
		required:    true
		type: object: {
			examples: [{
				address: "0.0.0.0:4317"
			}]
			options: {
				address: {
					description: """
						The socket address to listen for connections on.

						It _must_ include a port.
						"""
					required: true
					type: string: examples: ["0.0.0.0:4317", "localhost:4317"]
				}
				tls: {
					description: "Configures the TLS options for incoming/outgoing connections."
					required:    false
					type: object: options: {
						alpn_protocols: {
							description: """
																Sets the list of supported ALPN protocols.

																Declare the supported ALPN protocols, which are used during negotiation with peer. They are prioritized in the order
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

																If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
																"""
							required: false
							type: string: examples: ["/path/to/host_certificate.crt"]
						}
						enabled: {
							description: """
																Whether or not to require TLS for incoming or outgoing connections.

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

																If enabled, certificates must not be expired and must be issued by a trusted
																issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
																certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
																so on until the verification process reaches a root certificate.

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
			}
		}
	}
	http: {
		description: "Configuration for the `opentelemetry` HTTP server."
		required:    true
		type: object: {
			examples: [{
				address: "0.0.0.0:4318"
				keepalive: {
					max_connection_age_jitter_factor: 0.1
					max_connection_age_secs:          300
				}
			}]
			options: {
				address: {
					description: """
						The socket address to listen for connections on.

						It _must_ include a port.
						"""
					required: true
					type: string: examples: ["0.0.0.0:4318", "localhost:4318"]
				}
				keepalive: {
					description: "Configuration of HTTP server keepalive parameters."
					required:    false
					type: object: options: {
						max_connection_age_jitter_factor: {
							description: """
																The factor by which to jitter the `max_connection_age_secs` value.

																A value of 0.1 means that the actual duration will be between 90% and 110% of the
																specified maximum duration.
																"""
							required: false
							type: float: default: 0.1
						}
						max_connection_age_secs: {
							description: """
																The maximum amount of time a connection may exist before it is closed by sending
																a `Connection: close` header on the HTTP response. Set this to a large value like
																`100000000` to "disable" this feature

																Only applies to HTTP/0.9, HTTP/1.0, and HTTP/1.1 requests.

																A random jitter configured by `max_connection_age_jitter_factor` is added
																to the specified duration to spread out connection storms.
																"""
							required: false
							type: uint: {
								default: 300
								examples: [600]
								unit: "seconds"
							}
						}
					}
				}
				tls: {
					description: "Configures the TLS options for incoming/outgoing connections."
					required:    false
					type: object: options: {
						alpn_protocols: {
							description: """
																Sets the list of supported ALPN protocols.

																Declare the supported ALPN protocols, which are used during negotiation with peer. They are prioritized in the order
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

																If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
																"""
							required: false
							type: string: examples: ["/path/to/host_certificate.crt"]
						}
						enabled: {
							description: """
																Whether or not to require TLS for incoming or outgoing connections.

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

																If enabled, certificates must not be expired and must be issued by a trusted
																issuer. This verification operates in a hierarchical manner, checking that the leaf certificate (the
																certificate presented by the client/server) is not only valid, but that the issuer of that certificate is also valid, and
																so on until the verification process reaches a root certificate.

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
			}
		}
	}
}
