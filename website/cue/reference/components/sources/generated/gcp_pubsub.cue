package metadata

generated: components: sources: gcp_pubsub: configuration: {
	ack_deadline_seconds: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use `ack_deadline_secs` instead."
		description: """
			The acknowledgement deadline, in seconds, to use for this stream.

			Messages that are not acknowledged when this deadline expires may be retransmitted.
			"""
		required: false
		type: uint: {}
	}
	ack_deadline_secs: {
		description: """
			The acknowledgement deadline, in seconds, to use for this stream.

			Messages that are not acknowledged when this deadline expires may be retransmitted.
			"""
		required: false
		type: uint: {
			default: 600
			unit:    "seconds"
		}
	}
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
	api_key: {
		description: """
			An [API key][gcp_api_key].

			Either an API key or a path to a service account credentials JSON file can be specified.

			If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
			filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
			running on. If this is not on a GCE instance, then you must define it with an API key or service account
			credentials JSON file.

			[gcp_api_key]: https://cloud.google.com/docs/authentication/api-keys
			"""
		required: false
		type: string: {}
	}
	credentials_path: {
		description: """
			Path to a [service account][gcp_service_account_credentials] credentials JSON file.

			Either an API key or a path to a service account credentials JSON file can be specified.

			If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
			filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
			running on. If this is not on a GCE instance, then you must define it with an API key or service account
			credentials JSON file.

			[gcp_service_account_credentials]: https://cloud.google.com/docs/authentication/production#manually
			"""
		required: false
		type: string: {}
	}
	endpoint: {
		description: "The endpoint from which to pull data."
		required:    false
		type: string: {
			default: "https://pubsub.googleapis.com"
			examples: ["https://us-central1-pubsub.googleapis.com"]
		}
	}
	full_response_size: {
		description: """
			The number of messages in a response to mark a stream as
			"busy". This is used to determine if more streams should be
			started.

			The GCP Pub/Sub servers send responses with 100 or more messages when
			the subscription is busy.
			"""
		required: false
		type: uint: default: 100
	}
	keepalive_secs: {
		description: """
			The amount of time, in seconds, with no received activity
			before sending a keepalive request. If this is set larger than
			`60`, you may see periodic errors sent from the server.
			"""
		required: false
		type: float: {
			default: 60.0
			unit:    "seconds"
		}
	}
	max_concurrency: {
		description: "The maximum number of concurrent stream connections to open at once."
		required:    false
		type: uint: default: 10
	}
	poll_time_seconds: {
		description: """
			How often to poll the currently active streams to see if they
			are all busy and so open a new stream.
			"""
		required: false
		type: float: {
			default: 2.0
			unit:    "seconds"
		}
	}
	project: {
		description: "The project name from which to pull logs."
		required:    true
		type: string: examples: ["my-log-source-project"]
	}
	retry_delay_seconds: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use `retry_delay_secs` instead."
		description:        "The amount of time, in seconds, to wait between retry attempts after an error."
		required:           false
		type: float: {}
	}
	retry_delay_secs: {
		description: "The amount of time, in seconds, to wait between retry attempts after an error."
		required:    false
		type: float: {
			default: 1.0
			unit:    "seconds"
		}
	}
	subscription: {
		description: "The subscription within the project which is configured to receive logs."
		required:    true
		type: string: examples: ["my-vector-source-subscription"]
	}
	tls: {
		description: "TLS configuration."
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
}

generated: components: sources: gcp_pubsub: configuration: decoding: decodingBase & {
	type: object: options: codec: {
		required: false
		type: string: default: "bytes"
	}
}
generated: components: sources: gcp_pubsub: configuration: framing: framingDecoderBase & {
	type: object: options: method: {
		required: false
		type: string: default: "bytes"
	}
}
